# 3E receiver session protocol — Windows 1.4.9.0

Status notation: **O** = observed in recovered native code, not observed USB traffic; **I** = inference/derived consequence, explicitly qualified. No live sends performed by this worker. Raw offsets include HID report ID. Original `UWPAdvance.dll` SHA-256 `1f2325902b5d3ca2cc7b44405243247d3f276c39f99a66c97b193545be038352`. Evidence aliases below expand to full IDs; every address paired with an alias is an address+evidence citation.

| Alias | evidence_id |
|---|---|
| E0 | ev_16d3a5bb323bdf1261c88a7959f69521325a1a32ba6a150def23e1df0ba63561 |
| EH | ev_d2692710de786f469fdb89e7ca96057ca2a9c1e36c209d759ab7df48f991566f |
| EQ | ev_d8af8df7e5b2e999018e706966432d5dbad6401f998b73125b730ac5ae5525ec |
| ER | ev_eb24534b10694e07513f3199072969fa0bd9f4773ffe06f575a25e5f5a7adae2 |
| EC | ev_141ebcaeab4cf5f83a08a8f72aa99685cbce008b31880266775e209b859b5881 |
| EF | ev_a7c54ad48e1904243f4e2961d9a75c5e3e22653212ed980d869fdd2b5394ac62 |
| EG | ev_5219b564132d5009f8c50d6372f5534a2a887f820dbd3e944c76608594245243 |
| EL | ev_39b0c322e175c84dc4960238d244bd1606504337470bdca4d905deab3f3afd8d |
| EP | ev_5508bec9c41458dd86db3b2fe75340db31a7ff9af717ed74e12cd75f53c5e1f4 |
| EB | ev_bf2d765510b165465516f7cc43ff16742ca8132510d1555563538cbec7cf3ca7 |
| ED | ev_a2e4840518558b197fcceb1990abcf9c6887bb5d5fb99228473bd2eea8351591 |
| EI | ev_e1c08f54e67d47430a3b24fcd30f6f0eb78303a16021d2fa9e5ed7f60557816e |
| EN | ev_e2c1bb70af3bd49debb62270da6c9c2c12e6002b9999ce601c7ccbdd2919d71c |
| EU | ev_93ebe7869f5029053f0be92976d2129388189d0d2bf8ab59ca53c87ae6d53137 |
| EA | ev_995b61fb2d386871a2381c093e0ecf87c8977cc598711f89669bb66531eae6d0 |
| EZ | ev_4f2cf40de1612289b89c2c2d3cfdefc0cb480302ce2f02723b27aef21f160553 |
| EM | ev_b7bcc2da3eb329a704f93efab52acba24374640fba0b13173ac77a1ad8236dd5 |

## 1. Discovery, state, and the misleading “boot” name

**O:** `0x180cb1580` [ED] enumerates `GetDeviceSelector(usagePage=0xff00, usage=1, vid=0x2dc8, pid=0x3024)` through `0x1808f1870`, obtains device IDs, dispatches `0x180c4df50` → async `0x180c4e070` [EQ]. Open uses `0x1808f1820(deviceId,1)`; successful device object stored in `DAT_1805b7828`. The app sets `isHID` static (base behind `PTR_FUN_1805c1a88`) to 1 and current logical PID (model-table base+0x48) to model-table+0x3c = 3E. It attaches input callback `0x180c4e4e0` → `0x180c4e5f0` [EQ/ER], then publishes `Notification_findBootDevice` (`DAT_1805b7a58`). This specific receiver path does **not** discover logical 3E by querying GetPID; it assigns known model identity from its receiver-specific selector.

**O:** `0x180d99b60` [EU], the boot-device receiver initialization callback, sets read state base+0x10 to zero then calls `0x180cae1c0(0)` [EA] (disable report; bytes below). `0x180c4e070` itself has no output send, reboot, or bootloader-entry factory. `0x180c4f2f0` [ER] schedules UI work; `0x180c4f350` [ER] invokes UI/navigation work, no protocol factory. `0x180c4c9c0` [EQ] installs a periodic timer (static interval 0.4 seconds); `0x180c4cbd0` [EQ] may subsequently send heartbeat `0x55` only after read-state+0x10 !=0, upgrade-state+0x14 !=1, and its pending-queue gate permits it. Thus “findBootDevice” is a host notification label, **not evidence of a command entering a bootloader**. Explicit firmware/update operations exist separately; do not call them during discovery.

