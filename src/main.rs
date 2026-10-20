use std::{
    env,
    fs::File,
    io::{self, BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use ultimate_control::{
    Result, att,
    input::{self, HostProfile, Mapper},
    gui, invalid, protocol, ui,
};

#[cfg(target_os = "linux")]
extern "C" fn interrupt(_: libc::c_int) {
    ultimate_control::INTERRUPTED.store(true, Ordering::Relaxed);
}

#[cfg(windows)]
unsafe extern "system" fn console_interrupt(control: u32) -> windows::core::BOOL {
    use windows::Win32::System::Console::{CTRL_C_EVENT, CTRL_BREAK_EVENT};
    if control == CTRL_C_EVENT || control == CTRL_BREAK_EVENT {
        ultimate_control::INTERRUPTED.store(true, Ordering::Relaxed);
        windows::core::BOOL(1)
    } else {
        windows::core::BOOL(0)
    }
}
fn help() {
    println!(
        "Ultimate Control — native Rust controller tools\n\n  ultimate-control ui [--port PORT] [--profile HOST.json]\n  ultimate-control devices\n  ultimate-control scan [--timeout SECONDS]\n  ultimate-control inspect PROFILES.bin\n  ultimate-control backup PROFILES.bin [--address ADDRESS] [--timeout SECONDS]\n  ultimate-control gyro [--device /dev/input/eventN] [--address ADDRESS]\n      [--profile HOST.json] [--button R4|L4|PL|PR] [--sensitivity NUMBER]\n      [--deadzone NUMBER] [--invert-x] [--invert-y] [--map SOURCE=DESTINATION]\n  ultimate-control profile HOST.json [host options above] [--replace]\n  ultimate-control replay INPUT.jsonl [--profile HOST.json]\n  ultimate-control reassemble CAPTURE.jsonl [--output PROFILES.bin]\n  ultimate-control motion-packet --slot 1|2|3 --record HEX [--crc]\n  ultimate-control windows-motion --feature HEX_BYTE\n  ultimate-control windows-commit\n\nUI and gyro customize host input only; no profile writes or firmware flashing.\nBackup temporarily disables reporting and restores it before returning.\nGaming Bluetooth + configuration advertisement81HF01 required for gyro.\nFull Windows application equivalence is NOT established; see reconstruction/manifest.json."
    );
    println!("  ultimate-control gui [--demo] [--bench] [--debug-pointer] [--page NAME] [--screenshot PATH.png] [--assets DIR]");
    println!("  ultimate-control wired-info\n  ultimate-control wired-version --takeover\n  ultimate-control wired-backup PROFILES.bin --takeover\n  ultimate-control wired-write PROFILES.bin --takeover\n  ultimate-control wired-xinput-mode --takeover --confirm   (receiver stuck as 2dc8:202f -> back to PC mode 3024)");
    println!("  ultimate-control wired-calibrate sticks|triggers --takeover --confirm");
    println!("  ultimate-control wired-monitor SECONDS --takeover [--report-enable] [--heartbeat]   (diagnostic: log incoming GIP messages)");
    println!("  ultimate-control ble-write PROFILES.bin [--address ADDRESS] [--timeout SECONDS]");
    println!("  ultimate-control receiver-xbox-mode --confirm   (receiver 3024 -> Xbox/GIP 202f; reverse with wired-xinput-mode)");
}
fn option(args: &mut Vec<String>, name: &str) -> Result<Option<String>> {
    let positions: Vec<_> = args
        .iter()
        .enumerate()
        .filter(|(_, a)| a.as_str() == name)
        .map(|(i, _)| i)
        .collect();
    if positions.len() > 1 {
        return Err(invalid(format!("Duplicate {name}")));
    }
    if let Some(&at) = positions.first() {
        args.remove(at);
        if at == args.len() || args[at].starts_with("--") {
            return Err(invalid(format!("{name} needs a value")));
        }
        Ok(Some(args.remove(at)))
    } else {
        Ok(None)
    }
}
fn flag(args: &mut Vec<String>, name: &str) -> Result<bool> {
    let Some(at) = args.iter().position(|a| a == name) else {
        return Ok(false);
    };
    args.remove(at);
    if args.iter().any(|a| a == name) {
        return Err(invalid(format!("Duplicate {name}")));
    }
    Ok(true)
}
fn arity(args: &[String], n: usize) -> Result<()> {
    if args.len() != n || args.iter().any(|a| a.starts_with("--")) {
        return Err(invalid(format!(
            "Unexpected or missing arguments: {args:?}; use --help"
        )));
    }
    Ok(())
}
fn seconds(args: &mut Vec<String>) -> Result<Duration> {
    let value = option(args, "--timeout")?
        .unwrap_or_else(|| "10".into())
        .parse::<f64>()?;
    if !value.is_finite() || value <= 0.0 || value > 300.0 {
        return Err(invalid("Timeout must be finite and within0..300 seconds"));
    }
    Ok(Duration::from_secs_f64(value))
}
fn profile(args: &mut Vec<String>) -> Result<HostProfile> {
    let mut profile = if let Some(path) = option(args, "--profile")? {
        ui::load_profile(Path::new(&path))?
    } else {
        HostProfile::default()
    };
    if let Some(button) = option(args, "--button")? {
        profile.button = button;
    }
    if let Some(sensitivity) = option(args, "--sensitivity")? {
        profile.sensitivity = sensitivity.parse()?;
    }
    if let Some(deadzone) = option(args, "--deadzone")? {
        profile.deadzone = deadzone.parse()?;
    }
    if flag(args, "--invert-x")? {
        profile.invert_x = true;
    }
    if flag(args, "--invert-y")? {
        profile.invert_y = true;
    }
    let mut mapped = std::collections::BTreeSet::new();
    while let Some(at) = args.iter().position(|a| a == "--map") {
        args.remove(at);
        if at >= args.len() {
            return Err(invalid("--map needs SOURCE=DESTINATION"));
        }
        let mapping = args.remove(at);
        let (source, destination) = mapping
            .split_once('=')
            .ok_or_else(|| invalid("--map needs SOURCE=DESTINATION"))?;
        if !mapped.insert(source.to_owned()) {
            return Err(invalid(format!("Duplicate mapping for {source}")));
        }
        profile.mappings.insert(source.into(), destination.into());
    }
    profile.validate_structure()?;
    Ok(profile)
}
fn json(value: &impl serde::Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
fn image(path: &Path) -> Result<Vec<u8>> {
    let mut bytes = Vec::with_capacity(protocol::CONFIG_SIZE + 1);
    File::open(path)?
        .take((protocol::CONFIG_SIZE + 1) as u64)
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}

#[derive(serde::Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum Replay {
    Physical { data: protocol::PhysicalInput },
    Hid { hex: String },
    Stall,
    Close,
}
fn replay(path: &str, profile: &HostProfile) -> Result<()> {
    let source: Box<dyn BufRead> = if path == "-" {
        Box::new(BufReader::new(io::stdin()))
    } else {
        Box::new(BufReader::new(File::open(path)?))
    };
    let mut mapper = Mapper::new(profile, input::default_axes())?;
    for (index, line) in source.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let event: Replay = serde_json::from_str(&line)
            .map_err(|error| invalid(format!("Replay line{}: {error}", index + 1)))?;
        let frame = match event {
            Replay::Physical { data } => mapper.physical_input(data),
            Replay::Hid { hex } => mapper.input(&protocol::decode_hex(&hex)?)?,
            Replay::Stall => mapper.pause_motion(),
            Replay::Close => mapper.close(),
        };
        println!(
            "{}",
            serde_json::json!({"line":index+1,"events":frame.as_slice()})
        );
    }
    let frame = mapper.close();
    println!(
        "{}",
        serde_json::json!({"kind":"teardown","events":frame.as_slice()})
    );
    Ok(())
}

#[cfg(target_os = "linux")]
fn calibration_enter() -> Result<()> {
    io::stdout().flush()?;
    loop {
        if ultimate_control::interrupted() { return Err(invalid("Interrupted before Start; nothing sent")); }
        let mut fd = libc::pollfd { fd: 0, events: libc::POLLIN, revents: 0 };
        // SAFETY: one valid pollfd, bounded wait; stdin is read one byte at a time.
        let ready = unsafe { libc::poll(&mut fd, 1, 100) };
        if ready < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted { continue; }
            return Err(error.into());
        }
        if ready == 0 { continue; }
        let mut byte = [0];
        // SAFETY: byte is writable for one byte. Avoid buffered stdin prefetch
        // because readiness polling must correspond to each individual read.
        let count = unsafe { libc::read(0, byte.as_mut_ptr().cast(), 1) };
        if count < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted { continue; }
            return Err(error.into());
        }
        if count == 0 { return Err(invalid("No Enter confirmation on stdin; nothing sent")); }
        if byte[0] == b'\n' { return Ok(()); }
    }
}
#[cfg(target_os = "windows")]
fn calibration_enter() -> Result<()> {
    Err(invalid("Wired calibration is not yet supported on Windows"))
}

