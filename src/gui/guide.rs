//! Static help pages. Facts match the implementation and README; keep them in sync.
use super::{clay::{Size,Style},widgets::*};

pub const APP:&[(&str,&str)]=&[
("What this app is",
"A native replacement for 8BitDo Ultimate Software X for the Ultimate 3E controller, plus a Steam-Input-style host layer. It does two separate jobs. Controller pages (Mapping to Settings) edit the three profile slots stored inside the controller; nothing is written until you press Save, and Save reads the slot image back to verify it. Host pages (Remap, Host profiles) change only what this PC does with the controller's input while the host input mapper runs; they never write to the controller."),
("Connections",
"PC mode (2dc8:3024), over the 2.4 GHz receiver or USB cable: gamepad input and firmware gyro. Gamepad detected does not mean settings are connected. Firmware 1.38 may not answer USB settings requests: keep the cable/receiver connected and its gameplay mode unchanged, hold the front profile button for 3 seconds, then use Home > Connect Bluetooth. This separate configuration link supplies settings and physical clutch input. Gaming Bluetooth supplies raw gyro through HID. Xbox/GIP mode (2dc8:202f) has no gyro; its configuration, calibration and rumble are Linux-only. Windows host output requires ViGEmBus and HidHide."),
("Home",
"Shows the settings link, active slot, firmware record and battery (wired only), separately from the gameplay input. Refresh re-reads the controller. Host input mapper hides the physical pad from games and creates virtual output. The GUI keeps its configuration Bluetooth connection when starting or stopping the clutch; no second pairing gesture is needed while that connection remains open. Stop mapping before reading/saving controller settings or editing host profiles. On the receiver, Home can switch between PC and Xbox mode (Linux only)."),
("Controller slots",
"The controller stores three slots; a short press of the front profile button cycles 1 → 2 → 3 → default → 1. The app cannot see that button live: after switching slots, Refresh, and restart the mapper so it uses the new slot's mappings. Mapping: per-button targets (paddles L4/R4/PL/PR can become standard buttons; changes take effect after a controller power cycle). Sticks: response curve, deadzones, swap. Triggers: travel range. Vibration: motor strength per slot, plus a live rumble test. Lighting: effect, speed and colours (a low battery can stop the LEDs). Motion: the controller's own gyro (target stick or mouse, enable button, hold or press-toggle, sensitivity); shown only in PC mode, as in Windows. Profiles: select, rename, reset or delete a slot. Settings: polling rate and flags (read-only), Auto Reconnect, artwork folder, Restore factory profiles."),
("Test",
"Live view of every control, including the back paddles on the 2.4 GHz receiver (the app sends the Windows heartbeat while this page is open). The Motion section shows live gyro: raw gyro and accelerometer over Bluetooth, or the firmware's gyro-as-mouse output on the receiver/cable when the slot's Motion target is Mouse and turned on. Use it to check a remap or gyro before playing."),
("Remap and Host profiles",
"Remap edits the selected host profile: button mappings (Linux key names such as BTN_SOUTH or KEY_SPACE), turbo, toggle, stick-to-mouse, trigger-to-button, layers (hold a button to switch to another set of mappings), the gyro clutch and the gyro response. Save host profile writes it to your config folder. Host profiles can be switched automatically by the running game (rules match a process name or command line). Steam Input mode (Linux) instead exposes a virtual DualSense Edge so Steam does all remapping and gyro. See Gyro guide for every gyro setting."),
("Order of processing",
"Physical button → controller slot mapping → host profile mapping/layers/turbo → virtual device. Legacy receiver-heartbeat and Bluetooth gaming paths reapply slot mappings in the app. Bluetooth-assisted native-stick mode does not send the receiver heartbeat: firmware mappings remain active and are not applied twice."),
("Safety",
"Back up before saving: receiver-backup FILE.bin on the command line, or let Restore factory profiles save a backup first. Writing a backup back (receiver-write) restores all three slots exactly. Stick/trigger calibration is persistent and has no undo; it is command-line only. Firmware updates are not offered."),
];