**O:** Generic HID open `0x180c484d0`/body ending `0x180c4881e` [ev_db2d3d4de9130320e8aacfb3d5591e1ba436bd077841bebef9425e3872e847fe] sets logical PID to model+0x38 (other HID model), attaches `0x180c4a290` → `0x180c4a3a0`, publishes `Notification_FindeDevice`. Do not substitute this path for receiver3024.

**O:** Generic GetPID completion `0x180cafc00` [EC] publishes `Notification_GetPID`; handler `0x180d99ad0` [EU] reads little-endian `u16` from static buffer `PTR_PTR_1805b79c0` data+0x10, writes model base+0x48, refreshes UI, then calls `0x180cada70` [EA] (“version in boot” read). This is a second identity-learning path, distinct from direct receiver assignment.

**O:** Generic removal `0x180cb2c20` [EN] clears connection flag `DAT_1805c1b30`, removes matching opened device, emits `Notification_RemoveDevice` at `0x180cb30..`, clears pending channels through `0x180caa5c0`/related helpers, resets logical identity to model+0x44, restarts discovery (`0x180cb2a00`). Its decompiled VID/PID predicate uses several OR-like repeated inequalities; preserve observed predicate rather than infer an exact whitelist. `0x180d99910` [EU] removes connected UI pages. Receiver-specific removal registration/dispatch is not completely resolved; next probe: disconnect receiver under passive Windows log capture and record removeBootDevice versus RemoveDevice order.

## 2. Receiver framing and command state

**O:** Correct receiver serializer: `0x180c4bf40` [EH] → `0x180c4c260` [EQ] → `0x180c4b440` → `0x180c4b550` [EH]. HID output has 32 bytes including report ID: `raw[0]=0x81`; `raw[1..31]` are first31 bytes of a 32-byte zero-initialized inner frame. This is report ID0x81 plus31 output-data bytes, not a 31-byte total hidraw write.

```
raw offset  size  value
0           1     81
1           1     16 + actual copied payload count (NOT +1)
2           1     channel: 04 normal/config, 05 alternate command family
3           2     opcode u16 LE
5           2     parameter u16 LE
7           4     count u32 LE
11          4     total u32 LE
15          4     wire offset u32 LE
19          0..13 payload
remaining         zero padding, to32 total
```

**O:** `0x180c4bf40` allocates intermediate inner-size−1, reserves2 outer bytes and header16, caps request count to13, then caps to `total-sourceOffset`. It copies payload from `buffer[sourceOffset..sourceOffset+count]`, not wireOffset. Header uses wireOffset. Struct tuple stored at command+0x30 is `{sourceOffset:u32,wireOffset:u32,total:u32,remaining/requestCount:u32,opcode:u16,param:u16,pad:u32,buffer:ref}`. command+0x20 is routing key/opcode, +0x24 channel, +0x28 retry counter, +0x2c retry limit, +0x50 copy-incoming flag, +0x54 timeout seconds float, +0x58 timer reference.

**O:** Header is formed BEFORE the serializer handles null buffer. A null buffer or zero count forces mutable count to0 after header serialization; `0x180c4c260` then uses mutable count for outer length. Therefore a null-buffer read can retain nonzero header count but outer len16. **That is not the actual 3E full-read factory**, which supplies a nonnull cached-profile buffer. Do not infer header count from outer len alone.

