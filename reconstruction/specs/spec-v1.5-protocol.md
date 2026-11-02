# Ultimate 3E receiver protocol: Ultimate Software X 1.4.9.0 → 1.5.0

## Result and evidence boundary

**No wire-format change, new ordinary-configuration opcode, new channel, new checksum/encryption step, new input-tail readiness test, or 1.38 firmware gate was established in the inspected receiver configuration path.** The mapped serializer, queues, parser, report-control factory, heartbeat, version decoder, profile factories and profile codecs retain their old behavior. This is a static comparison, **not** a successful transaction against firmware 1.38. The reported firmware regression and the official app's continued operation remain real, unexplained observations; unchanged host code does not prove firmware accepts our traffic.

**O** = observed binary/source fact; **D** = byte sequence derived from observed code plus stated cache/progression assumptions; **U** = unresolved. Addresses are loaded virtual addresses, not file offsets. All multibyte protocol integers are little-endian; raw offsets include the HID report-ID byte. No hardware was opened, no frames were sent, and no source code was changed.

| Artifact | SHA-256 |
|---|---|
| Old `UWPAdvance.dll`, 1.4.9.0 | `1f2325902b5d3ca2cc7b44405243247d3f276c39f99a66c97b193545be038352` |
| New `UWPAdvance-1.5.0.dll` | `0d8e65173358417a8194c3d6165eee4a3c9aa22df2a6b799dc1f46e648df8d43` |

| Evidence alias | REA Evidence ID | Coverage |
|---|---|---|
| P149 | `ev_27b17ea8ab3448ba35dbc4d5e85c7f344b092532f5d390b151d2847b8c65cec9` | Old disassembly, `[0x180c40000,0x180cc6000)` |
| P150 | `ev_e9b8bf48fb025fe350335b543af305e4ce86b15e1502987add0f425d642c77d0` | New disassembly, same window |
| U149 | `ev_58eae2376814146ebc458a01431a64b186ab31dd6e8b8559d8859fc1cc97286a` | Old disassembly, `[0x180d00000,0x180d40000)` and `[0x180d90000,0x180dc0000)` |
| U150 | `ev_b4f614a876321a5b7c20ed5c1ef58529a4f868de1f807d7b792610d5d048d5e3` | New disassembly, same two windows |
| Hcodec | `ev_b7bcc2da3eb329a704f93efab52acba24374640fba0b13173ac77a1ad8236dd5` | Historical old whole-profile and slot codecs |
| Hopen | `ev_d8af8df7e5b2e999018e706966432d5dbad6401f998b73125b730ac5ae5525ec` | Historical old receiver opening, queues and heartbeat |
| Hparse | `ev_eb24534b10694e07513f3199072969fa0bd9f4773ffe06f575a25e5f5a7adae2` | Historical old receiver input parser/progress handler |
| Hinit | `ev_93ebe7869f5029053f0be92976d2129388189d0d2bf8ab59ca53c87ae6d53137` | Historical old discovery/report-completion UI handlers |

The four new captures are complete (`output_truncated=false`, exit 0, artifact digest unchanged). They reside outside the repository under `../8bitdo-3e-modifying-archive/v1.5.0/{old,new}-{protocol,ui}-disassembly-evidence.json`. REA's configured objdump launchers explicitly restrict the above address windows and select Intel syntax; their stdout, not a decompiler reconstruction, is the authority. The installed GNU objdump was `/nix/store/g2i5zxslx9msaqjy14hdp2af064i9pbv-binutils-2.46/bin/objdump`.

Method: delimit functions using each PE's `.pdata`, match instruction/constant sequences with relocated RIP operands normalized, then check direct callees and command routing. A uniform address delta is **not** a sufficient match. In particular, the two version launchers exchange code order: blindly adding `0x1010` falsely suggests exchanged retry limits. Ghidra MCP analysis timed out at 30 s; a separate CLI attempt reached its 330 s provider-startup deadline without a function dossier. No new Ghidra result is claimed.

## 1. Old-anchor → new-counterpart map

