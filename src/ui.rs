//! Loopback-only UI; controller logic stays in Rust. No network-origin mutations.
use crate::{
    Result, att,
    input::{self, HostProfile},
    interrupted, invalid, protocol,
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    env,
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};
#[cfg(target_os = "linux")]
use std::os::unix::fs::OpenOptionsExt;

pub fn default_profile_path() -> PathBuf {
    #[cfg(target_os = "windows")]
    return env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ultimate-control").join("host.json");
    #[cfg(target_os = "linux")]
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".config")
        })
        .join("ultimate-control/host.json")
}
pub fn load_profile(path: &Path) -> Result<HostProfile> {
    let mut data = Vec::new();
    File::open(path)?.take(65537).read_to_end(&mut data)?;
    if data.len() > 65536 {
        return Err(invalid("Host profile exceeds64KiB"));
    }
    let profile: HostProfile = serde_json::from_slice(&data)?;
    profile.validate_structure()?;
    Ok(profile)
}
static NEXT_TEMP: AtomicU64 = AtomicU64::new(1);
pub fn save_profile(path: &Path, profile: &HostProfile, replace: bool) -> Result<()> {
    profile.validate_structure()?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if metadata.file_type().is_symlink() {
            return Err(invalid("Refusing to replace a symlink host profile"));
        }
        if !replace {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Host profile exists; use --replace",
            )
            .into());
        }
    }
    let bytes = serde_json::to_vec_pretty(profile)?;
    let temporary = if replace {
        parent.join(format!(
            ".ultimate-control-{}-{}.tmp",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ))
    } else {
        path.to_path_buf()
    };
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(target_os = "linux")]
    options.mode(0o600);
    let mut file = options.open(&temporary)?;
    let result = (|| -> Result<()> {
        file.write_all(&bytes)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        if replace {
            fs::rename(&temporary, path)?;
            #[cfg(target_os = "linux")]
            File::open(parent)?.sync_all()?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

struct Worker {
    stopped: Arc<AtomicBool>,
    handle: JoinHandle<Result<()>>,
}
struct State {
    profile: HostProfile,
    path: PathBuf,
    worker: Option<Worker>,
    last_error: Option<String>,
}
impl State {
    fn reap(&mut self) {
        if self.worker.as_ref().is_some_and(|w| w.handle.is_finished()) {
            let worker = self.worker.take().unwrap();
            self.last_error = match worker.handle.join() {
                Ok(Ok(())) => None,
                Ok(Err(error)) => Some(error.to_string()),
                Err(_) => Some("Controller worker panicked".into()),
            };
        }
    }
    fn stop(&mut self) -> Result<()> {
        if let Some(worker) = self.worker.take() {
            worker.stopped.store(true, Ordering::Relaxed);
            worker
                .handle
                .join()
                .map_err(|_| invalid("Controller worker panicked"))??;
        }
        Ok(())
    }
    fn status(&mut self) -> Value {
        self.reap();
        json!({"profile":self.profile,"profile_path":self.path,"active":self.worker.is_some(),"error":self.last_error,"scope":"Host mapping + verified3E read-only configuration; full Windows parity unresolved"})
    }
    fn action(&mut self, path: &str, body: &[u8]) -> Result<Value> {
        self.reap();
        let value: Value = if body.is_empty() {
            json!({})
        } else {
            serde_json::from_slice(body)?
        };
        match path {
            "/api/status" => Ok(self.status()),
            "/api/devices" => Ok(
                json!({"devices":input::devices()?,"sources":input::SOURCE_KEYS,"destinations":input::DESTINATION_KEYS}),
            ),
            "/api/profile" => {
                if self.worker.is_some() {
                    return Err(invalid("Stop input mapping before changing host profile"));
                }
                let profile: HostProfile = serde_json::from_value(
                    value
                        .get("profile")
                        .ok_or_else(|| invalid("profile required"))?
                        .clone(),
                )?;
                profile.validate()?;
                save_profile(&self.path, &profile, true)?;
                self.profile = profile;
                Ok(self.status())
            }
            "/api/inspect" => {
                let bytes = protocol::decode_hex(
                    value
                        .get("hex")
                        .and_then(Value::as_str)
                        .ok_or_else(|| invalid("Profile hex required"))?,
                )?;
                protocol::inspect_config(&bytes)
            }
            "/api/scan" => {
                if self.worker.is_some() {
                    return Err(invalid("Stop input mapping before configuration discovery"));
                }
                Ok(json!(att::scan(
                    Duration::from_secs(10),
                    &AtomicBool::new(false)
                )?))
            }
            "/api/backup" => {
                if self.worker.is_some() {
                    return Err(invalid("Stop input mapping before profile backup"));
                }
                let path = value
                    .get("path")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| invalid("Backup output path required"))?;
                if Path::new(path).try_exists()? {
                    return Err(invalid("Backup exists; refusing to overwrite"));
                }
                let address = value
                    .get("address")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty());
                let mut connection = att::Connection::open(
                    address,
                    Duration::from_secs(10),
                    Arc::new(AtomicBool::new(false)),
                )?;
                let data = connection.read_profiles();
                let closed = connection.close();
                let data = data?;
                closed?;
                protocol::save_backup(Path::new(path), &data)?;
                Ok(json!({"backup":path,"inspection":protocol::inspect_config(&data)?}))
            }
            "/api/start" => {
                if self.worker.is_some() {
                    return Err(invalid("Controller mapping already active"));
                }
                self.profile.validate()?;
                let device = value
                    .get("device")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .map(PathBuf::from);
                let address = value
                    .get("address")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned);
                let profile = self.profile.clone();
                let stopped = Arc::new(AtomicBool::new(false));
                let flag = stopped.clone();
                let handle = thread::spawn(move || {
                    att::run_controller(
                        device.as_deref(),
                        &profile,
                        None,
                        address.as_deref(),
                        Duration::from_secs(10),
                        flag,
                    )
                });
                self.worker = Some(Worker { stopped, handle });
                self.last_error = None;
                Ok(self.status())
            }
            "/api/stop" => {
                self.stop()?;
                Ok(self.status())
            }
            _ => Err(invalid("Unknown application endpoint")),
        }
    }
}
impl Drop for State {
    fn drop(&mut self) {
        if let Err(error) = self.stop() {
            eprintln!("Controller shutdown: {error}");
        }
    }
}