**O:** Static constructor `0x18071fe20` [E0], behind `PTR_FUN_1805c1b80`, initializes the two HID queues only (`DAT_1805b7908`, `DAT_1805b7910`). Original PE static bytes at `0x1805c1b90` are inner size32; `0x1805c1b94` initially0. Channel is **not initialized to4 by this constructor**. Launchers write command.channel to static-base+4 before enqueue; continuation `0x180c4ad60` [EH] explicitly does this for 3E HID. Thus channel4 is dynamic per-command state. `0x18071fdc0` [E0], behind `PTR_FUN_1805c1a88`, zeros transfer-state fields. `0x18071fec0` [E0], behind `PTR_FUN_1805c1bd8`, initializes GIP queues; original size at0x1805c1be8 is60, next channel word initially0. Static-byte facts from original PE bytes parsed read-only, linked to source by these constructor addresses; no new REA Evidence ID was minted for raw PE parsing.

**O:** GIP/update serializer `0x180c4aef0` → `0x180c4b240` [EH] is different: size60 when !isHID,65 when isHID; first byte is channel; header begins1; no length byte. Count capacity41 (!isHID) or46 (isHID). !isHID sends through D4X `0x180c4b340` → `0x1808c6990`, using `DAT_1805b7818`, not the receiver HID handle. isHID update sends through the HID writer. Other HID model serializer `0x180c4c330` [EQ] prefixes `81 04`, no length. **Never mix those wrappers with receiver normal commands.** Android/mobile outer length convention (`header+payload+1`) is refuted for Windows receiver by `0x180c4c260`: Windows length is exactly16+payload count.

## 3. Factory table

All integer fields below little-endian. `C=min(13,total-sourceOffset)` for receiver HID; initial sourceOffset0 unless stated. Read factories use their cached zero-initialized/global buffer as OUT payload and IN destination; they are not payloadless. “Ack4” means receiver response kind4, echoed opcode; complete when reply.offset+reply.count >= requested total. Raw kind5 has separate routing below. **Factory availability does not prove a particular receiver UI action uses it; rows explicitly flag unconfirmed receiver wrappers.**

| Name / purpose | opcode | param | count / total / offset rule | payload | response expected | factory address | evidence |
|---|---:|---:|---|---|---|---|---|
| Disable physical report before configuration | 0007 | 0000 | 0/0/0 | none | Ack4 echoed7; publishes EnableReport only for disable | 180caa500; receiver launcher180cae1c0 | O EF/EA |
| Enable physical report afterward | 0007 | 0110 for3E (input1 converted); other model input1 stays0001 | 0/0/0 | none | ack optional to UI; launcher sleeps200ms and sets read-state1 | 180caa500 /180cae1c0 | O EF/EA |
| Full 3E profile read | 0002 | 0000 | C/868/S; S advances by response.offset+count | cached868-byte serialized blob slice[S..S+C] | Ack4 echoed2, chunk data; full-read notification | 180ca3840; receiver180caf1a0 | O EP/EC; wire868 EM |
| Full 3E profile write | 0001 | 0000 | C/868/S | serialized868-byte desired profile slice | Ack4 echoed1, then commit | 180c887c0; receiver180cae960 | O EL/EC |
| Commit saved configuration | 0006 | 0000 | 0/0/0 | none | Ack4 echoed6; save-complete notification family | 180c7fc50; receiver180cae7c0 | O EG/EC |
| ReadProfileFlag | 0002 | 0000 | initial8/8/0; receiver cap8 | global8-byte buffer b7988 | copy incoming; Notification_ReadProfileFlag | 180ca8570; launcher180ca8390 | O EB; receiver-specific wrapper not established |
| ReadCurindex/current slot | 0002 | 0000 | initial3/3/0 | global3-byte buffer b79a0 | copy incoming; Notification_ReadCurindex | 180ca74f0; launcher180ca7310 | O EB; receiver-specific wrapper not established |
| ReadVersion/3E version | 00e1 | 0000 | C/40/S; channel5; initialC13,S0 | global40-byte buffer b7978, initialzero | copy incoming; kind5 header at3 matches opcodee1; Notification_ReadVersion | 180caa310; receiver180caeb20 | O EF/EC; timeout2s retry5 |
| GetPID | 0008 | 0000 | initial2/2/0; channel5 | global2-byte buffer b79c0 | copy incoming; Notification_GetPID | 180ca9970; launcher180cafa60 | O EF/EC; generic launcher GIP queue, receiver wrapper unconfirmed |
| Boot version / memory read | 0005 | 0000 | total4, offset from static base+0xc (not assumed0); channel5 | global4-byte b7968 | getVersioninBoot notification | 180ca98c0; launcher180cada70 | O EF/EA; not first receiver identity probe |
| Heartbeat / connection poll | 0055 | 0000 | 0/0/0 | none | queue-liveness gated | 180c4d5f0 | O EQ |
| Runtime test value / vibration family | 0009 | caller u16 | 4/4/0 | caller u32 LE in b7998 | ack; collision/read-write gate | 180caa450; receiver180caf520 | O EF/EC; meaning of every param requires feature-specific mapping |
| Runtime mode selector | 000d | caller u8 promotedu16 | 0/0/0 | none | ack | 180ca9830; receiver180cae480 | O EF/EC; do not equate with local setPlat notification without caller trace |
| Partial byte write (generic model cache) | 0001 | 0000 | count1,total1,wireOffset1 or2,sourceOffset0 | L5 cache byte+9 or+10 | ack | 180ca3010 /180ca3260 | O EP; 3E reachability not established, do not use for3E |
| Calibration begin/end | 000e /000f | begin1 sticks,2 triggers,3 gyro; end1/2 observed | see maintenance spec | see maintenance spec | calibration notifications | 180ca5d30 /180ca5af0 | O ev_6784a1aee5aaccebeb1a1986c9aed07b3da8d79e15cbf78e3bac9753fb4be8b6; supplied peer slice |
| ReadKey | — | — | — | incoming physical stream, not a command factory | local Notification_ReadKey | 180c4e5f0 | O ER |
| ResetEvent | — | — | — | local cached-profile reset + UI event; no dedicated wire opcode proven | local Notification_ResetEvent | 180d2bea0 | O ev_1d31aa13fc6d1c2ca5e453b3b60c46b2bd02f1674f59ec326bd2aaf36b196ac3 |
| setPlat | — | — | — | local UI/platform-selection notification, no dedicated wire opcode proven | local Notification_setPlat | 180d2d9a0 | O ev_bfeeb44dccaf09bbf1998f3e22f24c1bbbeca811eb47433d5ab24c0eba5ac8f8 |