All rows below are covered by P149/P150 unless explicitly marked U149/U150 or offline. “Same” means the inspected instruction logic/constants and corresponding called operations agree, not identical relocated file bytes.

| Role | 1.4.9.0 | 1.5.0 | Result |
|---|---|---|---|
| Receiver HID header/payload serializer | `0x180c4bf40` | `0x180c4cf50` | Same; 796-byte function |
| Receiver length/channel wrapper | `0x180c4c260` | `0x180c4d270` | Same; 196 bytes |
| HID output async wrapper | `0x180c4b440` | `0x180c4c450` | Same; 269 bytes |
| Receiver output/report-ID insertion | `0x180c4b550` | `0x180c4c560` | Same; 494 bytes |
| GIP/update serializer | `0x180c4aef0` | `0x180c4bf00` | Same; not normal receiver framing |
| GIP/update channel wrapper | `0x180c4b240` | `0x180c4c250` | Same |
| Receiver reply parser | `0x180c4e6f0` | `0x180c4f700` | Same; 896 bytes |
| Receiver progress/ack handler | `0x180c4ea70` | `0x180c4fa80` | Same |
| Generic/GIP ack handler | `0x180ca24c0` | `0x180ca34d0` | Same; 2467 bytes |
| Global heartbeat timer creation | `0x180cc3090` | `0x180cc40a0` | Same |
| Global heartbeat callback | `0x180cb2aa0` | `0x180cb3ab0` | Same |
| Heartbeat dispatch helper | `0x180cb2ba0` | `0x180cb3bb0` | Same |
| Heartbeat launcher | `0x180cadc50` | `0x180caec60` | Same |
| Heartbeat command factory | `0x180c4d5f0` | `0x180c4e600` | Same; opcode `0055`, channel 4 |
| **Receiver** version launcher | `0x180caeb20` | **`0x180cafcd0`** | Same: HID queues, 2 s, retry limit 5 |
| **Wired/GIP** version launcher | `0x180caecc0` | **`0x180cafb30`** | Same: GIP queues, 2 s, retry limit 999 |
| Version factory | `0x180caa310` | `0x180cab320` | Same; channel 5, `00e1`, 40 bytes |
| Version decoder | `0x180ca1120` | `0x180ca2130` | Same; 1333 bytes; six displayed numbers already supported |
| Receiver full-profile read launcher | `0x180caf1a0` | `0x180cb01b0` | Same; 2 s, retry limit 5 |
| Full-profile read factory | `0x180ca3840` | `0x180ca4850` | Same; `0002`, 868-byte cached serialization |
| Full-profile write factory | `0x180c887c0` | **`0x180c89a00`** | Same; `0001`; nonuniform relocation |
| Receiver full-profile write launcher | `0x180cae960` | `0x180caf970` | Same; 1 s, retry limit 0 |
| Write-complete → commit callback | `0x180caeb00` | `0x180cafb10` | Same |
| Commit factory | `0x180c7fc50` | `0x180c80c60` | Same; `0006` |
| Receiver commit launcher | `0x180cae7c0` | `0x180caf7d0` | Same; 1 s |
| Receiver selector | `0x180cb1580` | `0x180cb2590` | Same; FF00:1, `2dc8:3024` |
| Model table/base | `0x1805c19a8` | `0x1805c19a8` | Offline static bytes/references; 3E member `+3c`, current logical PID `+48` |
| Receiver open wrapper/body | `0x180c4df50` / `0x180c4e070` | `0x180c4ef60` / `0x180c4f080` | Same |
| Receiver input wrapper/body | `0x180c4e4e0` / `0x180c4e5f0` | `0x180c4f4f0` / `0x180c4f600` | Same |
| Receiver normal/alternate queues | `0x180c4d7d0` / `0x180c4dbf0` | `0x180c4e7e0` / `0x180c4ec00` | Same |
| Generic normal/alternate queues | `0x180c7f2f0` / `0x180c7f710` | `0x180c80300` / `0x180c80720` | Same; distinguish from receiver queues |
| Report-enable/disable factory | `0x180caa500` | `0x180cab510` | Same |
| **Receiver** report-control launcher | `0x180cae1c0` | **`0x180caed90`** | Same; nonuniform relocation |
| Generic report-control launcher | `0x180cadd80` | `0x180caf0e0` | Same operation/routing; nonuniform relocation |
| Receiver heartbeat timer/callback | `0x180c4c9c0` / `0x180c4cbd0` | `0x180c4d9d0` / `0x180c4dbe0` | Same; 400 ms, host-state gated |
| Receiver initialization UI callback | `0x180d99b60` | `0x180d9b0f0` | Same; U149/U150: read state 0 then receiver disable |
| Physical input renderer | `0x180d10a10` | `0x180d11a90` | Same; U149/U150, 5363 bytes |
| Version display formatter | `0x180d1c9a0` | `0x180d1da20` | Same; U149/U150 |
| Version-read completion UI | `0x180d98de0` | `0x180d9a360` | Same mapped logic; U149/U150 |

