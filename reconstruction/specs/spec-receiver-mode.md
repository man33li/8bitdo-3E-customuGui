# Ultimate 3E receiver personality / opcode 0x0a

Scope: Ultimate Software X 1.4.9.0; original `UWPAdvance.dll` SHA-256 `1f2325902b5d3ca2cc7b44405243247d3f276c39f99a66c97b193545be038352`, image base `0x180000000`. **O** = observed static code/bytes/resource, **L** = parent's reported live observation, **I** = inference/proposal, **U** = unproven. No hardware access, device sends, host-driver changes, Rust edits, builds or tests performed.

## Conclusion and incident correction

- **L:** One receiver HID report `81 10 04 0a 00 00 00 || 25 zero bytes` preceded hub/receiver disappearance and re-enumeration as `2dc8:202f`, serial `60CC94CAB4700000`, GIP interfaces `ff/47/d0`, working xone input. This personality persists across unplug/replug. Previous receiver identity was `2dc8:3024`, serial `000023D000000010`. These are incident facts, not a Windows traffic capture or a device-firmware decoder.
- **O:** Windows really does associate this family with **XInput**: common success callback `0x180c87660` logs `发送设置Xinput指令` (“send set-XInput instruction”). Platform UI labels its control **`Xinput Mode` / `Xinput模式`**; handler `0x180d347c0` sends this exact opcode. The same bit/factory also backs the **Tournament / polling-rate** UI. Therefore the earlier `spec-header-bits.md` interpretation “Tournament only, safe polling toggle” is incomplete; its exact serializer bytes remain correct.
- **I:** The incident and explicit XInput control make a persistent PC/XInput versus Xbox/GIP personality selector a substantially stronger interpretation than a harmless polling-only toggle. They do **not** establish firmware's full semantics, whether Tournament intentionally couples mode and rate, or whether the hub loss was an intended part of the switch.
- **O:** Windows has the inverse **opcode `000a`, parameter `0001`, channel `04`** over **GIP custom message `0x13`**, not only HID. **U:** No captured or live-proven `202f → 3024` recovery exists in the supplied evidence. Exact Windows API payload is recovered below; exact provider-chosen raw GIP flags and successful receiver reversal remain unproven. Do not present the candidate as a verified recovery command.

## Evidence ledger

All native addresses below are UWP addresses unless marked D4X. Existing authenticated decompilation is in `reconstruction/ir/`; original-PE instruction inspections add byte-level argument/thunk evidence, not new REA IDs.

| Alias | Evidence ID / source |
|---|---|
| F | `ev_8023db373ce5c5f936d7316790bab2182785b085f1fd57dd68cad289db929220`; `UWPAdvance.009.c`: factories/launchers `c87970..c87be0`, callback `c87660` |
| X | `ev_7b5115a5544e546984915d1b84ee69393cebd4c482b7ab16cd2906071ae612b3`; `0x180d347c0` XInput action |
| P | `ev_63021b04bd745d1073996c0b85b85c262c3ecf03217d9777af95af65b58f721d`; `d3b510/d3b6c0` platform labels/state |
| XB | `ev_591f1c115e311268967d14a02338929f49275223ebf5032ea589f68ab79106a1`; `UWPAdvance.010.c`, connector `0x180dac1f0` |
| T | `ev_04269b9cf98a012c2446012d4f65822a2836914da082aee92d2f7ab7381ba500`; `0x180d1af30` toggle and `d1c940` UI dispatch |
| TB | `ev_d21a9880452a2f8e509964b49002b6703d92aca41ce400ab5967076a1af15127`; connectors `0x180d9e010`, `0x180da2070` |
| ON | `ev_7c1b07f8bb94eacac39855981b9110889346caf7056e72923ae04baf13ec8a91`; `0x180d21120` |
| OFF | `ev_dc088566f316c976f0c12d168ab4cd2b78d1c1ab867671af93306a00044dcf42`; `0x180d21770` |
| SETTINGS | `ev_b52f2e210711ee633e552e6b5b8742166e2fc57db085720350d4a92a4bd192b3`; `0x180d30a60` Tournament text |
| RATE | `ev_e8dd03488a043a96f2c19ef96e229bd1e7c239c5f922779afc0ad6f1e60b9801`; `0x180d16a50` 250/1000 Hz text |
| H | `ev_d2692710de786f469fdb89e7ca96057ca2a9c1e36c209d759ab7df48f991566f`; serializers `0x180c4aef0/c4b240/c4b340`, continuation `c4ad60` |
| QG | `ev_5219b564132d5009f8c50d6372f5534a2a887f820dbd3e944c76608594245243`; GIP queue `0x180c7f2f0` |
| QH | `ev_d8af8df7e5b2e999018e706966432d5dbad6401f998b73125b730ac5ae5525ec`; receiver queue/open `c4d7d0/c4e070` |
| LIFE | `ev_e2c1bb70af3bd49debb62270da6c9c2c12e6002b9999ce601c7ccbdd2919d71c`; `cb21c0` watcher, `cb2c20` D4X removal |
| ADD | `ev_09cd9be6ae389a71445a1beaad751e87659f693a6e5fdbaf6b479812ed221a1e`; `0x180cc2590` D4X addition |
| D4X | `ev_8b9eeb346c176a2fc1a905dd8415c84b0b5ddb4553dce8ecb77e46cf3d0608ac`; D4X `0x180001ad0` SendMessageId bridge; additional wrapper/metadata authority in `spec-gip.md` |