setPlat is a local notification, **not sufficient authority for a wire platform command**. Next static probe: follow each setPlat subscriber through reflection dispatch. Do not invent a platform opcode from the notification name. Dedicated firmware/update factories, identity challenge, memory erase/write are separate maintenance operations: see `local://spec-maintenance.md`; no firmware operation is part of ordinary profile read/save.

Immutable static buffers were resolved from original PE pointer table: b7980→180653a90 length5; b7988→180653ab0 length8; b79a0→180653b10 length3; b7998→180653af0 length4; b79c0→180653ba8 length2; b7968→1806539f8 length4. Initial data bytes zero. Original PE object layout length at+8,data+16; factories above explicitly consume lengths and references.

## 4. Input demultiplexing and response acknowledgment

**O:** Receiver input callback `0x180c4e5f0` [ER] gets HID InputReport buffer through `0x1808f1800`/`0x1808f17e0`/`0x180b79d30`, calls protocol parser `0x180c4e6f0`, then independently checks raw[1]==0x14 and raw[0]==0x81 and publishes the *whole raw array* as Notification_ReadKey. Therefore live idle stream `81 14 00…00 a0 00 00 00` is intentionally eligible physical/test input, not a malformed config acknowledgment. XInput interpretation of raw2..13 is parent-observed live fact; this worker has not recovered its consumer decoder. raw28=a0 has **no established battery meaning**. No byte28 consumer found in bounded direct byte-access search of chunks009/010; this does not prove absence.

**O:** Receiver config parser `0x180c4e6f0` [ER] uses header type `DAT_18000aad0` (16 bytes). Normal kind4 response:

```
raw0       report id81
raw1       outer byte (parser does not use it as response byte count)
raw2..3    response kind u16 LE =0004
raw4..5    echoed request opcode u16 LE
raw6..9    count u32 LE
raw10..13  total u32 LE
raw14..17  offset u32 LE
raw18..31  payload, at most14 bytes in32-byte report
```