fn wired_calibrate(kind: ultimate_control::gip::CalibrationKind) -> Result<()> {
    use ultimate_control::gip::{Calibration, CalibrationStep, Gip, calibration_axis};
    println!("WARNING: Calibration changes persistent physical calibration. No calibration backup or rollback is available; profile backups cannot restore it.");
    println!("Do not touch sticks or triggers before Start. Set the trigger mode switch OFF.");
    println!("Stop host mapping and active rumble. Gamepad/audio input pauses for the entire calibration.");
    println!("Press Enter to send Start once; Ctrl-C before End abandons calibration without a cancel or rollback.");
    calibration_enter()?;
    let device = ultimate_control::gip::find()?;
    let mut wired = Gip::takeover(&device.node)?;
    let mut start_acknowledged = false;
    let mut end_attempted = false;
    let result: Result<()> = (|| {
        if ultimate_control::interrupted() { return Err(invalid("Interrupted before Start; nothing sent")); }
        println!("Sending Start({}) once; waiting up to 10 s for vendor acknowledgment.", kind.selector());
        wired.calibration_start(kind.selector())?;
        start_acknowledged = true;
        let mut calibration = Calibration::new(kind);
        let mut step = calibration.step();
        println!("{}", step.prompt());
        let mut last_live = Instant::now() - Duration::from_millis(100);
        while step != CalibrationStep::Complete {
            let Some(values) = wired.next_physical(Duration::from_millis(100))? else { continue; };
            if last_live.elapsed() >= Duration::from_millis(100) {
                eprintln!("live LS=({:+},{:+}) RS=({:+},{:+}) LT={} RT={}",
                    calibration_axis(values[0]), calibration_axis(values[1]),
                    calibration_axis(values[2]), calibration_axis(values[3]), values[4], values[5]);
                last_live = Instant::now();
            }
            let next = calibration.advance(values);
            if next != step { println!("{}", next.prompt()); step = next; }
        }
        if ultimate_control::interrupted() { return Err(invalid("Interrupted before End")); }
        end_attempted = true;
        println!("Sending End({}) once; waiting up to 100 s for vendor acknowledgment.", kind.selector());
        let ended = wired.calibration_end(kind.selector());
        match &ended {
            Ok(()) => println!("calibration acknowledged"),
            Err(error) => println!("completion unknown: {error}"),
        }
        if ultimate_control::interrupted() { return ended; }
        println!("Release all controls; reading neutral LS/RS/LT/RT for 3 s (not an accuracy self-test).");
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut sums = [0i64; 6];
        let mut samples = 0u32;
        let summary: Result<()> = (|| {
            while Instant::now() < deadline {
                if let Some(values) = wired.next_physical(deadline.saturating_duration_since(Instant::now()))? {
                    for i in 0..6 {
                        sums[i] += i64::from(if i < 4 { calibration_axis(values[i]) } else { i16::from(values[i]) });
                    }
                    samples += 1;
                }
            }
            if samples == 0 {
                println!("neutral (3 s): no physical reports; LS/RS/LT/RT unobserved");
            } else {
                let mean = sums.map(|sum| sum as f64 / f64::from(samples));
                println!("neutral (3 s, {samples} samples; mean): LS=({:.1},{:.1}) RS=({:.1},{:.1}) LT={:.1} RT={:.1}",
                    mean[0], mean[1], mean[2], mean[3], mean[4], mean[5]);
            }
            Ok(())
        })();
        match (ended, summary) {
            (Ok(()), result) => result,
            (Err(error), Ok(())) => Err(error),
            (Err(error), Err(summary)) => Err(invalid(format!("{error}; neutral read failed: {summary}"))),
        }
    })();
    if result.is_err() && !end_attempted {
        if start_acknowledged {
            eprintln!("Calibration was started but not ended. State unknown; rerun to recalibrate. No End, cancel, reset, or rollback sent.");
        } else {
            eprintln!("Start was not acknowledged: completion unknown; calibration may have started but was not ended. No retry, End, cancel, reset, or rollback sent.");
        }
    }
    let handback = wired.close();
    match (result, handback) {
        (result, Ok(rebind)) => {
            eprintln!("Wired handback: {}", serde_json::to_string(&rebind)?);
            result
        },
        (Ok(()), Err(error)) => Err(error),
        (Err(error), Err(handback)) => Err(invalid(format!("{error}; handback ALSO failed: {handback}"))),
    }
}

