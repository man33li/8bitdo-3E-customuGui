# Ultimate 3E cable personalities and Linux binding

Scope: Windows Ultimate Software X 1.4.9.0; Linux 6.18 / installed 6.18.55; xone v0.5.7. **O** = inspected static code, source or cached host state; **L** = kernel log or parent's live observation; **I** = inference/proposed experiment; **U** = unresolved. Investigation performed no device sends, driver changes, authorization changes or controller writes. Commands below are **parent-only proposals**, not exercised results.

## Conclusion

**I:** First isolate xone's probe/reset from firmware host-mode selection, then try **stock `xpad`**, not `xpad_noone`. Stock Linux v6.18 `xpad` matches `2dc8:202f` through its vendor/interface wildcard, initializes GIP without an explicit `usb_reset_device`, and is present on this host. Installed `xpad_noone` deliberately removes Xbox One support; adding `202f` through `new_id` cannot restore its missing GIP parser/init path.

**L:** A reset is correlated with fallback, not sufficient to explain it: the successful 21:06 session also reset under xone and remained GIP for about 28 minutes. No host setting is yet proven to **keep** GIP. The minimal reversible diagnostic is prevent xone loading **before replug**, leave stock xpad unloaded for one bounded no-driver observation, then repeat with stock xpad. No alternate HID channel, report ID or mode command for cable-3024 was established in the Windows app.

## Evidence and identity

Original `reconstruction/original/UWPAdvance.dll` SHA-256 `1f2325902b5d3ca2cc7b44405243247d3f276c39f99a66c97b193545be038352`. IR address/evidence aliases:

| Alias | Evidence ID |
|---|---|
| E0 | `ev_16d3a5bb323bdf1261c88a7959f69521325a1a32ba6a150def23e1df0ba63561` |
| EH | `ev_d2692710de786f469fdb89e7ca96057ca2a9c1e36c209d759ab7df48f991566f` |
| EQ | `ev_d8af8df7e5b2e999018e706966432d5dbad6401f998b73125b730ac5ae5525ec` |
| ED | `ev_a2e4840518558b197fcceb1990abcf9c6887bb5d5fb99228473bd2eea8351591` |
| EN | `ev_e2c1bb70af3bd49debb62270da6c9c2c12e6002b9999ce601c7ccbdd2919d71c` |
| EC | `ev_141ebcaeab4cf5f83a08a8f72aa99685cbce008b31880266775e209b859b5881` |
| EU | `ev_93ebe7869f5029053f0be92976d2129388189d0d2bf8ab59ca53c87ae6d53137` |

### Kernel history: topology matters

Source: local `journalctl -k`, 2026-10-15, timestamps `+04:00`. Reproduce narrowly without USB transfers:

```sh
journalctl -k --since '2026-10-15 21:05:00' --until '2026-10-15 22:10:00' \
  --no-pager -o short-iso-precise \
  --grep='idVendor=2dc8|idVendor=045e|idVendor=1a40|SerialNumber: (60CC|0000)|reset .*USB device|gip[0-9]|xone|USB disconnect, device number'
```

| Time | Observed sequence |
|---|---|
| 21:05:42.059844 | Dock/receiver `3-2.2`, `3024`, bcdDevice `1.05`, serial `000023D000000010`. Parent hub `3-2` is `1a40:0101`, two ports. |
| 21:06:59.454292 | `3-2.2`, `202f`, bcdDevice `1.01`, serial `60CC94CAB4700000`. Reset at `59.683298`; xone registered `59.892295`; `gip0.0` gamepad at 21:07:00.666295. Remained until hub/device disconnect at 21:35:31.162. |
| 21:35:38.762291 | Direct `3-2`, `202f`; reset `38.892337`; disconnect `39.064292`; `3024` at `40.220297`, serial `000074E400000010`. |
| 21:49:20.753300 | Hub child **`3-2.1`** is `045e:0b12`, Product `8BitDo Ultimate 3E Xbox`; reset `20.948300`; disconnect `21.173308`. Same child reappears as `2dc8:2055` at `21.874316`, bcdDevice `1.00`, Mfr=0, SerialNumber=0; hidraw6 at `21.898293`; disconnect `23.460300`. |
| 21:49:24.693293 | Different hub child **`3-2.2`**, `202f`, serial `60CC94CAB4700000`; reset `24.892345`; entire hub disappears `25.145341`. Returning `3-2.2` at `26.752290` is `3024`, serial **`000023D000000010`**, the dock identity, not cable serial. |
| 22:00:01.049303 | Direct `3-2`, `202f`; reset `01.175302`; disconnect `01.348304`; cable-3024 `02.493301`, serial `000074E400000010`. |
| 22:05:26.774299 | Direct `3-2`, `202f`; reset `26.900390`; disconnect `27.070295`; cable-3024 `28.218290`, same cable serial. |

