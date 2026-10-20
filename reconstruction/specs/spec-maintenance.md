# Ultimate 3E maintenance/device-service specification

Scope: shipped Ultimate Software X 1.4.9.0, `UWPAdvance.dll` SHA-256 `1f2325902b5d3ca2cc7b44405243247d3f276c39f99a66c97b193545be038352`, supplied full native IR. Code read-only; specification updated. No controller I/O, firmware download, flash, reset, build, or tests performed. `O` = observed static code, not observed controller execution; `R` = retained read-only HTTP observation; `I` = inference; `U` = unresolved. UI presence is not proof its GIP command works through receiver3024 or Bluetooth.

## Evidence shorthand

Every shorthand below denotes the complete Evidence ID; addresses identify the individual function within that batch.

| Alias | Evidence ID |
|---|---|
| CAL | `ev_6784a1aee5aaccebeb1a1986c9aed07b3da8d79e15cbf78e3bac9753fb4be8b6` |
| CALUI | `ev_e8dd03488a043a96f2c19ef96e229bd1e7c239c5f922779afc0ad6f1e60b9801` |
| CALSTATE | `ev_b465b7ec19fcdf68a71a6ecc2e079205eaf0fb34411c44f483b56d0e6818b61b` |
| MOTIONUI | `ev_4f1b4543ad6798baa002ca6c06d81065a3331f86d338161d4586586b5d8d0664` |
| UI030 | `ev_030024085343b462a4b9def7759197b7a31d9a33fff614d53dee21ff24c068e9` |
| SERVICE | `ev_a7c54ad48e1904243f4e2961d9a75c5e3e22653212ed980d869fdd2b5394ac62` |
| SERVICE2 | `ev_bf2d765510b165465516f7cc43ff16742ca8132510d1555563538cbec7cf3ca7` |
| HIDQUEUE | `ev_141ebcaeab4cf5f83a08a8f72aa99685cbce008b31880266775e209b859b5881` |
| BOOT | `ev_e2c1bb70af3bd49debb62270da6c9c2c12e6002b9999ce601c7ccbdd2919d71c` |
| BOOTWIRE | `ev_ebdc5977288a0e584a213e4a1087fdb677f89b960bfc5eb5d354cfc8510751f7` |
| BOOTUI | `ev_995b61fb2d386871a2381c093e0ecf87c8977cc598711f89669bb66531eae6d0` |
| FILE | `ev_fa2a3d9b17b01ea0b053e4eb81b6151e6bd92e8f2e68b0e01917e28416915e50` |
| HTTP | `ev_facb6b75d321af6c137ff4df87472751c8d436e85e03b7853cad5791832772ae` |
| UPDATEUI | `ev_dc088566f316c976f0c12d168ab4cd2b78d1c1ab867671af93306a00044dcf42` |
| RECEIVER | `ev_d8af8df7e5b2e999018e706966432d5dbad6401f998b73125b730ac5ae5525ec` |
| RX | `ev_eb24534b10694e07513f3199072969fa0bd9f4773ffe06f575a25e5f5a7adae2` |
| TX | `ev_d2692710de786f469fdb89e7ca96057ca2a9c1e36c209d759ab7df48f991566f` |
| RESTORE | `ev_bfeeb44dccaf09bbf1998f3e22f24c1bbbeca811eb47433d5ab24c0eba5ac8f8` |
| DEFAULT | `ev_a13d5baaa72edca91d65a8d89eded7946a08bc83cf2abada83dc24e447274864` |
| HEADSET | `ev_94b344795cbeeac843e5ad7e9404a8bd2f0294b5747e4b82be243c31d63e7e26` |
| HEADSETCTOR | `ev_7e4805ad42512bd0d9b63a03486e66e4baeab5f09e99fb8765c566c8126ba9e8` |
| VIB | `ev_5847cc7002766b2f802e2c0eb50a9a04c37cf5a4950c50936dffaba95447f5b6` |
| VIBSTATE | `ev_de366c57126de24bc5aa2936d3e6041a84dab1f31894403389966ae2b7d37feb` |
| TEST | `ev_95b46a1a071025c2a277fa810b6519a093544be7f61268f18274777141260fa2` |
| D4X | `ev_8b9eeb346c176a2fc1a905dd8415c84b0b5ddb4553dce8ecb77e46cf3d0608ac` |
| WINMD | `ev_f9911df72ea3446360e14fcf4f2cc0324adec9f3538f95d01fee3f97279d38a0` |
| FWASM | `ev_ac86e1f45e8d1187c1b43b6aaad786000b37f3a1f865d9a89f50d23d1053788e` |
| FWLOOP | `ev_4f33daac815919af7748dd8b29c2cf61bafbc0bd4be72666b5c9575ad84d8673` |
| FWTABLE | `ev_113fffe14567128a277108b01d32588f85455f46555dce0845bc556f9c573fd5` |
| WATCHTHUNK | `ev_0c41099cca6b454507f862498577b9180e9fe03811e4b0fa24b28dd9064fcc05` |
| RESETCONNECT | `ev_591f1c115e311268967d14a02338929f49275223ebf5032ea589f68ab79106a1` |
| RESETBYTES | `ev_8dbb3b7313eb43b73a8d74660808cb420d34172fbbb32bad0c4e251eecac97ee` |
| PROFILEINIT | `ev_4f2cf40de1612289b89c2c2d3cfdefc0cb480302ce2f02723b27aef21f160553` |
| SAVE | `ev_39b0c322e175c84dc4960238d244bd1606504337470bdca4d905deab3f3afd8d` |
| COMMIT | `ev_5219b564132d5009f8c50d6372f5534a2a887f820dbd3e944c76608594245243` |
| RESETCONTINUE | `ev_93ebe7869f5029053f0be92976d2129388189d0d2bf8ab59ca53c87ae6d53137` |

Model condition throughout: current logical PID at model static base `+0x48`; 3E constant at `+0x3c` (logical `0x202f`). Do not compare physical receiver PID `0x3024` against logical PID.

## 1. Calibration

### Command matrix

`H(op,p) = LE16(op),LE16(p),LE32(0),LE32(0),LE32(0)`, no payload. Factories store channel4. **Calibration launchers use GIP queue even when receiver mode is selected; they do not select the working receiver configuration queue.** See routing audit below; inner headers alone do not establish safe receiver calibration. Timing field `command+0x54` is seconds: `0x180c4b7a0` [TX] multiplies float by1000 before creating timer. `+0x2c` is configured retry limit, not duration or wire field.