Call-graph cross-check for the version-order trap: old receiver `caeb20` calls `c4dbf0/c4d7d0`; new receiver `cafcd0` calls `c4ec00/c4e7e0`. Old wired `caecc0` calls `c7f710/c7f2f0`; new wired `cafb30` calls `c80720/c80300`. The wired call site moves `0x180db15d3 → 0x180db5913` (U149/U150). Completion thunks are old receiver `df1640 → ca8a50`, new receiver `df2748 → ca9a60`; old wired `df1658 → ca8a50`, new wired `df2730 → ca9a60` (offline thunk bytes).

## 2. Discovery, opening, initialization, and ordering

**O, P149/P150:** selector `cb1580 → cb2590` calls the corresponding `GetDeviceSelector` wrapper `8f1870 → 8f2870` with usage page `ff00`, usage `0001`, VID `2dc8`, PID `3024`. Open body `c4e070 → c4f080` uses `FromIdAsync(deviceId,1)` through `8f1820 → 8f2820`; successful HID object goes to static `0x1805b7828`. It sets HID-active to 1, assigns the known 3E logical identity from model `+3c` into `+48`, installs the receiver input callback, starts its timer, and publishes `Notification_findBootDevice`. There is no new GetPID challenge, feature-report exchange, opcode, key, or handshake send in this mapped open body.

**O, U149/U150:** `d99b60 → d9b0f0` clears host read state at static-base `+10`, then invokes `cae1c0(0) → caed90(0)`. That is opcode 7/report disable, **not** a bootloader-entry command. The receiver launcher uses factory `caa500 → cab510`, timeout 1 s, retry counter 0/limit 2, and the receiver normal queue. Disable completion publishes the existing `Notification_EnableReport`; the name also covers successful **disable**.

**O:** report enable uses the same launcher with argument 1, encoded as parameter `0110` for 3E. It sleeps 200 ms and sets **host** read state `+10=1`. This assignment is not a new input-tail readiness bit or proof of an enable acknowledgment. Receiver heartbeat remains suppressed while this read state is zero; its callback also requires upgrade state `+14 != 1` and the existing pending-command/model gates.

**U, important acceptance limit:** static command factories do not establish one universal actual application wire trace. The inspected startup callback proves the disable first send; the receiver read launcher proves the first read packet when invoked. Native notification/reflection/UI dispatch and its current state determine the intervening selection. The old main read-dispatch function `d99370` maps to new `d9aaa0` (U149/U150); its 3E branch still calls the **generic** read launcher `ca3310 → ca4320`, not by itself the receiver-specific `caf1a0 → cb01b0`. Its new extra register load/reload around `d9ab32..d9ab44` preserves the existing update-UI flag test; it is not a firmware-version gate. Thus silently concatenating every available receiver factory as the official app's observed connect sequence would overclaim. No official-app USB capture was supplied or acquired.

The **code-supported receiver transaction prefix** is therefore conditional: open/register → disable7 → matching disable completion → invoke full-profile read2. No extra mandatory pre-read handshake was recovered on these mapped low-level paths. Version `e1` is a separate read; a mandatory **version-before-profile** order is not established. A profile-completion handler has a wired/GIP version call at old `db15d3` / new `db5913`; that is not evidence to move a receiver version request before its first profile read.