pub const GYRO:&[(&str,&str)]=&[
("How the gyro clutch works",
"With the host mapper running, holding the clutch removes gyro aiming so you can reposition the controller; release resumes it. The physical right stick remains usable while clutched. Raw Bluetooth gyro is processed by the app. Native-stick mode uses firmware gyro, which must first be turned on with its separate toggle button. The selected clutch paddle must be unmapped in that controller slot; otherwise its firmware-mapped button would still reach the game."),
("Where the gyro comes from",
"Gaming Bluetooth: raw pitch/yaw from the controller plus physical paddle reports over configuration Bluetooth. USB cable and 2.4 GHz: native-stick mode uses the firmware's gyro-to-right-stick output and a separate Bluetooth connection for the clutch. It works around firmware 1.38's missing receiver paddle reports without sending a heartbeat. The older Linux firmware-mouse mode instead captures Mouse output at sensitivity level 6 (308 counts per degree). Xbox/GIP mode has no gyro."),
("USB / 2.4 GHz native-stick setup",
"1. Use PC mode; keep the cable/receiver connected. 2. Use Home > Connect Bluetooth to read settings. In the active slot set Motion target Right stick, press-toggle on a button other than the clutch (for example PL), and map R4 to None. Keep right-stick inversion and stick swap off. 3. Save, restart the controller and select that slot. 4. In Remap enable Bluetooth-assisted native-stick clutch, choose R4 and Save host profile. 5. If Bluetooth settings are not connected, hold the front profile button ~3 seconds until LEDs blink. Start Host input mapper; an existing settings connection is reused. 6. Press the firmware motion-enable button once if gyro is off. Startup checks the stored setup but never writes it."),
("Native-stick quality and tuning",
"Released: the app forwards the USB right-stick output including firmware gyro. Held: it replaces that with the physical right stick from Bluetooth, so thumbstick control is not lost. That Bluetooth stick has 8-bit resolution and a lower update rate (about 31 ms in the recorded setup). Firmware stick curves are not reconstructed while held. Adjust native gyro sensitivity and compensation in the controller's Motion page; the app's raw-gyro filters below do not apply to native-stick mode."),
("Windows setup and recovery",
"Install the official ViGEmBus final setup and HidHide separately; the app never installs drivers, elevates itself or disables security features. ViGEmBus is retired, so use its final installer without the old updater. The mapper selects only the matching gamepad HID/XUSB nodes, not the receiver's vendor/audio interfaces, and temporarily hides them while creating a virtual Xbox 360 controller. HidHide cannot block every Raw Input reader; verify the game sees only the virtual controller. Close HidHide's configuration client before mapping. Stop/error cleanup restores app-owned changes; after a crash, restart this app under the same Windows account to recover its journal, or unhide the selected controller in HidHide."),
("Windows output limits",
"Windows gyro output is a virtual Xbox 360 right stick; independent extra paddle outputs and keyboard/mouse destinations are not available. Map non-clutch paddles to ordinary Xbox buttons if you want game actions. Normal buttons, sticks, triggers and supported gamepad mappings remain usable. Bluetooth-assisted native-stick mode also requires a working Bluetooth adapter when gameplay uses USB."),
("Choosing the clutch button",
"Raw Bluetooth and Bluetooth-assisted native-stick mode use physical R4, L4, PL or PR, independently of ordinary USB button reports. In native-stick mode leave the selected paddle unmapped in the controller slot. The older Linux firmware-mouse path uses receiver heartbeat paddles where available; on a cable it instead needs a standard-button mapping, such as R4 → R3, which also makes physical R3 clutch. Prefer native-stick mode for an independent R4 clutch on firmware 1.38."),
("Output: Right stick or Mouse",
"Linux raw-gyro paths can output Mouse or Right stick. Mouse is precise when a game accepts mouse and controller together. Right stick is limited by the game's deadzone, response curve and maximum turn speed. Windows and native-stick clutch use right-stick output. Native-stick mode forwards the firmware's existing mixed output when released; the raw-gyro gain/filter controls apply only to the other gyro paths."),
("Mouse gain (px/degree)",
"How far the output moves per degree of controller rotation. 1.0 = 1 pixel (mouse count) per degree; range 0.1–10. In games the result also depends on the game's own mouse sensitivity: set this once (e.g. 3–5), then tune the game's sensitivity until a comfortable wrist turn (~30–45°) gives the camera turn you want. If 10 is still too slow, raise the game's sensitivity, not this."),
("Stick gain (×)",
"Multiplies rotation speed before it becomes stick deflection. Higher = less rotation needed to reach full stick. Change Saturation instead if you think in degrees per second."),
("Tightening (deg/s)",
"Softly reduces very slow rotation, which hides hand tremor and sensor noise. Below this speed the output is scaled down in proportion (half the threshold → a quarter of the movement); it is never a hard cut-off, so slow aiming still moves. Default 0.5. Keep it low (0–1): the 2.4 GHz firmware already outputs exactly zero while the controller is still. Raise it only if the aim drifts or shakes while you hold still."),
("Invert gyro X / Y",
"Flip horizontal or vertical direction. Leave off unless turning right moves the aim left, or tilting the front edge up aims down."),
("Slow / Fast sensitivity and thresholds (acceleration)",
"Rotation slower than Slow threshold uses Slow sensitivity; faster than Fast threshold uses Fast sensitivity; in between it blends linearly. Defaults are 1.0 / 1.0 (acceleration off) with thresholds 0 and 75 deg/s. Tip: first find a gain you like with both at 1.0. Then lower Slow sensitivity (e.g. 0.6) for finer precision on small adjustments and/or raise Fast sensitivity (e.g. 1.5–2.0) for quicker 180° turns. Fast threshold around 50–100 deg/s suits most people; Slow threshold usually stays at 0."),
("Smoothing threshold and window",
"Soft tiered smoothing: only small, slow movements are averaged over the window, large fast movements pass through untouched, so it removes jitter without adding lag to flicks. Rotation below half the threshold is fully smoothed, above the threshold it is direct, and blends in between. Defaults 5 deg/s and 40 ms. Increase the window (60–100 ms) if a sniper scope still shakes; set either value to 0 to disable smoothing."),
("Anti-deadzone (%) — stick output",
"Smallest stick deflection sent as soon as the controller moves, so slow gyro movement escapes the game's inner stick deadzone. Set it to the game's deadzone: start at 15%, lower it if tiny movements jump, raise it if slow movements do nothing."),
("Saturation (deg/s) — stick output",
"Rotation speed that produces full stick deflection. Default 180 deg/s. Lower = more sensitive (full turn speed with less rotation); higher = finer control but full turn speed needs fast motion. Match it to how fast the game turns at full stick."),
("Stick power — stick output",
"Curve between anti-deadzone and saturation. 1.0 is linear. Above 1 makes slow movement finer and fast movement steeper; below 1 the opposite. Use it to cancel a game's own stick curve: if small adjustments feel too strong, raise it (1.5–2.0)."),
("Stick smoothing (ms) — stick output",
"Short time constant on the final stick value to hide the step size of the input reports. Default 8 ms; 0 disables it. Values above ~20 ms add noticeable lag."),
("Tuning recipe",
"1. Pick the output (Mouse if the game allows it). 2. With acceleration off, set gain until a comfortable turn covers the view you need. 3. Stick output: set anti-deadzone to the game's deadzone, then saturation to taste. 4. Add acceleration only if you need both precision and fast turns. 5. Raise smoothing window or tightening only if aim shakes at rest. 6. Practise ratcheting: hold the clutch, recentre your hands, release. Save host profile after each change and restart the mapper to apply it."),
("Troubleshooting",
"Native-stick gyro does nothing: verify PC mode, the selected controller slot, Right stick target, and a separate press-toggle enable button; press that button once. Cannot start: enable configuration Bluetooth and close competing configuration apps; on Windows check the ViGEmBus/HidHide error and close the HidHide client. Stop mapping before editing slots. Legacy Linux firmware-mouse mode: Motion target must be Mouse at level 6; while stopped its enabled gyro moves the desktop cursor. On firmware 1.38 use Bluetooth-assisted native-stick mode if receiver paddles are unavailable."),
];

pub fn draw(ui:&mut Ui<'_,'_>,sections:&[(&str,&str)]){
    for (index,(title,body)) in sections.iter().enumerate(){
        ui.element("guide-section",index as u32,Style::column(Size::Grow,Size::Fit).padding(16).gap(8).rounded(10.0).background(PANEL),|ui|{
            ui.text(title,19,TEXT);ui.text(body,15,MUTED);
        });
    }
}
