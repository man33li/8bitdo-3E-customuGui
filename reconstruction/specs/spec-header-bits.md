# 3E device-managed header: Tournament, Auto Reconnect, slot

Scope: Windows Ultimate Software X 1.4.9.0, original `UWPAdvance.dll` SHA-256 `1f2325902b5d3ca2cc7b44405243247d3f276c39f99a66c97b193545be038352`. **O** = observed native code, not observed USB execution; **L** = parent-reported live hardware fact; **I** = inference; **U** = unresolved. No hardware I/O performed here.

## Evidence

|Alias|Evidence ID / source|
|---|---|
|FLAGS|`ev_8023db373ce5c5f936d7316790bab2182785b085f1fd57dd68cad289db929220`, `reconstruction/ir/UWPAdvance.009.c`|
|ON|`ev_7c1b07f8bb94eacac39855981b9110889346caf7056e72923ae04baf13ec8a91`, same IR|
|OFF|`ev_dc088566f316c976f0c12d168ab4cd2b78d1c1ab867671af93306a00044dcf42`, same IR|
|RATE|`ev_e8dd03488a043a96f2c19ef96e229bd1e7c239c5f922779afc0ad6f1e60b9801`, same IR|
|PLAT|`ev_63021b04bd745d1073996c0b85b85c262c3ecf03217d9777af95af65b58f721d`, same IR|
|SETTINGS|`ev_b52f2e210711ee633e552e6b5b8742166e2fc57db085720350d4a92a4bd192b3`, same IR|
|HID|`ev_d2692710de786f469fdb89e7ca96057ca2a9c1e36c209d759ab7df48f991566f`, `UWPAdvance.008.c`|
|QUEUE|`ev_d8af8df7e5b2e999018e706966432d5dbad6401f998b73125b730ac5ae5525ec`, same IR|
|RX|`ev_eb24534b10694e07513f3199072969fa0bd9f4773ffe06f575a25e5f5a7adae2`, same IR|
|BOOT|`ev_6784a1aee5aaccebeb1a1986c9aed07b3da8d79e15cbf78e3bac9753fb4be8b6`, `UWPAdvance.009.c`|
|SERVICE|`ev_a7c54ad48e1904243f4e2961d9a75c5e3e22653212ed980d869fdd2b5394ac62`, same IR|
|DISPATCH|`ev_a13d5baaa72edca91d65a8d89eded7946a08bc83cf2abada83dc24e447274864`, same IR|
|SLOT|`ev_0141225c72c8e41f104d3b3dbe988edd425e43d37008665a73526ea6a3817be6`, same IR|
|CACHE|`ev_e70a9da6317c68dc6ca1145adaeec51f1282b8987d885bd4818cb43f916e3d71`, same IR|

Instruction cross-checks used GNU objdump2.46 on this original image, not fresh REA evidence: `0x180c87250..0x180c87680`, `0x180d21120..0x180d211c0`, `0x180d34950..0x180d34a35`, `0x180df1238..0x180df12a0`. These resolve arguments omitted by pseudocode and callback thunk destinations. REA `procedure_assembly(0x180c874c0)` reported no active target; no target was opened/switched. Existing authenticated IR remains evidence authority.

## Windows setting actions and receiver route

|Setting|Observed UI/cache behavior|Factory and receiver dispatch|
|---|---|---|
|Tournament / polling|`0x180d16a50` RATE only refreshes label: byte3 bit0x10 clear=`250Hz`, set=`1000Hz`. Actual enable `0x180d21120` ON ORs cached byte with0x10; disable `0x180d21770` OFF rebuilds bits0..3, dropping all higher bits.|`0x180c87b50` FLAGS: opcode0x0a, parameter **boolean0/1**, ch4, zero source/wire offset/count/total, null payload, copyIncoming=false. HID launcher `0x180c87be0`; GIP alternative `0x180c879b0` is not receiver proof.|
|Auto Reconnect|Label/explanation `0x180d30a60` SETTINGS. Actual toggle `0x180d34860` PLAT: enable ORs cached bit0x08; disable reconstructs only bits0..2, also dropping cached Tournament/high flags.|`0x180c87430` FLAGS: opcode0x0c, parameter **boolean0/1**, otherwise same tuple. HID launcher `0x180c874c0`; GIP alternative `0x180c87290`. **OFF collides with firmware-entry inner header; blocked below.**|