| Feature/model | O: start | O: end | O: launch timing / result |
|---|---|---|---|
| **3E sticks** | `H(0x000e,1)`; `0x180d13b90` [CALUI] → `0x180ca6170` → factory `0x180ca5d30` [CAL] | `H(0x000f,1)`; `0x180d28010` [CALSTATE] → `0x180ca5fc0` → factory `0x180ca5af0` [CAL] | Start10s/retries2; end100s/retries2. Start success `0x180ca6320` emits `Notification_StartCalibration`. End `LAB_180df1440` is an original-PE `E9` tail jump to `0x180ca5860` [CAL] → `Notification_EndCalibration`. |
| **3E triggers** | `H(0x000e,2)`; `0x180d13a40` [CALUI] → `0x180ca5b80` → `0x180ca5d30` [CAL] | `H(0x000f,2)`; `0x180d27df0`/`0x180d27f10` [CALSTATE] → `0x180ca5940` → `0x180ca5af0` [CAL] | Start10s/retries2; end100s/retries2. Original-PE tail jumps `LAB_180df1418`→`0x180ca55d0` and `LAB_180df1400`→`0x180ca4e20` [CAL] prove `Notification_StartCalibrationTri` and `Notification_EndCalibrationTri`, despite different3E opcode. |
| **3E motion/gyro** | `H(0x000e,3)`; `0x180d13af0` [CALUI] → `0x180ca5dc0` → `0x180ca5d30` [CAL] | No explicit separate end packet proved for motion UI | Start10s/retries2; success `0x180ca5f70` [CAL] emits `Notification_StartCalibrationAxis`. Axis here means **motion calibration**, not stick path; page constructor says `Calibrate Motion Controls`. |
| L5 family, NOT 3E | `H(0x0060,p)` `p=1 sticks,2 triggers,3 motion`; factory `0x180ca5390` | `H(0x0061,p)` factory `0x180ca5100` | `0x180ca5190`:10s/retries2, success `0x180ca5340` → StartCalibrationL5. `0x180ca4f00`:100s/retries2, success `0x180ca50b0` → EndCalibrationL5. [CAL] |
| Legacy triggers, NOT 3E | `H(0x003a,0)` factory `0x180ca5620` | `H(0x013a,0)` factory `0x180ca4e70` | `0x180ca5420`/`0x180ca4c70`: start10s/end100s, retries2; success `0x180ca55d0`/`0x180ca4e20` → Start/EndCalibrationTri. [CAL] |
| Legacy sticks, NOT 3E | `H(0x0036,0)` `0x180ca6760` | `H(0x0038,0)` `0x180ca58b0`, or `H(0x0136,0)` `0x180ca6520` | Model-dependent alternative end dispatch in `0x180d28010`; do not reuse for 3E. [CAL,CALSTATE] |

Notification string map initializer `0x18071ff10` (`ev_16d3a5bb323bdf1261c88a7959f69521325a1a32ba6a150def23e1df0ba63561`): StartCalibration/EndCalibration, Start/EndCalibrationTri, Start/EndCalibrationAxis, Start/EndCalibrationL5. **EndCalibrationAxis is declared but no producer/consumer was found in complete function-text search**; no invented motion-end operation.

### User workflow and validation

| Feature | O: user instructions / exact state validation | Completion / risks |
|---|---|---|
| Sticks | Do not touch sticks before Start (`0x180d15b20` [CALUI]); Start ack initializes state1, displays `Push the LS to the top edge, then rotate it clockwise 3 times slowly` (`0x180d28eb0` [CALSTATE]). `0x180d28010` [CALSTATE]: left states1/5/9/13 accept top;2/6/10 right;3/7/11 bottom;4/8/12 left. On state13 top, prompt right stick, state14. Right states14/18/22/26 top;15/19/23 right;16/20/24 bottom;17/21/25 left. State26 top sends `0x0f,1`, advances27. Each cardinal gate requires squared radius `x*x+y*y > 0x3f00` (=16128), major component `<-120` or `>120`, minor component `-29..29` inclusive (`minor+29 unsigned <59`). This describes normalized UI inputs to function, not raw i16 receiver values. | `0x180d28850` [CALSTATE] zeroes state and displays `Joystick calibration complete`, reenables Start. `0x180d28eb0` clears terminal state27 for3E (other models19). No elapsed-time rotation deadline found. Calibration changes persistent hardware center/range: keep device connected, disable competing writes. No readback/power-cycle acceptance observed. |
| Triggers | Do not touch triggers; trigger mode switch must be OFF (`0x180d14cc0`/`0x180d14e80` [CALUI]; exact prompt also `0x18063ad30`). Ack initializes state1 and `Please press the left trigger to the bottom twice` (`0x180d28da0` [CALSTATE]). `0x180d27df0`/`0x180d27f10`: LT 255→state2;LT0→3;LT255→4;LT0→5 and right prompt; RT255→6;RT0→7;RT255→8 sends end `0x0f,2`, zeroes state. **Final RT release is not required by this host state machine.** | `0x180d28b00` [CALSTATE] displays `Trigger calibration complete`, reenables Start. Exact endpoints required, not thresholds near255/0. No per-step deadline found. Hardware limits change; no calibration cancellation/rollback packet proved. |
| Motion | Place controller flat, joystick side up; do not touch during calibration. `0x180d14030` [CALUI] explicitly labels `Calibrate Motion Controls`; `0x180d13af0` sends3E start param3 and changes prompt to `Calibrating. Do not touch the controller during calibration.` | `0x180d12790` → `0x180d128c0` shows loading overlay; worker `0x180d12610` sleeps **2000ms**, then `0x180d12430` displays `Motion calibration complete` and removes overlay via `0x180d12580` [MOTIONUI]. **This UI completion is a timer, not evidence that a device accuracy/self-test succeeded.** No numeric variance/quality validator found. |

Applicability: O: 3E branches explicitly select all three starts. Calibration menu `0x180d182d0` [UI030] offers Joysticks/Triggers/Motion. L5 opcode60/61 and old opcode36/38/3a/13a are alternative model dispatch, not3E. O: `CalibrationAxisView` subscribes StartCalibrationL5 and StartCalibrationAxis (`0x180d13ef0` [CALUI]); StickView subscribes Start/EndCalibration and L5 variants (`0x180d15b20`); TriggerView subscribes Tri/L5 variants (`0x180d14cc0` [CALUI]). Shared success notifications above are proven through original-PE tail-jump decoding into [CAL] functions.

### Receiver routing audit — gate remains BLOCKED

**Calibration changes device calibration; neither packet candidate below is authorized or validated for receiver calibration. Do not transmit either merely to discover which works.**

O: `0x180ca6170`, `0x180ca5fc0`, `0x180ca5b80`, `0x180ca5940`, `0x180ca5dc0` [CAL] set channel4 and unconditionally call `0x180c7f2f0` for that channel. That queue (`ev_5219b564132d5009f8c50d6372f5534a2a887f820dbd3e944c76608594245243`) stores commands in `DAT_1805b79e0`, starts timer, serializes through `0x180c4aef0` [TX]. There is no receiver-mode selection in these launchers/queue. In contrast the receiver's established channel4 queue is `0x180c4d7d0` [RECEIVER], `DAT_1805b7908` → `0x180c4bf40` [TX] → `0x180c4c260` [RECEIVER].

O: `0x180c4aef0`/`0x180c4b240` [TX] inspect mode at static descriptor `PTR_FUN_1805c1a88`: false→60-byte GIP buffer via D4X `0x180c4b340`; true→65-byte intermediate HID buffer via `0x180c4b440`. **This selection changes output backend, not queue or receiver length framing.** `0x180c4b550` [TX] prepends `81` and copies only first31 intermediate bytes for logical3E. Its GIP-derived intermediate begins `04 || H(op,p)`, unlike receiver serializer's `10 04 || H(op,p)`. This is a static serializer discrepancy, not proof Windows successfully calibrates through3024.

| Candidate | Exact raw32 layout, hex | Status |
|---|---|---|
| Shipped calibration launcher → GIP serializer → HID backend | `81 04 OP 00 PP 00` followed by **26 zero bytes**, `OP=0e` start /`0f` end;`PP=01` sticks /`02` triggers /`03` motion-start | O: bytes derived from cited serializer chain; **device acceptance unverified**. Not `receiver::frame()`. |
| Established receiver framing applied to calibration inner header | `81 10 04 OP 00 PP 00` followed by **25 zero bytes**, equivalently `receiver::frame(op,p,0,0,0,&[])` | I: exact framing composition from `0x180c4bf40`/`0x180c4c260`/`0x180c4b550` and `src/receiver.rs::frame`; **no calibration callsite selects it**. Not a recovered Windows calibration transmission. |

