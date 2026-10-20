//! Static help pages. Facts match the implementation and README; keep them in sync.
use super::{clay::{Size,Style},widgets::*};

pub const APP:&[(&str,&str)]=&[
("What this app is",
"A native replacement for 8BitDo Ultimate Software X for the Ultimate 3E controller, plus a Steam-Input-style host layer. It does two separate jobs. Controller pages (Mapping to Settings) edit the three profile slots stored inside the controller; nothing is written until you press Save, and Save reads the slot image back to verify it. Host pages (Remap, Host profiles) change only what this PC does with the controller's input while the host input mapper runs; they never write to the controller."),
("Connections",
"2.4 GHz receiver (PC mode, 2dc8:3024): full profile editing, Test page with L4/R4/PL/PR, host mapper with gyro clutch. Bluetooth: profile editing over the configuration link (hold the front profile button ~3 s until the LEDs blink), raw gyro for host gyro. Wired / Xbox mode (2dc8:202f): profile editing and calibration over GIP, rumble test, no gyro. USB cable in PC mode: input and firmware gyro only; the controller answers no configuration requests, so use the receiver or Bluetooth for editing. Home shows which link is active; press a controller button and Refresh if it reports the controller asleep."),
("Home",
"Shows the link, active slot, firmware record and battery (wired only). Refresh re-reads the controller. Host input mapper starts/stops the host layer: it hides the physical pad from games and creates a virtual pad (plus keyboard/mouse if your host profile needs them). Stop it before editing host profiles. On the receiver, Home can switch the receiver between PC and Xbox mode (Linux only)."),
("Controller slots",
"The controller stores three slots; a short press of the front profile button cycles 1 → 2 → 3 → default → 1. The app cannot see that button live: after switching slots, Refresh, and restart the mapper so it uses the new slot's mappings. Mapping: per-button targets (paddles L4/R4/PL/PR can become standard buttons; changes take effect after a controller power cycle). Sticks: response curve, deadzones, swap. Triggers: travel range. Vibration: motor strength per slot, plus a live rumble test. Lighting: effect, speed and colours (a low battery can stop the LEDs). Motion: the controller's own gyro (target stick or mouse, enable button, hold or press-toggle, sensitivity); shown only in PC mode, as in Windows. Profiles: select, rename, reset or delete a slot. Settings: polling rate and flags (read-only), Auto Reconnect, artwork folder, Restore factory profiles."),
("Test",
"Live view of every control, including the back paddles on the 2.4 GHz receiver (the app sends the Windows heartbeat while this page is open). Use it to check a remap before playing."),
("Remap and Host profiles",
"Remap edits the selected host profile: button mappings (Linux key names such as BTN_SOUTH or KEY_SPACE), turbo, toggle, stick-to-mouse, trigger-to-button, layers (hold a button to switch to another set of mappings), the gyro clutch and the gyro response. Save host profile writes it to your config folder. Host profiles can be switched automatically by the running game (rules match a process name or command line). Steam Input mode (Linux) instead exposes a virtual DualSense Edge so Steam does all remapping and gyro. See Gyro guide for every gyro setting."),
("Order of processing",
"Physical button → controller slot mapping → host profile mapping/layers/turbo → virtual device. On the 2.4 GHz receiver and Bluetooth the mapper re-applies the slot mapping itself, because the heartbeat that reveals the paddles also turns the firmware's own remapping off."),
("Safety",
"Back up before saving: receiver-backup FILE.bin on the command line, or let Restore factory profiles save a backup first. Writing a backup back (receiver-write) restores all three slots exactly. Stick/trigger calibration is persistent and has no undo; it is command-line only. Firmware updates are not offered."),
];

pub const GYRO:&[(&str,&str)]=&[
("How the gyro clutch works",
"Gyro aiming is on all the time; holding the clutch button suspends it. While held, the controller can be moved back to a comfortable position without moving the aim — like lifting a mouse off the desk ('ratcheting'). Release and gyro continues from where the aim was left. The clutch button is consumed: games never see it. This is the inverse of the controller's own 'hold to enable' motion mode."),
("Where the gyro comes from",
"Bluetooth: raw rotation from the controller (pitch and yaw; no roll or accelerometer), so no controller setup is needed. 2.4 GHz receiver and USB cable in PC mode: the receiver has no raw gyro, so set the slot's Motion page to target Mouse, sensitivity level 6 (the only level calibrated: 308 mouse counts per degree), and enable it with press-toggle on a button you don't need (press once to turn firmware gyro on). With the mapper running, that mouse movement is captured and converted back to degrees per second; with the mapper stopped it moves the desktop cursor. Xbox mode (GIP) and Steam Input mode: no host gyro."),
("Choosing the clutch button",
"Bluetooth and 2.4 GHz: R4, L4, PL or PR work directly, no controller mapping needed (2.4 GHz also allows a standard button). USB cable: unmapped paddles are invisible to the PC, so map the paddle to a standard button in the slot (over the receiver or Bluetooth), then pick that button here, e.g. R4 → R3 with clutch BTN_THUMBR; physical R3 then also clutches."),
("Output: Right stick or Mouse",
"Mouse moves the cursor/camera directly and is the most precise choice when a game accepts mouse and controller at the same time. Right stick works in every game but is limited by the game's own stick deadzone, curve and maximum turn speed; the stick settings below compensate for those. Gyro is added to your physical right stick, so both can be used together."),
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
"Nothing moves on 2.4 GHz/cable: press the slot's motion enable button once (press-toggle state isn't shown on the controller) and check Motion target is Mouse. Cursor moves while mapping: the mapper isn't running or 2.4 GHz gyro clutch is off. Too fast/slow after changing Motion sensitivity: keep the controller at level 6 and use the host gain instead. Clutch does nothing over the cable: choose a standard button as clutch (see above)."),
];

pub fn draw(ui:&mut Ui<'_,'_>,sections:&[(&str,&str)]){
    for (index,(title,body)) in sections.iter().enumerate(){
        ui.element("guide-section",index as u32,Style::column(Size::Grow,Size::Fit).padding(16).gap(8).rounded(10.0).background(PANEL),|ui|{
            ui.text(title,19,TEXT);ui.text(body,15,MUTED);
        });
    }
}