Matches pending channel4 request by echoed opcode through `0x180c4f0e0`. If raw2==5: header starts at raw3, payload starts19; matches pending channel5 request by the header's firstu16 through `0x180c4efb0`. This asymmetry is real in receiver parser. Other-model normal HID parser `0x180c4a4a0` [EH] instead uses typeabf0 and header at2 even for raw2==5; do not copy that behavior to3024.

**O:** Receiver ack handler `0x180c4ea70` [ER] copies incoming payload into buffer at stored sourceOffset iff command+0x50 true; count argument narrowed to byte. Computes next=`reply.offset+count`. If next<requestedTotal, stores sourceOffset=wireOffset=next and remaining=total-next, invokes `0x180c4ad60` [EH] → receiver13-byte serializer. At completion, it removes from normal queue when its fifth short argument==4, alternate queue when==0, then invokes completion callback and sets retryLimit0. The parser passes raw little-endianu16 at raw2 as that fifth argument: normal04/00 removes correctly; channel5/e1 response gives e105 and skips these explicit removals while still invoking completion. Do not infer correct channel5 queue/timer cleanup from normal-channel behavior. No separate “next chunk ack” packet: next request advances progress. Header.total is logged; host requested total governs termination. Replacement must validate payload count/bounds and explicitly release its completed pending request rather than reproduce these unchecked/inconsistent cleanup paths.

**O:** Generic GIP/update parser `0x180cabe30` [EI] and generic ack handler `0x180ca24c0` [EP] are **not the receiver normal callback**. Their kind vocabulary:

| kind / first headeru16 | observed handling |
|---:|---|
| 0004 | find normal-channel request by second headeru16; complete/remove b79e0 queue |
| 0000 | alternate request by second headeru16; complete/remove b79e8 queue |
| 0001 | no dedicated branch in inspected parser/ack handler; no meaning assigned |
| 0005 | second headeru16==0700 is special tolerated case; otherwise error UI via cac3a0(...,1), cac820, return |
| 0137,0139,013b | status-coded replies; low byte37/39/3b requires high byte01; otherwise error UI type2 |
| 3700,3900,3b00 | byte-swapped status forms; require low byte of second word==1; otherwise error UI type2 |
| e105 (signed -0x1efb) | special lookup caaf20(e105), alternate queue; copy entire destination size and bypass ordinary progress test |

`0x180ca24c0` cancels its pending timeout timer at entry; ordinary progress=`header.offset+header.count`, except kind0,param4 preserves old sourceOffset; incomplete calls ad60; special e105 or second word0100 treated complete. These generic status forms must not be assumed to occur on receiver normal HID packets. Receiver handler lacks these special cases.

**O:** Timer creation `0x180c4b7a0` [EH] schedules seconds*1000 when timeout>0 and command.timer==null. Timer callback `0x180c4b940` cancels/releases timer then invokes `0x180c4b9e0`. If retryCounter<retryLimit, increments and resends through bea0(GIP) or bef0(HID), restoring channel static; otherwise removes queued command and logs “超时结束”. This inspected exhaustion path does not itself restore report-enable state or synthesize successful read completion. Generic ack cancels timer; receiver ack path's timer cancellation is not evident in c4ea70. No interchunk sleep in receiver ack continuation. Collision gates prevent duplicate opcode7/9 and read2 concurrent with write1 in high-level report togglers.

## 5. Exact receiver read transaction

**O:** Disable `0x180cae1c0(0)` [EA] → factorycaa500 → HID queuec4d7d0 → bf40. Timeout1s, retryCounter0,retryLimit2. Disable completion publishes Notification_EnableReport (reflection handler name `enableReportIDDone`). This notification name describes successful report-mode command, even when argument was **disable**.

```
disable, raw32:
81 10 04 07 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
```

**O:** Receiver read launcher `0x180caf1a0` [EC] → ca3840 [EP], sets timeout2s,retryLimit5, copyIncoming=true, uses HID queuec4d7d0. Unlike generic launcherca3310, this is receiver-HID serialization. Snapshot buffer comes from marshaling cached3E config, exactly868 bytes (0x364), see `0x1808e9590` [EM] and profile spec. First C13. Full raw request formula, *including payload*:

