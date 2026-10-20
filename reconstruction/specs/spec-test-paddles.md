# Ultimate 3E Test page / receiver paddle reports

Scope: Ultimate Software X 1.4.9.0, logical PID `202f`, receiver USB `2dc8:3024`, vendor HID `FF00:1`, report ID `81`. Original `UWPAdvance.dll` SHA-256 `1f2325902b5d3ca2cc7b44405243247d3f276c39f99a66c97b193545be038352`. No hardware access, sends, builds, or tests in this investigation; `src/` unchanged. **O** = static code observed; **L** = supplied live observation; **I** = inference; **U** = unproved. Static code is not a Windows USB capture.

## Result and critical boundary

**L:** Windows Test shows L4/R4/PL/PR responding in **2.4 GHz mode**. Linux receives only `81 14` XInput-format reports, including after an acknowledged opcode `7`, parameter `0110`. BLE receives vendor kind-4/cmd-3 physical reports after that enable. These facts stand; absence of a recovered receiver bridge does not refute the Windows observation.

**O:** Test entry itself only subscribes to input; no separate start/poll/stop factory appears in the traced entry/exit functions. **Windows also sends receiver opcode `0055` every approximately 0.4 seconds**, subject to session/queue gates. This is the concrete additional command absent from the stated Linux experiment. **I:** heartbeat may maintain a firmware reporting/session mode needed for paddles. **U:** its firmware effect, physical-report format on receiver, and loss-of-heartbeat timeout. Do not claim opcode `55` is a proved paddle poll, or opcode `7/0110` alone reproduces Windows Test.

## Evidence index

Each alias expands to its complete retained Evidence ID. Function addresses identify individual functions within each batch. Source files are `../ir/UWPAdvance.008.c`, `.009.c`, `.010.c`, except ZERO in `.001.c`.

| Alias | Evidence ID |
|---|---|
| RX | `ev_eb24534b10694e07513f3199072969fa0bd9f4773ffe06f575a25e5f5a7adae2` |
| HID | `ev_d8af8df7e5b2e999018e706966432d5dbad6401f998b73125b730ac5ae5525ec` |
| TX | `ev_d2692710de786f469fdb89e7ca96057ca2a9c1e36c209d759ab7df48f991566f` |
| REPORT | `ev_995b61fb2d386871a2381c093e0ecf87c8977cc598711f89669bb66531eae6d0` |
| FACTORY | `ev_a7c54ad48e1904243f4e2961d9a75c5e3e22653212ed980d869fdd2b5394ac62` |
| PAGE | `ev_20188487ec5c072f82e76176d451b83e7a30e8efccef7406a0127858d7c587d9` |
| TEST | `ev_95b46a1a071025c2a277fa810b6519a093544be7f61268f18274777141260fa2` |
| RENDER | `ev_4f1b4543ad6798baa002ca6c06d81065a3331f86d338161d4586586b5d8d0664` |
| EXIT | `ev_ebdc5977288a0e584a213e4a1087fdb677f89b960bfc5eb5d354cfc8510751f7` |
| GIP | `ev_e2c1bb70af3bd49debb62270da6c9c2c12e6002b9999ce601c7ccbdd2919d71c` |
| INIT | `ev_93ebe7869f5029053f0be92976d2129388189d0d2bf8ab59ca53c87ae6d53137` |
| OTHERHID | `ev_db2d3d4de9130320e8aacfb3d5591e1ba436bd077841bebef9425e3872e847fe` |
| ZERO | `ev_c8e58e3d0b3534238f18c399c1f0dc0671abda8c41efaa39fb2c8d5f1d4b8aa8` |

Original-PE observations additionally made read-only: `0x1805c1aa4` contains float32 `0.4000000059604645`; `0x1805c1aa8` contains float32 `3.0`. GNU objdump resolves heartbeat callback thunks `0x180df05f0`, `0x180df05f8`, `0x180df0600` to `0x1807e3670` (each `e9 rel32`). No new REA Evidence ID is asserted for those byte reads.

## Windows command order and timing

### Receiver session, not a Test-only transaction