struct Request {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}
fn request(stream: &mut TcpStream) -> Result<Request> {
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
    let mut reader = BufReader::new(stream);
    let mut header = Vec::new();
    let mut total = 0;
    loop {
        let mut line = Vec::new();
        let size = reader.by_ref().take(8193).read_until(b'\n', &mut line)?;
        total += size;
        if size == 0 || total > 8192 || !line.ends_with(b"\r\n") {
            return Err(invalid("Malformed or oversized HTTP headers"));
        }
        if line == b"\r\n" {
            break;
        }
        header.push(String::from_utf8(line[..line.len() - 2].to_vec())?);
    }
    let first = header
        .first()
        .ok_or_else(|| invalid("Missing HTTP request line"))?;
    let fields: Vec<_> = first.split(' ').collect();
    if fields.len() != 3 || !fields[2].starts_with("HTTP/1.") || !fields[1].starts_with('/') {
        return Err(invalid("Invalid HTTP request line"));
    }
    let (method, path) = (fields[0].to_owned(), fields[1].to_owned());
    let mut headers = HashMap::new();
    for line in header.iter().skip(1) {
        let (key, value) = line
            .split_once(':')
            .ok_or_else(|| invalid("Malformed HTTP header"))?;
        let key = key.to_ascii_lowercase();
        if headers.insert(key, value.trim().to_owned()).is_some() {
            return Err(invalid("Duplicate HTTP header"));
        }
    }
    if headers.contains_key("transfer-encoding") {
        return Err(invalid("Chunked requests unsupported"));
    }
    let size = headers
        .get("content-length")
        .map_or(Ok(0), |value| value.parse::<usize>())?;
    if size > 65536 {
        return Err(invalid("HTTP body exceeds64KiB"));
    }
    let mut body = vec![0; size];
    reader.read_exact(&mut body)?;
    Ok(Request {
        method,
        path,
        headers,
        body,
    })
}
fn respond(stream: &mut TcpStream, status: u16, content_type: &str, body: &[u8]) -> io::Result<()> {
    stream.set_write_timeout(Some(Duration::from_secs(3)))?;
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        422 => "Unprocessable Content",
        _ => "Error",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nContent-Security-Policy: default-src 'self'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; connect-src 'self'; img-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)
}
fn serve_one(stream: &mut TcpStream, state: &mut State, port: u16, token: &str) -> Result<()> {
    let request = match request(stream) {
        Ok(request) => request,
        Err(error) => {
            respond(
                stream,
                400,
                "application/json",
                serde_json::to_string(&json!({"error":error.to_string()}))?.as_bytes(),
            )?;
            return Ok(());
        }
    };
    let expected = format!("127.0.0.1:{port}");
    if request.headers.get("host") != Some(&expected) {
        respond(stream, 403, "text/plain", b"Invalid loopback host")?;
        return Ok(());
    }
    if request.method == "GET" && request.path == "/" {
        respond(
            stream,
            200,
            "text/html; charset=utf-8",
            INDEX.replace("__TOKEN__", token).as_bytes(),
        )?;
        return Ok(());
    }
    if request.method != "POST" || !request.path.starts_with("/api/") {
        respond(stream, 404, "text/plain", b"Not found")?;
        return Ok(());
    }
    if request.headers.get("x-ultimate-token").map(String::as_str) != Some(token)
        || request.headers.get("content-type").map(String::as_str) != Some("application/json")
        || request
            .headers
            .get("origin")
            .is_some_and(|origin| origin != &format!("http://{expected}"))
    {
        respond(
            stream,
            403,
            "text/plain",
            b"Invalid application token or origin",
        )?;
        return Ok(());
    }
    let (status, result) = match state.action(&request.path, &request.body) {
        Ok(value) => (200, value),
        Err(error) => (422, json!({"error":error.to_string()})),
    };
    respond(
        stream,
        status,
        "application/json",
        serde_json::to_string(&result)?.as_bytes(),
    )?;
    Ok(())
}