Original instructions inspected: `0x180c87970..0x180c87d80`, `0x180c87b50..0x180c87bf0`, `0x180d347c0..0x180d34860`, `0x180d1afd0..0x180d1b070`, `0x180df1240..0x180df12a0`. Reproduce with the plan's `nix-shell -p binutils --run 'objdump -d --start-address=... --stop-address=... reconstruction/original/UWPAdvance.dll'`. Start at the function entry for whole-function decoding; the `d1afd0` window begins mid-instruction, so its first two decoded rows are not evidence. Valid following instruction boundaries contain the call-site/CL observations below.

## Complete recovered opcode-0x0a factory / launcher family

**O:** One literal command factory was found, `0x180c87b50` [F]. All rows below share opcode `000a`, channel `04`, source offset/wire offset/count/total = `0`, null backing array, `copyIncoming=false`. Parameter is zero-extended **incoming byte**, not the cached profile-bit byte. Factory itself permits any byte; observed UI callers use only `0/1`. Factory defaults retry limit9999/time0; both actual launchers override to **2 seconds, retry limit0**.

| Address / role | Parameter | Channel | Label / provenance | Route |
|---|---|---:|---|---|
| `0x180c87b50` factory [F] | Incoming CL → u16; observed0/1 | 4 | Shared set-XInput/Tournament family | Produces tuple; no send itself |
| `0x180c879b0` launcher [F] | Preserves incoming CL into factory | 4 | Same family; callback thunk `df1280 → c87660` logs set-XInput | `c7f2f0` [QG] → `c4aef0` [H] → GIP P60 when `!isHID` |
| `0x180c87be0` launcher [F] | Preserves incoming CL into factory | 4 | Same family; callback thunk `df1298 → c87660` | `c4d7d0` [QH] → receiver `c4bf40` → report81 |
| `0x180c87970` transport dispatcher [F] | Incoming DL copied to CL | 4 | No recovered direct caller; no additional UI label | `isHID ? c87be0 : c879b0` |

**O:** Direct callers of the two launchers, traced back to their UI connectors:

| UI action / guard | Parameter0 | Parameter1 | Label / evidence |
|---|---|---|---|
| `0x180d347c0` [X], wired to platform button `this+0xb0` by `0x180dac1f0`, cases7/0x23 [XB] | Cached `profileBit & 0x10` set | Cached bit clear | Adjacent label `this+0xa8` = **Xinput Mode**, `0x180d3b510` [P], strings `0x1806335f8/633628`. `d3b6c0` chooses button image from the same bit. Action does not update cached bit or wait for reconnect. Logical2038 takes a different opcode1 path; other PIDs enter its unguarded 3E-style else branch. |
| `0x180d1af30` [T], connector `0x180da2070`, cases8/0x1c, button `this+0xd8` [TB] | Clears cached0x10, keeps low four flags | ORs cached0x10 | Polling/Tournament toggle; same state rendered as250Hz/1000Hz by `d16a50` [RATE]. Sends0a only for logical202f; logical2038 takes another command. Schedules UI animation timer, not reconnection. |
| `0x180d21120` [ON], connector `0x180d9e010`, case3 [TB] | — | Sends1 only if cached0x10 clear | Enable/1000Hz action; updates cache then `d1c940 → d16a50`. Sends0a only for logical202f. |
| `0x180d21770` [OFF], connector `0x180d9e010`, case2 [TB] | Sends0 only if cached0x10 set | — | Disable/250Hz action; clears cache then same UI refresh. Sends0a only for logical202f. |