1. **O:** Receiver open `0x180c4e070` [HID] sets HID mode and logical 3E, starts timer `0x180c4c9c0`, attaches `0x180c4e4e0`, publishes `Notification_findBootDevice`. Receiver-init subscriber `0x180d99b60` [INIT] clears read-state `+10`, calls **`0x180cae1c0(0)`** [REPORT]. Factory **`0x180caa500`** [FACTORY]: opcode `0007`, parameter `0000`, channel `04`, count/total/offset `0/0/0`, null buffer/no payload. Receiver launcher: timeout **1 s**, retry counter **0**, limit **2**; channel-4 queue `0x180c4d7d0` [HID]. These are timeout retries, not a periodic input poll.
2. **O:** Disable completion emits **`Notification_EnableReport`** despite disabling reporting: `0x180cae170` [REPORT]; receiver completion thunk is `0x180df15d0`, as documented in [spec-transport.md](spec-transport.md). Higher-level initialization/profile/version traffic is session setup, not a discovered Test paddle command. The complete reflection-driven initialization order is not re-established here; ordinary read/version frames remain in that transport spec.
3. **O:** Enabling through receiver launcher **`0x180cae1c0(1)`** [REPORT] uses the same factory but maps input `1` to **parameter `0110` only for logical 3E**. All other wire fields remain zero, no payload. Same 1 s/2-retry setup. After submission it sleeps **200 ms**, then sets read-state `+10=1`; no success notification is installed for enable1. Global heartbeat gate can now pass. The low-level receiver read completion does not itself guarantee this enable; higher UI orchestration owns it. Generic launcher `0x180cadd80` must not be substituted: for HID+3E it still selects the generic GIP queue, unlike `cae1c0`.
4. **O:** Test navigation `0x180d973b0 → 0x180d97470` [PAGE] constructs TestController `0x180d711d0 → 0x180d71a10` [TEST] if needed, shows it, removes the parent's EnableReport subscription, subscribes the page to **`Notification_ReadKey`, method name `ReadKey`**, resets page `+64=0`. **No wire factory/send or report-mode change in these functions.** `0x180d98b30` [PAGE] likewise only subscribes/reset. Constructor contains page/UI/back-handler setup, not a transport command. No alternate report parameter, opcode3 request, profile read, or Test-specific polling read was found in these paths.
5. **O:** Global heartbeat below continues independently of Test visibility. It was started at receiver open, not Test entry. Other UI actions can interleave configuration traffic; no exact total order against heartbeat ticks or automatic guaranteed setup retransmission order is claimed without a Windows capture.
6. **O:** Leaving Test via `0x180c4fb50 → 0x180c51330` [RX/EXIT] cancels the two hold-back UI timers, resets indicator geometry, unsubscribes `Notification_ReadKey`, returns through `0x180c4fba0` [RX] to home UI. **No opcode7 disable/enable, restore opcode, commit, or heartbeat cancellation there.** There is no evidenced Test-exclusive receiver mode to undo in this host path.

### Heartbeat: exact route and applicability

**O:** `0x180c4c9c0` [HID] installs global `DAT_1805b78a0`, interval static float `DAT_1805c1aa4≈0.4 s`, callback `0x180c4ca60 → 0x180c4cb60 → 0x180c4cbd0` [HID]. This is periodic (approximately **2.5 Hz**), not the controller's approximately124-Hz input rate. First tick phase relative to clicking Test is unspecified.

`0x180c4cbd0` [HID] has **only two model branches**, current logical PID compared with model-table `+3c` (3E) and `+38` (other HID model/L5):

| Logical model | Preconditions | Route |
|---|---|---|
| 3E `+3c` | `0x180c4de30` gate permits; read-state `+10 != 0`; upgrade-state `+14 != 1` | factory `0x180c4d5f0` → dispatcher `0x180c4d790` → normal receiver queue `0x180c4d7d0` → receiver serializer `0x180c4bf40` |
| Other HID/L5 `+38` | analogous `0x180c4d670` gate; same read/upgrade state | same factory → other-HID dispatcher `0x180c4ce30` → `0x180c4ce70` → serializer `0x180c4c330` (different wrapper) |
| Every other model | no branch | no heartbeat from this timer callback |

Thus **not every logical model, and not a general wired-GIP heartbeat**. The timer is started by receiver opener `c4e070` and generic HID opener `c484d0` [HID/OTHERHID]. The callback does not independently check `isHID`; its transport is nevertheless HID. Merely having logical202f on another backend does not prove it emits this receiver frame.

**O factory `0x180c4d5f0` [HID]:** opcode/routing key `0055`, parameter **0000**, channel **04**, count **0**, total **0**, source/wire offsets **0**, null buffer → **no payload**; copyIncoming=false. Caller `c4cbd0` explicitly sets timeout **0** and retry limit **0**. `c4de30` returns true with an empty normal receiver queue, false if first queued opcode differs from55; nonempty traversal has an unresolved indirect continuation, so do not overstate complete gate semantics. `c4d7d0` special-cases opcode55 on3E: it serializes immediately **without enqueuing a new command**. Other queued ordinary configuration commands normally suppress ticks through the gate.