```
R(S,B) = [81, 16+C, 04]
       || LE16(0002) || LE16(0000)
       || LE32(C) || LE32(868) || LE32(S)
       || B[S:S+C] || zero_pad_to_32
C=min(13,868-S)
```

No count43 on receiver. Source/wire offset S advances from each response, not by blindly assuming thirteen response bytes. If every response count13, requests S=0,13,...858, final C10; 67 requests total. Response count may be14 within report capacity; use returned offset+count rather than fixed request-step assumption.

**I, cold-cache consequence of observed zero allocation:** c81440 [EZ] initially allocates zero managed object; fixed-array marshaler skips null arrays into zero output (profile slice EM). If cache has not been populated/reset before read, B is all zero. This allows concrete first packet; it is **not a universal packet for a warm Windows app**:

```
cold-cache first read, raw32:
81 1d 04 02 00 00 00 0d 00 00 00 64 03 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00

cold-cache last request if thirteen-byte response progression, S=858:
81 1a 04 02 00 00 00 0a 00 00 00 64 03 00 00 5a
03 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
```

Wait for Ack4 echo2 per chunk. Ignore independent physical81/14 stream; never interpret its zero raw2 as read progress. Finish only when reply.offset+reply.count>=868 with complete bounded assembled blob. `0x180caf340` [EC] publishes Notification_Read_CustomConfig_Onfinished **before** unmarshaling final buffer into cache via ca3750; then stores state `DAT_1805c1c38=1`. Replacement should avoid exposing stale cache to listeners even though Windows notification order is observable.

**O:** Restore report through cae1c0(1) → opcode7,param0110, no payload. Launcher sleeps200ms then sets read-state1. Raw32:

```
81 10 04 07 00 10 01 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
```

The low-level read launcher/ack callback do not themselves call enable1. Read→enable ordering is higher-level UI/event orchestration. Therefore guaranteed-finally restoration on timeout/disconnect is a replacement safety policy, **not proven Windows behavior**. No save/commit is required for reading. Read command OUT payload is cached data but opcode2 is observed read factory; no evidence that it writes configuration.

## 6. Exact receiver save transaction

1. **O:** Disable report as above; wait EnableReport completion before sending write. UI main save gating `0x180d994f0`/`0x180d996a0` [EU] invokes save dispatcher when appropriate model/save flag active; `0x180d99850`/`0x180d998b0` [EU] explicitly include3E.
2. **O:** Receiver write launcher `0x180cae960` [EC] uses factoryc887c0 [EL], timeout1s,retryLimit0,copyIncoming=false. R(S,B) formula above with opcode0001 instead0002 and B=desired868-byte profile. Count13 cap, total868, param0. Wait Ack4 echo1; advance from response.offset+count; no fire-and-forget burst.
3. **O:** On full write completion `0x180caeb00` [EC] immediately invokes `0x180cae7c0`, commit factoryc7fc50 [EG], timeout1s. Commit raw32:

```
81 10 04 06 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
```

4. **O:** Receiver commit launcher uses callback thunk `0x180df1618`; original PE bytes are `e9 f3 e5 e8 ff` = relative JMP `0x180c7fc10`. Destination callback [EG] emits Notification_Write_SaveCustomConfig_Onfinished. This closes receiver commit→save-complete bridge; raw thunk was parsed read-only from original SHA-bound PE, no new Evidence ID minted. UI sync-complete handler `0x180d2d9a0` [ev_bfeeb44dccaf09bbf1998f3e22f24c1bbbeca811eb47433d5ab24c0eba5ac8f8] refreshes cache/UI, emits local setPlat if relevant, calls receiver enable1 when isHID=true. Display success after commit acknowledgment, not after writes merely submitted.
5. No automatic timeout restoration proven. On error, fail save, do not commit incomplete transfer; attempt report-enable restoration only on still-connected same device. This last rule is replacement policy/inference, not an observed Windows packet sequence.

