# Ultimate 3E wired GIP configuration

Scope: Windows Ultimate Software X 1.4.9.0 → Linux wired `2dc8:202f`. **O** = observed static code/metadata, **L** = parent's live fact, **I** = inference or proposed implementation, **U** = unresolved. No controller I/O performed for this specification. `spec-transport.md` describes the receiver, not this transport.

## Evidence

| Alias | Evidence / artifact |
|---|---|
| EH | `ev_d2692710de786f469fdb89e7ca96057ca2a9c1e36c209d759ab7df48f991566f`, UWP serializer/timers |
| EW | `ev_0f7dc9e574448e7a198ec66992d6190b9c1fadc9ec403f16081ab753dd3d9d06`, UWP WinRT wrappers |
| ED | `ev_8b9eeb346c176a2fc1a905dd8415c84b0b5ddb4553dce8ecb77e46cf3d0608ac`, D4X native provider |
| EE | `ev_b445501973f8ca5f428b52097008ef8690922d55bf9a0d67e91f55aeaf64eb0c`, D4X event dispatch |
| EM | `ev_f9911df72ea3446360e14fcf4f2cc0324adec9f3538f95d01fee3f97279d38a0`, inspected `.winmd` members; focused view `ev_ee9602d650d44d44a40e0ef636bc71b42042f0964ddffb4f2621009fed8b2634` |
| EC | `ev_e2c1bb70af3bd49debb62270da6c9c2c12e6002b9999ce601c7ccbdd2919d71c`, UWP input event |
| EP | `ev_e1c08f54e67d47430a3b24fcd30f6f0eb78303a16021d2fa9e5ed7f60557816e`, reply parser |
| EA | `ev_5508bec9c41458dd86db3b2fe75340db31a7ff9af717ed74e12cd75f53c5e1f4`, acknowledgment/profile read |
| ES | `ev_39b0c322e175c84dc4960238d244bd1606504337470bdca4d905deab3f3afd8d`, profile write |
| EF | `ev_a7c54ad48e1904243f4e2961d9a75c5e3e22653212ed980d869fdd2b5394ac62`, command factories |
| EL | `ev_141ebcaeab4cf5f83a08a8f72aa99685cbce008b31880266775e209b859b5881`, version/rumble launchers |
| EG | `ev_5219b564132d5009f8c50d6372f5534a2a887f820dbd3e944c76608594245243`, commit |
| ER | `ev_995b61fb2d386871a2381c093e0ecf87c8977cc598711f89669bb66531eae6d0`, report launcher |

UWP SHA-256 `1f2325902b5d3ca2cc7b44405243247d3f276c39f99a66c97b193545be038352`; D4X DLL `faf80aed960f4bb78ef51ade061cce398fd249de597b87c0d9b4244200ebea18`; `.winmd` `b4af28b7a0a275d6aff3bf6cff260ec2cbf026827b7f8381f7ba4e409a4dccd3`, MVID `1859daaa-e6d9-475b-b28d-9377824f6658`. Native references are in `reconstruction/ir/`. Static-byte/thunk observations below additionally read the original UWP PE, not a new REA observation.