**O acknowledgment:** success/failure/timeout thunks above all return0 via `0x1807e3670` [ZERO]; no ReadKey publication, report-mode change, data copy, or successor command. Normal receiver parser can decode an echo55 reply, but the newly sent heartbeat is not pending in its normal queue. Therefore a standalone echo55 ordinarily finds no matching command and only logs the null lookup. **No acknowledgment is required or waited for; no exact heartbeat reply bytes are established.** Do not wait for echo55 before sending the next tick.

**O stopping:** `0x180cb2a50` [GIP] cancels/releases `DAT_1805b78a0`, sets it null, sends **nothing**. Test exit does not call it. Session gates also suppress heartbeat while read-state0, upgrading, or blocked by ordinary queued work. **U:** what receiver firmware does when ticks stop; no fallback opcode, grace interval, report-disable effect, or automatic return-to-XInput guarantee is established by Windows host code.

## Exact receiver output frames

`src/receiver.rs::frame()` and Windows `0x180c4bf40 → 0x180c4c260 → 0x180c4b440 → 0x180c4b550` [TX/HID]: 32 bytes including ID, `81 | 16+copied_count | channel | LE16(op) | LE16(param) | LE32(count) | LE32(total) | LE32(offset) | payload | zero-padding`. All three requests below have count/total/offset0, empty payload, channel4.

```text
Enable normal Windows report state; NOT a separately proved Test-start command:
frame(0x0007, 0x0110, 0, 0, 0, &[])
81 10 04 07 00 10 01 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00

Global heartbeat; repeat at 400 ms while its session gates permit:
frame(0x0055, 0x0000, 0, 0, 0, &[])
81 10 04 55 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00

Disable before configuration; NOT an evidenced Test-exit restore:
frame(0x0007, 0x0000, 0, 0, 0, &[])
81 10 04 07 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
```

**Start/poll/stop answer:** no Test-start-specific frame recovered; known session enable above, **additional periodic55 recovered**; no Test-exit-specific frame recovered. Stop heartbeat by stopping its host timer, not by inventing an opcode. Known `7/0110` is restoration after a client deliberately disabled ordinary reporting, not a proved command to clear any heartbeat-maintained extended state.

## Receiver input paths and headers

The supplied full IR has exactly two HID input-report accessor callsites (`0x1808f1800`), and two corresponding HID demultiplexers. This is a bounded static inventory, not proof about unavailable firmware or a different Windows binary.

| Path | Accepts `raw[1] != 14`? | Publishes physical ReadKey? | Receiver applicability |
|---|---|---|---|
| `c4e4e0 → c4e5f0 → c4e6f0` [HID/RX] | **Yes**, all arrays reach config parser before the length test; parser itself ignores outer length | only independent `raw[0]==81 && raw[1]==14`, whole raw array unchanged | **Proved**: callback registered by receiver opener `c4e070` |
| `c4a290 → c4a3a0 → c4a4a0` [TX] | Yes, config parser before same length test | again only `81/14`; no cmd3 exception | **Not receiver path**: generic opener `c484d0` [OTHERHID] assigns model+38 and attaches this callback |
| `cb34d0` [GIP] | no HID outer-length byte exists | logical3E/L5, configured messageID12, payload prefix **`04 00 03`** → whole vendor payload unchanged | **GIP/WinRT**, not registered by HID receiver opener; not evidence of a second receiver HID endpoint |
| `cabe30 → ca24c0` [GIP; transport spec] | generic GIP/update parser; different envelope | no new receiver physical publication in this chain | not the normal receiver parser; cannot substitute its special status cases |

**O receiver normal acknowledgment, `0x180c4e6f0 → 0x180c4ea70` [RX]:**

```text
raw[0]       81                         HID report ID
raw[1]       LL                         parser does not validate/use this as byte count
raw[2..4]    04 00                      response kind u16 = 4
raw[4..6]    OO OO                      echoed opcode LE16
raw[6..10]   CC CC CC CC                count LE32
raw[10..14]  TT TT TT TT                total LE32
raw[14..18]  SS SS SS SS                offset LE32
raw[18..32]  data / remaining report bytes
```