## 3. Exact receiver output framing

**O, P149/P150:** `c4bf40 → c4cf50` → `c4c260 → c4d270` → `c4b440 → c4c450` → `c4b550 → c4c560`.

| Raw byte offsets | Size | Meaning, unchanged |
|---|---:|---|
| 0 | 1 | report ID `81` |
| 1 | 1 | `16 + actual copied payload bytes`; **not** `17 + count` |
| 2 | 1 | command channel, normally `04` or alternate `05` |
| 3..4 | 2 | opcode |
| 5..6 | 2 | parameter |
| 7..10 | 4 | header count |
| 11..14 | 4 | total transfer length |
| 15..18 | 4 | wire offset |
| 19..31 | 0..13 | payload, then zero padding through byte 31 |

Output is exactly **32 bytes including report ID**, not 31 total. The writer makes a zero-initialized 32-byte output array, writes `81`, and copies at most 31 bytes from its inner frame. Offline original/new static bytes retain inner size 32 and initial channel 0 at `0x1805c1b90/+4`; launchers set the channel dynamically. Chunk count is capped to 13 and remaining source length. Source offset and wire offset are separate command fields; normal whole-image operations keep them equal.

No added CRC, XOR, encrypted wrapper, nonce, sequence counter, or packet checksum occurs in these serializers. That says nothing about the pre-existing profile image's stored CRC. Header formation precedes the null-buffer special case: a null buffer may leave a nonzero **header** count but produce outer length 16. Actual full-profile/version factories supply nonnull cached buffers and do not use that special case.

The separate GIP/update serializer remains `[channel:u8, header16, payload, padding]` with **no length byte**, size 60 when not HID and 65 on its HID/update branch; payload caps 41/46. Other-model HID framing remains `81 04 ...`. Neither is a newly introduced replacement for the mapped normal receiver serializer.

## 4. Exact first frames and permitted sequence claims

These are **D**, generated from the observed 1.5.0 factories/serializers, not frames captured from the official app. Each row below is 32 bytes. Cold cache means the relevant outgoing cached slice is zero; a warm app can send different read payload bytes without changing opcode/count/total/offset.

### 4.1 Receiver disable, then the first profile chunk

The strongest code-supported receiver prefix, assuming the receiver-specific read launcher is selected after disable completion:

```text
1. report disable, opcode7/parameter0
81 10 04 07 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00

   wait for a matching normal-queue reply (kind0004, echoed opcode0007)

2. first profile read, opcode2/count13/total868/offset0, cold cache
81 1d 04 02 00 00 00 0d 00 00 00 64 03 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
```

For a warm cache, replace only raw `[19..31]` in frame 2 with serialized profile bytes `[0..12]`. **No evidence-backed different first frame for firmware 1.38 was found.** In particular, the already attempted disable/version/heartbeat/enable bytes do not become obsolete merely because functions moved.

### 4.2 Separate receiver version request

```text
first version chunk, channel5/opcodee1/count13/total40/offset0, cold version buffer
81 1d 05 e1 00 00 00 0d 00 00 00 28 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
```

Subsequent requests use returned `offset+count`; if replies advance 13 bytes, offsets/counts are `(0,13),(13,13),(26,13),(39,1)`. For offsets 13/26 retain the first frame and change raw15 to `0d`/`1a`. Last cold request:

```text
81 11 05 e1 00 00 00 01 00 00 00 28 00 00 00 27
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
```

Do not send these four frames as a blind burst or claim they necessarily precede the first profile chunk. Wait for the matching alternate reply and use its progress.

### 4.3 Other exact header-only frames, unchanged

```text
heartbeat0055, zero count/total/offset, no payload
81 10 04 55 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00

report enable0007/0110
81 10 04 07 00 10 01 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00

commit0006; never required for a read
81 10 04 06 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
```