**L:** `2055` also appears on `3-2.1` during 21:36, 21:37, 21:55 and 22:09 hub sequences; `202f`/dock3024 are on `3-2.2`. **I:** This is evidence of separate hub children and a `045e:0b12 → 2055` personality change on child1, **not evidence that `2055 → 202f` is one physical USB-device transition**. Product strings alone cannot identify physical components. Which child belongs to which physical subsystem remains **U**; preserve USB topology and serials on every probe.

**L:** Failing direct connections disconnect ~170–175 ms after reset log, then rediscover cable3024 ~1.15 s after disconnect. The delay is not simply “one second until GIP initialization times out.” **U:** Reset completion, first GIP packet, successful audio setup, authentication failure, and firmware's actual mode-switch cause are not available in these ordinary kernel logs.

Parent observations from `replacement-plan.md`, CABLE MODE FALLBACK: cable3024 vendor FF00 input works, A changes byte3 bit0x10; report-disable opcode7/ch4, version E1/ch5 and profile-read requests received **no reply**. Dock3024 accepts the verified disable/read/enable and write/commit transactions. No repeat sends performed here.

## Windows dispatch: no cable-3024 branch found

### Model table is logical PID, not USB serial matching

**O, original PE static DWORDs**, read at VA `0x1805c19a8` (SHA above; static-byte inspection, not a new REA Evidence record):

```text
base+00 2000   +04 2004   +08 2012   +0c 2002
    +10 2023   +14 201c   +18 200a   +1c 2008
    +20 201e   +24 200f   +28 2030   +2c 2031
    +30 202c   +34 2018   +38 2038   +3c 202f
    +40 2072   +44 0000   +48 0000 (mutable current logical PID)
```

`0x18071fd20` [E0] initializes current +48 from +44 (zero). Neither `3024` nor `2055` is a model-table entry. `3024` is a transport USB PID which the app maps to logical3E `202f`; the table does not distinguish dock/cable using serial numbers.

**O:** `AppxManifest.xml:28–40` grants HID access only to `2DC8 3024` usage `FF00 0001`, and `2DC8 2051` / `2052` usage `008C 0001`. No HID grant for `2055` in this package. Bounded literal searches of recovered IR and `strings.tsv` found no exact `0x2055` constant, USB `SerialNumber` accessor, known dock/cable serial string, or cable-specific selector. “read_Serial” at `0x180cb0950` [EC] launches protocol factory `0x180caa0e0`; that is not proof of USB iSerial-based discovery. Absence of a literal is not proof against an unresolved indirect path; manifest and concrete selector provide the stronger boundary.