O: start timeout10s/end100s, retry limit2, current retry0. No report-disable call is present in inspected Start handlers `0x180d13b90`/`0x180d13a40`/`0x180d13af0`, subscription/setup `0x180d15b20`/`0x180d14cc0`, or above command launchers [CALUI,CAL]. `0x180c512b0` [BOOTWIRE], called while changing pages, only unsubscribes notifications; **it is not opcode7 report-disable**. This bounded path does not establish that no unrelated earlier operation disabled reports.

O: receiver reply parser `0x180c4e6f0` [RX] handles kind4 with header at raw[2] (LE16 kind4, LE16 echoed opcode) and looks up `0x180c4f0e0(echo)` in receiver queue; kind5 uses header at raw[3]. Completion `0x180c4ea70` [RX] advances offset+count then invokes command success delegate. Calibration instead registered in GIP queue `DAT_1805b79e0`; consequently **generic kind4/echo0e or0f is only a candidate receiver acknowledgement, not a proved calibration success path**. Exact device ack/status and restore/cancellation behavior remain U.

### Calibration input source — not the receiver i16 branch

O: modern stick `readKey` adapter `0x180dbb9d0` and trigger adapter `0x180dbb730` (`ev_aec5c91d8f726fcf919490f58c0b11732481600a040b81ab4c2ced68afa418dc`): logical3E(+3c) orL5(+38) reads axes **payload[20..23] as u8**, coerces123..131 inclusive to127, then passes `(LX-127,LY-127,RX-127,RY-127)` to `0x180d28010` [CALSTATE]. No Y inversion in this branch. Triggers are payload[24]/[25], unscaled u8, into `0x180d27df0`. These are the numbers compared against squared-radius0x3f00,±120,±29 and trigger255/0—not normalized receiver i16s.

O: GIP input callback `0x180cb34d0` [BOOT] forwards D4X message payload unchanged to `Notification_ReadKey` for3E/L5 when prefix is `04 00 03`; message ID must match current configured ID (SpecGip identified0x12). Thus offsets above belong to **vendor GIP payload**, not standard GIP0x20 input packet nor Bluetooth report. No Bluetooth input-origin chain proved here. Receiver callback `0x180c4e5f0` [RX] instead publishes unchanged `81 14 …` raw32; receiver opener `0x180c4e070` [RECEIVER] explicitly sets logical model3E, so same handler selects the u8 branch even for that raw32.

O: only the adapter's **non3E/nonL5 fallback** reads i16 at raw6/8/10/12 and computes float32 then truncates toward zero: `x = trunc(i16/32768*128)`, `y = trunc((~(i32)i16)/32768*128)`; same for right stick. It calls state machine **before** UI-only deadzone±4. The complement is `-i16-1`, not simple negation. I: this is a usable reference transformation for the live receiver axes, but attributing it to Windows3E receiver calibration would be false. **Closed:** host-state-machine input definition and source discrepancy. **Open:** safe receiver calibration route, ack and validated i16 adapter; BT calibration transport.

## 2. Version and firmware

**DESTRUCTIVE: firmware entry/erase/write/verify/finalize can replace controller/receiver component code and leave an unusable device if interrupted. No flash-ready claim. No controller writes authorized/performed.**

### Version reads

O: **older-model opcode100 alternative, NOT3E component decoder**: factory `0x180ca8a90` [SERVICE2] opcode`0x0100`, parameter0, offset0, channel4, read flag1, count=total5. Original-PE static `PTR_PTR_1805b7980`→VA`0x180653a90`, length+8=5, initial byteszero. Launcher `0x180ca8850` [SERVICE2]: timer2s/retry999; success `0x180ca8a50`→`Notification_ReadVersion`. Generic GIP `0x180c7f2f0` or other HID queue `0x180c4ce70`; not proven3024 serializer. `0x180ca1120` [DEFAULT] uses this5-byte buffer only for model+0x38 (Ultimate3), not3E.

O: boot read launcher `0x180cada70` [BOOTUI] → factory `0x180ca98c0` [SERVICE]: opcode5, parameter0, address/offset from boot static base+0xc, count=total=**4** (`PTR_PTR_1805b7968`→original-PE VA`0x1806539f8`, managed length+8=4), channel5/read1. Success `0x180cadc10` publishes `Notification_getVersioninBoot`. Timer2s initial constructor, default retry9999 unless overridden; no consumer of that notification in complete function-text search. This is legacy boot-channel read, **3E applicability U**, not automatically interchangeable with normal opcode100.

O: **actual3E normal component-version read**: factory `0x180caa310` [SERVICE] opcode`0x00e1`, parameter0, offset0, channel5, read1, count=total**40**; original-PE static`PTR_PTR_1805b7978`→VA`0x180653a50`, managed length+8=40. Inner header is `e1 00 00 00 28 00 00 00 28 00 00 00 00 00 00 00`; outgoing payload absent forread. Receiver launcher`0x180caeb20` [HIDQUEUE] queues`0x180c4dbf0` (channel5), timer2s/retries5. GIP launcher`0x180caecc0` [HIDQUEUE] queues`0x180c7f710`, timer2s/retries999. Original-PE`E9` tail jumps`LAB_180df1640`/`LAB_180df1658` both target`0x180ca8a50` [SERVICE2], publishing`Notification_ReadVersion`. Do not send opcode100 as3E version discovery.

O decoder`0x180ca1120` [DEFAULT], explicit model+3c branch: after assembling40-byte response into`PTR_PTR_1805b7978`, reads **payload offsets4,12,20,28,36** (one version byte in each8-byte component record). Each byte is formatted `"X"` (hex digits), then parsed as floating decimal, stored into globals`DAT_1805c1c20`, `1c28`, `1c2c`, `1c30`, `1c34`. This behaves as BCD→decimal forvalidBCD: e.g.byte`0x82` becomes string`"82"` then numeric82, **not130**. A-F nibble input would not parse as decimal; no fallback branch observed. Unknown remaining7bytes/record must be preserved; do not invent semantic component labels. Nonzero fourth and fifth parsed values set global update-mode1 (base+0x20); fourthzero returns early. `0x180d98de0` (`ev_20188487ec5c072f82e76176d451b83e7a30e8efccef7406a0127858d7c587d9`) is main readVersion handler→decoder→cache/UI rehydration/report-enable. Model+1c/+30 use separate32-byte versionbuffer, not3E. Receiver channel5 response fragmentation/matching belongs transport spec; no live version read performed here.

### Update request/response contract

| Item | O: shipped code |
|---|---|
| Default base | `http://dl.8bitdo.com:8080`, static initializer `0x18071fbc0` (`ev_16d3a5bb323bdf1261c88a7959f69521325a1a32ba6a150def23e1df0ba63561`), global `DAT_1805b7580` |
| Actual UI override | `0x180d1d150` (`ev_04269b9cf98a012c2446012d4f65822a2836914da082aee92d2f7ab7381ba500`) and `0x180d2d490` [RESTORE]: English/non-Chinese locale → `https://support.8bitdo.com/restapi/link/v1/client`; Chinese→same `.cn` host. Only when update-check and sync guards idle. Do not ship obsolete8080 as sole endpoint. |
| Model selector | `0x180cc8620` [HTTP]: model+3c → `0x75` =117. |
| Catalog | `POST {base}/firmware/select?firmware_type=117&beta=1`, **empty UTF-8 body**, Content-Type`application/json`; timeout9000ms. `0x180cc97b0` and helper `0x180cc9510` [HTTP]. `beta=1` includes beta candidates, not proof selected image is beta. |
| Response processing | `0x180cc97b0` [HTTP]: deserialize typed root; require root integer at object+8 ==200; list at+0x18 must contain at least one item; publish first item as `Notification_NewFirmwareUpdate`. Empty response routes state5 network error; empty list state6 latest; non200 state7 retrieval error. `0x180cc9ad0` [HTTP] shows corresponding text then sleeps2000ms and hides tip. |
| Download statistics | `0x180cc87d0` [HTTP] constructs `POST {base}/firmware/download?firmware_key={selected key}&platform=6&soft_type=4`, empty JSON body, via same9000ms helper. `0x180cc9720` launches only if downloader's `downloadAttempted` flag true. This is **statistics**, not firmware binary endpoint. |
| Binary download | `GET selected.fileURL`, 1024-byte HTTP read buffer; ApplicationData local directory + URL basename; 1 initial attempt + at most5 retries in `0x180d247f0` [CALSTATE]/`0x180d21dc0` [UPDATEUI]. No binary downloaded in this task. |