For kind5 the parser instead starts its16-byte header at **raw3**, payload **raw19**, lookup by header.firstu16 in alternate queue. Neither route requires `raw1==14`. Kind4 matches **pending echoed opcode**, not arbitrary command family. Completion copies data only if pending command copyIncoming=true; heartbeat sets false and is not queued.

**L opcode7 acknowledgment prefix:** `81 10 04 00 07 00 …`. This differs from the request's `81 10 04 07 00 …`! No byte-for-byte promise for all remaining bytes; parser matching uses kind4/echo7. **U heartbeat acknowledgment:** if supplied in ordinary response form its identifying bytes would be `81 LL 04 00 55 00 …`; `LL`, count, total, offset, payload and whether it is emitted at all are unproved. This is a parser-conditional pattern, **not captured expected traffic**.

**O receiver cmd3 boundary:** `81 LL 04 00 03 00 …` is accepted syntactically by `c4e6f0` as a kind4/echo3 response. With no pending opcode3, it logs a null lookup; it does **not** publish ReadKey merely because the prefix is cmd3. If `LL!=14`, the independent ReadKey gate also rejects it. No other receiver callback/handler accepting such a packet as physical input was recovered. `c4ea70` completion does not synthesize ReadKey. A changed `81/14` body, a different Windows input origin, or a different report convention remains possible; none is assumed.

### Ordinary receiver stream: established live layout

**L:** `raw0=81`, `raw1=14`; content is raw2..21 inclusive. Raw22..31 is stale buffer residue, not input, battery, or paddles. Only raw2..13 is decoded in live captures / `src/receiver.rs::pad`:

| Field | Offset / bits |
|---|---|
| buttons LEu16 | raw2..3 |
| DUp,DDown,DLeft,DRight | raw2 masks01,02,04,08 |
| Menu,View,L3,R3 | raw2 masks10,20,40,80; Menu/View naming follows XInput convention |
| LB,RB,Xbox | raw3 masks01,02,04; LB/RB naming follows XInput convention |
| A,B,X,Y | raw3 masks10,20,40,80 |
| LT,RT | raw4,raw5 u8,0..255 |
| LX,LY,RX,RY | LEi16 at raw6,8,10,12 |
| raw14..21 | no paddle encoding established; zero in supplied Linux observations |

**U:** receiver L4/R4/P1/P2 bit/byte positions. The profile's32-bit physical masks are **not** the receiver's16-bit XInput masks. Do not place them in raw2..5 or in residue.

## Vendor physical renderer: exact keys/axes, not a proved receiver packet

**O:** native renderer **`0x180d10a10` [RENDER]** uses array indices16..25. Managed array data begins at object+10, so its accesses object+20..29 mean **array[16..25]**, not raw32 object offsets. Its UI fields match TestController and its hold-back display. **I:** this is the reflected `ReadKey` renderer; the reflected name→native-entry metadata bridge was not independently decoded. The direct byte accesses and masks below are observed regardless. **O:** GIP callback `cb34d0` publishes matching kind4/cmd3 vendor payload; **U:** no receiver-HID adaptation into this renderer was found.

Let **Q** be vendor payload with kind4/cmd3 at Q0..3, **K=LE32(Q16..19)**. Masks come from [spec-profile.md](spec-profile.md), encoder `0x180c8bbf0`, decoder `0x180c8d2c0`, label decoder `0x180c08730` and its cited Evidence IDs. These labels/masks identify the renderer's observed bit tests; they do not prove a receiver frame's origin.

| Key | K mask | Q byte / bit mask | Renderer field / use |
|---|---:|---|---|
| Menu / Start | `00000001` |16 /01| page+f8 |
| L3 | `00000002` |16 /02| page+d0 |
| R3 | `00000004` |16 /04| page+d8 |
| View | `00000008` |16 /08| page+f0 |
| X | `00000010` |16 /10| page+168 |
| Y | `00000020` |16 /20| page+158 |
| Right | `00000040` |16 /40| page+150 |
| Left | `00000080` |16 /80| page+148 |
| Down | `00000100` |17 /01| page+140 |
| Up | `00000200` |17 /02| page+138 |
| LB | `00000400` |17 /04| page+a8 |
| RB | `00000800` |17 /08| page+c0 |
| B | `00001000` |17 /10| hold-to-return state/timers |
| A | `00002000` |17 /20| page+160 |
| LT digital | `00004000` |17 /40| profile mask; renderer uses analog Q24 instead, no direct bit test found |
| RT digital | `00008000` |17 /80| profile mask; renderer uses analog Q25 instead, no direct bit test found |
| L4 | **`00200000`** |**18 /20 (bit5)**| page+b0 |
| P1 | **`02000000`** |**19 /02 (bit1)**| page+e0 |
| P2 | **`04000000`** |**19 /04 (bit2)**| page+e8 |
| R4 | **`40000000`** |**19 /40 (bit6)**| page+c8 |