O assembly: Tournament enable explicitly `mov $1,%cl` at `0x180d21182` before receiver `call 0x180c87be0` at `0x180d21189`; reconnect enable `mov $1,%cl` at `0x180d349d3` → `0x180c874c0` at `0x180d349da`, disable `xor %ecx,%ecx` at `0x180d34a0b` → `0x180c874c0` at `0x180d34a11`. Launchers preserve incoming CL into factories. Factory `0x180c87430` captures CL at `0x180c87437`, zero-extends to parameter u16 at `0x180c87489..0x180c8748d`. These are **not full-byte profileBit writes**.

O FLAGS launchers `0x180c87be0` and `0x180c874c0` set timeout float2.0, retry limit0, channel4 → `0x180c4d7d0` QUEUE. Queue inserts into receiver normal pending queue `DAT_1805b7908`, starts timer via `0x180c4b7a0`, sends tuple via `0x180c4bf40` HID (`0x180c4d910..0x180c4da6b`). HID serializer → `0x180c4c260` → `0x180c4b440` → `0x180c4b550`, reportID0x81 plus31 data bytes: same normal receiver path as verified read/write. Continuation `0x180c4ad60` HID explicitly selects model+0x3c/3E HID serializer, not GIP serializer `0x180c4aef0` and not other-model `0x180c4c330`.

Derived raw32 from observed receiver factories/serializer (not captured traffic):

```text
Tournament off: 81 10 04 0a 00 00 00 00 00 00 00 00 00 00 00 00
                00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
Tournament on:  81 10 04 0a 00 01 00 00 00 00 00 00 00 00 00 00
                00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
Reconnect off:  81 10 04 0c 00 00 00 00 00 00 00 00 00 00 00 00
                00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
Reconnect on:   81 10 04 0c 00 01 00 00 00 00 00 00 00 00 00 00
                00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
```

O expected host ack: receiver parser `0x180c4e6f0` RX, raw0=81, raw2..3=LE16(4), raw4..5=echoed opcode0a/0c, raw6..9=count, raw10..13=total, raw14..17=offset, payload raw18 onward. Queue lookup `0x180c4f0e0(echo)`; ack `0x180c4ea70` completes immediately for requestedTotal0. It does not validate a setting value. Count/total/offset allzero is a plausible device ack, **not observed device reply bytes**. No payload destination/readback because copyIncoming=false.

O callback success for both receiver commands is `0x180c87660` FLAGS (log + return1). Thunk `0x180df1298`→c87660 for Tournament; reconnect directly references c87660. Pre-send and failure thunks df1290/df1288 and df1250/df1248 jump to `0x180836810` (return1). No opcode6 commit, report-disable or follow-up read in these bounded action/launcher/callback chains. Thus **Windows does not issue these in those chains**, not proof firmware never needs prior state or that persistence/read visibility is immediate.

## Safety collision: settings versus firmware entry

**Do not send Auto Reconnect OFF. Same inner command as firmware entry; no device-side disambiguation proved. Host callback names, queue objects, timeouts and retry counts are not on-wire safety discriminators.**

Tuple layout: sourceOffset u32, wireOffset u32, total u32, requestCount u32, opcode u16, parameter u16, reserved u32, buffer reference. All omitted fields below zero.

|Path / UI provenance|Factory|Opcode / param / ch|Count / total / offsets / payload|Launcher / queue / serialization|Comparison|
|---|---|---|---|---|---|
|Tournament enable/disable actions d21120/d21770 ON/OFF|c87b50 FLAGS|0a /1 or0 /4|0/0/0/none|null copyIncoming=false; receiver c87be0→c4d7d0→bf40; GIP c879b0→c7f2f0|Only0a command factory found across all supplied `reconstruction/ir/*.c`, including D4X/firmware/reset/boot families and all channels. Literal routing-key/opcode tuple-assignment search also found unrelated metadata `0x1809f45a0` (`ev_ed499bdbf0928c0cd8ae1ea6f8e23465c283a2941f501d3cae3cc361f0948043`): +20=10, but references at+30/+38 and flags+40, no command allocation/tuple/send. No second opcode0a factory found. Search covers literal factories, not unknown device implementation or arbitrary computed opcodes.|
|Auto Reconnect OFF action d34860 PLAT|c87430 FLAGS|0c /0 /4|0/0/0/none|receiver c874c0→c4d7d0→bf40; GIP c87290→c7f2f0|**Entire tuple/header identical to ca69c0.**|
|Auto Reconnect ON, same action|c87430 FLAGS|0c /1 /4|0/0/0/none|same receiver/GIP launchers|Parameter differs from ca69c0, but required OFF transition remains unsafe.|
|Explicit upgrade-mode launcher; log `发送升级模式命令` / success `发送升级模式命令成功`; direct UI caller unresolved|ca69c0 BOOT|0c /0 /4|0/0/0/none|ca67f0 BOOT→c7f2f0; timeout10s/retries2; success ca69a0. Alternate ca8d80 SERVICE→c7f2f0, timeout0/retries0.|Inner header bytes `0c 00 00 00` +12 zeros, exactly Auto OFF. ca69c0 allocates same command type, copyIncoming=false.|

