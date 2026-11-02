# Ultimate Control

Native replacement for **8BitDo Ultimate Software X** (Windows 1.4.9.0), written in Rust with a
[Clay](https://github.com/nicbarker/clay) + raylib GUI. It edits the settings stored in an
**8BitDo Ultimate 3E** controller and adds host-side, Steam-Input-style remapping.
Linux supports the existing host mapper. Windows 11 x64 has native HID/Bluetooth configuration
and a ViGEmBus/HidHide right-stick mapper; see [Windows quick start](#windows-quick-start).

The protocol was recovered from the original Windows binaries (see `reconstruction/`) and every
device feature listed as *working* below was verified on real hardware.

## Windows quick start

Download `ultimate-control.exe` from the
[releases page](https://github.com/man33li/8bitdo-3E-customuGui/releases) (currently
[v0.1.0-alpha.2](https://github.com/man33li/8bitdo-3E-customuGui/releases/tag/v0.1.0-alpha.2)).
It is a single file; verify it against the `.sha256` file published with the release.

This release adds an **R4 gyro clutch over USB cable or the 2.4 GHz receiver**, working on
firmware 1.38: hold **R4** (or L4/PL/PR) to pause gyro aiming while you reposition the
controller, and release it to resume. While held, the physical right stick and R3 still work.
The clutch reads the physical paddle and right stick over the controller's
**configuration Bluetooth** link, while gameplay stays on cable/receiver. Games see a
**virtual Xbox 360 controller** (ViGEmBus); the real controller is hidden from them with
**HidHide** so they don't see double input.

### Requirements

- Windows 11 x64.
- **A working Bluetooth adapter on the PC**, even when you play over cable or the 2.4 GHz receiver.
- 8BitDo Ultimate 3E in **PC mode** (`2dc8:3024`) over USB cable or the 2.4 GHz receiver.
  Xbox mode (`2dc8:202f`) has no gyro source.
- [ViGEmBus v1.22.0](https://github.com/nefarius/ViGEmBus/releases/tag/v1.22.0) (final version)
  and [HidHide](https://github.com/nefarius/HidHide/releases).

The app never installs drivers, never asks for administrator rights and never disables
Windows security features. Reboot if a driver installer asks for it.

### Setup

**1. Install the drivers (once)**

1. Install ViGEmBus v1.22.0 (retired project; this final setup also removes its old updater).
2. Install HidHide, then **close the HidHide configuration client**. While it is open, the mapper cannot start.
3. Close 8BitDo Ultimate Software and the 8BitDo mobile app; they compete for the configuration link.

**2. Connect the controller**

1. Plug in the 2.4 GHz receiver (or the USB cable) with the controller in PC mode.
2. Start `ultimate-control.exe gui`. Home should say **Gamepad detected**.
3. Hold the **front pixel-logo/profile button for about 3 seconds** until the configuration
   LEDs blink. Leave the cable/receiver connected.
4. Click **Home → Connect Bluetooth**. Home shows **Settings connected** and the active slot.

**3. Configure the controller slot (once)**

1. Back up first: close the GUI, hold the profile button until the LEDs blink, run
   `ultimate-control.exe backup ultimate-backup.bin` in PowerShell, then restart the GUI and
   reconnect (steps 2.2–2.4).
2. In the active slot, open **Motion**: target **Right stick**, enable method **Press-toggle**,
   enable button **PL** (anything except the clutch button).
3. On **Mapping → Back**, set **R4 to None**. Keep right-stick inversion and stick swap off.
4. Click **Save + verify**, power-cycle the controller and select that slot again (short press
   of the profile button). Repeat steps 2.3–2.4 to reconnect settings.

**4. Configure the host profile (once)**

1. Open **Remap**, enable **USB / 2.4 GHz native-stick clutch (Bluetooth-assisted)** and choose **R4**.
2. Click **Save host profile**.

**5. Play**

1. Turn on **Home → Host input mapper**. The physical pad is hidden and a virtual Xbox 360
   controller appears.
2. Press **PL once** to turn firmware gyro on (its state can't be read, so the app does not
   toggle it for you).
3. Hold **R4** to pause gyro aiming; release to resume.
4. Keep the app open while playing. Turn the mapper off before changing slots or saving
   controller settings.

### Host remapping (keyboard, mouse, turbo, layers)

The host mapper also works on Windows without the gyro clutch: over USB cable or the 2.4 GHz
receiver (PC mode), and over Bluetooth gaming mode. It needs the same ViGEmBus and HidHide
drivers. Bluetooth settings are optional for plain USB remapping.

1. Open **Remap**. Leave the native-stick clutch off unless you also want the gyro clutch.
2. Choose a source button and type a destination: an Xbox button (`BTN_SOUTH` = A,
   `BTN_EAST` = B, `BTN_NORTH` = X, `BTN_WEST` = Y), a keyboard key (`KEY_SPACE`, `KEY_W`,
   `KEY_LEFTSHIFT`, `KEY_F1`…) or a mouse button (`BTN_LEFT`, `BTN_RIGHT`, `BTN_MIDDLE`,
   `BTN_SIDE`, `BTN_EXTRA`).
3. Optionally enable **Turbo**, **Toggle / latch**, **Stick to mouse**, **Trigger to button**
   or a **layer**.
4. Click **Save host profile**, then turn on **Home → Host input mapper**.
5. Optional: on **Host profiles**, add rules such as `game.exe` → profile and enable
   **Auto-switch**. Windows matches executable names (`game` or `game.exe`) of programs in
   your session; command-line rules are Linux-only.

Over USB/2.4 GHz on Windows, the controller slot's own button mapping stays active and host
mappings apply on top of it. L4/R4/PL/PR reach the host only as the buttons the slot maps
them to (map R4 → R3 in the slot, then remap R3 on the host). Keys and mouse buttons held
when the mapper stops are released. Games running as administrator only receive
keyboard/mouse output if Ultimate Control also runs as administrator (Windows UIPI).

### Limits

- While R4 is held, the right stick comes from Bluetooth: 8-bit resolution and a lower update
  rate (~31 ms). Firmware stick curves don't apply during that time.
- Tune gyro sensitivity on the controller's **Motion** page; the app's raw-gyro filters don't
  apply to this mode.
- On Windows, gamepad output is a virtual Xbox 360 controller plus keyboard/mouse; no
  independent extra-paddle outputs, rumble, firmware-mouse capture or Steam Input mode.
- HidHide cannot block every Raw Input reader. If a game sees two controllers, check HidHide
  and the game's input settings.
- **Crash recovery:** HidHide changes are journaled in
  `%APPDATA%\ultimate-control\hidhide-journal.json` and undone on the next start. If HidHide
  settings were edited meanwhile, the app refuses to overwrite them; unhide the controller in
  HidHide manually.

### Troubleshooting

- **Connect Bluetooth fails:** hold the profile button 3 seconds again until the LEDs blink,
  close other 8BitDo apps, then retry within 20 seconds.
- **Mapper won't start:** close the HidHide client; confirm ViGEmBus and HidHide are installed;
  read the error on Home.
- **Gyro does nothing:** check PC mode, the selected slot, Motion target Right stick, and press PL once.
- **R4 also presses a button in game:** R4 is still mapped in the controller slot; set it to
  None and save.

Technical details on the Windows backend, driver handling and building the `.exe` yourself are
in [build-windows](#build-windows).

## Status

**Firmware 1.38:** receiver configuration replies and heartbeat paddle reports can be
unavailable even while ordinary input works. The new **Bluetooth-assisted native-stick
clutch** avoids that channel. It is verified on Linux over both the 2.4 GHz receiver and
PC-mode USB cable, and was verified on real Windows with ViGEmBus and HidHide. Older table entries
describe their recorded Linux verification, not a promise that every firmware supports them.

| Feature | 2.4 GHz receiver | Wired USB | Bluetooth |
|---|---|---|---|
| Read controller profiles (868-byte image, 3 slots) | ✅ | ✅ verified over GIP (receiver in Xbox mode, `2dc8:202f`); input pauses ~1 s | ✅ |
| Edit + save profiles (mapping, sticks, triggers, vibration levels, lighting, motion) | ✅ persists across power cycle | ✅ verified (CLI edit persisted across power cycle; GUI save read back). Motion page hidden in Xbox mode, as in Windows | ✅ verified (edit written over Bluetooth, read back over Bluetooth and receiver) |
| PC (XInput) mode / Tournament flag and Auto Reconnect | ✅ GUI Home → Switch receiver to Xbox mode (confirmation required; persistent; verified). Header flags stay read-only | ✅ GUI Home → Switch receiver to PC (XInput) mode (verified); intended for the Xbox-mode receiver | Flags display only; no mode switch |
| Live input test page | ✅ including L4/R4/PL/PR (via the Windows heartbeat; verified); Motion shows firmware gyro-as-mouse when Motion target is Mouse | ✅ verified (xone evdev, read without grabbing; buttons, D-pad, sticks, triggers; resumes after Re-read); no motion data | ✅ verified, including L4/R4/PL/PR; Motion shows raw gyro + accelerometer (verified 2026-10-25, firmware 1.38) |
| Host remapping / turbo / toggle / stick-to-mouse / trigger-to-button | ✅ standard controls verified (A→B, turbo 10.0 Hz, toggle, RT→Space, right stick→mouse); paddles as remap sources implemented, live check pending | ✅ remap verified (physical pad hidden, A→B); other features share the receiver code | ✅ remap, turbo, toggle verified; trigger sides fixed for Bluetooth's trigger layout (unit-tested) |
| Layers, named profiles, auto profile switching, gyro → mouse | ✅ layers, profile migration and auto switching verified (also hidden); firmware-mouse gyro clutch verified with physical R4 directly (no controller remap) | Xbox-mode GIP has no gyro; PC-mode clutch below | ✅ raw gyro → mouse verified (follows tilt, R4 clutch stops it) |
| Bluetooth-assisted native-stick R4 clutch | Verified on Linux, firmware 1.38 | Verified on Linux in PC mode (`3024`), not Xbox/GIP | Separate configuration link supplies physical R4 and stick |
| Vibration motor test | Hidden: receiver xpad node has no FF | ✅ verified (left/right grip motors; trigger motors not reachable via kernel) | Shown only if kernel advertises FF_RUMBLE |
| Factory reset (all 3 slots to Windows defaults) | ✅ GUI Restore factory profiles → Save + verify; persisted, read-back identical | ✅ verified (device image = `factory-preview`, byte 3 device-managed) | ✅ uses the verified Bluetooth save |
| Stick / trigger calibration | Switch to Xbox mode, then use the Wired column | ✅ verified (`wired-calibrate sticks\|triggers`; acknowledged, full range afterwards); CLI only, no rollback | Not exposed (Android BLE frames recovered, not tested) |
| Battery level | Not reported in PC mode | Shown on Home from `xone`'s four-level report (Low/Normal/High/Full); source verified live (Low), GUI line unit-tested | Not reported |
| Firmware update | Not exposed | Not exposed | Not exposed |

**Gyro on 2.4 GHz / wired:** the controller's own motion setting (Motion page, per slot:
target stick/mouse, hold-to-enable or press-to-toggle, enable button) is stored on the controller
and works without this app; verified on 2.4 GHz (hold PL → gyro moves the right stick, released →
none). Neither receiver nor wired provides raw IMU samples. Bluetooth host gyro uses raw motion.
For PC-mode cable/2.4 GHz, **Bluetooth-assisted native-stick clutch** uses the native
right-stick output and a separate configuration-Bluetooth link for physical R4 and the
unmixed stick. The older Linux firmware-mouse clutch remains available. Xbox-mode
GIP (`202f`) has no gyro source.

On firmware that supports the receiver heartbeat, back paddles appear in receiver **physical**
reports while the app sends the Windows heartbeat (`81 10 04 55` + 28 zero bytes) every 400 ms;
captured live independently of controller remapping.
Test and the host mapper use those reports, not stale `81 14` XInput
residue. They share one heartbeat schedule per receiver in the app, paused during profile
transactions and stopped when neither consumer needs it. Physical reports are sent only when a
key changes, so a paddle stays held until the next report. D-pad Up/Down/Left also set the PL and
PR bits in these reports; the app drops that pair while a D-pad direction is held (a real
PL+PR+D-pad combination is therefore not seen). Verified live: Test page paddles, D-pad without
phantom paddles, direct R4 clutch held indefinitely, and the receiver stops sending physical
reports once the heartbeat stops. Paddles as remap sources: **live check pending**.

The heartbeat disables firmware slot button remapping on the receiver's xpad stream.
The host mapper therefore reapplies the active slot's key map to **all** buttons (standard
buttons, D-pad and triggers from the xpad stream, paddles from the heartbeat's physical reports,
whose standard-button bits drop presses/releases; verified live 2026-10-20: PL→L3, PR→R3 in Steam);
Bluetooth does the same. A paddle mapped to a standard control produces only that control;
a Null/unmapped paddle remains a host-mappable L4/R4/PL/PR source. A trigger mapped away
loses its analog output; another button mapped to a trigger presses it fully. Before the
first receiver physical report, xpad buttons remain the fallback. The firmware default
slot (`0xff`) means identity standard controls and unmapped paddles. Wired GIP is unchanged:
firmware remapping is inferred to remain active there.

Native-stick clutch is different: it sends no receiver heartbeat and preserves the
controller's existing USB slot mappings. The separate Bluetooth link controls only
the clutch and the physical right stick while held; standard USB controls are not remapped twice.

The mapper snapshots the active slot's map from the GUI's last profile read (not unsaved
Mapping-page edits); CLI `gyro` reads profiles once on receiver/Bluetooth startup. The profile
button does not appear in live reports: switching slots while mapping requires stopping,
**Refresh / re-read**, then restarting the mapper (CLI: restart). Changing host profiles
does not detect a controller slot switch.
The firmware version is shown as a raw record because its meaning on this link is not yet proven.

## Requirements

- Linux x86-64, or Windows 11 x86-64. Windows host clutch requires ViGEmBus and HidHide;
  see the Windows setup and hardware-verification limits below.
- Nix provides the Linux build environment (`shell.nix`) and Windows cross toolchain (`windows.nix`).
- Linux builds use Clay at `/home/knd/AI/clay` (`CLAY_DIR` overrides it); Windows builds fetch pinned Clay.
- On Linux, read/write access to the receiver's HID node and (for host remapping) `/dev/uinput`.
  Host mapping also needs read access to its evdev node; vibration testing needs read/write access.
  Check with `ultimate-control devices`. If access is denied, add udev rules. They must load
  **before** `73-seat-late.rules`, otherwise `TAG+="uaccess"` is silently ignored; on NixOS
  `services.udev.extraRules` lands in `99-local.rules` (too late), so use a package instead:

  ```nix
  services.udev.packages = [ (pkgs.writeTextFile {
    name = "8bitdo-3e-udev";
    destination = "/etc/udev/rules.d/70-8bitdo-3e.rules";
    text = ''
      SUBSYSTEM=="hidraw", ATTRS{idVendor}=="2dc8", TAG+="uaccess"
      KERNEL=="uinput", TAG+="uaccess"
      KERNEL=="uhid", TAG+="uaccess"
      SUBSYSTEM=="usb", ENV{DEVTYPE}=="usb_device", ATTR{idVendor}=="2dc8", ATTR{idProduct}=="202f", MODE="0660", TAG+="uaccess"
    '';
  }) ];
  ```

  The last rule (USB access to the `202f` Xbox-mode device) is needed only for `wired-*` commands
  and the GUI's wired link.

## Build

```bash
cd ~/Projects/8bitdo-3e-modifying
nix-shell --run 'cargo build --release'
```

The binary is `target/release/ultimate-control`. A desktop entry named **Ultimate Control**
launches the GUI.

### build-windows

From the repository root on Linux/NixOS:

```bash
nix-shell windows.nix --run 'cargo build --locked --release --target x86_64-pc-windows-gnu'
```

Output: `target/x86_64-pc-windows-gnu/release/ultimate-control.exe`. The shell pins nixpkgs
at `26f4107e47e3ad0ffd0db220469ba83b6f6d96e8` (the workstation system-flake revision),
Rust (including Windows std), MinGW-w64, raylib **5.5**, and Clay. Raylib and its bundled GLFW
are compiled from source by `cc`; Linux still uses pkg-config. MinGW support libraries are
linked statically; only Windows system DLLs are needed, not adjacent runtime DLLs.

Run `ultimate-control.exe gui --demo` for offline editing. Native Windows receiver HID and
Bluetooth GATT backends support profile read/save and live device reports. Checked so far:
under Wine (whose HID layer passes the real receiver through), the Windows receiver code read
the version and a full 868-byte backup byte-identical to the Linux backend's read of the same
controller. Wine counts 31 of 32 bytes for output reports (report ID excluded); both counts are
accepted. Real Windows 11 hardware use was verified for the native-stick clutch release.

Windows right-stick output is implemented for gaming Bluetooth and Bluetooth-assisted
PC-mode USB/cable/receiver clutch. It requires **ViGEmBus and HidHide**; neither is bundled
or installed automatically. Verified on a real Windows PC with the controller and both drivers.
Keyboard/mouse destinations, stick/gyro mouse, USB remapping and automatic profile switching
are supported. Firmware-mouse capture, Steam Input mode, independent extra-paddle output,
Xbox-mode GIP and rumble remain unavailable on Windows.
The GUI hides the PC→Xbox switch on Windows, because switching back currently requires Linux.
Bluetooth `scan` shows empty `adapter_address`/`adapter_address_type` (WinRT doesn't report the
adapter). No driver replacement or administrator token is needed for HID.

Install drivers only from their official projects:

- [ViGEmBus final setup](https://github.com/nefarius/ViGEmBus/releases/tag/v1.22.0).
  The project is retired; the final setup removes its obsolete updater. Do not disable
  Windows security features to make an old driver load.
- [HidHide](https://github.com/nefarius/HidHide/releases). Close its configuration client
  before starting the mapper. The app temporarily whitelists its executable and hides
  only the selected gamepad HID/XUSB nodes, preserving existing settings. It refuses
  ambiguous device identities, inverse cloaking, or activation of unrelated inactive entries.

Stop and error cleanup remove only this session's additions. A durable, per-user journal
(`%APPDATA%\ultimate-control\hidhide-journal.json`) allows the next GUI/mapper startup to
recover an interrupted session, even if the executable was moved. If settings have changed
outside the recorded transitions, recovery refuses to overwrite them; use HidHide to unhide
the intended controller manually. HidHide's control handle is held during mapping, so its
configuration client cannot edit settings until mapping stops.

[HidHide cannot block all Raw Input readers](https://docs.nefarius.at/projects/HidHide/FAQ/).
Verify that the target game sees only the virtual Xbox controller; already-open handles,
Raw Input and anti-cheat behavior are not universal compatibility guarantees.

First receiver checks on real Windows 11, from PowerShell with the original Ultimate Software
closed, the receiver plugged in and the controller awake in 2.4 GHz mode:

```powershell
.\ultimate-control.exe devices
.\ultimate-control.exe receiver-version
.\ultimate-control.exe receiver-pad
.\ultimate-control.exe receiver-backup .\receiver-before.bin
```

`devices` lists the FF00:1 vendor HID interface path, `input_report_bytes` /
`output_report_bytes` (expected **32/32**, including report ID `81`) and `access`
(expected `read_write`). Other lengths stop configuration with an explicit error; access
errors retain the Windows message. Version prints `record_hex` and tentative `components`
(the receiver's version layout remains unproven). Pad prints buttons, triggers and sticks.
Backup creates a new **868-byte** CRC-checked file and prints profile JSON; existing files are
never overwritten. Open `gui` without `--demo`, verify the profiles load, then use Test to
check standard controls and L4/R4/PL/PR (heartbeat active only while needed). No Windows
hardware behavior is claimed by the cross-build alone.

Only after read-only checks pass, an identity write tests the persistent write/commit path:

```powershell
.\ultimate-control.exe receiver-write .\receiver-before.bin
```

Expected: `Read-back identical to written image (CRC + bytes 4..867; header bytes 2/3 are
device-managed)`. This writes controller storage; keep the backup. Ctrl+C cancels bounded HID
waits and attempts report re-enable before exit; if restoration fails, unplug/replug the receiver.

Wine smoke command (requires a working graphical session/OpenGL):

```bash
wine target/x86_64-pc-windows-gnu/release/ultimate-control.exe gui --demo --screenshot 'Z:\tmp\win-demo.png'
```


## Using the GUI

1. Plug in the 2.4 GHz receiver and switch the controller to 2.4 GHz mode. Wake it by pressing any button.
2. Start the GUI from the app menu (**Ultimate Control**) or with:

   ```bash
   target/release/ultimate-control gui
   ```

3. The app detects gamepad input separately from controller settings. Older firmware can load
   settings directly over the receiver; on a PC-mode cable or firmware 1.38 use **Home →
   Connect Bluetooth** as described below. Home shows the settings link and active slot once
   loaded. Header flags stay read-only and full-profile saves preserve them.
4. Change settings on **Mapping, Sticks, Triggers, Vibration, Lighting, Motion** and **Profiles**.
   Edits are *staged* only; nothing reaches the controller yet.
5. Click **Save + verify**. The app writes all three slots, commits them, then reads the controller
   back and only reports success if the read-back is byte-identical. The notice lists how many
   bytes changed. If nothing changed it says **Nothing to save** and writes nothing.
6. **Revert / re-read** discards staged edits and reloads from the controller.
7. **Test** shows live buttons, sticks and triggers.

For Bluetooth configuration, **keep the receiver/cable connected and do not change gameplay
mode**. Stop the host mapper and close competing configuration apps. Hold the front
pixel-logo/profile button for **3 seconds** until configuration LEDs blink, then select
**Home → Connect Bluetooth**. Discovery has a 20-second window. The separate `81HF01`
configuration radio supplies all three profiles to the editor; **Revert / re-read** reloads
them and **Save + verify** writes and checks them.

Starting the GUI mapper reuses this connection on its owning worker instead of disconnecting
and scanning again. Stopping the mapper keeps settings connected, so starting it again does
not require another profile-button gesture. Reading/saving settings is blocked while the
mapper consumes the same Bluetooth stream. A real disconnect or closing the app still
requires enabling configuration Bluetooth again.

The BLE CRC investigation and exact parent verification steps are in
[`spec-ble-write.md`](reconstruction/specs/spec-ble-write.md).

**Remap** edits the selected host profile: button destinations use Linux names such as `KEY_SPACE`,
`BTN_SOUTH` or `BTN_LEFT`; turbo and toggle affect the selected source. Stick-to-mouse and
trigger-to-button controls are independent. Linux names follow the button position, not the
label: Xbox **X** is `BTN_NORTH`, **Y** is `BTN_WEST`.

**Named profiles** live in `~/.config/ultimate-control/profiles/<name>.json` on Linux
(`$XDG_CONFIG_HOME` overrides `~/.config`), or `%APPDATA%\ultimate-control\profiles\<name>.json`
on Windows. `host.json` and `auto.json` use the same platform-specific root.
On first start an existing `host.json` is copied to `profiles/default.json`;
`host.json` is never deleted. Profiles can be created from the current one, renamed and deleted;
the default profile and profiles used by a rule cannot be deleted.

**Layers** (Steam Input "mode shift"): a layer names a hold button and its own mappings, turbo,
toggle, stick-to-mouse and trigger buttons. While the hold button is held, each non-empty layer
field replaces the base field. If several hold buttons are down, the first matching layer in the
profile's order wins; hold buttons are never sent to games.
Entering or leaving a layer releases everything the previous layer was pressing and clears its
toggle latches, so no key stays stuck.

**Gyro output**: right stick (default) or mouse. Both Bluetooth raw gyro and opt-in receiver
firmware mouse motion use angular velocity → soft tiered smoothing → tightening → optional
velocity-based acceleration → output. Holding the clutch suspends gyro. Mouse output integrates
pixels/degree with fractional carry; match game mouse sensitivity before tuning. Real-world
degrees are not in-game degrees unless the game's mouse scale is calibrated. Right-stick
output compensates an inner deadzone, saturates at a configurable speed, applies a power
curve and light time-based smoothing; the game's own curve/turn limit still matters.

Host JSON adds a `gyro` object (all fields optional):

```json
{"gyro":{"min_sensitivity":1.0,"max_sensitivity":1.0,"min_threshold":0.0,
 "max_threshold":75.0,"smooth_threshold":5.0,"smooth_time_ms":40.0,
 "stick_deadzone":0.15,"stick_saturation":180.0,"stick_power":1.0,
 "stick_smoothing_ms":8.0}}
```

Sensitivity endpoints multiply the existing output gain, interpolated linearly between
thresholds in **deg/s**. Equal endpoints disable acceleration. Smoothing applies fully below
half `smooth_threshold`, blends to direct input up to that threshold, and bypasses new fast
motion; `smooth_threshold=0` or `smooth_time_ms=0` disables it. Legacy `deadzone` (stick) /
`gyro_mouse.deadzone` (mouse), default **0.5 deg/s**, now softly tighten small movements rather
than discard them; zero disables tightening. Legacy `sensitivity` stays stick gain (default
1); `gyro_mouse.sensitivity` stays pixels/degree (default 1); inversion and `receiver_gyro`
remain compatible. `stick_deadzone` is output anti-deadzone (0..0.99), `stick_saturation` is
deg/s at full deflection, `stick_power` is the response exponent, smoothing times are ms.
Every control is available on Remap.

These are conservative, Steam-like **project defaults**, not claimed Steam defaults: Steam's
implementation/settings are closed. They prioritize linear gain, stable small movements and
low latency; tune anti-deadzone to each game (zero for games without a stick deadzone).
The open reference is JibbSmart's
[Good Gyro Controls](https://gyrowiki.jibbsmart.com/blog:good-gyro-controls-part-1:the-gyro-is-a-mouse)
and [soft tiered smoothing](https://gyrowiki.jibbsmart.com/blog:tight-and-smooth:soft-tiered-smoothing):
natural sensitivity, min/max acceleration, no hard minimum velocity, optional filters.
[JoyShockMapper source](https://github.com/Electronicks/JoyShockMapper/blob/master/JoyShockMapper/src/main.cpp)
implements `GYRO_SENS`, `MIN/MAX_GYRO_SENS`, `MIN/MAX_GYRO_THRESHOLD`,
`GYRO_SMOOTH_THRESHOLD/TIME`, `GYRO_CUTOFF_SPEED/RECOVERY`. Our tightening corresponds to
zero cutoff plus a recovery threshold. The requested `GYRO_STICK_DEADZONE/SATURATION/POWER/SMOOTHING`
names are not present in that upstream: its gyro-stick controls use
`RIGHT_STICK_UNDEADZONE_INNER/OUTER`, `RIGHT_STICK_UNPOWER` and angular-speed calibration instead.
Our saturation/power/smoothing controls provide the same game-specific compensation concepts,
not a claim of bit-identical JSM or Steam processing. No accelerometer/roll is available for
player-space sensor fusion. Xbox-mode GIP (`202f`) host gyro remains unsupported; the USB cable
in PC mode uses the 2.4 GHz firmware-mouse source.

**Auto profile switching**: rules map a process to a profile (`auto.json`:
`{"default": "default", "rules": [{"profile": "shooter", "process": "game.exe"}]}`). While the
mapper runs and auto mode is on, running processes of your own user are checked every second; the
first matching rule wins (exact `comm` or executable name, or case-insensitive command-line
substring), otherwise the default profile is used. A change restarts the mapper with the new
profile after a 2 s debounce and shows a notice. Auto mode is off at every app start.

**Steam Input mode** (Linux; Remap → "Steam Input (virtual DualSense Edge)", JSON `"steam": true`):
the mapper exposes the controller as a virtual Sony DualSense Edge (054c:0df2) over `/dev/uhid`
instead of the remapped uinput pad, so Steam Input does remapping and gyro. Sticks, triggers,
buttons, the controller slot's key map, L4/R4 → Edge Fn left/right, PL/PR → Edge back paddles,
and gyro (pitch/yaw; no roll or accelerometer exists, so the pad reports level and still) are
passed through; host remaps, layers and host gyro output are ignored. 2.4 GHz gyro needs
"2.4 GHz gyro clutch" on (controller Motion target Mouse, level 6). Needs the `uhid` udev rule
above and Steam's PlayStation controller support enabled. **Not yet tested live.**

**Home → Host input mapper** starts/stops the detected receiver or Bluetooth input runtime.
Only one controller on the selected link may be present. Receiver limitations and validation errors
are displayed; Bluetooth uses the existing configuration-BLE/gyro-clutch runtime.
Stop mapping before editing its profile. Closing the window stops the worker, releases its input
grab, and destroys its virtual outputs. These host changes never write controller profiles.

**Vibration → Test** appears only when passive sysfs discovery finds a controller evdev node
advertising `FF_RUMBLE`. It plays the displayed left/right grip levels for 600 ms using Linux
force feedback (strong = left, weak = right), then stops/removes the effect. The kernel bounds
playback even if the GUI worker is busy; closing the app also cleans up. `FF_RUMBLE` cannot
independently address trigger motors. Wired testing works without loading controller profiles;
those test-only grip levels start at zero. **Test** does not save settings. Demo mode never
opens input devices or starts the mapper/rumble.

**Guide** explains every page, the connections and the order in which slot and host mappings
apply. **Gyro guide** (also reachable from the button at the top of Remap) explains the gyro
clutch, where gyro comes from on each link, every gyro parameter with its range and default,
and a tuning recipe. The page list scrolls when the window is short.

Navigation: mouse, or Tab/arrow keys and the controller D-pad; Enter/A activates; Left/Right adjust
sliders; Escape/B closes dialogs.

**Gamepad detected — controller settings not connected** means USB input was found but no
settings image has been loaded. A USB configuration timeout is not proof that the controller
is asleep: cable/firmware 1.38 can supply input while ignoring those commands. Use
**Connect Bluetooth** without unplugging or changing gameplay mode. A short press of the
front profile button changes slots; stop mapping and re-read settings after changing slots.

Useful flags: `--demo` (fake device, no I/O), `--page NAME`, `--debug-pointer` (crosshair to check
mouse alignment on scaled displays), `--bench` (frame timings), `--screenshot /abs/path.png`.

### Original artwork

Original 8BitDo artwork is optional, loaded at runtime from **your own copy of Ultimate Software X**.
It remains copyright 8BitDo; it is not bundled, committed or redistributed with Ultimate Control.
Point to the application's `Assets` directory, containing both `Xbox3E/` and `XboxWired/Test/`:

```bash
ultimate-control gui --demo --assets /path/to/UltimateSoftwareX/Assets
ultimate-control gui --demo --page Test --assets /path/to/Assets --screenshot /tmp/assets-test.png
```

Resolution selects the first valid directory: `--assets DIR`, then `ULTIMATE_ASSETS`, then
**Settings → Original artwork → Save artwork folder**, then automatic probes. The setting is
`assets.json` beside the host settings (`$XDG_CONFIG_HOME/ultimate-control`, otherwise
`~/.config/ultimate-control` on Linux; `%APPDATA%\ultimate-control` on Windows).
Blanking the field restores automatic discovery; command-line/environment overrides still win.
Invalid, missing, inaccessible or incomplete directories are skipped without an error dialog.
Missing/corrupt individual images use the generic illustration or existing text indicators.
PNG decoding/upload happens once per requested image per loaded root; failures are cached too.

Linux probes executable ancestors for `reconstruction/original/Assets`, then
`~/.local/share/ultimate-control/Assets`. Windows finds an installed **8BitDo Ultimate Software
X** from the Microsoft Store automatically (package `E0680B27.8BitDoUltimateSoftware`, located
through the per-user package registry, so no administrator rights are needed), then
`%LOCALAPPDATA%\ultimate-control\Assets`. Without the Store app, copy its `Assets` folder there
or choose it in **Settings → Original artwork**. Do not change WindowsApps permissions.

Home/Mapping use `XboxWired/mapping/Mapping_bg_L7.png` (Mapping's Back view:
`Mapping_back_L7.png`), not the similarly named Ultimate 3 images. Mapping rows show
`Xbox3E/Mapping_<button>.png` sources and destination `_Default`/`_Change` variants; P1/P2 use
the original PL/PR labels. Sticks, Triggers and Vibration use their respective `*_bg_L7.png`
illustrations. Lighting reuses the 3E front picture, as the original RGB controller does.
Test uses `Test_L7.png`, `Test_*Normal`/`Test_*Enter` for LB/RB/LT/RT/L3/R3/L4/R4/PL/PR,
pressed face/D-pad/View/Menu/Guide images, and `Test_Sticks.png` markers in live plots.

These selections follow the original model dispatch: `0x180d0e700`
(`ev_4f1b4543ad6798baa002ca6c06d81065a3331f86d338161d4586586b5d8d0664`),
`0x180d69cb0` (`ev_4d052f7d138ac155a4f2dde52fb589196237a5a821b18bf4dff8b7db0ce83ed9`),
`0x180d6ca60` (`ev_dd9d6382ab8f87fba66340a55e0380eae6e76b9b29653c1c574fb5ef2651d04d`),
`0x180d71de0` (`ev_95b46a1a071025c2a277fa810b6519a093544be7f61268f18274777141260fa2`),
and RGB `0x180d7c580` (`ev_b461008a7c56b4a11a55a73f1ddc105b8ae03991638e8fa25348620112b3d384`)
in the decompiled Windows app (`UWPAdvance.dll`; the decompiled code is not part of this
repository); mapping image construction is also visible in its string table. `UWPAdvance.xr.xml` describes XAML
types/properties, not authored page layouts. Exact Test overlay-to-controller coordinates
were not recovered from the available IR/metadata: overlays are displayed in labelled cells,
with normal/pressed state, rather than claiming a pixel-identical Windows composite.
Illustrations retain their aspect ratios in the native layout.

### Bluetooth-assisted native-stick clutch (Linux and Windows)

This mode uses firmware gyro-to-right-stick output over **PC-mode USB (`2dc8:3024`)**,
through either the receiver or cable. A separate configuration-Bluetooth connection
provides the physical paddle and unmixed right stick. A Bluetooth adapter is required
even when gameplay uses USB. No receiver heartbeat or runtime profile write is sent.

1. Back up the controller. In its active slot, set Motion target to **Right stick** and
   **press-toggle on a different button**, for example PL. Map the clutch paddle, usually
   R4, to **None**. Keep right-stick inversion and stick swap off.
2. Save and verify the controller slot, restart the controller, and select that slot.
3. On **Remap**, enable **USB / 2.4 GHz native-stick clutch (Bluetooth-assisted)**,
   choose R4, and save a host profile.
4. If Bluetooth settings are not already connected, hold the front profile button for about
   three seconds until the LEDs blink. Start **Host input mapper** on Home. An existing GUI
   settings connection is reused; close competing configuration apps.
5. Press the separate firmware toggle button once if gyro is off. Its toggle state is
   not observable, so startup validates the stored setup but does not pretend to enable it.

While R4 is released, the app forwards the native USB stick/gyro output. While held, it
forwards only the physical right stick, so R3 and thumbstick control remain usable.
The held-state stick has the Bluetooth report's **8-bit resolution** and lower update rate
(about 31 ms in the earlier capture); firmware stick curves are not reconstructed there.
Tune gyro sensitivity/compensation in the controller's Motion page: host raw-gyro
smoothing, acceleration and mouse settings do not apply to this source.

Minimal host profile:

```json
{"native_gyro_clutch": true, "button": "R4"}
```

Or start from the command line:

```bash
ultimate-control gyro --native-gyro-clutch --button R4
```

The setting is mutually exclusive with `receiver_gyro`, `gyro_mouse`, Steam output and
right-stick-to-mouse. A source disconnect stops mapping and releases the virtual output.
Stop before changing controller slots or reading/saving controller profiles.

**Verification:** real Linux receiver and USB-cable captures showed gyro when released,
sustained suppression while R4 was held, full physical right-stick travel and R3 while
clutched, preserved PL→L3 slot mapping, and no events reaching a separate reader of the
grabbed physical gamepad. Cable shutdown exited cleanly, removed the virtual pad and
released the physical input device. Both release builds succeeded; 197 Linux tests and
174 Windows tests passed (the latter under Wine).
The real GUI also loaded settings and started the mapper with both receiver and cable
gameplay connected. Receiver stop/restart retained the same Bluetooth connection without
another profile-button gesture; the GUI-started cable capture verified R4 suppression and
physical right-stick/R3 operation.

Windows HID/ViGEm/HidHide behavior was verified on a real Windows PC with the controller
and both drivers installed.

### Legacy firmware-mouse gyro clutch (Linux)

On **Remap**, enable **Firmware-mouse gyro clutch (Linux)**, select **R4**, **L4**, **PL**, **PR** or a
receiver-visible Xbox button, and choose **Right stick** or **Mouse**, then save the host
profile and start the mapper on Home. Opt-in defaults to `false`; selecting the receiver
link alone never rewrites a stored Bluetooth clutch. Unsupported selections reset to `R4`
when enabling. Minimal host JSON (in `profiles/<name>.json`):

```json
{"receiver_gyro": true, "button": "R4", "sensitivity": 1.0}
```

Set the controller's **Motion target to Mouse**, save the controller slot, and enable firmware
motion with its enable button: press once in **press-toggle** mode, or keep it held in **hold**
mode. Enable button **None** never enables gyro (verified: hold-to-enable + None → no motion).
The host clutch is separate: it is consumed (never emitted to the game), and holding it suspends
firmware mouse motion. With the app closed or mapper stopped, enabled firmware gyro moves the
**desktop cursor**; press the toggle button again to turn firmware gyro off.

Verified setup (2026-10-19, 2.4 GHz): controller slot 1 Motion target **Mouse**, **press once to
enable/disable** on **PL**, no paddle remap; host clutch `R4`, output Right stick. Press PL once
→ motion drives the virtual right stick; hold R4 → freezes; release → resumes; R3 stays a normal
button; desktop cursor stays still while mapping. Toggle state is not shown on the controller;
if nothing moves, press PL. Source names match Bluetooth: L4=`BTN_TRIGGER_HAPPY1`,
R4=`BTN_TRIGGER_HAPPY2`, PL=`BTN_Z`, PR=`BTN_C`; clutch names are `L4`/`R4`/`PL`/`PR`.

Receiver counts become deg/s using actual fractional report intervals and
`input::RECEIVER_COUNTS_PER_DEGREE = 308.0`, measured live on 2026-10-19 at firmware
**Motion sensitivity level 6** (raw 5, maximum): three 90° yaw sweeps gave +27659, −27522,
+27975 REL_X counts. Fast/slow turns agreed within 1.5%, supporting a linear scale.
Other firmware sensitivity levels are unmeasured: **keep level 6**, adjust host gain instead.
The firmware already produces exactly zero counts while still (its own small deadzone);
keep host tightening low (default 0.5 deg/s) or zero. Mouse sensitivity is now px/degree,
not a count multiplier; old receiver mouse gain can be multiplied by 308 to retain its
unfiltered cursor scale. Both outputs use the shared pipeline above. Stick output stays
continuous between ordinary ~4 ms reports, then decays smoothly to the physical stick after
two measured report periods without motion; there is no hard 30 ms reset. This is firmware
mouse output, not raw IMU, and firmware filtering cannot be undone.

### Legacy USB-cable firmware-mouse clutch (Linux)

For an independent R4 clutch without sharing R3, use Bluetooth-assisted native-stick mode above.

With the USB cable the controller enumerates as the same `2dc8:3024` device as the receiver
(gamepad, firmware gyro mouse, vendor interface), so the same **Firmware-mouse gyro clutch (Linux)** setting is
used. Differences, verified live 2026-10-20: the controller never answers configuration requests
over the cable (no profile read/save, version or Test-page paddles; use the receiver or
Bluetooth for those), and paddles are invisible unless the controller slot maps them to a
standard button. The slot's firmware remapping stays active. The mapper detects the cable when
the startup profile read gets no reply (about 10 s), then runs without it.

The clutch must therefore be a standard button. Verified: slot 1 maps PR→R3, Motion target Mouse
with L4 press-toggle; host clutch `BTN_THUMBR`, output Right stick → gyro drives the right stick,
holding PR freezes it, releasing resumes. Physical R3 also acts as the clutch in that setup. A
paddle clutch (`R4`/`L4`/`PL`/`PR`) is refused over the cable with instructions.

## Command line

| Command | What it does | Writes to controller |
|---|---|---|
| `receiver-backup FILE.bin` | Read all profiles to a new file (never overwrites) and print them as JSON | no |
| `receiver-write FILE.bin` | Write an 868-byte image, commit, verify read-back | **yes, persistent** |
| `receiver-pad` | Print one live input sample | no |
| `receiver-version` | Print the raw version record | no |
| `inspect FILE.bin` | Decode a backup offline | no |
| `devices` | List detected controllers and access rights | no |
| `scan`, `backup FILE.bin` | Bluetooth configuration radio (`81HF01`) discovery and read-only backup | no |
| `gyro …` | Host gyro clutch; add `--native-gyro-clutch` for PC-mode USB plus configuration BLE | no |
| `profile`, `replay` | Create / test host-mapping profiles offline | no |
| `ble-write FILE.bin [--address ADDR]` | Bluetooth write, commit, verify read-back | **yes, persistent** |
| `wired-info` | List `2dc8:202f` (Xbox-mode) USB devices, drivers and access; opens nothing | no |
| `wired-version --takeover` | Read the raw version record over GIP | no |
| `wired-backup FILE.bin --takeover` | Read all profiles over GIP to a new file | no |
| `wired-write FILE.bin --takeover` | Write over GIP, commit, verify read-back | **yes, persistent** |
| `factory-preview IN.bin OUT.bin` | Write offline the image a factory reset would produce | no |
| `receiver-xbox-mode --confirm` | Switch the receiver to Xbox mode (`2dc8:3024` → `2dc8:202f`) | **yes, persistent** |
| `wired-xinput-mode --takeover --confirm` | Switch the receiver back to PC (XInput) mode (`202f` → `3024`) | **yes, persistent** |
| `wired-monitor SECONDS --takeover [--report-enable] [--heartbeat]` | Diagnostic: log incoming GIP messages; optional report enable and Windows GIP heartbeat (`04 55 00`, every 400 ms) | no |
| `wired-calibrate sticks\|triggers --takeover --confirm` | Guided physical calibration over GIP; no automatic retries or rollback | **yes, persistent** |

Every `--takeover` command detaches the `xone` driver and hands the device back afterwards.
Gamepad/audio input pauses for the entire operation (including calibration). The `202f` udev rule
from Requirements is required.

### Switching the receiver between PC and Xbox mode

The receiver has two USB personalities; the setting persists across re-plugging. Windows labels the
same flag both "Xinput Mode" and "Tournament Mode" (profile byte 3, bit `0x10`).

| Mode | USB ID | Linux driver | Settings link | Motion page |
|---|---|---|---|---|
| PC (XInput), default | `2dc8:3024` | `xpad` + hidraw | receiver HID (`receiver-*`) | shown |
| Xbox | `2dc8:202f` | `xone` (GIP) | GIP (`wired-*`, `--takeover`) | hidden, as in Windows |

In the GUI, use **Home → Switch receiver to Xbox mode** or **Switch receiver to PC (XInput) mode**,
then confirm. Save or revert staged edits and stop the host input mapper first; switching is refused
while a transfer or mapper start/stop is pending. Input pauses while USB re-enumerates, then normal
discovery reconnects the GUI on the new link. The profile bit `0x10` remains a read-only display.
The PC-mode action is shown on the wired/GIP link because no proven descriptor distinguishes a
directly cabled `202f` controller from an Xbox-mode receiver; it is intended for the receiver.
The transport operations are hardware-verified; this GUI action still needs a live round-trip check.

```bash
# Check the current mode
lsusb -d 2dc8:

# PC -> Xbox (receiver drops and re-appears as 202f within ~3 s)
ultimate-control receiver-backup ~/ultimate-before-mode.bin
ultimate-control receiver-xbox-mode --confirm

# Xbox -> PC (prints "switched", the acknowledgment, handback and the new receiver node)
ultimate-control wired-version --takeover       # optional: confirm the GIP link first
ultimate-control wired-xinput-mode --takeover --confirm
```

Each command sends exactly one command and never retries. Both print `"switched": true` when the
receiver re-appeared in the new mode and exit with an error otherwise. The receiver sometimes drops
off USB before acknowledging, so an `acknowledged`/`handback` error next to `"switched": true` is
normal. Both directions are verified on hardware (2026-10-15/16), and profiles were unchanged after
each switch.

In Xbox mode, if the controller sits idle for roughly 6–10 minutes, profile and version reads can
come back empty (`0xff`). The app refuses that data; power-cycle the controller (hold Home about 5 s,
then turn it on again) and retry.

Run `ultimate-control --help` for all options.

## Calibration

**Calibration changes persistent physical calibration. No calibration backup or rollback is
available; profile backups and factory profile restoration cannot restore it.** Verified on
hardware (2026-10-16) over the receiver in Xbox mode (`2dc8:202f`): triggers and sticks were
calibrated, both Start and End were acknowledged, and afterwards the triggers covered 0–1023 and
the sticks ±32767 with a centered rest. A re-run fixes a bad calibration. Direct-cable support
is not established.

```bash
ultimate-control wired-calibrate sticks --takeover --confirm
ultimate-control wired-calibrate triggers --takeover --confirm
```

Do not touch sticks or triggers before Start; set the trigger mode switch **OFF**, stop host mapping
and active rumble, read the warning, then press Enter. Follow each prompt: LS then RS clockwise
three revolutions; or LT full press/release twice, then RT full press/release/press. Start waits
10 seconds and End waits 100 seconds for matching vendor acknowledgments, without resending.
Movement steps have no deadline. Reporting stays enabled; after End, release all controls for the
3-second neutral summary. “calibration acknowledged” means a vendor completion reply, not a
numeric quality result. Timeout/disconnect means “completion unknown.” Ctrl-C before End sends
no End/cancel/reset: calibration was started but not ended, state is unknown, and rerunning is
required to recalibrate. Driver handback still runs; no safe halfway rollback is known.

## Backups and safety

- **Make a backup first:** `ultimate-control receiver-backup ~/ultimate-$(date +%F).bin`.
  Restore it any time with `receiver-write` on that file.
- Every save recomputes the profile checksum (CRC-16, poly 0x8408, init 0xFFFF over bytes 4..867),
  and `receiver-write` refuses images whose checksum is wrong.
- Every read checks the stored checksum. A corrupted transfer is re-read once, then refused, so a
  bad read is never saved or loaded.
- Physical reporting is paused during profile transfers and is always re-enabled afterwards, even
  on error. If a profile transfer is interrupted and the controller stops responding, unplug and
  replug the receiver. This is not a rollback procedure for interrupted calibration.
- Calibration has no recovered rollback; profile backups do not cover physical calibration.
  Firmware flashing remains unavailable because its destructive transport flow is not proven.
  The vendor firmware catalog's `md5` field hashes the file *length*, not its contents, so it
  gives no integrity protection.

## Troubleshooting

| Symptom | Fix |
|---|---|
| `receiver (2dc8:3024) vendor interface not found` | Receiver/dock unplugged, or a wired controller in Xbox mode (2dc8:202f) is connected instead. |
| `Controller did not acknowledge report disable` / USB configuration did not reply | Input can still work when USB settings commands are unavailable, including PC-mode cable and firmware 1.38. Keep the cable/receiver connected and gameplay mode unchanged; hold the profile button for 3 seconds, then use **Home → Connect Bluetooth**. If no gamepad input is detected either, check controller power, pairing and access. |
| Permission denied on `/dev/hidraw*` or `/dev/uinput` | Add the udev rule above, re-plug. |
| Permission denied during vibration test | Grant read/write access to the controller's `/dev/input/eventN`; FF capability discovery does not imply access. |
| Mouse clicks land on the wrong control | Run `gui --debug-pointer` and report the overlay values. |
| Receiver shows up as `2dc8:202f` "Microsoft Xbox Controller" and `receiver-*` commands fail | The receiver is in Xbox mode. Use the `wired-*` commands, or switch back with `ultimate-control wired-xinput-mode --takeover --confirm` (see "Switching the receiver between PC and Xbox mode"). Permission denied → install the `202f` udev rule from Requirements as an early (70-) rules file; for a one-off, `sudo setfacl -m u:$USER:rw /dev/bus/usb/BBB/DDD` (numbers from `lsusb`). |
| Wired reads fail with "empty (0xff) profile buffer" | Controller idled in Xbox mode and closed its configuration link. Power-cycle the controller (hold Home ~5 s, turn on) and retry. |
| GUI exited with `*** buffer overflow detected ***` (fixed) | raylib 5.5 bug: its GLFW joystick callback writes the gamepad name into a 4-entry array for any joystick id (0–15), so a fifth connected joystick (e.g. Wine/Steam virtual Xbox pads next to ours) wrote out of bounds. The GUI now drops joystick ids ≥ 4 before they reach raylib (`src/gui/bridge.c`); raylib only polls gamepads 0–3 anyway. Reproduced with six virtual pads before the fix; the fixed GUI survives them. |

## Project layout

- `src/receiver.rs` – 2.4 GHz receiver transport (read, save, version, live input).
- `src/profile.rs` – typed, lossless codec for the 868-byte profile image.
- `src/gui/`, `src/live.rs` – native GUI and its device worker.
- `src/input.rs` – host remapping (uinput), Bluetooth gyro and optional receiver mouse-event clutch.
- `src/att.rs`, `src/protocol.rs` – Bluetooth configuration channel and shared codecs.
- `src/gip.rs` – wired / Xbox-mode GIP transport (settings, calibration); `src/steam.rs` – virtual DualSense Edge for Steam Input.
- `reconstruction/specs/` – protocol and behaviour notes recovered from the original Windows app, with live hardware findings.
- `fixtures/` – real 868-byte profile images used by the codec tests.

Not included: the 8BitDo software packages, their artwork, firmware, and the decompiled code.
Artwork is loaded at runtime from your own copy (see "Original artwork").