R: exact English HTTPS POST above returned HTTP200 during task; schema:

```json
{"code":200,"message":"","data":[{"created_at":"2026-09-21","fileName":"猎户座 3E 无线手柄（XBOX）","firmware_type":117,"firmware_key":"17899777983684755","file_version":"82201938.30","ic_version":"","fileSize":660368,"fileURL":"https://support.8bitdo.com/bd-uploads/files/2026-09/1968bd56b2464a9e9a37c5e2f5f692c6.dat","exists":true,"readme":"...","readme_en":"...","beta":0,"md5":"26072a0ec0b27612ab7b2ab581b641e1"}]}
```

`...` above denotes omitted literal release prose, not an unknown JSON field. Actual response has5 release entries, latest2026-09-21 plus2026-08-27/18/13/04. Latest English release notes: (1) support for saving programmable button mappings; (2) fix reconnect by holding Profile after exiting Ultimate Software V2 mobile; (3) fix Tournament Mode retention after disconnecting wired while enabled in Ultimate Software X. R: obsolete8080 POST returned HTTP200 `{"msgState":1,"error":"","list":[]}`; **different schema**, not a successful current catalog. HTTP observations are unledgered tool observations, not claimed REA Evidence; static format/field consumers cite HTTP batch above. Native metadata also lists all supplied JSON property names at original DLL file offsets `0x3b2b..0x3b2f..` (derived by read-only binary parsing).

### Windows validation: critical distinction

O: `0x180ccb8b0` [HTTP] reads file stream **Length**, converts integer Length to decimal text, passes that text to `0x180ccb630` [HTTP]. That helper encodes supplied string and MD5-hashes bytes, formats lowercase `x2`; comparisons normalize case. **Catalog `md5` is MD5 of decimal file length, NOT MD5 of firmware file bytes.** R: `MD5("660368") = 26072a0ec0b27612ab7b2ab581b641e1`, equal to every660368-byte release's catalog field. Prior local analysis's file-byte MD5 mismatch therefore does not show corruption. Conversely this vendor check does not authenticate contents: arbitrary same-length bytes pass it.

| Boundary | What code actually validates | What is NOT established |
|---|---|---|
| HTTP/cache | Cached-file acceptance: case-normalized `MD5(decimal(fs.Length)) == catalog.md5`. New download: `fs.Length >= response.ContentLength`, then same digest equality; **not equality against ContentLength**. Failure closes streams/deletes local file. `0x180ccb8b0` [HTTP]. | No independent comparison to catalog `fileSize`, file-content MD5/SHA-256, detached signature, publisher certificate, or pinned image hash in this path. HTTPS transport does not convert length digest into authenticity. |
| File ingestion | `0x180cc6920` [FILE] reads StorageFile into byte array `DAT_1805b77c8`, publishes ReadDone. `0x180d246b0`/`0x180d24750` [UPDATEUI] immediately call splitter. | No additional content validation in these entrypoints. File/stream/runtime errors are not cryptographic checks. |
| 3E container | `0x180cc6ba0` [FILE] reads five16-byte outer headers and stores each W2 length; `0x180cc7700` marshals struct. Array.Copy gives runtime bounds checks on each header read, and later transfer copies check their slices. | No3E magic, authenticated signature, PID/version match, block-size/table/footer agreement, checksum calculation, exact EOF, or explicit all-five-body-bounds preflight before E0. Fifth body's end is not checked by reading a sixth header. **Truncated bodies can reach destructive transfer entry before a later runtime bounds failure.** Other-model PID checks must not be attributed to3E. |
| Transfer ack | `0x180cb36f0` [BOOT]: E0/EA/E4/E6 status0; E5 compares all16 returned bytes to current body's final16 bytes [FWASM,FWLOOP,FWTABLE]. | No host-side CRC32/CRC16 algorithm established for3E opaque image contents. Device may validate encrypted/checksummed data; algorithm/authenticity remains U. |
| Offline local inspector (NOT Windows behavior) | Existing `ultimate_firmware.py` checks five-image bounds, outer/inner tag/version/block-size agreement, power-of-two blocks, table geometry, sum(table u32) mod2^32 vs stored total, exact EOF. Existing analysis records opaque payloads. | These extra structural checks are user tooling, not evidence Windows performs them; not payload decryption or signature verification. |

O: integrity-check inventory is bounded to Windows3E download→file-read→splitter→transfer path above, not every model's updater. Container parser's3E branch joins directly to `0x180cb9a80` E0 factory → `0x180c52da0` [FILE,BOOTWIRE]; no intervening image validator. Extra header words and body/footer bytes are passed through, not independently authenticated. Recovery/transport gates remain mandatory even if these weak vendor checks pass; **matching length does not make an image safe to flash**.

Known existing file (not downloaded/rehash-verified this task): `8BitDo_Ultimate_3E_firmware_80201937.30.unverified.dat`,660368 bytes; retained analysis SHA-256 `ff1a859b15cb23488acf6ebf7b64c7016295fef346907f4275c1d770ba9c2d60`. Do not apply that hash to latest82201938.30.

### File format and 3E transfer

O: `0x180cc6ba0` [FILE] 3E branch splits five records:

```
outer_i: 16 raw bytes = four LE u32 words W0,W1,L_i,W3
body_i : L_i opaque bytes, copied unchanged to device
outer_0 offset = 0
outer_i offset = sum_{j<i}(16+L_j)
body_i offset  = outer_i+16
```

O: `0x180cc7700` [FILE] uses runtime struct marshal; literal names/endian semantics beyond LE native platform not recovered in that routine. I/retained file inspection: W0 component identifier, W1 version word, W3 block size. Existing older-file sample: `(W0,W1,L_i,W3)` = `(0x33a,0x180,111104,1024)`, `(0x20d,0x120,26816,1024)`, `(0x33c,0x119,444176,1024)`, `(0x33b,0x137,45312,1024)`, `(0x1ef,0x130,32880,4096)`. O: Windows3E splitter uses L_i at struct+8; no host decrypt step occurs in `0x180c51af0` [BOOTWIRE].

**Transport warning:** firmware stream below is `message-type5 + custom header`, through `0x180c4b340` [TX] → `FUN_1808c6990(DAT_1805b7818, bytes, messageID, ...,0)`, i.e. Windows device-message API object. This is NOT proven receiver3024 `0x81 + length + channel4` firmware transport. A60-byte firmware message cannot be placed unfragmented into31-byte receiver reports. Do not conflate normal receiver discovery's misleading `findBootDevice` event with actual transition into flash bootloader.

Custom boot header uses four LE32 words (16bytes), unlike normal read/write field meanings. Prefix5 is a message type passed as data here, not proven HID report ID.