**O:** Machine instructions resolve decompiler-omitted arguments: `d347c0` has `xor ecx,ecx` at `0x180d3481b`, `mov cl,1` at `0x180d34845`, HID call at `0x180d34822`, GIP call at `0x180d34849`. `d1af30`: off calls at `d1afde/d1afe5` follow CL=0; on calls at `d1b056/d1b05d` follow `mov cl,1` at `d1b04f`. Factory `c87b57` saves CL; `c87ba4` writes opcode0a; `c87ba9..bad` zero-extends the saved byte and writes parameteru16. A byte scan for x86 E8-relative calls in the original `.text` found exactly these caller sites plus `c87970`, `d21120`, `d21770`; both factory calls are from the two launchers. This corroborates recovered **direct** callers, not absence of unresolved computed dispatch.

**O:** `0x180d30a60` [SETTINGS] labels its UI **Tournament Mode / 赛事模式**, strings `0x18063cdd0/63cdb0`; explanation `0x1806336a8`: “When Tournament Mode is enabled, the polling rate increases to1000Hz … reduces battery life.” `d16a50` [RATE] reads the same cached bit. These are genuine Windows labels, not evidence that the command cannot switch USB personalities. No literal **PC/Xbox mode** label was recovered. The explicit **Xinput Mode** label and shared callback log are the missing semantic evidence in the earlier spec.

**O/U model boundary:** Model-table+0x3c=`202f` (3E); +0x38=`2038` (other/L5 family). Generic receiver3024 open `c4e070` [QH] sets `isHID=1`, logicalPID202f; D4X202f addition `cc2590` [ADD] assigns logical202f and its MessageB callback. Thus both observed USB PIDs **3024/HID** and **202f/GIP** can reach the same0a factory. Logical2038 UI branches use `c87690 → c87830` (opcode1, parameter0, count/total1, channel4), or `ca2e70`; they are **not another opcode0a factory**. The XInput action's else branch lacks an explicit202f guard: no claim that the factory intrinsically rejects other models, or that every model's UI-visibility gate has been proved. No other model-specific0a producer was recovered. The unrelated `0x1809f45a0` +0x20=10 object is metadata, not a command (see `spec-header-bits.md`).

## Windows reconnect behavior: ordinary discovery, no proved mode-switch state machine

**O:** Neither `d347c0` nor the other0a action chains arm a PID-transition watcher, correlate old/new serials, wait for3024/202f, close/reopen a handle, disable reports, send commit6, or reread profiles. Both success thunks jump to `c87660`, which only logs and returns1. Pre-send/failure thunks `df1270/1278/1288/1290` jump to `0x180836810` (return1). The launchers use ordinary command queues and a2-second timeout with no retries, not a special reconnect transaction.

**O:** Generic lifecycle support exists independently: `cb21c0` [LIFE] watches HID3024/FF00/1; its removal callback `cb2190` logs and clears a discovery-state byte. D4X addition `cc2590` [ADD] recognizes202f, installs `cb34d0` MessageBReceived, stores the device and publishes normal device-added notification `DAT_1805b79f0`. D4X removal `cb2c20` [LIFE] clears normal state, removes listeners/device-list membership and publishes `DAT_1805b79f8`. **I:** Those generic paths can react to re-enumeration. **U:** They do not prove Windows specifically expects `3024 → 202f` after0a, or that a given physical receiver's changed serial is correlated correctly. No dedicated expected-disconnect flow was found in the bounded action/launcher/callback/discovery spine.

## Inverse request: exact API bytes, candidate USB framing

**O:** Select parameter1 through the XInput action or enable action while `!isHID`: `c879b0 → c87b50 → c7f2f0 → c4aef0 → c4b240 → c4b340` [F/QG/H]. `c4b340` invokes `D4XDevice.SendMessageId(payload,0x13)`; D4X `0x180001ad0` [D4X] forwards `GipGameControllerProvider.SendMessage(Command=0,id=0x13,payload)`. No rumble, HID GET/SET_REPORT, vendor control request, firmware/update path, or special “receiver recovery” API is involved in this recovered inverse setting path. See `spec-gip.md` for WinRT slot/metadata authority.