Upstream xone references pin **dlundqvist/xone v0.5.7**, commit [`a16304ccdecc5bfc7aafdeb759c706da314d15fa`](https://github.com/dlundqvist/xone/tree/a16304ccdecc5bfc7aafdeb759c706da314d15fa), matching installed nixpkgs package. Local derivation `4wr2lins6k8dsy1bnwdi0y61qhdzfadk-xone-0.5.7.drv` produces `/nix/store/xix9asm6q56nmmya0i75f6vcn3nab1v0-xone-0.5.7`; source derivation `mcska5zndg4d5j6hj6myfywcgvd0rpgn-source.drv` records owner `dlundqvist`, repo `xone`, rev `refs/tags/v0.5.7`, URL `https://github.com/dlundqvist/xone/archive/refs/tags/v0.5.7.tar.gz`, NAR hash `sha256-9bflLH4lPGM7Ziv6w0+HC56jMU0IchL/9udbIqTIMd8=`, no package patches. Parent-observed module srcversion `EF4F5EB825C19A9A9B15C47`; module version string unset. GitHub tag resolves to that commit. Fetched v0.5.7 `wired.c`, `protocol.c`, `gamepad.c` are byte-identical to initially inspected master files, so line citations and audio1 analysis below apply unchanged; no module build or hardware verification performed.

## Windows boundary: payload, not raw USB

**O:** `0x180c4b340` [EH] calls `0x1808c6990(DAT_1805b7818,array,DAT_1805c1a78)`. The wrapper obtains array length/data and invokes interface slot **14** through `0x1808c6a40` [EW]. `.winmd` ID4XDevice method `SendMessageId(byte[],uint)` is token `06000024`, slot14 after IInspectable; adjacent `SendMessageA(byte[])` token `06000023` is slot13. Configuration uses **SendMessageId**, not SendMessageA, XInput vibration, HID, or a USB control transfer.

**O:** D4X `0x180001ad0` [ED] forwards `(provider,0,messageId,byteCount,data)` to provider slot6. This is [`GipGameControllerProvider.SendMessage(GipMessageClass,byte,byte[])`](https://learn.microsoft.com/en-us/uwp/api/windows.gaming.input.custom.gipgamecontrollerprovider.sendmessage). `0` is [`GipMessageClass.Command`](https://learn.microsoft.com/en-us/uwp/api/windows.gaming.input.custom.gipmessageclass), **not flags=0 and not flags=0x20**. D4X `0x180001aa0` [ED], the unused-in-this-chain SendMessageA method, instead fixes messageId to `0x0f`.

**O:** Original static DWORDs: `0x1805c1a78=0x13` (send), `0x1805c1a7c=0x12` (config receive), `0x1805c1a80=0` (other-model input receive). UWP initialization `0x18071fd70` does not overwrite them. Thus the configuration API call is exactly:

```text
D4XDevice.SendMessageId(P60, 0x13)
  → GipGameControllerProvider.SendMessage(Command=0, messageId=0x13, P60)
```

**O:** Neither app nor D4X selects GIP sequence, ACK flag, fragmentation flags, or expansion index in that API. Windows GIP provider supplies them. The recovered payload60 must receive a GIP header before usbfs sends it.

### P60 / P65 exact serializer

**O:** `0x180c4aef0` → `0x180c4b240` [EH]: `!isHID` chooses state60; `isHID` chooses65. State65 is HID/update branch sent through `0x180c4b440`, **not a second wired GIP mode**. Wired3E normal configuration chooses60.

All integers little-endian; offsets below are inside the WinRT message payload, excluding GIP transport header:

| Offset | Size | Value |
|---:|---:|---|
| 0 | 1 | channel, command+0x24: `04` normal config, `05` version/alternate |
| 1 | 2 | opcode u16 |
| 3 | 2 | parameter u16 |
| 5 | 4 | fragment count u32 |
| 9 | 4 | total length u32 |
| 13 | 4 | wire offset u32 |
| 17 | C | payload copied from backing array at **source offset**, not necessarily wire offset |
| 17+C..59 | variable | zero padding to **exactly60 bytes** |

State60 count cap **41**, state65 cap **46**: intermediate allocation=`state−1`; subtract2 reserved bytes and16-byte header. Final array begins with channel, copies at most `state−2` intermediate bytes, leaving two guaranteed trailing zeros at58..59 or63..64 when full. **No HID report ID and no outer length byte.** Header count is capped before marshaling. Unlike receiver serializer, GIP does not additionally clamp count to `total−sourceOffset`; factories/continuations provide remaining length. Null backing array sets mutable count0 only *after* header marshaling; do not infer a zero header count from absent payload.

`src/protocol.rs::WindowsCommand::gip60_buffer` already models this offline array. Transport implementation should reuse it, not mobile count43 or receiver count13 framing.

### Input path / reply offsets

**O:** [`IGipGameControllerInputSink.OnMessageReceived(timestamp,class,id,sequence,buffer)`](https://learn.microsoft.com/en-us/uwp/api/windows.gaming.input.custom.igipgamecontrollerinputsink.onmessagereceived) reaches D4X `0x180001800` [ED]. It copies payload and dispatches `MessageBReceived(device,id,byte[])` via `0x180002220` [EE]; timestamp, message class and transport sequence are **not forwarded**. `GetMessageB` exposes last buffered payload, but UWP uses event callback `0x180cb34d0` [EC], registered on device addition.

For messageId **0x12**, UWP calls `0x180cabe30` [EP]. `isHID=false`: it parses header **at payload offset0**, data at16. It does **not** strip a channel byte as the send serializer does.

| Reply payload offset | Size | Meaning |
|---:|---:|---|
| 0 | 2 | response kind u16 |
| 2 | 2 | echoed opcode / status / parameter word |
| 4 | 4 | count u32 |
| 8 | 4 | total u32 |
| 12 | 4 | offset u32 |
| 16 | count | response data; bound against actual received payload |

| Kind | Handling in parser/acknowledgment |
|---:|---|
| `0004` | look up normal queue by second u16; configuration acknowledgments |
| `0000` | look up alternate queue by second u16 |
| `0005` | second u16 `0700` tolerated; otherwise error handling |
| `e105` | special version queue lookup; entire destination copied, completes without normal offset/count progression |
| `0137/0139/013b` | status-coded operation replies; high byte must01 |
| `3700/3900/3b00` | alternate byte-order status forms; second word low byte must01 |

**O:** `0x180ca24c0` [EA] cancels pending timer before processing. If copyIncoming=true, copies count bytes to pending sourceOffset; special kinde105 or second word0100 copies entire destination. Ordinary next=`reply.offset+reply.count`; if next<requestedTotal, updates offsets/remaining and calls `0x180c4ad60` [EH]. Ordinary completion removes pending entry then invokes callback. This is vendor-protocol acknowledgment, independent of GIP transport ACK command01.

**O:** Before config parsing, 3E/L5 message0x12 with `payload[0..3] = 04 00 03` is routed **unchanged** to `Notification_ReadKey` and bypasses config parser (`0x180cb34d0` [EC]). Consequently vendor physical/calibration offsets are payload offsets, not standard GIP command20 input offsets; see calibration spec. Never treat these physical reports as config progress.

## Wired command transactions and timeouts

**O:** Same 868-byte3E image as receiver; see `spec-profile.md`. All channel4 replies normally kind4/echo opcode. Full write callback thunk `0x180df12e0` is original-PE JMP to `0x180c7f950` [EG], which launches commit. Commit success callback `0x180c7fc10` publishes save-complete notification. Read → disable/restore orchestration is higher-level, not automatic in the serializer.

| Operation | Channel | Opcode/parameter | Header count/total/offset; payload | Wired launcher; timeout/retries |
|---|---:|---|---|---|
| Disable vendor physical reporting | 4 | `0007/0000` | 0/0/0; none | `0x180cadd80` [ER] → `0x180caa500` [EF];2s, counter initialized4, factory limit9999 |
| Enable3E vendor physical reporting | 4 | `0007/0110` | 0/0/0; none | same; sleeps200ms then read-state1 |
| Read full profile | 4 | `0002/0000` | C/868/S; cached serialized image slice, cold zero-filled | `0x180ca3310` → `0x180ca3840` [EA];3s,retry limit2,copyIncoming=true |
| Write full profile | 4 | `0001/0000` | C/868/S; desired image slice | `0x180c88620` → `0x180c887c0` [ES];2s,retry limit0,copyIncoming=false |
| Commit | 4 | `0006/0000` | 0/0/0; none | `0x180c7f970` → `0x180c7fc50` [EG];2s,factory limit9999 |
| Read3E version | 5 | `00e1/0000` | 40/40/0;40-byte cached version buffer, initiallyzero | `0x180caecc0` [EL] → `0x180caa310` [EF];2s,retry limit999; special kinde105 |
| Runtime value/vibration command family | 4 | `0009/caller parameter` | 4/4/0; caller u32LE | `0x180caf7c0` [EL] → `0x180caa450` [EF];2s,retry limit2 |

For868-byte reads/writes, C=min(41,remaining). If responses advance41 bytes: S=0,41,...861, lastC7 →22 vendor transactions. Advance from validated returned offset+count, not a hardcoded step. Version40 fits one vendor transaction. Do not substitute receiver version fragmentation or its special reply header-at3 rule.

**O:** Timer `0x180c4b7a0` [EH] schedules timeoutSeconds×1000; `0x180c4b940` cancels timer then `0x180c4b9e0` increments counter and resends while counter<limit. Retry limit is additional sends after initial attempt. Retries repeat current fragment, not entire868-byte transaction. Windows huge limits above are facts, **not recommended replacement policy**. Replacement should use bounded deadlines, fail on malformed/no-progress replies, never commit an incomplete write, and restore report mode in a guarded cleanup when it changed it.

**U:** Opcode9 selector semantics are feature-specific; see rumble spec. It is **not** GIP motor command09: its raw GIP message type is13, payload channel4/header opcode9. Receiver live opcode9 produced no vibration. Prefer evdev for wired host vibration, below.

## GIP wire mapping

Primary sources: [MS-GIPUSB Message Header](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-gipusb/0c2e568f-8274-45ce-bc34-16b519e8a9f0), [Flags](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-gipusb/6f1fca56-3d86-40bd-8da6-94a2a724c04a), [fragmentation](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-gipusb/b7af910e-a999-40ad-a645-df110961477b), [message summary](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-gipusb/c66cee2f-7728-4ec1-8f3c-e9ab3aa4d696). xone [protocol.c L242–324](https://github.com/dlundqvist/xone/blob/a16304ccdecc5bfc7aafdeb759c706da314d15fa/bus/protocol.c#L242-L324) supplies implementation encoding.

| Wire offset | Field |
|---:|---|
| 0 | MessageType = data class bits7..5 + message number bits4..0. Command class0 + id13 → **13**; reply id12 → **12** |
| 1 | options/flags:80 fragment,40 initial fragment,20 System/internal,10 request transport ACK; primary expansion/client index0 |
| 2 | sequence; nonzero wrapping counter,0 reserved |
| 3.. | unsigned base128 length varint, continuation bit80; optional total/offset varint when fragment flag set |
| header end | message payload, length excludes GIP header |

Downstream header must have even size; normal payload60 uses four-byte header and fits64-byte USB interrupt packet. xone [wired.c L12–29,L127–183](https://github.com/dlundqvist/xone/blob/a16304ccdecc5bfc7aafdeb759c706da314d15fa/transport/wired.c#L12-L183) uses interface0,64-byte interrupt endpoints. Discover actual endpoint addresses from descriptors; do not hardcode guessed81/01. Interface1 is audio, not a second configuration channel.

**I, best-evidence first candidate:** vendor/non-System primary-device message, no requested transport ACK: `13 00 SS 3c || P60` (64 bytes). Rationale: custom id13 is Command-class, absent from system message summary; flags20 means **system**, not WinRT Command. App supplies neither ACK nor System choice. **U:** Windows provider's exact vendor flags are not established by app/D4X code; `00` is a protocol-grounded probe choice, not a captured Windows byte. Do not automatically try20,10,30 or other modes on timeout. A passive Windows USB capture or successful parent probe resolves it.

xone internally caps outgoing payload to58 and fragments larger messages ([protocol.c L408–435](https://github.com/dlundqvist/xone/blob/a16304ccdecc5bfc7aafdeb759c706da314d15fa/bus/protocol.c#L408-L435)); this conservative generic policy does **not** establish that Windows fragments P60. Direct64-byte single-packet vendor messages are the smallest wire representation consistent with recovered API array and USB packet capacity. Inbound metadata can still be GIP-fragmented; handle it separately from vendor profile chunks.

### Transport ACKs and sequence ownership

**O, upstream:** xone [protocol.c L438–461](https://github.com/dlundqvist/xone/blob/a16304ccdecc5bfc7aafdeb759c706da314d15fa/bus/protocol.c#L438-L461) acknowledges any incoming unfragmented packet with flag10, before dispatch ([L1663–1686](https://github.com/dlundqvist/xone/blob/a16304ccdecc5bfc7aafdeb759c706da314d15fa/bus/protocol.c#L1663-L1686)). Exact ACK for primary client0, received commandXX, sequenceSS, accepted lengthN, no remaining bytes:

```text
01 20 SS 09  00 XX 20 NNlo NNhi 00 00 00 00
^^^^^^^^^^^  ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
GIP header   9-byte ACK payload
```

ACK payload struct: byte0 control/unknown0; byte1 acknowledged command; byte2 `client|20`; bytes3..4 accepted length u16LE; bytes5..6 zero padding; bytes7..8 remaining u16LE. Echo **incoming sequence**, not own next sequence, and command12 for a vendor12 reply. For **unfragmented** packets without flag10, send no transport ACK. This ACK does not replace kind4/echo-opcode validation.

**O, fragmented exception:** xone [protocol.c L1605–1686](https://github.com/dlundqvist/xone/blob/a16304ccdecc5bfc7aafdeb759c706da314d15fa/bus/protocol.c#L1605-L1686) ACKs a nonempty fragment when flag10 is set **or when offset+length reaches the declared total**, even if that final data fragment does not request ACK. Report accepted cumulative offset+length and remaining bytes. Initial TLO is total length, then treated as offset0. All fragments retain sequence. Reaching total data length is **not dispatch/completion**: wait for the subsequent empty fragment, then dispatch assembled payload and free reassembly state. xone's fragmented branch does not separately ACK that empty completion packet. Do not feed fragments directly to the vendor parser or apply the unfragmented no-flag10 rule to the last data fragment.

**O:** xone [protocol.c L357–405](https://github.com/dlundqvist/xone/blob/a16304ccdecc5bfc7aafdeb759c706da314d15fa/bus/protocol.c#L357-L405) serializes output under adapter send_lock and assigns adapter-wide nonzero data_sequence, skipping0. Protocol documents a counter per MessageType; xone uses one adapter counter. No userspace API exports/imports it.

**I:** Exclusive takeover ends host-side xone sequence ownership; it does not reset firmware counters. Start our own nonzero counter for new vendor13 requests; do not assume reply sequence equals request sequence. Since xone does not send vendor13, it has no established vendor13 counter to continue in inspected source. ACK incoming transport sequences exactly. No evidence requires USB reset or matching xone's last generic sequence; whether firmware accepts a newly started counter is **U**, tested by the first read-only vendor request. Do not interleave usbfs and xone sends or submit competing IN URBs.

## Linux transport recommendation: exclusive usbfs, no module

**L:** Plan WIRED fact: xone owns `2dc8:202f`; no hidraw; usbfs root:root0664 → user cannot write. Wired FF available on evdev. **O:** xone has kernel bus/protocol APIs, not a generic userspace vendor-message endpoint. Receive dispatcher drops unknown noninternal commands such as12 ([protocol.c L1562–1603](https://github.com/dlundqvist/xone/blob/a16304ccdecc5bfc7aafdeb759c706da314d15fa/bus/protocol.c#L1562-L1603)); interrupt-IN URB is continuously resubmitted ([wired.c L50–82](https://github.com/dlundqvist/xone/blob/a16304ccdecc5bfc7aafdeb759c706da314d15fa/transport/wired.c#L50-L82)). Neither evdev nor FF carries arbitrary config messages. usbmon can observe, not supply a safe send/receive channel.

**Recommended implementation policy:**

1. Read cached sysfs descriptors/identity first; validate VID/PID/interface0 ff/47/d0 and interrupt-IN/OUT endpoint capacity64. Snapshot both interface drivers and evdev identity; stop active host FF before takeover. Open exact usbfs node `O_RDWR|O_CLOEXEC` only when a session is requested.
2. `USBDEVFS_DISCONNECT_CLAIM` with `{interface:0, flags:USBDEVFS_DISCONNECT_CLAIM_IF_DRIVER, driver:"xone-wired"}`. It atomically disconnects matching kernel owner and claims interface for this fd. No sysfs unbind/root helper/module. Never detach an unexpected driver or steal an existing usbfs owner.
3. Observe interface1 ownership. **Confirmed in matching v0.5.7 source:** xone claims audio1 in [wired.c L434–470](https://github.com/dlundqvist/xone/blob/a16304ccdecc5bfc7aafdeb759c706da314d15fa/transport/wired.c#L434-L470), but data-interface disconnect kills both URBs without releasing that extra interface ([L525–543](https://github.com/dlundqvist/xone/blob/a16304ccdecc5bfc7aafdeb759c706da314d15fa/transport/wired.c#L525-L543)). If interface1 remains bound to xone-wired, also disconnect-and-claim1 with the same guarded driver predicate; its disconnect has NULL data and returns immediately. This is ownership cleanup, not audio traffic. Keep audio altsetting unchanged/disabled; no audio configuration sends. Before handback, release1 so xone can claim it again. Otherwise reprobe can fail EBUSY: Linux [driver.c L552–563](https://github.com/torvalds/linux/blob/v6.18/drivers/usb/core/driver.c#L552-L563) rejects an already bound interface.
4. Submit interrupt-IN receive before first OUT. Drain/classify pending standard input/control packets for a bounded observation window. Do **not** issue USB reset, set_configuration, power-off, calibration, profile writes, or commit. Already initialized device may still accept vendor13: **I**, not established live. xone disconnect does not itself send power-off; gamepad remove only deletes rumble timer ([gamepad.c L532–541](https://github.com/dlundqvist/xone/blob/a16304ccdecc5bfc7aafdeb759c706da314d15fa/driver/gamepad.c#L532-L541)).
5. First vendor probe below. Send required GIP transport ACKs for any packet requesting them. Ignore independent standard input20 and vendor physical prefix040003. Bound every read/deadline; no retry storm. On fresh Hello02/metadata startup instead of an active stream, implement/complete standard identify04, metadata fragmentation ACKs, then power-on05/internal20/payload00; xone does announce→identify and gamepad probe→power-on/auth. This is a separate session-initialization step, not license to send arbitrary initialization blobs. **U:** which subset the3E requires after takeover, and whether its security/auth session survives; capture or implement evidenced GIP handshake before claiming cold-start support.
6. Cleanup on success/error: restore vendor report mode only if this session changed it; cancel/reap outstanding URBs; `USBDEVFS_RELEASEINTERFACE` for any claimed audio1, then0. Invoke `USBDEVFS_IOCTL` with `usbdevfs_ioctl{ifno:0,ioctl_code:USBDEVFS_CONNECT,data:NULL}` while fd remains open. **CONNECT is the nested ioctl, not a bare top-level request.** Check return value and subsequent bindings, then close. Closing/releasing alone does not promise xone rebind. Do not drop usbfs privileges before cleanup, since driver reconnect would then be rejected.

Linux [devio.c L2468–2501](https://github.com/torvalds/linux/blob/v6.18/drivers/usb/core/devio.c#L2468-L2501) implements disconnect-claim; [L2324–2376](https://github.com/torvalds/linux/blob/v6.18/drivers/usb/core/devio.c#L2324-L2376) reconnect; [L2605–2614](https://github.com/torvalds/linux/blob/v6.18/drivers/usb/core/devio.c#L2605-L2614) requires writable fd, not CAP_SYS_ADMIN. Existing privileges_dropped/LSM/interface authorization still apply. libusb [linux_usbfs.c L1778–1865](https://github.com/libusb/libusb/blob/v1.0.29/libusb/os/linux_usbfs.c#L1778-L1865) demonstrates release→nested CONNECT. No need to add libusb solely to implement these few usbfs operations.

### Permission rule

Proposed `/etc/udev/rules.d/70-8bitdo-3e-wired.rules`, installed by administrator, not created by this investigation:

```udev
SUBSYSTEM=="usb", ENV{DEVTYPE}=="usb_device", ATTR{idVendor}=="2dc8", ATTR{idProduct}=="202f", MODE="0660", TAG+="uaccess"
```

Active local logind session receives ACL; verify with `getfacl`/`test -w`. For this workstation without working seat ACLs, deterministic per-user alternative:

```udev
SUBSYSTEM=="usb", ENV{DEVTYPE}=="usb_device", ATTR{idVendor}=="2dc8", ATTR{idProduct}=="202f", OWNER="knd", MODE="0600"
```

Choose one access policy; never0666 or blanket vendor/device access. Installing/reloading/applying a rule is administrative host mutation, not a read-only probe. Device number changes across reconnects; rediscover by identity, not saved bus/device path.

## Parent first probes

### Entirely passive/read-only, xone retained

- Resolve USB device path from sysfs idVendor/idProduct; read cached `descriptors`, busnum/devnum, bConfigurationValue, interface bInterfaceNumber/class/subclass/protocol, endpoint descriptors, and `driver` symlinks. Cached sysfs descriptors avoid control transfers. Record interface0 **and1** owners.
- Read event-node sysfs ancestry, name/phys/id, `capabilities/ev`, `capabilities/ff`, permissions/ACLs. Use EVIOCGNAME/EVIOCGID/EVIOCGBIT/EVIOCGEFFECTS only; these query kernel state. Read normal input events without EVIOCGRAB if physical baseline needed.
- Read `modinfo xone_wired`/module version, existing kernel logs, and `/sys/bus/xone-gip/devices` to identify installed driver behavior. Passive usbmon capture can reveal standard traffic and sequence/ACK behavior; loading usbmon or enabling debug is host mutation, not required for this spec.

### Read-only *device operations* after exclusive takeover

These probes transmit protocol requests/ACKs and temporarily remove gamepad/audio input; parent alone runs them. They do not write configuration. Do not call them passive.

1. Bounded receive-only observation after takeover. If active input continues, try **one version request**, no report-disable:

```text
P60 = 05 || LE16(00e1) || LE16(0000)
         || LE32(40) || LE32(40) || LE32(0)
         || 40 zero bytes || 3 zero pad bytes
candidate USB = 13 00 SS 3c || P60
```

Require complete incoming GIP message12. Its **payload** starts `05 e1` (kindu16=e105), contains16-byte header then at least40 actual data bytes before applying Windows' whole-destination copy behavior. Record complete raw header/options/sequence and exact payload length. Do not blindly copy40 from truncated data or interpret receiver's repeated13-byte records as a wired version result. Timeout once → restore ownership, report unknown; do not spray alternate flags or bootloader/version-in-boot commands.

2. If vendor transport is proven, full profile **read opcode2**, no writes/commit:

```text
P60(S) = 04 || LE16(0002) || LE16(0000)
            || LE32(C) || LE32(868) || LE32(S)
            || C zero bytes || zero padding to60
C=min(41,868-S)
```

Recognize GIP12 payload kind0004/echo0002, count/total/offset at4/8/12, data16. Validate ranges, monotonic progress, advertised total, and stored CRC against assembled868-byte image. Continue only on matching reply; separate GIP ACK and vendor progress. `040003` physical messages do not advance read. If direct read while vendor reporting active is unsupported, stop; a later explicit disable7/read2/enable7 transaction is an additional state-changing probe, not silently part of this first read.

### Verify handback without sending rumble/config

- Nested CONNECT result>0 means an interface driver attached;0 means none. Re-read interface0 driver symlink: must be xone-wired, interface1 should be reacquired as before. `EBUSY` with usbfs still owner means release order wrong, not success.
- Wait for recreated xone-gip client/gamepad input node by USB ancestry, **not old eventN**. Query name/phys, EV_KEY/EV_ABS, FF_RUMBLE and normal input arrival. Inspect new kernel errors, especially audio claim EBUSY, metadata/auth failures, URB errors. Standard physical input plus expected driver/client/FF capabilities establishes read-only handback health, not configuration functionality.
- xone reprobe itself calls `usb_reset_device` ([wired.c L473–516](https://github.com/dlundqvist/xone/blob/a16304ccdecc5bfc7aafdeb759c706da314d15fa/transport/wired.c#L473-L516)); allow reenumeration/node changes and re-discover. Do not issue an additional userspace reset as routine cleanup. Physical replug is explicit recovery if reconnect fails, not a hidden successful fallback.

## Kernel vibration test: keep xone bound

**O:** xone [gamepad.c L107–117,L172–216](https://github.com/dlundqvist/xone/blob/a16304ccdecc5bfc7aafdeb759c706da314d15fa/driver/gamepad.c#L107-L216): `FF_RUMBLE.strong_magnitude` u16 → left body motor `floor(strong*100/65535)`; weak → right body motor, same scaling. It does **not** map either magnitude to trigger motors. Trigger fields remain0 from zero allocation. Packet motor mask0f enables all four, but trigger amplitudes are zero. Memless FF layer can advertise/emulate additional types; that is not independent physical constant-force hardware.

Direct GIP rumble payload9 bytes: `[00,0f,LT=00,RT=00,left,right,ff,00,eb]`; command09, flags/client0, length09 ([protocol.c L583–593](https://github.com/dlundqvist/xone/blob/a16304ccdecc5bfc7aafdeb759c706da314d15fa/bus/protocol.c#L583-L593)). xone coalesces/delays sends by10ms to work around firmware behavior; initialization sends zero amplitudes. Replay timing/stopping handled by `input_ff_create_memless`, not by changing the packet duration for every effect.

**I, host test design:** discover correct event node by wired USB ancestry+EV_FF+FF_RUMBLE; open writable, EVIOCSFF upload `ff_effect{type:FF_RUMBLE,id:-1,strong,weak,replay.length:bounded_ms,delay:0}`; write EV_FF event `{code:effect_id,value:1}`; stop with value0 on timeout/cancel/error, EVIOCRMFF cleanup. No EVIOCGRAB, usbfs takeover, vendor opcode9, persistent config save, or trigger-motor claim. Event-node write ACL needed separately from usbfs rule. No FF upload/play performed here.

## Residual unknowns / boundaries

- Exact Windows on-wire13 flags/ACK policy and actual12 reply lengths: app hands these to Windows; passive Windows capture or parent-owned probe needed. `13 00 SS 3c` remains a labeled candidate.
- Warm initialized takeover acceptance, sequence restart tolerance, surviving authentication and any required power-on/identify subset: not established by static lifecycle code. Do not advertise cold-start GIP transport until implemented/observed.
- Installed package source identity is resolved to v0.5.7 with no patches; live audio1 release/rebind behavior remains unexercised. Static source requires explicit audio1 ownership cleanup before xone's claim-on-reprobe; verify real bindings and input, not only CONNECT return.
- Wired version record semantics and physical vendor calibration stream contents are not live-proven; receiver observations do not establish them.
- Full GIP transport fragmentation/ACK state machine is separate from868-byte vendor fragmentation; robust metadata handling is needed if session restarts. No firmware/update/calibration command belongs in first probes.

Deliverable is a static transport specification and safe experiment order, not a claim of live wired configuration success. Existing receiver path remains the only profile read/write transport verified live in the plan.