| Stage | O: emitted bytes / ack / source |
|---|---|
| Enter update | `0x180cb9a80` [BOOT] creates opcode`0xe0`, parameter/channel field0, timeout1s/retry10000. `0x180c52da0` [BOOTWIRE] serializes command+parameter as firstword, word1=`command+0x3c`, word2=`command+0x38`, word3=0; creates64-byte scratch; output60bytes=`05 || first58 scratch bytes || zero padding`. Header-only normal constructor fields are zero. |
| E0 acknowledgement | `0x180cb36f0` [BOOT] requires received buffer[0]==5; parses16-byte header at1; opcode low16`0xe0`, response data byte at17 must0. On3E success copy first42bytes of current component body (offsets16, L0+32, L0+L1+48, etc), then command`0xea`. Nonzero→error cleanup/end command. |
| Select/prepare component data | `0x180cb9eb0(0,58)` + `0x180cb9d00` [BOOT]:58-byte inner buffer; header `[LE32(0xea),0,0,0]`, followed by42bytes from start of selected body. Prefix5→59-byte output. Timeout1s/retry10000. EA reply removes queued EA entries, requires byte17==0, zeros active cursor, creates **five initial42-byte E2 slices without short-body clamping**, then E4 at offset0 and `0x180c530d0`. Erase/prepare semantics I, not recovered literal name. |
| Stream body | `0x180c534c0` [BOOTWIRE]: inner header `[LE32(0xe2),LE32(batch_relative_offset),0,LE32(component_body_offset)] || payload`; payload≤42. Full inner size58, prefix5→59. `0x180c51af0` [BOOTWIRE] copies body slices with +16/+32/+48/+64/+80 outer-header skips for five components. Batch offsets0,42,84,126,168; absolute component offsets advance accordingly. |
| Batch barrier | `0x180c533e0(batch_start)` [BOOTWIRE]:59-byte zero-filled inner array, first16bytes `[LE32(0xe4),0,0,LE32(batch_start)]`; prefix5→60-byte output. `0x180c530d0` [BOOTWIRE] sends up to5E2 messages in increasing absolute-offset order, then barrier. E4 ack first response data byte17 must0; success advances active component cursor by`0xd2`=210, capped to L_i, generates next5 chunks. `0x180cb36f0` [BOOT]. |
| End component data | `0x180c52ee0` [BOOTWIRE]: opcode`0xe5`, param0, timeout1s/retry10000; `0x180c52da0` sends padded60-byte message. **E5 compares all16 received bytes raw[17..33] to current body's final16 file bytes**, not only byte17. Trailer start `T_i=sum_{j<=i}L_j+16*i`; require `reply[17+j]==file[T_i+j]` for every `j=0..15`. Native loop recovery below [FWASM,FWLOOP,FWTABLE]. All equal→E6; any mismatch→failure cleanup/EB. |
| Verify/finalize component | `0x180cb9bc0` [BOOT]: opcode`0xe6`, zero header params, timeout1s/retry10000. E6 byte17==0: stage0→1→2, each next stage sends E0. Stage2 mode!=1 finishes; mode1 sends EB **without advancing**. Only **EB response** then advances2→3 and sends E0; EB-response branch does not check status byte. Stage3 E6→4/E0; stage4 E6→finish/EB. Nonzero E6→failure. `0x180cb36f0` [BOOT]. |
| End session | `0x180cb9990` [BOOT]: opcode`0xeb`, requested parameter0 success/1 failure. If3E update-view+0x90 nonzero, force7 for stage<3 or3 for stage>=3, **even overriding requested failure1**. Header firstword bytes `eb 00 07 00` /`eb 00 03 00` (otherwise`eb 00 00 00` /`eb 00 01 00`), remaining header wordszero, padded60-byte prefix05 output. Physical reboot/re-enumeration semantics U. |

O: boot response `[0]=5,[1..16]=header,[17..]=body`, `0x180cb36f0` [BOOT]. E0/EA/E4/E6 success requires byte17==0; E5 requires **at least33 bytes** and full16-byte equality; no host checksum calculation established in this3E flow. No separate per-E2 success ack observed; E4 acknowledges batch. Opcode1 retry/error increments separate error counter: `<10` resend retained message, otherwise cleanup/error; distinct from configured retry10000. Failures call `0x180cac960` [BOOTUI], clear counters and send EB failure path. U: all response-header word meanings, algorithm/device validation producing16-byte trailer result, recovery. Other-model CRC helper `0x180cbb310` in same large function is **not evidence of3E per-block CRC**.

#### Recovered E5 loops — former decompiler gate CLOSED

O: original-file GNU objdump via REA process capture recovered below. Ghidra assembly/instruction/bytes requests each exceeded MCP30s timeout while import continued; no binary annotation/change. FWASM/FWLOOP/FWTABLE/WATCHTHUNK record objdump executable/scenario and raw terminal output, **not a separately hashed input-DLL identity**; original path was `reconstruction/original/UWPAdvance.dll`. Cite alongside immutable original SHA and retained [BOOT] IR, not as Ghidra dossier or device execution.

| Component | Table slot→loop head | Compare source `T_i+j`, `j=0..15` | Increment/test → E6 factory call |
|---|---|---|---|
| 0 | `1806b3848`→`180cb7dc1` | `L0+j`, compare`180cb8013..8047` | `180cb804d: ff c1`;`804f: 83 f9 10`;`8052: 7d 06`→`180cb805a` |
| 1 | `1806b3850`→`180cb837e` | `L0+L1+0x10+j`, compare`180cb8402..843f` | `180cb8445: ff c2`;`8447: 83 fa 10`;`844a: 7d 06`→`180cb8452` |
| 2 | `1806b3858`→`180cb8776` | `L0+L1+L2+0x20+j`, compare`180cb880b..884d` | `180cb8853: ff c2`;`8855: 83 fa 10`;`8858: 7d 06`→`180cb8860` |
| 3 | `1806b3860`→`180cb8b84` | `L0+…+L3+0x30+j`, compare`180cb8c1c..8c61` | `180cb8c67: ff c2`;`8c69: 83 fa 10`;`8c6c: 7d 06`→`180cb8c74` |
| 4 | `1806b3868`→`180cb8f98` | `L0+…+L4+0x40+j`, compare`180cb8f9d..8fe5` | `180cb8feb: ff c2`;`8fed: 83 fa 10`;`8ff0: 7d 06`→`180cb8ff8` |

Loop index starts0; each indirect table jump re-enters **after initialization**, preserving increment. E.g. component0 `180cb8054: ff 25 ee b7 9f ff` jumps via table; on16 matches, `180cb805a: e8 61 1b 00 00` calls E6 factory, `180cb805f: 48 8b c8`, `180cb8062: e8 39 ad f9 ff` calls serializer. Source/response accesses have per-byte bounds checks. [FWASM,FWLOOP,FWTABLE] This proves opaque trailer equality, **not CRC/MD5/signature semantics**.

O: EA-success transfer-state values (`base+0x28`) are1,2,3,4,**4** for stages0..4; corresponding cursors `DAT_1805c1a28/34/44/40/48`, lengths `DAT_1805c1a0c/10/14/18/1c`. Active stage=`DAT_1805c1a58`. Initial five chunks at0/42/84/126/168 precede E4; later batches clamp final slice≤42. [BOOT,BOOTWIRE] Do not treat these state values as additional wire fields.

### Progress, done, errors

O: `0x180d24750`/`0x180d246b0` [UPDATEUI] file-read done→container splitter→worker. `0x180d23d70` [UPDATEUI] polls every100ms. 3E update-mode1 percentage is integer floor100*(sum completed component lengths + active component cursor)/sum five L_i; other mode uses first3. Success gate checks sync-state1 AND percentage100, clears busy, then **waits60000ms for3E** before `0x180d224f0` [UPDATEUI] displays `Your firmware is up-to-date.` This is observed UI delay, not a confirmed reconnection/firmware-read verification. Other model+30 waits20000ms. O: transfer completion code sets update-view+0x91 flag and resets state; `Notification_UpdateFirmwareDone` also exists in other transfer family (`0x180cad010` [BOOTUI], `0x180cb0000` [HIDQUEUE]); not assumed universal3E event.