Exact **60-byte Windows API payload**, all integers little-endian:

```text
P60(1) = 04 0a 00 01 00 || 55 zero bytes
           ^^^^^ ^^^^^
           op0a  param1

payload[0]     = 04       channel
payload[1..3]  = 0a 00    opcode
payload[3..5]  = 01 00    parameter
payload[5..17] = 00×12    count / total / wire offset, each u32LE
payload[17..60]= 00×43    no data; serializer padding
```

Notation `00×N` / “N zero bytes” specifies the exact byte count, not omitted unknown bytes. Parameter0 is identical except payload[3]=00. `gip60_buffer(4)` reproduces this layout. **Do not use65**: `c4aef0`'s state65 is the HID/update branch, not the normal202f GIP route. No report81 or receiver length10 belongs inside P60.

**O:** App passes only Command class, message ID13 and P60 to Windows. It does not choose GIP flags/sequence/fragmentation. **I:** The existing Linux packet-layer candidate, not captured Windows bytes, is exactly:

```text
64-byte interrupt OUT candidate, primary client0:
13 00 SS 3c 04 0a 00 01 00 || 55 zero bytes
   ^^ ^^                  SS = own nonzero sequence
   flags00 = unproved Windows-provider choice
```

Interface0 endpoints must come from cached descriptors; do not guess endpoint addresses. Receiver HID inverse, **only while3024 exists**, would be `81 10 04 0a 00 01 00 || 25 zero bytes` (32 bytes). It cannot be sent to the present202f GIP interface as a HID report.

**O:** Windows expects a normal vendor acknowledgment through incoming GIP message12: payload kindu16=0004, echo u16=000a, count/total/offset at4/8/12; total0 permits queue completion. A plausible zero-length acknowledgment payload is `04 00 0a 00 || 12 zero bytes`, but **U:** that reply has not been captured. GIP transport ACK01 is separate and must follow incoming flags; do not require reply transport sequence to equal request sequence. **U:** If firmware disconnects before acknowledging, Windows action code supplies no recovered success criterion for the transition.

**Answer:** Yes, the **exact equivalent of opcode0a parameter1 is statically sent over GIP**. Whether it returns this receiver from Xbox/GIP202f to PC/XInput3024 is **inferred, not observed**. It is the strongest recovered inverse candidate, not a guaranteed repair.

## Non-software recovery exposed by this Windows package

**O:** Bounded searches covered `strings.tsv` for hold/按住/长按, mode/模式, PC, Xbox, switch/切换, reconnect, receiver/pairing; printable ASCII and UTF-16LE strings in original `resources.pri` and `resources.en.pri`; native XInput/Tournament action chains. No instruction to hold a button combination to restore receiver3024, or an explicit PC/Xbox hardware-mode recovery command, was recovered.

| Found text/resource | Observed context | Recovery authority |
|---|---|---|
| `Press and hold to enable`, `Press once to enable/disable` (`633b88/633b08`) | Motion-enable method UI; `d21700` and motion-view code | Not a receiver-mode combo |
| `Press and hold … to return` (`641670`) | Test-page UI, alongside “Unable to test the XBOX, Square, Star, Share, and Profile buttons”; `UWPAdvance.010.c:18970–18971` | Return from app test page, not a USB-personality reset |
| Hold LT/RT, PL+LT / PR+RT (`636ec8`, `636af8`) | Grip/trigger vibration tests | Not recovery |
| Trigger mode switch (`63ad30`) | Trigger-calibration preparation | Physical trigger lock, not PC/Xbox selector |
| `Connect 8BitDo device to Xbox via USB` in PRI | Connection/help UI text | Says how to connect, supplies no mode-reset steps |
| `XinputRp`, `XinputTextBlock`, `Xinput` in PRI; `Switch_Open/Close.png` | Confirms XInput control and graphical toggles | Software UI, not evidence of a physical mode switch |
| Reset warning (`63b290`): profiles, Bluetooth pairing, other settings | ResetView; reconstructed action is ordinary host-profile reset/save (`spec-reset.md`) | Not a proved receiver-personality reset; do not use reset/firmware entry as recovery |