O boot factory `0x180ca69c0` BOOT zeroes all four locals, writes opcode0c at tuple+16 without any parameter, sets routing opcode0c and channel4. Auto factory `0x180c87430` FLAGS zeroes same locals, then writes opcode0c **and caller parameter** at same positions. Therefore parameter0 gives byte-for-byte equal tuple wire fields. Both are normal custom-header commands, not the later firmware channel5 E0/EA/E2/... stream.

O wrappers differ on known routes: receiver full report is `[81,10,04] || header16 || pad`; GIP serializer `0x180c4aef0` HID produces `[81,04] || header16 || pad` (no receiver length), or !HID sends channel+header over D4X. This proves distinct host serialization, **not that receiver firmware cannot interpret opcode0c/param0 destructively**. No receiver firmware-entry launcher or device decoder proves the required distinction. GIP Auto OFF and GIP firmware entry have equal wire fields and same queue/serializer. Consequently no combined `Receiver::set_profile_bits`, Auto Reconnect API or CLI arm added. Following the parent safety review, Tournament alone exposes `tournament_frame(bool)` and `Receiver::set_tournament(bool)`: exact raw32 vectors above, one send,2s kind4/echo0a wait, no retries/report-disable/commit. Frame-vector tests compare every byte for both booleans; parent received a `receiver-tournament on|off` CLI arm. API success acknowledges only the command, not observed rate/readback/persistence.

## Active slot / setPlat: cache selection, not a recovered device command

O `0x180d34cf0` PLAT writes boxed3E object+10 = wire byte2; `0x180d34de0` reads host cache, not device. Selected-slot actions `0x180d3a250` (slot2), `0x180d3a8a0` (slot1), `0x180d3aee0` (slot0) set host index and refresh UI. Valid slot2 branch d3a250→d34cf0(index2)→d35c20 (UI enable). At end, completed-read flag gates `0x180ca0e00` DISPATCH; 3E branch→`0x180c8e570` (host lock/cache object)→`0x180c9f260` SLOT. The latter chooses a valid host slot when index invalid, then calls c9eb40/c93900/c91450/c910e0/c908a0/c8cb80/c8c840 to populate cached editor fields. `0x180c9eb40` selects a cached slot mapping into `DAT_1805b78d0`. No command factory/receiver queue in this selection spine. `0x180c86cd0` CACHE is another cache-only slot setter.

O platform action `0x180d36600` PLAT emits local `DAT_1805b7a90` (`Notification_setPlat`) after cache/UI changes and `d34cf0(index0xff)`; `0x180d2d9a0` also emits it after ordinary save completion (spec-transport). Notification name alone proves no opcode. No select-slot factory/parameter/payload/receiver launcher or device ack recovered. Do not substitute opcode0d: `0x180ca9830` SERVICE is light/runtime selection, not proved active-slot selection. Do not substitute generic one-byte opcode1 factories ca3010/ca3260: other-model cache, not proved3E slot writes. **No `Receiver::select_slot` or CLI arm added.**

## Read visibility, commit and reset policy

L parent factory-reset test: opcode1 image with byte3=00 + opcode6 commit acknowledged, immediate full read still byte3=18; opcode1 cannot apply these bits. L byte2 changes on physical profile-button selection, including ff→0→1, visible next read; ordinary image writes do not establish a host-select command. O both header bytes are outside CRC (`spec-profile`).

U immediate device byte3 visibility after0a/0c, device side effects, bit preservation and power-cycle persistence are not observable from host cache mutations. Windows updates its cache **before sending** and does not reread it in those callbacks. Host byte2 getter is likewise not fresh device state. A correct report parser cannot turn these host observations into hardware readback guarantees. No hardware test is authorized here; the unsafe0c collision must be resolved from device-side/protocol authority before any such test.

`Profiles::factory_defaults` preserves input bytes2/3 exactly, resets only slot bodies, repairs CRC. Test covers all256 byte3 values with byte2=0/1/2/ff; fixture preview retains18, CRCc002,141 changed bytes. [spec-reset.md](spec-reset.md) distinguishes Windows cache clear from achievable receiver reset. This preserves unknown flags and avoids a deterministic Save+verify mismatch without sending any new command.
