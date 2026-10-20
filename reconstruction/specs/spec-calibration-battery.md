# Ultimate 3E calibration and battery

Scope: logical PID `0x202f`; Windows Ultimate Software X 1.4.9.0 and Android Ultimate Software V2 2.00. **O** = observed shipped code/metadata or upstream source, **L** = parent-supplied live observation, **I** = inference/composed expected packet, **U** = unproven. Static command recovery is not firmware acceptance. No hardware access, controller traffic, code changes, builds, or tests performed for this document.

## Evidence and coordinates

Windows addresses are VAs in `reconstruction/original/UWPAdvance.dll`, SHA-256 `1f2325902b5d3ca2cc7b44405243247d3f276c39f99a66c97b193545be038352`, with retained function IR in `reconstruction/ir/`. The fuller UI/routing audit remains in [spec-maintenance.md](spec-maintenance.md#1-calibration); transport details in [spec-gip.md](spec-gip.md). The live facts in [replacement-plan.md](replacement-plan.md) supersede older statements there that GIP configuration is not yet live.

| Alias | Retained REA Evidence ID |
|---|---|
| CAL | `ev_6784a1aee5aaccebeb1a1986c9aed07b3da8d79e15cbf78e3bac9753fb4be8b6` |
| UI | `ev_e8dd03488a043a96f2c19ef96e229bd1e7c239c5f922779afc0ad6f1e60b9801` |
| STATE | `ev_b465b7ec19fcdf68a71a6ecc2e079205eaf0fb34411c44f483b56d0e6818b61b` |
| TX | `ev_d2692710de786f469fdb89e7ca96057ca2a9c1e36c209d759ab7df48f991566f` |
| RX | `ev_e1c08f54e67d47430a3b24fcd30f6f0eb78303a16021d2fa9e5ed7f60557816e` |
| ACK | `ev_5508bec9c41458dd86db3b2fe75340db31a7ff9af717ed74e12cd75f53c5e1f4` |
| INPUT | `ev_e2c1bb70af3bd49debb62270da6c9c2c12e6002b9999ce601c7ccbdd2919d71c` |
| INPUTUI | `ev_aec5c91d8f726fcf919490f58c0b11732481600a040b81ab4c2ced68afa418dc` |
| D4X | `ev_8b9eeb346c176a2fc1a905dd8415c84b0b5ddb4553dce8ecb77e46cf3d0608ac` |
| WINMD | `ev_f9911df72ea3446360e14fcf4f2cc0324adec9f3538f95d01fee3f97279d38a0` |
| MOBILE | `ev_a9ebae27127a8374e3359964851b97ee759c59ff0a1ab5d04dbdee854361ae82` |
| MOBILERX | `ev_0d0a3140abe3037c4a170feec2b81cc4741dc484bf1cc3475de3c75a9a561bce` |

MOBILE/MOBILERX are retained in `reconstruction/ble-write-evidence.json`. Their Ghidra addresses have image base `0x100000`: e.g. `0x127280` corresponds to ELF VA `0x27280`. Android addresses below use **ELF VAs**, not Ghidra rebased addresses or ARM offsets.

**O, inspected artifact identities:** APK SHA-256 `7f225242d9c899b8b9c316ef3442c0679ecc0d6160cfe2e87fe89bb809e5f1e2`; package `com.bbitdo.advanceandroidv2`, version 2.00/code28. `lib/x86_64/libadvancelib.so` SHA-256 `5a0bb940a91c7f6eebb2c806faf13bd3e75cf982d06112177db690f087cf252c`; `classes.dex` SHA-256 `5b44a8d4e53f81b5f067aa0974dc5e9463c164b913b586631acf19f7222b725e`. New native observations used GNU `objdump -d -C -Mintel` on that exact APK member; new Java observations used JADX 1.5.6. The overall JADX run reported 68 errors in the application/dependencies; the focused methods cited below contain no failed-decompilation placeholder. REA Android inspection was unavailable because `REA_JADX_MCP_JAR` is unset; these new observations are local static observations, not invented REA Evidence IDs.

Java shorthand below is under `com.bbitdo.advanceandroidv2`. DEX coordinates are **`code_item` file offsets within classes.dex**, independently read from its class-data/method tables, not native addresses or generated-source line numbers:

| Java method | DEX code offset |
|---|---:|
| `activity.StickCalibrationActivity.RefreshStep(int,int,int,int)` | `0x336328` |
| Stick `WriteCalibrationStartDone / WriteCalibrationEndDone` | `0x3367e4 / 0x336798` |
| Stick `setBack / onDestroy` | `0x3375c4 / 0x337528` |
| `activity.TriggerCalibrationActivity.RefreshStep(int,int)` | `0x3465b8` |
| Trigger `WriteCalibrationStartDone / WriteCalibrationEndDone` | `0x3466d0 / 0x346684` |
| Trigger `setBack / onDestroy` | `0x347540 / 0x3474a4` |
| `activity.StepActivity.readKeyWith(byte[]) / read3EKeyWith(byte[])` | `0x332188 / 0x331e44` |
| `activity.MainActivity.readKeyWith(byte[])` | `0x2f8bd0` |
| `activity.SettingActivity.initSetting / timer1` | `0x31a064 / 0x31b784` |
| `utils.HIDChannel.WriteBleHidData(byte[])` | `0x372220` |
| `utils.HIDChannelTimer.start(int,int,int)` | `0x371f74` |

## 1. Windows calibration over wired/GIP

**L:** GIP version, profile read/write/commit and PC/Xbox-mode round trips are now verified over the `2dc8:202f` receiver-in-Xbox-mode link. **U:** neither stick nor trigger calibration has been exercised on that link or a directly cabled 3E. The transport is no longer the missing prerequisite; calibration acknowledgment, behavior, persistence, and interruption recovery remain separate gates. A live GIP receiver is not proof of direct-cable calibration.

### Exact commands and acknowledgments

Define `Z(n)` as exactly n zero bytes; `||` concatenation; `SS` as the next nonzero GIP transmit sequence. All multi-byte integers are little-endian. There is **one start/end pair per feature**, not separate LX, LY, RX, RY, LT, RT commands.

| Operation | Inner 16-byte header, exact | Exact Windows GIP payload P60 |
|---|---|---|
| Start both sticks | `0e 00 01 00 || Z(12)` | `04 0e 00 01 00 || Z(55)` |
| End both sticks | `0f 00 01 00 || Z(12)` | `04 0f 00 01 00 || Z(55)` |
| Start both triggers | `0e 00 02 00 || Z(12)` | `04 0e 00 02 00 || Z(55)` |
| End both triggers | `0f 00 02 00 || Z(12)` | `04 0f 00 02 00 || Z(55)` |
| Motion start, distinct feature | `0e 00 03 00 || Z(12)` | `04 0e 00 03 00 || Z(55)` |

**O:** channel4; count/total/offset all0; no payload, checksum, profile image, or commit command. Start factory `0x180ca5d30`, end `0x180ca5af0` [CAL]. Stick launchers `0x180ca6170 / 0x180ca5fc0`; trigger launchers `0x180ca5b80 / 0x180ca5940` [CAL]. Both select GIP queue `0x180c7f2f0`, not receiver HID queue. `0x180c4aef0 → 0x180c4b240 → 0x180c4b340` [TX] produces P60 and calls D4X `SendMessageId(P60,0x13)`; D4X `0x180001ad0` [D4X] forwards WinRT `GipMessageClass.Command=0`, message ID13. This is **not** standard GIP rumble09 or status03.

**I, exact composition using the now-live framing:** every table row becomes one USB interrupt-OUT packet `13 00 SS 3c || P60` (64 bytes), as `src/gip.rs::command_frame` uses on the parent-verified GIP configuration link. Flags00 are live-proven for that implementation, but not a captured Windows-provider choice. Discover endpoint addresses; do not assume endpoint numbers. Do not copy the 62-byte BLE wrapper onto GIP.

**O expected vendor reply contract; U actual calibration reply:** incoming GIP message ID12 is parsed by `0x180cabe30` [RX], then `0x180ca24c0` [ACK]. The minimum expected acknowledgment header is:

```text
start: 04 00 0e 00 || Z(12)
end:   04 00 0f 00 || Z(12)
       kind  echo    count=0,total=0,offset=0
```

These are **expected zero-length completion headers derived from the pending zero-length command**, not captured calibration replies. Reply padding/actual GIP payload length and firmware error replies remain U. There is no parameter echo in this header: second u16 is opcode, not selector1/2. Permit only one outstanding calibration operation; do not confuse `04 00 03…` physical input with acknowledgments. Validate the actual reply, rather than fabricating zero fields on a timeout. Generic Windows completion cancels the timer, removes the command, invokes success when offset+count reaches requested total0; it does not inspect calibrated limits/quality.

**O:** vendor completion and GIP transport acknowledgment are independent. If a complete incoming GIP12 message requests transport ACK, reply according to [spec-gip.md](spec-gip.md#transport-acks-and-sequence-ownership): `01 20 RR 09 00 12 20 NNlo NNhi 00 00 00 00` for unfragmented primary client0, incoming sequenceRR and accepted payload lengthN. No ACK flag → no transport ACK for an unfragmented message. Fragmented replies require the documented cumulative-length ACK/reassembly procedure. A transport ACK alone never completes calibration.

### Required user actions and timing

**O, sticks:** `0x180d15b20` [UI] says do not touch sticks before Start. After start acknowledgment (`0x180ca6320`, StartCalibration), `0x180d28eb0` [STATE] initializes state1 and prompts: **push LS to top edge, rotate clockwise slowly three times**. `0x180d28010` [STATE] requires cardinal sequence `top → right → bottom → left → top`, repeated three revolutions; final LS-top is state13, then the same sequence for RS from states14 through26. Final RS-top sends End(1), then host state27. No start/end per cardinal or individual coordinate. Release/center check after final RS-top is not present in this host state machine.

**O, exact stick acceptance gates:** squared radius `x*x+y*y > 0x3f00` (integer ≥16129); major coordinate `< -120` or `>120`; minor coordinate -29..29 inclusive. Top means negative Y. For 3E, `0x180dbb9d0` [INPUTUI] takes vendor-GIP payload bytes20..23 as u8, coerces123..131 inclusive to127, subtracts127, and supplies those values. Do not run these comparisons directly on standard GIP20 or evdev i16 values. Vendor message12 prefix `04 00 03` is forwarded unchanged by `0x180cb34d0` [INPUT].

**O, triggers:** `0x180d14cc0 / 0x180d14e80` [UI]: do not touch triggers before Start; **trigger mode switch OFF**. Start ack publishes StartCalibrationTri (`0x180ca55d0` via original-PE thunk `0x180df1418`). User must perform **LT full press → full release → full press → full release; RT full press → full release → full press**. `0x180d27df0 / 0x180d27f10` [STATE] use exact values255/0, not near-endpoint thresholds. Last RT255 sends End(2); final RT release is not required. `0x180dbb730` [INPUTUI] supplies vendor payload bytes24/25 as unscaled u8.

**O, timer semantics:** every Start waits10 seconds per attempt, End100 seconds per attempt; retry limit2 means initial send plus up to2 resends (`0x180c4b7a0 / 0x180c4b9e0` [TX]). These are response deadlines, **not durations to hold a stick/trigger or fixed delays between start and end**. No per-user-step deadline or inter-command sleep was found in the cited state machines. Reporting must remain available for progression; inspected start paths do not issue opcode7-disable.

**O, completion:** End(1) ack → `0x180ca5860` [CAL], EndCalibration → `0x180d28850` [STATE], “Joystick calibration complete”; End(2) ack → `0x180ca4e20` [CAL] via thunk `0x180df1400`, EndCalibrationTri → `0x180d28b00` [STATE], “Trigger calibration complete.” These mean the app received a generic completion ack, **not** a numeric accuracy/self-test result. Verify ordinary neutral/full-range input afterward if a future parent-authorized calibration succeeds.

**O, failure limitation:** original-PE disassembly resolves start/end unfinished/timeout thunks `0x180df13f0/13f8/1408/1410/1430/1438/1448/1450` to `0x180836810`, which only returns1 (`ev_6d91936ebfc759d2e7d71e10fba09886b8002d52f992cb869e33f6eb1237fc50`). Thus these callbacks do not report a calibration-specific error or roll back. Exhausted response timeout is a communication failure; failure to reach the gates stalls the workflow; disconnect loses the session. **U:** firmware rejection/status codes, partial-result persistence, quality failure, and whether Windows visibly recovers from its silent timeout. A replacement must report “completion unknown,” not copy this silent success-looking failure policy.

**O, motion distinction:** selector3 starts motion calibration; no separate 3E motion End call was proved. Windows motion UI waits2000ms and declares completion (`0x180d12610 → 0x180d12430`, `ev_4f1b4543ad6798baa002ca6c06d81065a3331f86d338161d4586586b5d8d0664`). Do not invent End(3) from the factory accepting an integer.

### Receiver and BLE are not the same Windows path

**O/U:** Windows launches calibration through GIP queue even with its HID backend selected. As documented in maintenance, its resulting 3024 raw report would start `81 04 OP 00 PP 00…`, whereas the known-working receiver config wrapper is `81 10 04 OP 00 PP 00…`. Neither is calibration-verified on3024. Live GIP success resolves the wired route, not this receiver serializer/queue mismatch. Do not send either receiver candidate just to see which changes calibration.

## 2. Android calibration over BLE

**O: yes, this APK has a model-specific BLE calibration path. U: acceptance/persistence on this 3E has not been live-verified.** Stick/trigger activities explicitly call `HIDChannel.write3ECalibrationStart(1/2)` and End(1/2) under `PIDVID.is3E()`. JNI exports at ELF `0x312c0 / 0x314f0` call factories `get3E_WriteCalibrationStartChannel` `0x2afb0` and End `0x2b090`. The factories store opcode0e/0f, caller u16 parameter, channel4, zero count/total/offset, no data, retry limit2. JNI dispatch goes through `baseCumstomChannel` `0x27f40 → Generate_CustomDataCmd 0x27280 → Send_CustomCmdPacket 0x28080 → WriteBleHidData 0x282a0`; latter calls Java `HIDChannel.WriteBleHidData` (`0x372220` DEX), which selects `BLEUtils.GattCallback.writeBLE → SHBLEUtils.writeBLE` except for a different model's USB branch. This is an actual app route, not merely compatible-looking inner opcodes.

**O/I exact ATT characteristic values for selected type2/2B11**, with static framing recovered in MOBILE and factory bytes directly disassembled above:

| Operation | Exact 62-byte value |
|---|---|
| Stick Start | `81 11 04 0e 00 01 00 || Z(55)` |
| Stick End | `81 11 04 0f 00 01 00 || Z(55)` |
| Trigger Start | `81 11 04 0e 00 02 00 || Z(55)` |
| Trigger End | `81 11 04 0f 00 02 00 || Z(55)` |
| Motion Start | `81 11 04 0e 00 03 00 || Z(55)` |

`11` is unpadded inner length+1; it is not ATT value size. No channel byte is stripped from TX. Type2 has no packet CRC. The actual characteristic is `00010203-0405-0607-0809-0a0b0c0d2b11` under service ending1910, selected by `SHSupportDevicesTools`; handles must be discovered. Use app-preferred write-without-response and notifications; ATT write success does not prove calibration success. For the alternative FF13/type1 encoder only, replace outgoing bytes9/10 with `ff ff` (empty-payload CRC16), leaving all other bytes unchanged. This does not establish that this 3E uses FF13.

**O expected reply parser; U actual calibration packets:** `readBleHidData` `0x28c90` [MOBILERX] skips first2 bytes for PID202f, matches kind4, echoed command low byte and total0, then `handleChannel` `0x28810` [MOBILE] ends timer and calls completion. Expected minimum matching reply is `?? ?? 04 00 0e 00 || Z(12)` for Start, or opcode0f for End; count/total/offset0. Outer two bytes, padding, and exact calibration status remain uncaptured. Do not require the outgoing `81 11 04` wrapper on receive or treat absence of a reply as success. Native success callbacks `0x311b0 / 0x313e0` invoke Java `writeCalibrationStartDone / writeCalibrationEndDone`; overtime `0x31250 / 0x31480` invokes corresponding `OverTime` methods.

**O timing:** factory and JNI both set float2000 (`0x44fa0000`) in Channel+0x64. `Channel::start` `0x35940` converts it to int and calls Java `HIDChannelTimer.start`; DEX `0x371f74` uses milliseconds in `Timer.schedule`. Thus **2 seconds per attempt for both start and end**, retry limit2, unlike Windows'10/100 seconds. `Addedovertime` `0x27a90` calls the overtime callback before deciding whether to resend, so the UI overtime callback can run before retry exhaustion. Android stick/trigger UI polls every20ms. No required inter-command dwell or per-user-action deadline was found.

**O user sequence:** 3E `StickCalibrationActivity.RefreshStep` (`0x336328`) uses the same cardinal states1..26, ±120/±29 and squared-radius≥16129; final RS-top requests End(1). Trigger `RefreshStep` (`0x3465b8`) uses the same LT255/0 twice then RT255/0/255 and requests End(2). Unlike Windows adapter deadzone123..131, `StepActivity.readKeyWith` (`0x332188`) subtracts127 and only centers a stick when both coordinates are127 or128. Do not silently mix their input normalization. Android remains at final stick26/trigger7 until EndDone, so repeated20ms polls can request End again before the ack; this is recovered behavior, not a recommended probe/retry policy.

**O completion/failure:** Stick StartDone `0x3367e4` starts state1; EndDone `0x336798` runs success UI/state27. Trigger equivalents `0x3466d0 / 0x346684` start state1 and complete state19. Their OverTime handlers clear busy/dismiss the wait dialog; they do not send rollback or restore old calibration. Motion activity sends Start(3), then waits2000ms before showing success, not a measured quality result. No BLE-specific calibration store readback or accuracy/result structure was recovered.

## 3. Abort, restore, and backup safety

**Calibration writes persistent physical calibration or enters a state that can change it. The exact commit point is unproven. Do not send Start, End, a guessed reset, or a power-cycle as an exploratory battery/read-only probe. A normal 868-byte profile backup does not establish calibration recoverability.**

| Question | Evidence-bounded answer |
|---|---|
| Abort/back midway | **O:** Android Stick/Trigger `setBack` and back-click handlers only finish the activity; `onDestroy` removes delegates/cancels UI timer (DEX offsets above). They send neither End nor cancel/rollback. Windows page-change helper `0x180c512b0` unsubscribes notifications; cited starts/end state machines contain no cancellation packet [UI/CAL and maintenance routing audit]. **U:** device state on abandonment/disconnect, auto-timeout, discarded vs already-written samples. |
| Can End safely cancel? | **U.** It is the completion command, not a recovered abort operation. Sending it before the required movement could accept incomplete calibration. Never use it as an unconditional cleanup action. |
| Power-cycle recovery | **U.** It may leave or exit the mode; neither restored factory calibration nor rollback is proved. Recovery of an idle GIP config link by power cycle is a separate live fact and must not be generalized to interrupted calibration. |
| Factory reset/restore | **O:** Windows ResetView's 3E path constructs three default profiles then uses ordinary profile write1 + commit6 (`0x180d16e70 → 0x180c818d0`, details in [spec-reset.md](spec-reset.md)). **L:** this profile factory reset is verified. **U:** a distinct physical-calibration reset/restore opcode or firmware recovery sequence. Do not advertise “Restore factory profiles” as restoring factory stick/trigger calibration. |
| Is calibration inside868 bytes? | **O:** full-profile marshal `0x1808e8cd0 / 0x1808e9590` and [spec-profile.md](spec-profile.md) describe header + three mapping/stick-response/trigger-response/vibration/light/motion profile records. Calibration factories carry no image data and their success callbacks do not read/modify/write that image. **I:** physical calibration is controller-global/separately stored, not a profile curve field. **U:** firmware storage layout and proof that every opaque profile byte is unrelated to calibration. No before/after calibration image comparison has been performed. |
| Can separate calibration be backed up? | **U:** no calibration-data read opcode, address, size, checksum, restore command or schema was recovered in these paths. Opcode2/total868 backs up profiles only. No safe calibration snapshot/rollback procedure can currently be supplied. Do not probe arbitrary offsets/totals to look for one. |

Consequently, shipping a one-click calibration action remains unsafe without explaining that it is destructive and rollback is unavailable. A full profile backup is useful but is **not** a calibration safety net. No automatic retries of a state-changing command beyond a deliberately chosen policy; a lost ack can mean the write occurred.

## 4. Battery and charging

### Android: bytes28/29 are not a supported 3E battery API

**O, exact path:** BLE notification → Java `HIDChannel.readBleHidData` → native `0x28c90` [MOBILERX] → for unsolicited kind4/cmd3, copy **12 bytes** starting at full BLE byte18 into `HIDChannel.readKeysAndJoyBlock(byte[])` → each `ReadKeysAndJoyInterface.readKeyWith`. Thus callback byte10/11 correspond to **full BLE bytes28/29**, not receiver raw28/29.

**O:** `StepActivity.readKeyWith` DEX `0x332188` decodes keys0..3, sticks4..7 and triggers8/9. It then explicitly does:

```java
if (PIDVID.is3E()) return;
Const.battery = bArr[10];
```

`StepActivity.read3EKeyWith` DEX `0x331e44` independently reads full-frame keys18..21, sticks22..25, triggers26/27 and ends without using28/29. Native forwarding does not itself assign battery semantics; it copies both bytes unchanged.

**O important exception:** generic `MainActivity.readKeyWith` DEX `0x2f8bd0` assigns `Const.battery = bArr[10]` **without the 3E guard**. Therefore “no APK code ever reads byte28” would be false: the generic consumer can read it as a battery byte. However this is not a validated 3E decoder: the 3E-aware Step consumer deliberately skips it, the generic Main method also uses the older trigger order, and **SettingActivity hides the indicator specifically for3E**. No consumer of callback byte11/full byte29 was found in application activity/utils/mode code; no charging-bit interpretation was found there. These are bounded source-search results, not firmware feature absence.

**O Android UI:** `SettingActivity.initSetting` DEX `0x31a064` creates battery icon, `Battery_View`, percentage text; its `if (PIDVID.islashen() || PIDVID.is3E())` branch sets all three to `View.GONE` (8). `timer1` DEX `0x31b784` otherwise renders `Const.battery + "%"` and fill width `battery*16/100` (scaled UI units). `Const.battery` initially0. **No 3E percentage/charging read opcode was recovered:** UI uses pushed generic input, not a battery-specific getter; exported native calibration/report/key inventory has no battery getter. Hidden UI plus the explicit3E guard is strong evidence that this APK does **not support displaying 3E battery**, not evidence that byte28 is a percentage. Leave28/29 raw/opaque until firmware or controlled correlation proves otherwise.

### Windows and WinRT

**O:** WINMD declares `D4XDevice.TryGetBatteryReport()` via `Windows.Gaming.Input.IGameControllerBatteryInfo`; member reference token `0x0a00001f`, signature hash `8cacfe34e51159510540f56ae1aae1dfe6317312aa902b66a82c3f3f20016c71`, parameter count0. WinMD SHA-256 `b4af28b7a0a275d6aff3bf6cff260ec2cbf026827b7f8381f7ba4e409a4dccd3`, MVID `1859daaa-e6d9-475b-b28d-9377824f6658`. This is platform battery reporting, not a proprietary BLE opcode. [Microsoft API contract](https://learn.microsoft.com/en-us/uwp/api/windows.gaming.input.igamecontrollerbatteryinfo.trygetbatteryreport) permits a null report when unavailable; capacity fields can also be unavailable.

**O/U:** D4X provider forwarding/send/input functions `0x180001ad0 / 0x180001800` [D4X] expose custom messages; the inspected Windows calibration/input paths `0x180cb34d0` [INPUT] and `0x180dbb9d0 / 0x180dbb730` [INPUTUI] consume keys/sticks/triggers, not a recovered3E battery field. **Unresolved:** exact native D4X battery-interface forwarding thunk/vtable, whether UWP actually calls TryGetBatteryReport for3E, and what WinRT returns for this controller. The generic interface declaration alone proves none of these. No3E battery opcode or Windows percentage/charging formula was recovered. A future Windows read-only API call should record null vs report plus Status/RemainingCapacity/FullChargeCapacity, not infer a percentage merely because the method exists.

**L:** receiver pad reports have inner length0x14; raw[22..32] is stale residue from earlier replies. Values raw28=`a0/20/0d` are not status. This finding says nothing about the *different* full BLE bytes28/29 and does not identify either as battery.

### GIP standard status and xone

**O, upstream source, pinned to installed-package research:** [dlundqvist/xone v0.5.7, commit a16304ccdecc5bfc7aafdeb759c706da314d15fa](https://github.com/dlundqvist/xone/tree/a16304ccdecc5bfc7aafdeb759c706da314d15fa). [bus/protocol.c](https://github.com/dlundqvist/xone/blob/a16304ccdecc5bfc7aafdeb759c706da314d15fa/bus/protocol.c), `gip_pkt_status`, `gip_handle_pkt_status` (lines1190ff), parses standard **System/internal GIP command03**, at least4 payload bytes:

```text
03 OPTIONS SEQ LENGTH || STATUS U1 U2 U3 [optional additional bytes]
STATUS bit7      connected; clear → xone removes client
STATUS bits3..2  battery type: 0 none, 1 standard, 2 kit; 3 undefined here
STATUS bits1..0  level: 0 low, 1 normal, 2 high, 3 full
```

Enums: [bus/protocol.h](https://github.com/dlundqvist/xone/blob/a16304ccdecc5bfc7aafdeb759c706da314d15fa/bus/protocol.h). Unknown status bits4..6 and U1..3 are not decoded as charging. This is neither standard input20 nor vendor12 payload `04 00 03…`; all three can coexist.

**O:** `gip_handle_pkt_status` calls gamepad driver's `ops.battery`. [driver/gamepad.c](https://github.com/dlundqvist/xone/blob/a16304ccdecc5bfc7aafdeb759c706da314d15fa/driver/gamepad.c), `gip_gamepad_op_battery`, forwards to `gip_report_battery`; its probe **unconditionally calls `gip_init_battery` before creating input**, not only after receiving a battery-bearing packet. [driver/common.c](https://github.com/dlundqvist/xone/blob/a16304ccdecc5bfc7aafdeb759c706da314d15fa/driver/common.c), `gip_init_battery/gip_report_battery`, registers `power_supply` initially UNKNOWN, exports **CAPACITY_LEVEL, not percentage CAPACITY**, maps four levels, typeNONE→NOT_CHARGING/unknown capacity, other types→DISCHARGING. This implementation does **not detect CHARGING from kit type or unknown bits**.

**L:** no `power_supply` entry exists for3E on any link, and BlueZ exposes no battery percentage. Accept these observations; no hardware recheck performed. **Important inference limit:** absence of a power_supply entry does not prove missing GIP03 battery bits. The pinned xone source creates its entry even without status data, so absence is not explained simply by “3E never sent battery.” Actual loaded-driver/build/sysfs exposure must be reconciled independently, without contradicting the live fact. Likewise, the source parser's existence does not prove3E emits03, nonzero type, useful level, or charging. **U:** this 3E's actual standard-status bytes; no retained3E03 capture was supplied in the research sources.

## 5. Safest first parent-owned live probes

These are proposed experiments, **not executed or authorized by this document**. Passive probes first. Do not reset, switch receiver modes, stop xone, or start calibration solely to discover battery. Link takeover and CCCD/report changes are not passive, even when no persistent configuration is written.

| Feature/link | Safest first probe, exact transmitted bytes | Expected observation/reply; limit |
|---|---|---|
| GIP battery | Passive usbmon observation with xone retained; **TX none**. Capture through an already-intended user reconnect if steady-state has no03. | Standard03 status shape above; illustrative unfragmented primary internal header is `03 20 RR 04 || B U1 U2 U3`, **not a predicted3E value**. Record actual options, length, B and extra bytes; require ≥4 status payload bytes. No03 during a bounded window→unobserved, not absent capability. Decode only documented type/level; no synthetic percent or charging. |
| Windows battery | Call `TryGetBatteryReport()` read-only on selected3E game-controller object; **no custom message bytes to send**. | Null or Windows BatteryReport with optional capacity fields; actual3E result U. The OS may internally communicate; no raw USB request is recovered here. |
| BLE battery | If config notification stream already owned, receive-only and log complete kind4/cmd3 reports; **TX none**. Compare raw28/29 only alongside independently known charging/charge conditions; a single value cannot identify semantics. | Existing physical frame starts `81 1c 04 00 03 00 || Z(12)`; keys18..21, axes22..25, triggers26/27, raw28/29. No battery reply/percentage can currently be required. No dedicated battery opcode is known; do not send guessed ones or poll nonexistent Battery Service handles. |
| Stick/trigger GIP prerequisite | First receive-only: confirm vendor12 `04 00 03…` input and correct normalized endpoints while **not calibrating**. If vendor reporting is unavailable, an explicitly transient enable is `13 00 SS 3c || 04 07 00 10 01 || Z(55)` (64 bytes). | Expected generic vendor12 ack header `04 00 07 00 || Z(12)`, then physical input at payload20..25. This command is live-proven reporting/config infrastructure, not proof calibration works. Don't disable reports before calibrating; the state machine needs them. Restore prior reporting mode only if changed. |
| Stick/trigger BLE prerequisite | Reuse owned/subscribed2B11; if necessary enable reports with `81 11 04 07 00 10 01 || Z(55)` (62 bytes, type2). | Matching kind4/echo7 notification header at offset2; existing physical64 frames as above. CCCD setup and empirical2-second app-readiness delay are separate existing BLE prerequisites, not calibration dwell times. Confirm axis/trigger data before any destructive Start. |

**First actual calibration transaction, only after a deliberate parent/user decision:** use **one** recovered Start(1) or Start(2) on the chosen proven link, wait for its matching vendor ack before asking the user to move controls, follow the full sequence above, send its matching End only after gates are met, and require its vendor completion ack. Exact frames and expected minimum reply headers are in sections1/2. No read-only command can prove that a calibration Start/End will be safe. Since rollback/store backup is unknown, **there is no currently evidence-backed “safe abort halfway” experiment**. On lost ack/disconnect, report uncertain completion and retain raw evidence; do not silently repeat Start, finish partial calibration, issue profile factory reset, or promise that power cycling restores the old calibration.

## Result boundary

**Closed statically:** exact3E GIP/BLE start/end frames, selectors, routes, user gates, timing, generic ack semantics; Android deliberately hides3E battery and skips its generic battery byte; xone's standard status decoding is four-level and not charging/percentage reporting. **Still unproven:** live calibration on any link, exact calibration ack/error payloads, abort/recovery/physical-calibration backup,3E battery/charging semantics in BLE bytes28/29, actual3E GIP03 status, WinRT battery result, and receiver3024 calibration routing. No capability should be advertised beyond those boundaries.