`PL=P1` / `PR=P2` follows motion resolver and live BLE naming, but mapping UI has a documented swapped-image inconsistency. Preserve numeric masks internally; confirm physical left/right against an actual single-paddle capture. Xbox/Square/Star/Share/Profile are explicitly excluded by TestController text (`d711d0` [TEST]); no extra bits for them are inferred here.

**O `d10a10` [RENDER] axes:** LX,LY,RX,RY = **Q20,Q21,Q22,Q23** u8; neutral pairs127 or128 display0. Else display `(value−127)/128`, with zero forced−1, formatted `N5`; no Y inversion in this renderer. Triggers **LT=Q24, RT=Q25**, unsigned0..255, displayed `trunc(float32(value)/255*100)` percent. No IMU decoder in this function. Holding B starts a3-second timer (`d12170`, static3.0) and a10-ms UI animation timer (`d11f10`); release cancels them. These are **not USB polls**. Exit/cleanup `c51330` only changes UI subscriptions/timers.

**L BLE comparison, not receiver assertion:** `src/protocol.rs::physical_input` accepts the64-byte BLE report `81 1c 04 00 03 00 | twelve zero header bytes | K_u32 | LX LY RX RY LT RT | opaque28/29 | padding`. Its key/axis offsets are Q+2: keys raw18..21, axes22..25, triggers26..27. Applying that offset shift to a hypothetical receiver kind4/cmd3 envelope is mechanically plausible, **not evidence that receiver emits or Windows accepts it**. No exact receiver paddle-report `raw1`, full header, payload length, rate, or reply bytes can yet be specified.

## Safest focused probe: reproduce only the missing heartbeat

**Parent-owned experiment; not performed here.** Existing observed opcode7/0110 enable already acknowledged; preserve that state. Open a capture that retains **every complete vendor report**, including `raw1!=14`, and timestamps both directions. No profile write/commit, mode selector0a, calibration, firmware, or guessed parameters.

1. Capture idle/individual A,L4,R4,P1,P2 presses without55 as baseline.
2. Send **only the exact32-byte heartbeat above every400 ms**, for one bounded observation window; repeat the same isolated presses/moves. Do not require an acknowledgment or filter reports through the current XInput-only parser. Retain entire report length/content; inspect raw14..21 of81/14 as well as new report kinds/lengths.
3. Stop the host heartbeat timer; continue passive capture through release/idle to measure any transition back. No Windows Test-exit restore frame is known. If this experiment changed reporting through opcode7 disable, send the known7/0110 restoration on the still-connected same receiver; do not claim it undoes an unknown firmware watchdog mode. Record normal XInput reception after stopping; do not conceal failure to restore.

This is one controlled heartbeat/no-heartbeat comparison, not opcode discovery. **I:** smallest state-matched reproduction of the concrete Windows/Linux difference; **U:** any positive change's exact firmware mechanism until captured. If no extended state appears, safest next evidence is a passive Windows USB capture from app startup through Test entry, isolated paddles, Test exit, app close, covering **all receiver interfaces**, with no speculative Linux writes. That capture must establish the missing input origin and any initialization command not present in the traced page paths.

## Acceptance status

| Question | Status |
|---|---|
| Exact additional Windows receiver command | **Closed:**55/ch4/p0/count0/total0/offset0/no payload, global400-ms timer; not Test-specific |
| Different7 parameter or polling read for Test | **Not found in traced entry/exit:** only known3E enable0110; no proof of a distinct Test-start packet |
| All receiver input paths, other lengths/cmd3 | **Static inventory closed:** config parser accepts other lengths, only81/14 publishes physical ReadKey; generic HID and GIP origins explicitly separated |
| Paddle/other-control layout | **Closed for vendor Q/GIP and observed BLE; open for receiver**; exact masks/axes above cannot be silently transplanted |
| Leaving Test / stopping heartbeat | **Host path closed:** unsubscribe/UI cleanup, no wire stop; global heartbeat continues. **Firmware fallback behavior open** |
| Exact receiver paddle replies | **Open:** receiver packet/header/length/rate not captured. Heartbeat comparison above is the focused probe; no fabricated expected frame |