| Stage | Static observation |
|---|---|
| Find3024 | `0x180cb1580` [ED] calls `GetDeviceSelector(FF00,1,2dc8,3024)` through `0x1808f1870`, enumerates IDs, dispatches `0x180c4df50`. No serial/topology/cable predicate in this path. Watcher `0x180cb21c0` [EN] uses exactly the same selector. |
| Open3024 | `0x180c4e070` [EQ] opens read/write through `0x1808f1820(id,1)`, stores handle `DAT_1805b7828`, sets `isHID=1`, assigns logical PID model+3c (`202f`), attaches input callback and publishes `Notification_findBootDevice`. First successful opened object wins; there is no established preference between two simultaneously present3024 devices. |
| Init | `0x180d99b60` [EU] clears read-state then invokes `0x180cae1c0(0)` (report-disable). Open itself has no boot-entry/reboot/mode-select send. The notification's “Boot” name does not establish bootloader mode. |
| Normal3024 config | `0x180c4bf40 → 0x180c4c260 → 0x180c4b440 → 0x180c4b550` [EH/EQ]: report `81`, 31 data bytes; inner `[length,channel,header16,payload<=13]`. Channel4 for normal profile/report commands, channel5 for E1 version. See `spec-transport.md`. |
| GIP202f config | `!isHID` continuation `0x180c4ad60` [EH] uses `0x180c4aef0/0x180c4b240` [EH], payload60 beginning with channel, D4X `SendMessageId(...,13)`; receive message12. See `spec-gip.md`. |
| Other HID/update framing | `isHID` branch of `0x180c4aef0` uses65, but normal3E-HID continuation explicitly selects receiver serializer `0x180c4bf40` when current==model+3c. Other HID model+38 uses `0x180c4c330`. Neither is a demonstrated cable3024 alternative. |

**Answer:** Windows has a cabled **GIP202f** configuration path and a **generic3024 HID** path. A cable3024 exposing FF00/1 satisfies that HID selector; app code attempts ordinary receiver framing without a serial distinction. **U:** No Windows capture proves successful configuration of the observed cable3024 personality. Static code does **not** justify saying “Windows supports cable3024 configuration” or “needs channelX/mode commandY.” No such command found along selector → open → report-disable → normal3E queue. Do not spray alternate reports/channels or run firmware/update routines to compensate for silence.

## Linux drivers and why suggested shortcuts fail