O: `0x180cac960` [BOOTUI] stops progress busy flag, resets percentage and global update state; `0x180cac9e0` [BOOTUI] currently displays `Download failed.` even though called from transfer-failure branches. Do not treat that UI wording as proof network failure. Error text resource separately includes `Update failed, please try again.` and warning not to exit/shut down/disconnect during update.

### Device enumeration — boot identity gate remains OPEN

O: normal receiver selector `0x180cb1580` (`ev_a2e4840518558b197fcceb1990abcf9c6887bb5d5fb99228473bd2eea8351591`) requests `2dc8:3024`, usage`ff00:1`. `0x180c4df50`→`0x180c4e070` [RECEIVER] opens it, stores `DAT_1805b7828`, sets logical3E, installs input callback, emits `Notification_findBootDevice`. **Event name does not establish bootloader identity.**

| Selector in `0x180cb21c0` [BOOT] | Static association |
|---|---|
| `2dc8:3024`, usage`ff00:1` | Normal3E receiver |
| `2dc8:ee11`, usage`ff7a:1` | Additional watcher;3E/boot association **unproved** |
| `2dc8:2051`, usage`008c:1` | Ultimate3 family, **not3E** |
| `2dc8:2052`, usage`008c:1` | Ultimate3 family, **not3E** |

O: Added delegate tail jumps `180df1750: e9 0b 0a ec ff`, `180df1758: e9 03 0a ec ff`, `180df1760: e9 fb 09 ec ff` all target`0x180cb2160`, which only logs/init; removed callbacks target`0x180cb2190` log/clear state. [BOOT,WATCHTHUNK] `0x180d805a0` [HEADSETCTOR] labels3024 `Ultimate 3E Controller for XBOX`,2051/2052 artwork under **Ultimate3** title. Another selector`0x180cb1210` requests2051 usage`ff7a:1`; not a3E boot mapping.

O: `0x180cc2590` (`ev_09cd9be6ae389a71445a1beaad751e87659f693a6e5fdbaf6b479812ed221a1e`) is D4XDevice Added: physicalPID202f→logical model+3c, stores D4X object/registers`0x180cb34d0`; `0x180cb2c20` [BOOT] is D4XDevice Removed with normal/update cleanup. Neither binds a newly enumerated post-E0/EB identity to3E firmware session. **Open:** whether normal202f/3024 identities persist during update or distinct boot VID/PID appears; receiver HID fragmentation, EB physical restart/reconnect timing, device trailer algorithm, recovery and post-update version verification. No flash-ready claim. Remaining discovery requires authoritative static boot binding/recovery evidence or parent-owned deliberately authorized update trace—not guessed writes.

## 3. D4XGaming.Devices.dll role; battery/charging

O: WinMD SHA-256`b4af28b7a0a275d6aff3bf6cff260ec2cbf026827b7f8381f7ba4e409a4dccd3`, MVID`1859daaa-e6d9-475b-b28d-9377824f6658` [WINMD]. It declares a **WinRT Windows.Gaming.Input custom controller adapter**, not firmware parser/decrypter. Native factory registration uses `Windows.Gaming.Input.Custom.GameControllerFactoryManager` at`0x1800038d0`, native D4X evidence`ev_ca762341d397f52af3af700ac65cd6f57f2b68718c6fb5161a0c61f78d330461`; runtime class literal`D4XGaming.Devices.D4XDevice` at`0x180002ec0` (`ev_dfe3b4a20ee7df0dd838dfc7061d666b4edac6b74dfb783fed25136b6f9e4423`).

| O: API declaration [WINMD] | O: native implementation / limitation |
|---|---|
| InstanceId u32; FirmwareVersionInfo; HardwareProductId u16; HardwareVendorId u16; HardwareVersionInfo | Thin forwards `0x1800019f0`,1a10,1a30,1a50 [D4X] call underlying provider interface at object+0x68, slots30/38/40/48. Exact individual property↔RVA mapping not proven; WinMD property list itself is authoritative. FirmwareVersionInfo is platform struct, not custom boot version command. |
| `SendMessageA(byte[])` | `0x180001aa0` [D4X] provider+0x70/vtable30 `(provider,0,0x0f,length,data)`; fixed messageID0f. |
| `SendMessageId(byte[],u32 messageID)` | `0x180001ad0` [D4X] same provider slot30 `(provider,0,(u8)messageID,length,data)`; truncates ID tobyte. |
| `GetMessageB(byte[] destination,out u32 actualLength)` | `0x180001b00` [D4X]: selects current double-buffered bytevector by generation&1, requires destination capacity≥stored size or returns`0x80070057`, copies, retries if generation changed. |
| `MessageBReceived(device,byte id,byte[] data)` | WinMD delegate token0600002e; native buffer ingestion`0x180001800` [D4X] copies provider data to alternate buffer then updates generation. |
| `SendReceiveMessageC(MessageCRequest,out MessageCResponse)` | `0x180001bc0` [D4X]: sends4-byte request via provider slot38 `(provider,0,1,4,request,20,response)`; copies20-byte response when native call succeeds, but decompiled method returns HRESULT`0x80004005` unconditionally at end. Treat as real observed wrapper behavior/possible artifact, not a Linux firmware protocol. |
| D4XDeviceAdded/Removed, D4XDevices, FromGameController | Platform enumeration/custom-controller factory, not raw USB scan. |
| HeadsetConnected/Disconnected, Headset, IsWireless, User/UserChanged, `TryGetBatteryReport()` | Platform game-controller interfaces are declared inWinMD. No3E-specific battery opcode, proprietary charging report, percentage equation, or UI battery/charging indicator recovered. |

Applicability: O D4X wrapper used by Windows GIP transport; I not required as Linux dependency when using documented native HID/Bluetooth transport. Battery/charging user-facing support for3E is U, not inferred from generic WinRT interface. **Parent live observation supersedes prior byte28 hypothesis:** raw[22..32] in receiver pad frames are stale reply-buffer residue, not status; `0xa0`/`0x20`/`0x0d` there must not be decoded as battery. Next probe: read-only map D4X WinRT battery-interface vtable/forward or find an independently bounded status report; do not infer from bytes outside pad length0x14.

## 4. Reset: two different operations

**DESTRUCTIVE: confirming ResetView replaces three profile records and initiates configuration sync. Its warning additionally declares Bluetooth pairing/all other settings erased and device restart, but those device effects are not code-confirmed below. Warn and require intentional action.** Do not advertise proven pairing erase/restart or substitute local defaults for a verified full-device erase.

| Operation | O: recovered behavior | 3E applicability / packet confidence |
|---|---|---|
| `View/ResetView.xaml` reset dialog | `0x180d17370` [UI030] sets Ok label at+0x88, Cancel at+0x90. Connector`0x180daf880` [RESETCONNECT] binds confirm`0x180d22220`→`0x180d16e70` [UPDATEUI,CALUI].3E branch defaults slots0..2, clears config+0xb, starts opcode7 disable. | **Closed:** confirm→first sender and3E applicability. Subsequent ordinary profile-write/commit path detailed below; named subscription→continuation still partly unresolved. **Open:** pairing erase, all-other-settings erase, restart/disconnect/ack timing and receiver/BT full reset. No distinct reset opcode proved. |
| `StatusBar/RestoreFactoryController.xaml` | `0x180d2cad0` [RESTORE] asks `Restore to default factory settings`, Yes/No. Confirm`0x180d2d790` [RESTORE] invokes`0x180c9f880`, then`0x180ca0e00` [DEFAULT], publishes `Notification_ResetEvent` with null. `0x180c9f880` branches old-model default-reset helpers, but **has no3E branch**. `0x180ca0e00` has3E path that sets dirty/readiness and rehydrates UI via`0x180c8e570`→`0x180c9f260`; not itself a wire command. | O local/state reset notification; not an observed all-device erase. Need profile spec's established write/commit path if defaults are subsequently synced. No exact factory-reset3E opcode proved. |
| `Notification_ResetEvent` | Also emitted at end`0x180d2bea0` (`ev_1d31aa13fc6d1c2ca5e453b3b60c46b2bd02f1674f59ec326bd2aaf36b196ac3`) after replacing cached config and invoking page helpers. Many page `resetEvent` subscribers rebuild controls. | **Host UI event, not device report/opcode.** Never serialize its enum/string as command. |