pub fn serve(port: u16, path: PathBuf) -> Result<()> {
    let profile = if path.try_exists()? {
        load_profile(&path)?
    } else {
        HostProfile::default()
    };
    let mut state = State {
        profile,
        path,
        worker: None,
        last_error: None,
    };
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    let port = listener.local_addr()?.port();
    listener.set_nonblocking(true)?;
    let mut random = [0; 32];
    File::open("/dev/urandom")?.read_exact(&mut random)?;
    let token: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
    println!("Listening on http://127.0.0.1:{port}");
    while !interrupted() {
        state.reap();
        match listener.accept() {
            Ok((mut stream, peer)) => {
                if !peer.ip().is_loopback() {
                    continue;
                }
                if let Err(error) = serve_one(&mut stream, &mut state, port, &token) {
                    eprintln!("UI request: {error}");
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(20))
            }
            Err(error) => return Err(error.into()),
        }
    }
    state.stop()?;
    Ok(())
}

const INDEX: &str = r##"<!doctype html>
<html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Ultimate Control — Rust</title>
<style>:root{color-scheme:dark light;font:16px system-ui;background:#111820;color:#e7edf4}body{max-width:1080px;margin:30px auto;padding:0 22px}h1{margin:0}header{display:flex;align-items:center;justify-content:space-between;gap:20px}small,.hint{color:#aebdce}section{background:#1b2633;border:1px solid #344658;border-radius:12px;padding:22px;margin:20px 0}h2{font-size:20px;margin:0 0 16px}label{display:flex;flex-direction:column;gap:6px;font-size:14px}input,select,button{font:inherit;color:inherit;background:#111b27;border:1px solid #506378;border-radius:6px;padding:9px}button{cursor:pointer;background:#23485c}button:hover{background:#2d617c}button:disabled{opacity:.5;cursor:default}.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(200px,1fr));gap:16px}.buttons{display:flex;flex-wrap:wrap;gap:10px;margin-top:16px}.check{flex-direction:row;align-items:center}#status{border-radius:8px;padding:10px 15px;background:#273b47}#error{color:#ffc0bc;white-space:pre-wrap;min-height:24px;margin:12px 0}pre{white-space:pre-wrap;word-break:break-word;background:#111b27;padding:15px;border-radius:8px}table{width:100%;border-collapse:collapse}td,th{text-align:left;padding:8px;border-bottom:1px solid #344658}.warning{color:#f3cf85}a{color:#9ecbff}footer{margin:30px 0;font-size:14px;color:#aebdce}</style>
<header><div><h1>Ultimate Control</h1><small>Native Rust · host input customization</small></div><span id="status" role="status">Loading…</span></header>
<p class="warning">Verified Ultimate 3E paths, not a complete Windows software replacement. No controller profile writes or firmware flashing.</p><div id="error" role="alert"></div>
<section><h2>Controller connections</h2><div class="grid"><label>Bluetooth gaming input<select id="device" aria-label="Bluetooth gaming input"><option value="">Automatic selection</option></select></label><label>Configuration BLE address<input id="address" placeholder="Automatic discovery of81HF01" autocomplete="off"></label></div><div class="buttons"><button id="refresh">Refresh gaming devices</button><button id="scan">Scan configuration BLE</button><button id="start">Start host mapping</button><button id="stop">Stop and release input</button></div><p class="hint">Connect gaming Bluetooth, then hold front pixel-logo/profile button for3seconds until configuration LEDs blink. Close mobile app. Virtual gamepad uses evdev/uinput; HIDAPI games may need SDL_JOYSTICK_HIDAPI=0.</p><pre id="devices">Reading device inventory…</pre></section>
<section><h2>Gyro right-stick aiming</h2><form id="profile"><div class="grid"><label>Hold to disable gyro<select id="button"><option>R4</option><option>L4</option><option>PL</option><option>PR</option></select></label><label>Sensitivity<input id="sensitivity" type="number" min="0" max="100" step="any" required></label><label>Deadzone (degrees/second)<input id="deadzone" type="number" min="0" step="any" required></label><label class="check"><input id="invert_x" type="checkbox">Invert horizontal gyro</label><label class="check"><input id="invert_y" type="checkbox">Invert vertical gyro</label></div><p class="hint">Physical right stick and View preserved while clutch held. These host units are not firmware sensitivity levels. Motion stalls clear only gyro contribution.</p><h3>Button mappings</h3><div class="grid"><label>Controller source<select id="source"></select></label><label>Gamepad / keyboard / mouse destination<select id="destination"></select></label></div><div class="buttons"><button id="addmap" type="button">Add mapping</button><button id="clearmap" type="button">Reset mappings</button></div><table><thead><tr><th>Source</th><th>Destination</th><th>Action</th></tr></thead><tbody id="mappings"></tbody></table><div class="buttons"><button type="submit">Save host profile</button></div><p class="hint" id="profilepath"></p></form></section>
<section><h2>Controller profiles — read only</h2><div class="grid"><label>Inspect existing868-byte backup<input id="upload" type="file" accept=".bin"></label><label>New backup output path<input id="backup_path" placeholder="/home/user/profiles.bin"></label></div><div class="buttons"><button id="backup">Read profiles and save new backup</button></div><p class="hint">Backup temporarily disables controller reporting and restores it. Existing files never overwritten. CRC validity and current numbered profile not inferred.</p><pre id="inspection">Select a backup or read connected controller.</pre></section>
<footer>Steam Input-like host remapping and gyro clutch. Virtual device does not expose rumble or raw IMU to games. Receiver-only gyro, native inverse-hold firmware and full multi-model Windows parity remain unverified.</footer>
<script>
const token='__TOKEN__';const el=id=>document.getElementById(id);let mappings={};let loaded=false;let active=false;
async function api(path,body={}){const response=await fetch('/api/'+path,{method:'POST',headers:{'Content-Type':'application/json','X-Ultimate-Token':token},body:JSON.stringify(body)});let data;try{data=await response.json()}catch{throw Error(await response.text())}if(!response.ok)throw Error(data.error||response.statusText);return data}
async function act(fn){el('error').textContent='';try{await fn()}catch(error){el('error').textContent=error.message}}
function renderMappings(){el('mappings').replaceChildren();for(const[source,destination]of Object.entries(mappings)){const row=document.createElement('tr');for(const text of[source,destination]){const cell=document.createElement('td');cell.textContent=text;row.append(cell)}const cell=document.createElement('td');const button=document.createElement('button');button.type='button';button.textContent='Remove';button.disabled=active;button.onclick=()=>{delete mappings[source];renderMappings()};cell.append(button);row.append(cell);el('mappings').append(row)}}
function status(data){active=data.active;el('status').textContent=active?'Starting / running host mapping':'Input released';el('start').disabled=active;el('stop').disabled=!active;for(const control of el('profile').querySelectorAll('input,select,button'))control.disabled=active;el('scan').disabled=active;el('backup').disabled=active;if(!loaded){for(const id of['button','sensitivity','deadzone'])el(id).value=data.profile[id];for(const id of['invert_x','invert_y'])el(id).checked=data.profile[id];mappings={...data.profile.mappings};renderMappings();loaded=true}el('profilepath').textContent='Host settings: '+data.profile_path;if(data.error)el('error').textContent=data.error}
async function refresh(){const data=await api('devices');const current=el('device').value;el('device').replaceChildren(new Option('Automatic selection',''));for(const device of data.devices)el('device').add(new Option(device.name+' — '+device.path,device.path));el('device').value=current;el('devices').textContent=data.devices.length?JSON.stringify(data.devices,null,2):'No Ultimate 3E Bluetooth gaming pad detected. USB receiver is not a gyro source.';for(const[id,choices]of[['source',data.sources],['destination',data.destinations]]){el(id).replaceChildren();for(const[name]of choices)el(id).add(new Option(name,name))}}
el('refresh').onclick=()=>act(refresh);el('scan').onclick=()=>act(async()=>{el('devices').textContent='Scanning for81HF01…';const radios=await api('scan');el('devices').textContent=JSON.stringify(radios,null,2);if(radios.length===1)el('address').value=radios[0].address});
el('start').onclick=()=>act(async()=>status(await api('start',{device:el('device').value,address:el('address').value})));el('stop').onclick=()=>act(async()=>status(await api('stop')));
el('addmap').onclick=()=>{if(!el('source').value||!el('destination').value)return;mappings[el('source').value]=el('destination').value;renderMappings()};el('clearmap').onclick=()=>{mappings={};renderMappings()};
el('profile').onsubmit=event=>{event.preventDefault();act(async()=>{const profile={button:el('button').value,sensitivity:Number(el('sensitivity').value),deadzone:Number(el('deadzone').value),invert_x:el('invert_x').checked,invert_y:el('invert_y').checked,mappings};status(await api('profile',{profile}));el('status').textContent='Host profile saved'})};
el('upload').onchange=()=>act(async()=>{const file=el('upload').files[0];if(!file)return;if(file.size!==868)throw Error('Expected exactly868profile bytes');const bytes=new Uint8Array(await file.arrayBuffer());const hex=Array.from(bytes,b=>b.toString(16).padStart(2,'0')).join('');el('inspection').textContent=JSON.stringify(await api('inspect',{hex}),null,2)});
el('backup').onclick=()=>act(async()=>{el('inspection').textContent='Reading profiles; reporting temporarily disabled…';el('inspection').textContent=JSON.stringify(await api('backup',{path:el('backup_path').value,address:el('address').value}),null,2)});
act(async()=>{status(await api('status'));await refresh()});setInterval(()=>api('status').then(status).catch(error=>{el('error').textContent=error.message}),1500);
</script></html>"##;

#[cfg(test)]
mod host_profile_tests {
    use super::*;
    #[test]
    fn persistence_roundtrip_is_link_agnostic_and_preserves_host_controls(){
        let directory=env::temp_dir().join(format!("ultimate-host-test-{}-{}",std::process::id(),NEXT_TEMP.fetch_add(1,Ordering::Relaxed)));
        let path=directory.join("host.json");
        let mut profile=HostProfile::default();
        profile.mappings.insert("BTN_TRIGGER_HAPPY2".into(),"KEY_SPACE".into());
        profile.turbo.insert("BTN_SOUTH".into(),12.5);
        profile.toggle.push("BTN_EAST".into());
        profile.stick_mouse=Some(input::StickMouse{stick:input::Stick::Left,sensitivity:1500.0,deadzone:0.12});
        profile.trigger_buttons.right=Some(input::TriggerButton{destination:"BTN_LEFT".into(),threshold:0.3});
        save_profile(&path,&profile,false).unwrap();
        let loaded=load_profile(&path).unwrap();
        assert_eq!(serde_json::to_value(&loaded).unwrap(),serde_json::to_value(&profile).unwrap());
        assert!(loaded.validate_for_link(input::LinkKind::Receiver).is_ok());
        assert!(loaded.validate_for_link(input::LinkKind::Bluetooth).is_err());
        assert!(save_profile(&path,&profile,false).is_err());
        save_profile(&path,&profile,true).unwrap();
        fs::remove_file(path).unwrap();fs::remove_dir(directory).unwrap();
    }
}