Pinned sources: [Linux v6.18 xpad.c](https://github.com/torvalds/linux/blob/v6.18/drivers/input/joystick/xpad.c), [xone v0.5.7 wired.c](https://github.com/dlundqvist/xone/blob/a16304ccdecc5bfc7aafdeb759c706da314d15fa/transport/wired.c). Installed xone provenance is established in `spec-gip.md`; local `modinfo xone_wired` confirms srcversion `EF4F5EB825C19A9A9B15C47`, version `#VERSION#` (template, not a semantic version).

| Driver | Matching and initialization |
|---|---|
| xone-wired v0.5.7 | [L22–29](https://github.com/dlundqvist/xone/blob/a16304ccdecc5bfc7aafdeb759c706da314d15fa/transport/wired.c#L22-L29), [L546–570](https://github.com/dlundqvist/xone/blob/a16304ccdecc5bfc7aafdeb759c706da314d15fa/transport/wired.c#L546-L570): vendor2dc8, interface0 ff/47/d0 wildcard; not a special202f quirk. [L473–516](https://github.com/dlundqvist/xone/blob/a16304ccdecc5bfc7aafdeb759c706da314d15fa/transport/wired.c#L473-L516): unconditionally calls `usb_reset_device` at probe, return not checked, then data/audio initialization and receive URB. [L434–470](https://github.com/dlundqvist/xone/blob/a16304ccdecc5bfc7aafdeb759c706da314d15fa/transport/wired.c#L434-L470): claims audio1, sets alternate0. |
| Stock xpad v6.18 | `202f` absent from friendly-name table [L403–409](https://github.com/torvalds/linux/blob/v6.18/drivers/input/joystick/xpad.c#L403-L409), **but supported generically** by vendor2dc8 + ff/47/d0 [L510–518,L577–578](https://github.com/torvalds/linux/blob/v6.18/drivers/input/joystick/xpad.c#L510-L578). Unknown PID defaults to XboxOne for protocol208, rejects nonzero data interface [L2064–2146](https://github.com/torvalds/linux/blob/v6.18/drivers/input/joystick/xpad.c#L2064-L2146). [L1498–1527](https://github.com/torvalds/linux/blob/v6.18/drivers/input/joystick/xpad.c#L1498-L1527) sets audio alt0, queues initialization; no explicit `usb_reset_device` anywhere in this source. Generic packets include power-on, LED-on and auth-done [L734–752](https://github.com/torvalds/linux/blob/v6.18/drivers/input/joystick/xpad.c#L734-L752), filtered/sequence-numbered at [L1274–1310](https://github.com/torvalds/linux/blob/v6.18/drivers/input/joystick/xpad.c#L1274-L1310). Input open starts receive+XboxOne initialization [L1796–1809](https://github.com/torvalds/linux/blob/v6.18/drivers/input/joystick/xpad.c#L1796-L1809). Not a full xone authentication implementation or a generic vendor-config userspace endpoint. |
| Installed xpad_noone | `modinfo`: `/nix/store/k16njrzl36izln3km1sab91ipaf6aajx-xpad-noone-0-unstable-2024-01-10/.../xpad-noone.ko`; only ff/5d/01 and ff/5d/81 aliases for2dc8, **no ff/47/d0**. Source derivation `vnjmp1vspaj21ic4l5vm85hin5sv8il8-source.drv` pins [medusalix/xpad-noone tag c3d1610](https://github.com/medusalix/xpad-noone/tree/c3d1610), NAR `sha256-jDRyvbU9GsnM1ARTuwnoD7ZXlfBxne13UpSKRo7HHSY=`. [xpad.c L91–95](https://github.com/medusalix/xpad-noone/blob/c3d1610/xpad.c#L91-L95) has no XboxOne type; [L380–429](https://github.com/medusalix/xpad-noone/blob/c3d1610/xpad.c#L380-L429) has360-only matching; [L1480–1487](https://github.com/medusalix/xpad-noone/blob/c3d1610/xpad.c#L1480-L1487) maps unknown vendor-specific protocol208 to **Xbox360**, not GIP. `new_id` would bind the wrong parser. |

**O, host:** `/etc/modprobe.d/nixos.conf` blacklists `xpad`; `modinfo -n xpad` nevertheless finds installed stock module at `/run/booted-system/kernel-modules/lib/modules/6.18.55/kernel/drivers/input/joystick/xpad.ko.xz`. Its aliases include `usb:v2DC8p*d*dc*dsc*dp*icFFisc47ipD0in*`. `blacklist` stops alias-driven autoload, not an explicit `modprobe xpad`. Both stock and noone register USB driver name **xpad**; unload noone before loading stock, with affected controllers unplugged.

Host-control distinctions:

- **No per-PID modprobe blacklist.** `blacklist xone_wired` suppresses alias autoload of the entire module; it does not evict loaded code or prevent explicit loading. Temporary `install xone_wired /bin/false` also rejects ordinary explicit modprobe requests. Revert both by removing only the temporary file. Kernel direct module loading can bypass modprobe policy; check module/binding after replug.
- **`new_id` adds matches, not exclusions.** Linux [driver.c L40–112](https://github.com/torvalds/linux/blob/v6.18/drivers/usb/core/driver.c#L40-L112) installs a dynamic match and probes immediately. `remove_id` [L148–177](https://github.com/torvalds/linux/blob/v6.18/drivers/usb/core/driver.c#L148-L177) removes only dynamic entries; cannot subtract xone's compiled vendor wildcard. It neither prevents xone nor supplies absent noone GIP code. No `new_id` needed for stock xpad.
- **Unbind-on-add udev rule is too late to prevent reset.** xone probe occurs during kernel interface enumeration, before userspace reacts to interface add. An `unbind` rule can detach an already-probed interface, not undo its reset. No persistent race-based rule recommended.
- **Device `ATTR{authorized}=0` is not “disable xone only.”** It deconfigures/blocks the whole device; GIP cannot work. Reauthorization with xone present leads to probe/reset again. Interface authorization can gate all drivers before binding, but must default-deny on the bus before arrival and requires explicit authorization/reprobe. That affects unrelated devices/hub children; not minimal here. See [USB authorization](https://github.com/torvalds/linux/blob/v6.18/Documentation/usb/authorization.rst) and authorization checks [driver.c L338–350](https://github.com/torvalds/linux/blob/v6.18/drivers/usb/core/driver.c#L338-L350). USB v6.18 matching does not offer the generic PCI-style per-interface `driver_override` recipe; do not prescribe a nonexistent sysfs file.

## Ranked hypotheses

1. **I, strongest:** Firmware host-mode detection reacts to reset/early GIP initialization timing; xone's unconditional probe reset is an avoidable trigger candidate. Repeated ~170ms reset→disconnect supports it. Counterevidence: same reset succeeded behind hub at21:06. Distinguish no-driver vs stock-xpad vs xone at same port and controller startup state; only a controlled no-reset-xone build would separate reset from xone's later handshake without changing driver families.
2. **I:** Startup state/topology/power timing changes detection. Successful session followed boot/hub arrival; direct later retries failed, but hub retries also failed and sometimes lost the whole hub. A hub is not a proven fix. Preserve dock unplugged/plugged state, physical switch, cold/warm controller state, cable, port and elapsed times; change one at a time.
3. **I:** Cable3024 intentionally provides XInput/HID mirroring but not the receiver's configuration bridge, despite descriptor/PID reuse. Working pad reports plus total vendor-reply silence supports separate command routing, not bad input decoding. Windows has no observed alternate cable3024 path; descriptor equality would not prove firmware equivalence.
4. **I, weaker:** Cable3024 differs in endpoint/report descriptor, selected HID collection, report length or transport firmware. Passive descriptor comparison can resolve descriptor/node mismatch without new sends. Existing same-report input is not sufficient evidence that output/report handling matches dock.
5. **I, weakest from current evidence:** An undiscovered initialization or mode-select command is required. No such branch found in the relevant Windows chain; firmware internals/Windows runtime remain unknown. Do not guess command bytes or identify2055 as a config route merely from its Product string.

## Ordered parent probes and exact reversals

All host mutations below affect every wired xone controller while active. Close mapper/rumble/GUI consumers; unplug cable **and receiver/dock** before unloading drivers. Keep keyboard/mouse on unaffected ports. No force-unload, USB reset, `set_configuration`, firmware tools, profile writes, or commit. Replug/driver initialization can itself transmit USB/GIP traffic; these are not passive even though no persistent profile is written.

### 1. Preserve passive baseline

Run source/history commands above, then:

```sh
modinfo -n xpad
modinfo xone_wired
modinfo xpad_noone
cat /proc/modules
cat /etc/modprobe.d/nixos.conf
```

Changes: none. Revert: none. Use descriptor snapshot below once with dock3024, once with cable3024; never select by cached hidraw number.

### 2. Suppress xone before reconnect; no-driver trial first

With all wired controllers/dock unplugged, run in one shell:

```sh
sudo mkdir -p /run/modprobe.d
sudo sh -c 'test ! -e /run/modprobe.d/90-3e-cable-probe.conf && printf "%s\n" "blacklist xone_wired" "install xone_wired /bin/false" > /run/modprobe.d/90-3e-cable-probe.conf'
sudo modprobe -r xone_wired
```

Stop if exclusive file creation precondition fails or unload fails; do not overwrite existing policy or force dependencies. Leave `xpad_noone` loaded; it does not match202f. Ensure stock `xpad` was not already loaded (`/proc/modules`); if it was, unplug its controllers and unload it before this **no-driver** trial. Reconnect only cable, wait ten seconds, inspect cached sysfs and:

```sh
journalctl -k --since '-1 minute' --no-pager -o short-iso-precise \
  --grep='usb 3-|idVendor=2dc8|xone|xpad|reset|disconnect|gip'
```

Changes: temporary modprobe policy, removal of wired transport driver, physical reconnect. No automatic GIP initialization if truly unbound. Retained202f proves only stable enumeration, **not usable Xbox input/config**. Fallback with no driver substantially weakens xone-reset hypothesis. Rollback: unplug cable, remove temporary file and load xone (step5). `/run` policy also disappears on reboot, but do not rely on reboot as experiment cleanup.

### 3. Stock-xpad no-explicit-reset trial

Unplug cable and receiver first; then:

```sh
sudo modprobe -r xpad_noone
sudo modprobe xpad
```

The existing blacklist does not block explicit loading. Reconnect cable, wait ten seconds, repeat passive identity/driver snapshot and recent kernel log. If202f persists, open its **correct USB-ancestry-matched** evdev node to start input/init. Resolve event nodes without opening them:

```sh
EVENT="$(python3 - <<'PY'
from pathlib import Path
nodes = []
for e in Path('/sys/class/input').glob('event*'):
    ancestry = list(e.resolve().parents)
    if any((p/'idVendor').is_file() and (p/'idProduct').is_file()
           and (p/'idVendor').read_text().strip() == '2dc8'
           and (p/'idProduct').read_text().strip() == '202f' for p in ancestry):
        nodes.append('/dev/input/'+e.name)
if len(nodes) != 1:
    raise SystemExit('Expected exactly one 202f evdev node; stop and inspect sysfs')
print(nodes[0])
PY
)" && test -n "$EVENT" && timeout 10s evtest "$EVENT"
```

If read permission is denied, repeat only the final observation as `sudo timeout 10s evtest "$EVENT"` after checking the selected node still belongs to202f. **Opening evdev is an active driver-init probe**, not merely cached-state reading: xpad open submits initialization packets. Move sticks/press ordinary buttons; do not test rumble or grab. Record whether fallback occurs before or only after open. `timeout` status124 simply ends bounded observation. Changes: swaps noone→stock driver, stock init sends, input observation. Revert: unplug cable, `sudo modprobe -r xpad`, `sudo modprobe xpad_noone`; retain xone suppression until planned rollback. If stock module fails registration/loading, stop; do not force-bind noone or add dynamic IDs.

### 4. Separate port/startup effects, then vendor configuration

Only if earlier results leave ambiguity: with xone still suppressed and same chosen driver, repeat step2/3 once on the previously successful hub path and once direct; preserve switch/power state and dock absence. Commands unchanged; physical connection is the only deliberate variable. Revert physical cabling to baseline. Do not infer physical2055 identity from temporal ordering.

If202f+stock-xpad is stable and input works, later parent may exercise existing GIP version/read path **only after exclusive ownership**, following `spec-gip.md` framing and report restoration rules. Do not reconnect xone as automatic handback during this no-xone experiment: that reintroduces its reset. No extra mode/firmware request belongs in this probe list. Vendor-config success remains separate acceptance from stable Xbox enumeration/input.

### 5. Explicit rollback

Unplug cable and dock before module changes. If stock xpad was loaded only by step3:

```sh
sudo modprobe -r xpad
sudo modprobe xpad_noone
sudo rm /run/modprobe.d/90-3e-cable-probe.conf
sudo modprobe xone_wired
```

If step3 was skipped, omit the stock/noone swap. Replug baseline hardware. Recheck cached driver symlinks and journal; restoring xone can reproduce fallback, which is not rollback failure in host policy. Do not restore a module that was absent at baseline or remove a preexisting one without recording that baseline. No `/etc`, NixOS generation, udev rule or controller setting was changed by these transient probes.

**Conditional durable policy, not installed here:** If stock-xpad trial proves the desired behavior, replace `xpad_noone` with stock `xpad` in NixOS module/package policy, remove conflicting `blacklist xpad`, and blacklist `xone_wired` (all wired xone devices affected). For a conventional host, an administrator-owned `/etc/modprobe.d/90-3e-cable.conf` containing the two lines from step2 prevents normal xone loading, but does not replace a currently loaded module; reconnect after unloading. No proven per202f module-only exclusion exists in inspected xone. Keep configuration guidance here rather than advertising an unverified README fix.

## Passive descriptor comparison: cable3024 vs dock3024

Run once per physical setup, while its3024 personality is present. Script reads **cached sysfs only**: no usbfs/libusb/`lsusb -v`, HID GET_REPORT, HID GET_DESCRIPTOR, open hidraw, or report-disable. It saves all2dc8 children, serial/topology, raw USB descriptors, HID report descriptors and interface/endpoint attributes; this also records2055 if still present. Output directory must be new. Exact command (change only `dock` to `cable` for second run):

```sh
python3 - dock <<'PY'
from pathlib import Path
import json, sys
out = Path('/tmp/3e-'+sys.argv[1]+'-descriptors')
out.mkdir(exist_ok=False)
rows = []
for d in Path('/sys/bus/usb/devices').iterdir():
    if not (d/'idVendor').is_file() or (d/'idVendor').read_text().strip() != '2dc8':
        continue
    row = {'path': str(d.resolve()), 'usb': d.name, 'attributes': {}, 'interfaces': []}
    target = out/d.name
    target.mkdir()
    for key in ('idVendor','idProduct','serial','manufacturer','product','bcdDevice',
                'busnum','devnum','speed','bNumConfigurations','bConfigurationValue'):
        p = d/key
        if p.is_file(): row['attributes'][key] = p.read_text().strip()
    (target/'usb-descriptors.bin').write_bytes((d/'descriptors').read_bytes())
    for i in sorted(d.glob(d.name+':*')):
        item = {'interface': i.name, 'driver': None, 'attributes': {}, 'endpoints': {}}
        if (i/'driver').is_symlink(): item['driver'] = str((i/'driver').resolve())
        for key in ('bInterfaceNumber','bAlternateSetting','bInterfaceClass',
                    'bInterfaceSubClass','bInterfaceProtocol','bNumEndpoints','modalias'):
            p = i/key
            if p.is_file(): item['attributes'][key] = p.read_text().strip()
        for ep in i.glob('ep_*'):
            item['endpoints'][ep.name] = {p.name: p.read_text().strip() for p in ep.iterdir()
                if p.name in ('bEndpointAddress','bmAttributes','wMaxPacketSize','bInterval',
                              'direction','type','interval') and p.is_file()}
        for rd in i.rglob('report_descriptor'):
            name = i.name+'-'+rd.parent.name+'-report-descriptor.bin'
            (target/name).write_bytes(rd.read_bytes())
        row['interfaces'].append(item)
    rows.append(row)
(out/'identity.json').write_text(json.dumps(rows, indent=2)+'\n')
print(out)
PY
```

Changes: new `/tmp/3e-{dock,cable}-descriptors` host files only. Revert: delete those two **new** directories after retaining evidence. If transient node disappears during read, retain partial capture and rerun with a new label/directory; do not treat missing report descriptor as an empty descriptor. Sysfs report descriptor requires bound HID; no report descriptor is expected for a GIP-only interface.

Compare exact cached binary files with `sha256sum`/`cmp` and inspect `identity.json`. Match corresponding **interface number + usage/report descriptor**, not hidraw index or USB device number. In particular compare: device/class/bcdDevice, configuration/interface counts and alternate settings; ff/5d vs ff/47 vs HID/audio interfaces; FF00/1 collection/report81 input/output bit counts; report IDs, endpoint direction/type/max packet/interval; mouse interface separately; serial and physical ancestry. Use `cmp -l FILE_DOCK FILE_CABLE` with actual recorded filenames for byte offsets; no guessed sysfs/hidraw path.

**U:** Equal descriptors establish equal advertised interfaces, not equal command handlers. Different descriptors may identify a framing/node error, but do not authorize inventing new bytes. The two3024 serials are observed unit identities, not a documented universal serial prefix classification.

## Remaining unknowns / completion boundary

- Root cause of firmware's202f→3024 decision; necessity/sufficiency of reset vs later handshake, startup mode, cable/power or hub behavior. Successful21:06 reset must remain in causal analysis.
- Whether unbound202f persists, stock-xpad initializes this firmware successfully, and either avoids fallback. Proposed host policy is reversible and evidence-driven, **not a verified fix**.
- Physical role of hub child2055 and its045e:0b12 predecessor; app1.4.9.0 has no demonstrated2055 HID config path. Need passive topology/descriptors, not a mode-switch assumption.
- Cable3024 vs dock3024 cached descriptor identity and Windows runtime success. App selector makes no serial distinction; observed cable3024 protocol silence remains ground truth.
- GIP configuration via exclusive takeover after stable enumeration, exact Windows flags and warm/cold initialization remain governed by `spec-gip.md`; none resolved by changing kernel input driver alone.

Investigation-only deliverable: no Rust/README/host configuration edits and no build/test run needed for this specification. Parent owns every future live probe.