**U:** This is a bounded package/code/resource search, not proof that undocumented firmware button combinations do not exist; PRI printable extraction is not a full semantic XBF decoder, and all artwork was not OCR-audited. No manufacturer manual/web recovery claim was used. Do not invent a Home/View/Menu/face-button combination, pairing-reset procedure, or hold duration.

## Existing `src/gip.rs`: transport can encode it; recovery transaction is missing

**O, source inspected:** `command_frame(sequence,channel,&mut WindowsCommand)` already accepts arbitrary opcode/parameter, emits `13 00 SS 3c || gip60_buffer(channel)`, and can encode the candidate unchanged. `Gip::takeover` is restricted to2dc8:202f/interface0 ff/47/d0, guarded xone disconnect-claim, descriptor-discovered interrupt endpoints; no packet-layer or PID-expansion change is needed for the currently reported202f receiver. It does not distinguish a cabled controller from a receiver sharing202f; identity must be established externally before choosing the device. `transact` already separates incoming12 replies and normal opcode echoes.

**O:** No public0a operation exists. Existing public operations are version read, profile read/write, close. Merely reusing profile-write is wrong: it disables/restores reports and commits. Ordinary `transact` regards disappearance as error; `cleanup` always attempts ownership restore and then demands202f+xone bindings at the original sysfs path. An intentional transition to3024 would fail that criterion even if firmware accepted the command. `discover()` only returns202f, so it cannot prove3024 success.

**I, minimal change after parent authorization/evidence review:** Add one explicitly named, one-shot inverse-setting operation (fixed opcode0a/parameter1/channel4, count/total/offset0, no payload), using existing command frame/sequence/ACK machinery. No alternate flags, retries, report-disable, profile write or commit. Accept matching vendor ack as **setting acknowledgment**, not proof of3024 recovery. Add an expected-disconnect finalization path: cancel/reap/release owned resources, distinguish confirmed device disappearance from arbitrary errors, avoid CONNECT/rebind checks against a vanished202f object, then passively rediscover the same physical topology as3024 and its FF00/1 HID interface. Do not turn any timeout/ENODEV into success: require observed target identity transition. If202f remains, retain ordinary checked xone handback. Reporting-restoration state must remain unchanged. No Rust code changed here.

**U:** Existing flags00, warm takeover/authentication survival and outgoing vendor13 acceptance are still transport candidates per `spec-gip.md`; successful gamepad input under xone does not prove vendor config routing. Re-enumeration-aware cleanup is required before advertising a robust mode-recovery operation, even though the packet encoder already carries the bytes.

## Safest next probes — parent only, not performed

1. **No-send first:** Preserve physical topology/serial/descriptor identity and current202f input state. Best missing authority is a passive Windows USB capture of the genuine **Xinput Mode** control: observe P60, actual GIP flags/fragmentation, acknowledgments, disconnect, destination PID/serial and Windows rediscovery. Opening Windows/app can itself initialize devices; this is a proposed parent-controlled experiment, not a claim of passive app behavior.
2. If a Linux experiment is chosen, first establish the existing GIP transport with **one bounded read-only version request**, not0a, firmware entry or report-disable, using `spec-gip.md`'s takeover/cleanup policy. A successful complete message12/version reply validates the path more safely than a blind persistent setting send. Failure → checked handback and stop; no alternate flags/channel spray. This still transmits and temporarily removes xone input/audio, so requires the parent's authority.
3. Only after transport/ownership are understood and an explicit recovery send is authorized: **one** inverse P60(1), no automatic retry. Arm host-side topology observation before sending; record OUT completion, exact vendor reply if any, disconnect and re-enumerated VID/PID/serial/interfaces. A3024 appearance alone is not full recovery: confirm the correct physical receiver, normal input and, in a separately authorized read transaction, the previously verified receiver configuration link. Persistence/replug testing is a further explicit observation, not assumed from parameter0's persistence.

**Safety boundary:** Do not send opcode0c/parameter0 (“Auto Reconnect off” versus firmware-entry collision), firmware/update commands, factory-reset saves, guessed physical combinations, or the original parameter0 again to diagnose this incident. No proven recovery bytes are being withheld: exact inverse API payload is above; firmware reversal and raw Windows-provider framing are the precise remaining gaps.