No receiver3E partial configuration writer was established. Full-profile write remains the evidenced save contract. Generic opcode1 one-byte factories listed above are not authorization to mutate arbitrary3E offsets.

## 7. Identity-probe wrapper boundary / unresolved items

**O:** Actual receiver3E version launcher `0x180caeb20` [EC] → factory `0x180caa310` [EF] → channel5 HID queue `0x180c4dbf0` [EQ] → receiver serializerbf40. Opcode00e1,param0000,total40, initialcount13,offset0; timeout2s,retryLimit5. Global40-byte destination/output buffer `PTR_PTR_1805b7978` initiallyzero, PE target0x180653a50. Generic five-byte version opcode0100/factoryca8a90 [EB] belongs to model+0x38, **not3E**. Its previously considered receiver wrapper is discarded.

```
actual3E version first request, cold static buffer, raw32:
81 1d 05 e1 00 00 00 0d 00 00 00 28 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
```

**O:** Apply receiver channel5 response parsing: raw2==5, header starts3, payload19. Match header.firstu16==00e1 against channel5 queue. Continue with S=reply.offset+reply.count until40; requestcount=min(13,40-S), outerlen16+count, OUT payload from static buffer slice. On13-byte progression requests S0,13,26,39 with counts13,13,13,1. ReadVersion completion thunks df1640/df1658 jump to ca8a50 [EB], which emits Notification_ReadVersion (thunk targets supplied by maintenance slice). `0x180d98de0` [ev_20188487ec5c072f82e76176d451b83e7a30e8efccef7406a0127858d7c587d9] calls decoder `0x180ca1120` [ev_a13d5baaa72edca91d65a8d89eded7946a08bc83cf2abada83dc24e447274864]. Its3E branch uses buffer byteoffsets4,12,20,28,36 → globals c1c20,c1c28,c1c2c,c1c30,c1c34. Each byte is formatted uppercasehex `"X"` then parsed as decimalfloat; e.g. packed12 becomes12, not18. Digits A–F cannot be assumed accepted. Component naming remains maintenance/UI mapping, not invented here.

**GetPID remains wrapper-unconfirmed:** applying receiverbf40 mechanically to factoryca9970 gives this candidate, **not an observed Windows receiver emission**:

```
81 12 05 08 00 00 00 02 00 00 00 02 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
```

| Unknown | next resolving probe |
|---|---|
| Receiver-specific GetPID caller and exact wrapper | Follow native reflection thunks for getPID in main view, or passive Windows USB capture during connect; direct3024 open assigns3E without GetPID |
| Physical81/14 consumer, byte28=a0 meaning | Follow ReadKey subscriber into Test page's concrete native handler; observe packet changes across charging/battery states without sending writes; do not label a0 battery |
| Does enabling0110 change raw physical packet format or merely enable delivery? | Parent-owned disable/read/enable capture; compare physical streams and command responses |
| Is full-read OUT payload ignored by firmware? | Only a firmware decompile or controlled parent-owned read-only opcode2 capture with known cache variations proves it; do not infer write behavior from payload presence |
| Exact save-notification→UI-handler reflection binding | Callback bridge is resolved above; follow notification subscriber registration to d2d9a0, or passive save capture on Windows if explicitly allowed later |
| Receiver-specific removal/reset cleanup ordering | Passive unplug logs + pending queue trace |
| Exact Bluetooth Windows path | Inspected native path is D4X GIP + separate BLE firmware machinery, not proof of a direct GATT profile client. Existing mobile ATT protocol is separate evidence. Do not reuse receiver wrapper on ATT |
| Complete3E command reachability beyond normal read/save | Enumerate native reflection command dispatch and cross-reference maintenance/profile specs; several generic factories above intentionally remain “receiver wrapper unconfirmed” |

No guessed bytes, battery interpretation, mode-entry command, firmware write, or implicit commit is part of the receiver read contract. This specification is sufficient to construct normal receiver-HID profile frames and parse their observed-code response contract; identity-wrapper and physical-input consumer gaps above remain explicit rather than silently substituted.