Heartbeat remains 400 ms, opcode `0055`, channel 4, parameter/count/total/offset zero, no payload. The global timer chain and receiver timer retain their existing liveness/queue gates; do not treat the existence of the global heartbeat factory as proof it emits before configuration.

## 5. Receiver reply layouts the code expects

**O, P149/P150, Hparse:** parser `c4e6f0 → c4f700`, header reader `c4f210 → c50220`, progress handler `c4ea70 → c4fa80`. Neither parser checks raw0/report ID or interprets raw1 as a length before parsing. Dispatch selects only **raw2==4** or **raw2==5**, not a newly added report type. No new offsets/kinds or input-tail handshake flags were added.

| Field | Normal branch, raw2=`04` | Alternate branch, raw2=`05` |
|---|---|---|
| Header span | raw2..17 | raw3..18 |
| Header first u16 | raw2..3; normally kind `0004` | raw3..4; opcode/key, e.g. `00e1` |
| Normal echoed opcode / second u16 | raw4..5 | raw5..6 (parameter) |
| Count u32 | raw6..9 | raw7..10 |
| Total u32 | raw10..13 | raw11..14 |
| Offset u32 | raw14..17 | raw15..18 |
| Payload | raw18 onward; up to 14 bytes in a raw32 report | raw19 onward; up to 13 bytes |
| Lookup | normal queue, key raw4..5 | alternate queue, key raw3..4 |

Thus the **interpreted** normal disable reply prefix is `81 LL 04 00 07 00 ...`, profile-read reply `81 LL 04 00 02 00 ...`, write `81 LL 04 00 01 00 ...`, commit `81 LL 04 00 06 00 ...`. Alternate version prefix is `81 LL 05 e1 00 00 00 ...`. `LL` and unused/tail bytes are intentionally unspecified: the native parser does not determine the firmware's exact emitted value, and no firmware1.38 reply was observed. Do not invent a full acknowledgment byte string from the request.

Completion narrows count to u8, advances from returned `offset+count`, copies into the retained destination when requested, and invokes the existing callbacks when complete. Dispatch tests raw2's low byte; queue-removal completion separately compares the supplied unshifted raw2..3 u16 against exactly `0004`. Alternate completion preserves that pre-existing asymmetry. The Windows parser is not a recommended trust-boundary validator; the replacement's stricter bounds/identity checks remain a separate implementation choice.

## 6. Version payload and the apparently new six-number display

**O, P149/P150:** version factory `caa310 → cab320` remains opcode `00e1`, parameter 0, channel 5, count/total 40, source/wire offset 0, copy-incoming true. Offline static pointer slot `0x1805b7978` still refers to a 40-byte zero-initialized object: old `0x180653a50`, new `0x180653b60`. Receiver launcher is **`cafcd0`**, not `cafb30`; its timeout 2 s/retry limit 5 is unchanged.

Decoder `ca1120 → ca2130` reads **five** one-byte values from the 40-byte record:

| Record byte | Stored numeric global | Interpretation established by code |
|---:|---|---|
| 4 | `0x1805c1c20` | first component's version byte |
| 12 | `0x1805c1c28` | second |
| 20 | `0x1805c1c2c` | third |
| 28 | `0x1805c1c30` | fourth; user-reported changed component is `0x3b` |
| 36 | `0x1805c1c34` | fifth |

Each byte is converted with `ToString("X")` then parsed as decimal floating point: valid BCD-like `82` means decimal 82, not hex130; this is not an ordinary raw numeric cast. The other bytes in each 8-byte record are not assigned new meaning here. The user-supplied component identity/change `1.36 → 1.38` is a live observation, not inferred from arbitrary unknown bytes.

**O, U149/U150:** display `d1c9a0 → d1da20` already prefixes literal `1.` to component values. Its existing presence flag selects `1.A.B.C` versus `1.A.B.C.D.E`; the flag becomes true when both fourth/fifth decoded values are nonzero. Thus **six displayed numbers do not establish a new sixth component or changed decoder**. Old/new decoder instructions and display logic agree.