fn dispatch() -> Result<()> {
    let mut args: Vec<_> = env::args().skip(1).collect();
    let command = if args.is_empty() {
        "ui".into()
    } else {
        args.remove(0)
    };
    #[cfg(target_os = "windows")]
    if command == "gyro" || command.starts_with("wired-")
    {
        return Err(invalid(format!("{command} is not yet supported on Windows")));
    }
    #[cfg(windows)]
    if command == "receiver-xbox-mode" {
        return Err(invalid("Receiver Xbox mode switching unavailable on Windows: recovery to PC mode currently requires Linux wired-xinput-mode --takeover --confirm"));
    }
    let stopped = Arc::new(AtomicBool::new(false));
    match command.as_str() {
        "--help" | "-h" | "help" => {
            arity(&args, 0)?;
            help();
        }
        "devices" => {
            arity(&args, 0)?;
            #[cfg(target_os = "linux")]
            json(&input::devices()?)?;
            #[cfg(windows)]
            json(&ultimate_control::receiver::devices()?)?;
        }
        "scan" => {
            let timeout = seconds(&mut args)?;
            arity(&args, 0)?;
            json(&att::scan(timeout, &stopped)?)?;
        }
        "inspect" => {
            arity(&args, 1)?;
            json(&protocol::inspect_config(&image(Path::new(&args[0]))?)?)?;
        }
        "factory-preview" => {
            // Offline: the exact image Settings → Restore factory profiles would save.
            arity(&args, 2)?;
            let input: [u8; protocol::CONFIG_SIZE] = image(Path::new(&args[0]))?
                .try_into()
                .map_err(|_| invalid("Profile image must be exactly 868 bytes"))?;
            let reset = ultimate_control::profile::Profiles::decode(&input)?.factory_defaults().encode();
            protocol::save_backup(Path::new(&args[1]), &reset)?;
            let changed = input.iter().zip(&reset).filter(|(a, b)| a != b).count();
            eprintln!("{changed} bytes differ from input");
        }
        "backup" => {
            let address = option(&mut args, "--address")?;
            let timeout = seconds(&mut args)?;
            arity(&args, 1)?;
            let path = Path::new(&args[0]);
            if path.try_exists()? {
                return Err(invalid("Backup exists; refusing to overwrite"));
            }
            eprintln!("Reading profiles; controller reporting temporarily disabled");
            let mut connection = att::Connection::open(address.as_deref(), timeout, stopped)?;
            let result = connection.read_profiles();
            let closed = connection.close();
            let data = result?;
            closed?;
            protocol::save_backup(path, &data)?;
            json(&protocol::inspect_config(&data)?)?;
        }
        "wired-info" => {
            arity(&args, 0)?;
            eprintln!("Reading wired USB identity, cached descriptors, drivers and permissions only; xone remains attached");
            json(&ultimate_control::gip::discover()?)?;
        }
        "wired-version" | "wired-backup" | "wired-write" => {
            let takeover = flag(&mut args, "--takeover")?;
            arity(&args, if command == "wired-version" { 0 } else { 1 })?;
            if !takeover {
                return Err(invalid("Explicit --takeover required: wired gamepad/audio input is suspended briefly. Stop active rumble first. USB write access needs the README udev rule for 2dc8:202f installed as an early (70-) rules file (NixOS extraRules/99-local is too late for uaccess). Rediscover after reconnect; never use mode 0666."));
            }
            let desired: Option<[u8; protocol::CONFIG_SIZE]> = if command == "wired-write" {
                let bytes: [u8; protocol::CONFIG_SIZE] = image(Path::new(&args[0]))?.try_into()
                    .map_err(|_| invalid("Profile image must be exactly 868 bytes"))?;
                if ultimate_control::profile::crc(&bytes) != u16::from_le_bytes([bytes[0], bytes[1]]) {
                    return Err(invalid("Refusing wired write: invalid profile CRC"));
                }
                Some(bytes)
            } else {
                if command == "wired-backup" && Path::new(&args[0]).try_exists()? {
                    return Err(invalid("Backup exists; refusing to overwrite"));
                }
                None
            };
            let device = ultimate_control::gip::find()?;
            let action = match command.as_str() {
                "wired-version" => "read one 40-byte version record; no report-mode change",
                "wired-backup" => "read all 868 profile bytes directly; no report-mode change or commit",
                _ => "disable reporting, WRITE + COMMIT 868 bytes persistently, restore reporting, then verify read-back",
            };
            eprintln!("Exclusive wired takeover of {}: {action}. Gamepad/audio input pauses; candidate vendor flags 00, no automatic alternate flags or retries. Stop active host rumble before continuing.", device.node.display());
            let mut wired = ultimate_control::gip::Gip::takeover(&device.node)?;
            let result: Result<serde_json::Value> = (|| {
                if command == "wired-version" {
                    let record = wired.read_version()?;
                    return Ok(serde_json::json!({
                        "record_hex": record.iter().map(|byte| format!("{byte:02x}")).collect::<String>(),
                        "note": "Raw wired version; component interpretation not live-proven",
                    }));
                }
                if let Some(image) = desired.as_ref() { wired.write_profiles(image)?; }
                let data = wired.read_profiles()?;
                if let Some(image) = desired.as_ref() {
                    // Bytes 2/3 are device-managed (slot, XInput/profile bits); verify CRC + body.
                    if data[..2] != image[..2] || data[4..] != image[4..] {
                        return Err(invalid("Wired commit acknowledged but read-back differs from written image"));
                    }
                    eprintln!("Wired read-back identical to written image (CRC + bytes 4..867)");
                } else {
                    protocol::save_backup(Path::new(&args[0]), &data)?;
                }
                Ok(serde_json::to_value(protocol::inspect_config(&data)?)?)
            })();
            let restored = wired.close();
            let value = match (result, restored) {
                (Ok(value), Ok(rebind)) => {
                    eprintln!("Wired handback: {}", serde_json::to_string(&rebind)?);
                    value
                }
                (Err(error), Ok(rebind)) => {
                    eprintln!("Wired handback: {}", serde_json::to_string(&rebind)?);
                    return Err(error);
                }
                (Ok(_), Err(error)) => return Err(error),
                (Err(error), Err(cleanup)) => return Err(invalid(format!("{error}; handback ALSO failed: {cleanup}"))),
            };
            json(&value)?;
        }
        "wired-monitor" => {
            // Diagnostic: log incoming GIP messages; optional live-proven report enable only.
            if !flag(&mut args, "--takeover")? { return Err(invalid("Requires --takeover (gamepad input pauses)")); }
            let enable = flag(&mut args, "--report-enable")?;
            let heartbeat = flag(&mut args, "--heartbeat")?;
            arity(&args, 1)?;
            let seconds: f64 = args[0].parse().map_err(|_| invalid("SECONDS must be a number"))?;
            let device = ultimate_control::gip::find()?;
            let mut wired = ultimate_control::gip::Gip::takeover(&device.node)?;
            let result: Result<()> = (|| {
                if enable { wired.enable_reporting()?; eprintln!("report enable acknowledged"); }
                let start = std::time::Instant::now();
                let mut last: std::collections::BTreeMap<(u8, u8, Vec<u8>), usize> = Default::default();
                let mut beat_at = start;
                while start.elapsed().as_secs_f64() < seconds {
                    if heartbeat && std::time::Instant::now() >= beat_at {
                        wired.heartbeat()?;
                        beat_at += ultimate_control::receiver::HEARTBEAT_INTERVAL;
                    }
                    if let Some((kind, flags, payload)) = wired.next_message(std::time::Duration::from_millis(100))? {
                        let prefix = payload.iter().take(3).copied().collect::<Vec<_>>();
                        let count = last.entry((kind, flags, prefix)).or_default();
                        *count += 1;
                        if *count <= 3 || payload.iter().any(|&b| b == 0xff) && *count % 25 == 0 {
                            println!("{:6.2} kind={kind:02x} flags={flags:02x} len={} {}", start.elapsed().as_secs_f64(), payload.len(),
                                payload.iter().map(|b| format!("{b:02x}")).collect::<String>());
                        }
                    }
                }
                for ((kind, flags, prefix), count) in last {
                    println!("summary kind={kind:02x} flags={flags:02x} prefix={} count={count}", prefix.iter().map(|b| format!("{b:02x}")).collect::<String>());
                }
                Ok(())
            })();
            let closed = wired.close();
            result?;
            eprintln!("Wired handback: {}", serde_json::to_string(&closed?)?);
        }
        "wired-calibrate" => {
            let takeover = flag(&mut args, "--takeover")?;
            let confirm = flag(&mut args, "--confirm")?;
            arity(&args, 1)?;
            let kind = match args[0].as_str() {
                "sticks" => ultimate_control::gip::CalibrationKind::Sticks,
                "triggers" => ultimate_control::gip::CalibrationKind::Triggers,
                _ => return Err(invalid("Calibration must be sticks or triggers")),
            };
            if !takeover || !confirm {
                return Err(invalid("Requires --takeover --confirm: persistent physical calibration, no backup or rollback; gamepad/audio pauses until handback."));
            }
            wired_calibrate(kind)?;
        }
        "receiver-xbox-mode" => {
            // Windows "Xinput Mode" off. Persistent; reverse with `wired-xinput-mode`.
            if !flag(&mut args, "--confirm")? {
                return Err(invalid("Requires --confirm: sends ONE persistent opcode 0x0a parameter 0; the receiver re-enumerates as Xbox/GIP 2dc8:202f. Reverse with `wired-xinput-mode --takeover --confirm`."));
            }
            arity(&args, 0)?;
            let device = ultimate_control::receiver::find()?;
            ultimate_control::receiver::Receiver::open(&device)?.enter_xbox_mode()?;
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            let gip = loop {
                let found = ultimate_control::gip::discover().unwrap_or_default();
                if !found.is_empty() || std::time::Instant::now() >= deadline { break found; }
                std::thread::sleep(std::time::Duration::from_millis(200));
            };
            // `writable` can lag the udev ACL by a moment right after re-enumeration (live).
            json(&serde_json::json!({
                "switched": !gip.is_empty(),
                "gip_202f_devices": gip.iter().map(|device| device.node.display().to_string()).collect::<Vec<_>>(),
            }))?;
            if gip.is_empty() {
                return Err(invalid("Receiver did not re-appear as 2dc8:202f within 10 s. Not retried."));
            }
        }
        "wired-xinput-mode" => {
            // Recovery for a receiver switched to Xbox/GIP 202f by opcode 0x0a p0. Persistent.
            if !flag(&mut args, "--takeover")? || !flag(&mut args, "--confirm")? {
                return Err(invalid("Requires --takeover --confirm: sends ONE persistent opcode 0x0a parameter 1 (Windows \"Xinput Mode\") over GIP; the device may re-enumerate. Not retried."));
            }
            arity(&args, 0)?;
            let device = ultimate_control::gip::find()?;
            eprintln!("Exclusive wired takeover of {}: send Xinput Mode ON once, then watch for 2dc8:3024", device.node.display());
            let mut wired = ultimate_control::gip::Gip::takeover(&device.node)?;
            let sent = wired.enable_xinput_mode();
            // Handback is expected to fail if the device left 202f mode.
            let handback = wired.close();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            let receiver = loop {
                if let Ok(path) = ultimate_control::receiver::find() { break Some(path); }
                if std::time::Instant::now() >= deadline { break None; }
                std::thread::sleep(std::time::Duration::from_millis(200));
            };
            let still_gip: Vec<String> = ultimate_control::gip::discover().unwrap_or_default().iter()
                .map(|device| device.sysfs.display().to_string()).collect();
            // Live: the receiver may drop before acknowledging (ESHUTDOWN) and handback then
            // fails with ENODEV. Success is the outcome: 3024 vendor interface back, no 202f left.
            let switched = receiver.is_some() && still_gip.is_empty();
            json(&serde_json::json!({
                "switched": switched,
                "acknowledged": sent.as_ref().map(|_| true).map_err(|error| error.to_string()),
                "handback": handback.map(|rebind| serde_json::to_value(rebind).unwrap_or_default()).map_err(|error| error.to_string()),
                "receiver_3024_vendor_interface": receiver.map(|path| path.display().to_string()),
                "gip_202f_devices": still_gip,
            }))?;
            if !switched {
                return Err(invalid("Receiver did not return to PC mode (2dc8:3024); see the JSON above. Not retried."));
            }
        }
        "ble-write" => {
            // Verified live 2026-10-15: write + commit, then read back (CRC + body).
            let address = option(&mut args, "--address")?;
            let timeout = seconds(&mut args)?;
            arity(&args, 1)?;
            let image: [u8; protocol::CONFIG_SIZE] = image(Path::new(&args[0]))?
                .try_into()
                .map_err(|_| invalid("Profile image must be exactly 868 bytes"))?;
            let mut connection = att::Connection::open(address.as_deref(), timeout, stopped)?;
            eprintln!("Writing and committing 868-byte profile image over Bluetooth (persistent)");
            let result = connection.write_profiles(&image).and_then(|()| connection.read_profiles());
            let closed = connection.close();
            let readback = result?;
            closed?;
            if readback[4..] != image[4..] {
                return Err(invalid("Commit acknowledged but read-back body differs from written image"));
            }
            eprintln!("Read-back body identical (bytes 4..867); header bytes 2/3 are device-managed");
            json(&protocol::inspect_config(&readback)?)?;
        }
        "receiver-backup" => {
            arity(&args, 1)?;
            let path = Path::new(&args[0]);
            if path.try_exists()? {
                return Err(invalid("Backup exists; refusing to overwrite"));
            }
            let device = ultimate_control::receiver::find()?;
            eprintln!("Reading profiles over {}; reporting temporarily disabled", device.display());
            let data = ultimate_control::receiver::Receiver::open(&device)?.read_profiles()?;
            protocol::save_backup(path, &data)?;
            json(&protocol::inspect_config(&data)?)?;
        }
        "receiver-write" => {
            arity(&args, 1)?;
            let image: [u8; protocol::CONFIG_SIZE] = image(Path::new(&args[0]))?
                .try_into()
                .map_err(|_| invalid("Profile image must be exactly 868 bytes"))?;
            let device = ultimate_control::receiver::find()?;
            let mut receiver = ultimate_control::receiver::Receiver::open(&device)?;
            eprintln!("Writing and committing 868-byte profile image over {} (persistent)", device.display());
            receiver.write_profiles(&image)?;
            let readback = receiver.read_profiles()?;
            // Bytes 2 (runtime slot) and 3 (Tournament/Auto-Reconnect) are device-managed:
            // opcode-1 writes do not change them. Verify CRC + body only.
            if readback.len() != image.len() || readback[..2] != image[..2] || readback[4..] != image[4..] {
                return Err(invalid("Commit acknowledged but read-back differs from written image"));
            }
            eprintln!("Read-back identical to written image (CRC + bytes 4..867; header bytes 2/3 are device-managed)");
            json(&protocol::inspect_config(&readback)?)?;
        }
        "receiver-version" => {
            arity(&args, 0)?;
            let device = ultimate_control::receiver::find()?;
            let record = ultimate_control::receiver::Receiver::open(&device)?.read_version()?;
            json(&serde_json::json!({
                "record_hex": record.iter().map(|byte| format!("{byte:02x}")).collect::<String>(),
                "components": ultimate_control::receiver::version_components(&record),
            }))?;
        }
        "receiver-pad" => {
            arity(&args, 0)?;
            let device = ultimate_control::receiver::find()?;
            let mut receiver = ultimate_control::receiver::Receiver::open(&device)?;
            let state = receiver
                .pad(Duration::from_secs(1))?
                .ok_or_else(|| invalid("No pad report within 1 s"))?;
            json(&state)?;
        }
        "motion-packet" => {
            let slot = option(&mut args, "--slot")?
                .ok_or_else(|| invalid("--slot required"))?
                .parse()?;
            let record = protocol::decode_hex(
                &option(&mut args, "--record")?.ok_or_else(|| invalid("--record required"))?,
            )?;
            let crc = flag(&mut args, "--crc")?;
            arity(&args, 0)?;
            let packet = protocol::motion_packet(slot, &record, crc)?;
            for byte in packet {
                print!("{byte:02x}");
            }
            println!();
        }
        "windows-commit" => {
            arity(&args, 0)?;
            let inner = protocol::WindowsCommand::commit().hid32_buffer(4)?;
            for byte in protocol::windows_hid_frame(&inner) {
                print!("{byte:02x}");
            }
            println!();
        }
        "windows-motion" => {
            let feature =
                option(&mut args, "--feature")?.ok_or_else(|| invalid("--feature required"))?;
            arity(&args, 0)?;
            let bytes = protocol::decode_hex(feature.strip_prefix("0x").unwrap_or(&feature))?;
            if bytes.len() != 1 {
                return Err(invalid("Feature requires exactly one byte"));
            }
            let state = protocol::WindowsMotionState::decode(bytes[0]);
            json(
                &serde_json::json!({"state":state,"normalized_feature":format!("0x{:02x}",state.encode())}),
            )?;
        }
        "profile" => {
            let replace = flag(&mut args, "--replace")?;
            let profile = profile(&mut args)?;
            arity(&args, 1)?;
            ui::save_profile(Path::new(&args[0]), &profile, replace)?;
            json(&profile)?;
        }
        "gyro" => {
            let profile = profile(&mut args)?;
            let device = option(&mut args, "--device")?.map(PathBuf::from);
            let address = option(&mut args, "--address")?;
            let timeout = seconds(&mut args)?;
            arity(&args, 0)?;
            att::run_controller(
                device.as_deref(),
                &profile,
                None, // Read the active controller slot once at startup.
                address.as_deref(),
                timeout,
                stopped,
            )?;
        }
        "replay" => {
            let profile = profile(&mut args)?;
            arity(&args, 1)?;
            replay(&args[0], &profile)?;
        }
        "reassemble" => {
            let output = option(&mut args, "--output")?;
            arity(&args, 1)?;
            let mut transfer = protocol::ConfigRead::new(false);
            let mut buffer = String::new();
            let mut reader = BufReader::new(File::open(&args[0])?);
            while !transfer.complete() {
                buffer.clear();
                if reader.read_line(&mut buffer)? == 0 {
                    return Err(invalid(
                        "Capture ended before complete868-byte profile image",
                    ));
                }
                if buffer.len() > 65536 {
                    return Err(invalid("Capture line exceeds64KiB"));
                }
                let value: serde_json::Value = serde_json::from_str(&buffer)?;
                let Some(raw) = value.get("raw").and_then(|v| v.as_str()) else {
                    continue;
                };
                let bytes = protocol::decode_hex(raw)?;
                if bytes.len() >= 18 && bytes[2..6] == [4, 0, 2, 0] {
                    transfer.accept(&bytes)?;
                }
            }
            if let Some(output) = output {
                protocol::save_backup(Path::new(&output), transfer.data())?;
            }
            json(&protocol::inspect_config(transfer.data())?)?;
        }
        "gui" => {
            let demo = flag(&mut args, "--demo")?;
            let bench = flag(&mut args, "--bench")?;
            let debug_pointer = flag(&mut args, "--debug-pointer")?;
            let screenshot = option(&mut args, "--screenshot")?.map(PathBuf::from);
            let assets = option(&mut args, "--assets")?.map(PathBuf::from);
            let page = option(&mut args, "--page")?;
            arity(&args, 0)?;
            let options = gui::Options { bench, screenshot, page, debug_pointer, assets };
            if demo {
                gui::run(&mut gui::state::DemoState::new(), options)?;
            } else {
                gui::run(&mut ultimate_control::live::LiveState::new(), options)?;
            }
        }
        "ui" => {
            let port = option(&mut args, "--port")?
                .unwrap_or_else(|| "0".into())
                .parse()?;
            let path = option(&mut args, "--profile")?
                .map(PathBuf::from)
                .unwrap_or_else(ui::default_profile_path);
            arity(&args, 0)?;
            ui::serve(port, path)?;
        }
        _ => return Err(invalid(format!("Unknown command {command:?}; use --help"))),
    }
    Ok(())
}
fn main() {
    #[cfg(target_os = "linux")]
    unsafe {
        libc::signal(libc::SIGINT, interrupt as *const () as libc::sighandler_t);
        libc::signal(libc::SIGTERM, interrupt as *const () as libc::sighandler_t);
    }
    #[cfg(windows)]
    // SAFETY: static callback only stores an atomic flag; workers perform cancellation/cleanup.
    if let Err(error) = unsafe { windows::Win32::System::Console::SetConsoleCtrlHandler(Some(console_interrupt), true) } {
        eprintln!("Error installing console interrupt handler: {error}");
        std::process::exit(1);
    }
    if let Err(error) = dispatch() {
        eprintln!("Error: {error}");
        std::process::exit(if ultimate_control::interrupted() {
            130
        } else {
            1
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_mapping_overrides_loaded_profile_but_duplicate_flags_fail() {
        let path = env::temp_dir().join(format!(
            "ultimate-profile-precedence-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let _cleanup = Cleanup(path.clone());
        let mut loaded = HostProfile::default();
        loaded.mappings.insert("BTN_SOUTH".into(), "KEY_A".into());
        ui::save_profile(&path, &loaded, false).unwrap();
        let mut options = vec![
            "--profile".into(),
            path.to_string_lossy().into_owned(),
            "--map".into(),
            "BTN_SOUTH=KEY_B".into(),
        ];
        let changed = profile(&mut options).unwrap();
        assert_eq!(changed.mappings["BTN_SOUTH"], "KEY_B");
        assert_eq!(
            ui::load_profile(&path).unwrap().mappings["BTN_SOUTH"],
            "KEY_A"
        );
        let mut duplicate = vec![
            "--map".into(),
            "BTN_SOUTH=KEY_A".into(),
            "--map".into(),
            "BTN_SOUTH=KEY_B".into(),
        ];
        assert!(profile(&mut duplicate).is_err());
    }
}