O: legacy restore dialog precondition`0x180d2cd20` [RESTORE]: completed-read flag(s) required;3E profile getter`0x180c81440` readiness+0x3d8/+0x3d9/+0x3da must not allzero. Otherwise logs `没有读取完成` /`没有读取完成1`. Modern3E reachability of this **legacy restore dialog** remains U; not confused with now-traced ResetView confirm. **Do not test guessed opcode0a/0b/0c/0d**: observed0c is firmware entry`0x180ca69c0` [CAL],0d is light selection`0x180ca9830` [SERVICE], not reset evidence.

### ResetView confirm → ordinary configuration sync

| Link | O: exact static evidence |
|---|---|
| Type/connector | `DAT_180034e70` type-relative method table: original bytes`180035008: 14 fe ff ff`→`180034e1c`; entry`180034e20: 60 aa d7 00`→`180daf880`. Connector cases2/10→+60,3/11→+68,4/12→+70,5/13→+78 attaches Cancel`d21a40`,6/14→+80 attaches Confirm`d22220`,7/15→+88,8/16→+90; guard+98. [RESETCONNECT,RESETBYTES; PE table capture`ev_fff0f407d5e258ea717e1c771b87b17f209b2779e2884c53b23e88d5fb6f8ba5`] |
| Confirm target | `d22220` loads settings `DAT_1805b7680` into RCX then calls`d16e70`; bytes`180d22231: 48 8d 05 28 53 89 ff 48 8b 88 20 01 00 00 80 39 00 e8 29 4c ff ff` resolve decompiler hidden-RCX ambiguity. [UPDATEUI,RESETBYTES] |
| 3E host mutations | `d16e70` compares logicalPID+48==+3c, calls`c818d0(0)`, `(1)`, `(2)`, clears`DAT_1805b7b98+0xb` (**wire profileBit byte3**), sets settings+0xa0=1, calls`cadd80(0)`. `c818d0` zeros each slot's **six record markers and32-byte name**, builds mapping/light/motion/deadZone constants, and replaces managed records+10/+b0/+150. **Reset invalidates records/empties names; it is not structural initializer`c80ce0`, which installs valid magic/localized names.** [CALUI,PROFILEINIT; managed→wire`0x1808e9590`, `ev_b7bcc2da3eb329a704f93efab52acba24374640fba0b13173ac77a1ad8236dd5`] No direct Bluetooth-store access established. |
| First wire operation | `cadd80(0)`→`caa500(0)`: `H(7,0)` =`07 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00`, no payload,channel4/readfalse; timeout2s/retry9999, subject to queue/busy guards. For3E selects`c7f2f0` generic GIP queue, **not receiver queue**; same backend/framing discrepancy as calibration audit applies. [BOOTUI,SERVICE] |
| Disable success/continuation | `cae170` publishes `DAT_1805b7a00` (`enableReportIDDone` family). Settings continuation`d99850` calls`ca0fe0` when3E/L5 and settings+0xa0!=0;`ca0fe0`3E branch→`c88620`. **U:** independently decoded reflection-name subscription linking that notification to`d99850`; observed matching flag/model continuation is not proof of that missing link. [BOOTUI,RESETCONTINUE,DEFAULT] |
| Configuration write | `c88620`→`c887c0`: ordinary opcode1,p0,channel4, full868-byte marshalled`c81440()` configuration; timeout2s/retry0. Per-fragment bytes `LE16(1)||LE16(0)||LE32(count)||LE32(868)||LE32(offset)||configuration_slice`. Reset payload contains **invalidated markers, empty names and exact reset constants**; all864 slot bytes and headerbyte3 determined. Only runtime indexbyte2 and Windows' storedCRC are retained; Rust explicitly repairsCRC. Complete byte contract/diff: [spec-reset.md](spec-reset.md). Not ordinary valid named-default profiles. [SAVE,PROFILEINIT] |
| Commit | Full-write success thunk`180df12e0: e9 6b e6 e8 ff`→`c7f950`→`c7f970`→`c7fc50`: `H(6,0)`=`06 00` followed by14zero bytes,channel4/no payload. Success`c7fc10` publishes`DAT_1805b7a20` save-finished family. [COMMIT; thunk capture`ev_fbeecbda57a82d9b4ea322864dc3ffb2a28c9d3f2a74f6b8b73aa2776e0ba0b1`] |
| Return to UI | `0x180d31800` clears settings+0xa0, refreshes config, calls`cadd80(1)` report-enable. `0x180d31480`'s2s timer only hides tip. Neither establishes hardware restart. [`ev_b52f2e210711ee633e552e6b5b8742166e2fc57db085720350d4a92a4bd192b3`] |

**Closed:** Ok callback,3E dispatch, exact reset-image bytes ([spec-reset.md](spec-reset.md)), initial opcode7 and normal opcode1/6 payload family. **Still open:** complete reflected Windows ack continuation, firmware invalid-record interpretation, pair-store deletion/reboot semantics. The replacement may intentionally stage **Restore factory profiles** from exact recovered bytes after backing up baseline; ordinary **Save + verify** owns transfer/commit/read-back through the established receiver path. Staging alone does not write. Preserve invalid markers/empty names—not structural initializer`c80ce0` output—and do not label this “erase Bluetooth pairing.” No new reset opcode or device semantics inferred. Original-file observations obtained read-only; Ghidra requests timed out at30s. No reset executed in this research.

## 5. Headset volume / mix

O: `HeadsetController` constructor`0x180d80c10` [HEADSETCTOR] composes `MixVolume` and`VolumeView`, manages focus links. Not every compiled page is3E UI: scout's3E title inventory reports no Headset tab in L7Title_New/L7Title_GIP (separate UI spec). Applicability to3E **not observed**; audio receiver USB interfaces alone do not establish app feature.

| Feature | O: user controls and state | Packet/result |
|---|---|---|
| Mix | Constructor`0x180d80e00` [HEADSETCTOR] initializes integer10. Left/right keyboard`0x180d1d390` [HEADSET] decrements/increments. `0x180d1d410` clamps0..21 inclusive; highlights corresponding ticks via`0x180d1d250`/`0x180d1d2d0`. Pointer`0x180d46d10` (`ev_c722a339db1dc45dd36ba2cf753d94686a2da2c8e379a500b33e775caf5df18d`) maps pixel x as`int((x-15)/13+0.5)`, clamps throughsame helper. | **No device send, property persistence, opcode, payload, ack or timeout in traced mix handlers.** O local visual state only. |
| Volume | Constructor`0x180d81210` [HEADSETCTOR]. `0x180d1e120` [HEADSET] clamps slider coordinate0..actual control width. `0x180d1e010` moves layout, calls`0x180d1e090`, which displays`int(position/width*100)`. Pointer/key handlers update visual coordinate. Keyboard repeat increment accelerates2→10→15→20 after repeat counts>4,≥10,≥15 (`0x180d1e230` [HEADSET]); Escape restores focus. | **No device send in traced volume coordinate/display path.** U whether XAML two-way binding/external adapter adds real OS-audio action. Do not invent headset opcodes/USB mixer ranges. |