No `>=1.38`/`0x0138` branch was found in the mapped normal configuration/version path. Additional bounded offline search found no float/double literal1.38, and the new whole-disassembly immediate `0x138` load at `0x180cd3442` is a UI dimension312, not a firmware gate. This does **not** prove that no differently encoded tuple/comparison exists elsewhere in the complete application/firmware.

## 7. Full-profile read/write/commit and image size

**O:** profile image remains **`0x364 = 868` bytes**, four-byte header plus three `0x120`-byte slots. The managed configuration copy remains `0x1e8` bytes. Whole encode/decode and slot codec comparisons were performed offline against both SHA-bound PE files; Hcodec supplies historical old decompilation, not a new REA capture of the new codec.

| Codec role | Old | New | Observation |
|---|---|---|---|
| Whole decode | `0x1808e8cd0` | `0x1808e9cd0` | Same |
| Slot decode | `0x1808e8d30` | `0x1808e9d30` | Same |
| Whole encode | `0x1808e9590` | `0x1808ea590` | Same; wire slot offsets `004/124/244` hex |
| Slot encode | `0x1808e95f0` | `0x1808ea5f0` | Same |
| Deadzone decode/encode | `0x1808e8e40` / `0x1808e9700` | `0x1808e9e40` / `0x1808ea700` | Same |
| Lighting decode/encode | `0x1808e92f0` / `0x1808e9c20` | `0x1808ea2f0` / `0x1808eac20` | Same |
| Key-map decode/encode | `0x1808e94e0` / `0x1808e9e20` | `0x1808ea4e0` / `0x1808eae20` | Same |

Normal receiver request formula, unchanged:

```text
R(op,S,B) = 81 || u8(16+C) || 04
         || LE16(op) || LE16(0)
         || LE32(C) || LE32(868) || LE32(S)
         || B[S:S+C] || zero-pad-to-32
C = min(13,868-S)
op = 0002 read; 0001 write
```

Read B is the serialized **cached** profile, not an absent payload; write B is the desired serialized profile. Cold-cache first read is shown in §4. If replies advance13, there are67 chunks, offsets0,13,…858; final count10. A normal reply can physically carry14 bytes; use returned progress instead of hardcoding13. Wired/GIP remains cap41 and, with41-byte progress,22 chunks ending offset861/count7; that does not change the receiver contract.

Read completion remains notification/cache update; no commit follows a read. Write completion `caeb00 → cafb10` launches commit `cae7c0 → caf7d0` only after the whole write completes. Commit is channel4/opcode6, parameter/count/total/offset0, no payload. Stored profile CRC semantics are the existing profile protocol, not a newly introduced packet checksum. No replacement source migration is justified by a format delta found here.

## 8. `81 14`, physical/paddle `81 1c`, and tail bytes

**O, P149/P150:** receiver callback `c4e5f0 → c4f600` first calls the reply parser and then independently publishes the **entire unchanged raw array** as `Notification_ReadKey` only when raw0=`81` and raw1=`14`. It does not inspect tail bytes, connection-ready bits, paddles, or a firmware version. It does not newly publish `81 1c`.

| Raw offsets | `81 14` interpretation | Authority |
|---|---|---|
| 0,1 | `81 14` | Native gate, P149/P150 |
| 2..3 | XInput buttons u16 | Existing `src/receiver.rs::pad` and prior live observations, not a newly recovered native decoder |
| 4,5 | LT, RT u8 | Same |
| 6..7,8..9,10..11,12..13 | LX, LY, RX, RY i16LE | Same |
| 14..21 | no additional field established here | Unassigned |
| 22..31 | outside the stated20-byte inner report content | Opaque; not readiness/paddles |

The supplied post-update observations raw22..25 becoming zero and raw28 becoming `a0` are **not explained by a changed Windows-native parser**. Their firmware-side cause is unknown. In particular `a0` is not established as a configuration lock, authentication challenge, battery field, or mode-ready value.

The established reconstruction/live receiver physical layout is separate:

| Raw offsets | `81 1c` physical report |
|---|---|
| 0..1 | `81 1c` |
| 2..5 | `04 00 03 00` |
| 6..9 | count `0c 00 00 00` |
| 10..17 | zero in the reconstruction's accepted receiver shape |
| 18..21 | physical key bitmask u32LE |
| 22..25 | LX,LY,RX,RY u8 |
| 26..27 | LT,RT u8 |
| 28..31 | opaque; no new meaning established |

Native physical renderer `d10a10 → d11a90` (U149/U150) uses supplied vendor-payload Q offsets16..19 keys,20..23 axes,24..25 triggers, unchanged. The raw receiver table corresponds mechanically to Q+2, **but a native receiver envelope-stripping/notification bridge for `81 1c` was not established**. The receiver callback's `14`-only publication gate remains; an `81 1c 04 00 03 00` packet can enter ordinary kind4/opcode3 lookup, not a new automatic physical-input branch.

| Paddle | Physical mask | Q byte/mask | Receiver raw byte/mask |
|---|---|---|---|
| L4 | `00200000` | 18 / `20` | 20 / `20` |
| R4 | `40000000` | 19 / `40` | 21 / `40` |
| P1/PL | `02000000` | 19 / `02` | 21 / `02` |
| P2/PR | `04000000` | 19 / `04` | 21 / `04` |

The Rust `pad` reserved-bit check and `81 1c` acceptance bounds are reconstruction policy, not new Windows acceptance rules. No firmware1.38 physical capture or new bit-mask calibration was performed.

## 9. Confidence and unresolved prerequisite

| Requested question | Outcome / confidence |
|---|---|
| Map every supplied old anchor | Answered; high; version launchers and write factory require nonuniform mapping |
| Device selection/open changes | No change established; high for mapped path |
| Normal framing/header/chunk/checksum changes | No change established; high |
| Reply accepted kinds/offsets | Unchanged; high |
| Heartbeat/report control | Unchanged; high |
| Six-number decoder | Not new; high |
| Profile868/read/write/commit | Unchanged; high; new codec comparison is offline |
| New mandatory initialization or1.38 gate | None recovered in inspected paths; bounded negative result, not whole-app/firmware proof |
| Exact bytes produced by receiver factories | Answered conditionally on cache and response progress; high |
| One complete actual1.5.0 connect→first-read wire trace | **Unresolved**: current reflection/UI routing/cache state and official-app emission order were not observed |
| Why firmware1.38 ignores our correctly formed requests | **Unresolved**; no evidence-backed decisive protocol delta or safe new handshake bytes |

A supplied passive official-app receiver USB trace from open through first read, or authenticated resolution of the native runtime dispatch selecting every receiver launcher, is the exact missing prerequisite for the universal chronological sequence. Comparing the corresponding firmware/component or Windows transport behavior may then explain the regression; this report does not guess either. No tests/builds/formatters, hardware operations, firmware commands, or source changes were performed. The concrete first frames above must not be presented as a verified firmware1.38 fix.

## 10. Live findings after the firmware update (2026-10-25)

| Observation | Detail |
|---|---|
| Versions | GIP version record `033a0182 00000114 020d0120 00000000 033c0119 00000000 033b0138 00000000 01ef0130 00000000`: component 0x3b 1.36 → **1.38** (controller); others unchanged. Official app shows `1.82.20.19.38.30`, i.e. the BCD low bytes, confirming record = `[id, ?, BCD hi, BCD lo]`. |
| Mode after update | Profile byte 3 = 0x08 (PC flag 0x10 clear): controller in Xbox mode; the receiver re-enumerated as 202f by itself at 22:00. Slot key maps preserved. |
| Xbox mode (GIP) | `wired-version` and `wired-backup` work (read-only). |
| PC mode (3024) after `wired-xinput-mode` + controller power cycle | Input works, but no reply to report-disable, version or heartbeat, including with a sustained 400 ms heartbeat. `81 14` bytes 22..27 (forwarded physical sticks/triggers) are always 0 and byte 28 = 0xa0 (was `01 00 41 00`): the 1.38 controller no longer forwards vendor data through the receiver in PC mode. With no 1.5.0 protocol change, this points to a firmware/receiver incompatibility (receiver component not updated). |