Precondition: actual headset/output endpoint and proven supported3E action, otherwise don't offer a misleading functional hardware slider. Risks: sudden loud audio; safe UI should not equate current local default10 or percentage with actual hardware state. Next probe: inspect XBF binding/connectors of these threepages and their property notifications; passive Windows trace on supported model if necessary. Current evidence supports inventory, not3E headset command implementation.

## 6. Vibration test — exact3E command recovered

O3E applicability: modern `Vibration_New` constructor`0x180d688c0` (`ev_4d052f7d138ac155a4f2dde52fb589196237a5a821b18bf4dff8b7db0ce83ed9`) invokes`0x180cefc50` (`ev_e4f9af7cf57362b54fbc5235e095bc60534ef9f3aa1a6a16c58f0e150b49e8c6`): `Hold LT/RT to test grip vibration. Hold PL+LT / PR+RT to test trigger vibration.` Four levels0..5; `0x180cedce0` [VIB] clamps controls, profile codec owns persistent packed levels.

O exact inner packet factory`0x180caa450` [SERVICE]:

```
LE16(0x0009) || LE16(duration) || LE32(4) || LE32(4) || LE32(0)
|| left_grip_u8 || right_grip_u8 || left_trigger_u8 || right_trigger_u8
```

`0x180cedb20` [VIB] explicitly checks3E, builds each nonzero-enabled motor byte as integer`configured_level*255/5`; disabled motor0. Globals`DAT_1805c1898/189c/18a0/18a4` correspond Lgrip/Rgrip/Ltrigger/Rtrigger. UI test calls pass duration500 (`0x180ced130` [VIBSTATE]); **500 unit milliseconds is I**, not proved by firmware. Stop call passes allzeros/duration0 at`0x180dbcb60` (`ev_aec5c91d8f726fcf919490f58c0b11732481600a040b81ab4c2ced68afa418dc`). O values are not profile featureword; do not confuse persistent0..5 with test0..255.

O transport route: `0x180cedb20` [VIB] static device-mode flag false→`0x180caf7c0` [HIDQUEUE] generic GIP; true→`0x180caf520` [HIDQUEUE], **correct3E receiver queue**`0x180c4d7d0` forchannel4. Receiver launcher timer0.1s (`0x3dcccccd`), retries2, current retry0. GIP launcher2s/retries2. Neither uses read flag. Both refuse nonzero test while profile readopcode2/writeopcode1 outstanding or duplicateopcode9 queued. Zero can bypass duplicate9 rejection, but early read/write guards still return before send; do not infer guaranteed stop under all queue states. Timer helper units established`0x180c4b7a0` [TX]. No per-motor physical validation or battery threshold observed. Risk: unexpected motion/continuous rumble; stop on leaving test/disconnect, no automatic tests during startup.

`0x180ced130` [VIBSTATE] drives test from live trigger/paddle state and animates active motors. `0x180cf7880`/`0x180cf7b30` (`ev_b80cb4669cad7b97b771e0eb359b65954d366c54660a905bf77aabd7fde4158d`) are **switch animation timers**, not motor transport. No need to reconstruct those timers as extra rumble commands. U: exact normalized live trigger/paddle field mapping into view offsets+0x80/+0x84/+0x88/+0x8c; parent input spec owns reports. Next probe: offline generated ReadKey handler assigning those fields, then parent-owned intentional duration-limited test only after config transport acceptance.

## 7. TestController raw-input display

O reachable home menu Test label`0x180c4fe40` [RX]; TestController constructor`0x180d711d0` [TEST], page`0x180d71a10` [TEST], stores page global and shows **`Unable to test the XBOX, Square, Star, Share, and Profile buttons.`** plus press/hold-back instruction. Do not promise every physical button from this screen. This is visual input testing, not calibration/write/test-rumble opcode.

O receiver callback`0x180c4e5f0` [RX] passes raw array to response parser`0x180c4e6f0`; when array[1]==0x14 andarray[0]==0x81, publishes wholearray as`Notification_ReadKey`. Parent live observation confirms Xbox360-style receiver fields: [2..3]buttons,[4]LT,[5]RT,[6..13]fourLEi16axes. **Live offsets are parent evidence, not independently validated TestController field accesses here.** No opcode required merely to read already-streaming physical receiver input. Configuration/extended report enable and BT reports belong transport/input spec; do not force report-enable as prerequisite for ordinary idle input if already present.

U: exact TestController per-control byte/bit and stick-number display mappings, hold-to-return duration, controller-button exclusions by3E mode, no-activity timeout. No host battery/charging widget was found in inspected TestController constructor. Next probe: XBF-loaded event connector and shared ReadKey callback for TestController, following raw receiverarray rather than inferring logical mapping fields. Raw-axis visualization can be implemented from verified parent input fields, but mark any additional paddle/IMU claims unsupported until report source known.

## Unresolved implementation gates

| Gate | Status / exact remaining blocker |
|---|---|
| Calibration through3024 or Bluetooth | **OPEN; input/routing audit CLOSED.** Exact H(0e/0f,p),10s/100s/retry2 and two raw32 candidates documented. Windows launchers use GIP queue/HID framing without receiver length byte; kind4 ack lookup uses different queue.3E host consumes vendorGIP u8 payload20..25, not receiver i16 stream. Need authoritative receiver/BT route+ack and validated input adapter; no discovery-by-calibration-write. |
| Firmware flash readiness | **OPEN; EA/E5 success/host sequencing CLOSED.** E5 all16 trailer-byte equality, E6, EB stage2→3 handshake recovered. Still lack3E/receiver boot identity binding, receiver transport fragmentation, physical restart/recovery, trailer algorithm/authenticity.2051/2052 are Ultimate3, not3E. Windows image checks enumerated; length digest is not content integrity. |
| Full factory reset | **OPEN for pairing erase/restart; profile-reset byte gate CLOSED.** Ok→3E exact reset image→H(7,0), normal opcode1/6 family recovered. [spec-reset.md](spec-reset.md) enumerates every byte, invalid markers/empty names, runtime-index preservation and explicit RustCRC repair. Backed-up staging followed by ordinary Save+verify can implement **Restore factory profiles**; Windows reflection continuation and device invalid-record/pairing/restart behavior remain unproved. Do not promise full-device erase. |
| Battery/charging | **OPEN.** GenericWinRT declaration only; no3E status mapping. **Closed rejection:** receiver raw22..32, including byte28a0, is stale residue outside pad length—not battery. Need bounded status source/WinRT property mapping. |
| Headset | **OPEN.** No3E tab or hardware action proved; traced handlers visual only. Need XBF binding→OS audio action/supported model, not guessed opcode. |
| TestController extra inputs | **OPEN.** Parent's main axes/triggers/buttons independently usable; paddle/IMU and exact UI extra-control mappings lack report source. Need generatedReadKey mapping; vendorGIP calibration offsets must not be applied to receiver raw32. |

Implementation scope now: offline firmware splitter/length-check compatibility and **all16-byte E5 comparison/host transfer state logic**, without exposing flash; wired vendorGIP calibration input decoder/state-machine logic, without claiming receiver calibration; exact **Restore factory profiles** staging after baseline backup, followed only by intentional ordinary **Save + verify**. Preserve invalid markers/empty names and distinguish from structural defaults. Receiver/BT calibration, full-device pairing erase/restart and firmware flashing remain gated. These static findings do not constitute live verification of restored-profile firmware behavior; this research performed no controller I/O or checks.
