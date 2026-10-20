# Ultimate 3E factory-profile reset

O = observed static instructions/pseudocode or read-only backup calculation; I = implementation policy; U = unobserved runtime behavior. Original `UWPAdvance.dll` SHA256 `1f2325902b5d3ca2cc7b44405243247d3f276c39f99a66c97b193545be038352`. Offsets hex, inclusive; integers little-endian. No device writes or grabs performed.

## Evidence

- E-R = `ev_4f2cf40de1612289b89c2c2d3cfdefc0cb480302ce2f02723b27aef21f160553`, `reconstruction/ir/UWPAdvance.008.c`: factory slot reset `0x180c818d0` (lines114231–114373), mapping `0x180c806e0` (113500–113528), motion builder `0x180c80920` (113598–113609), light `0x180c80950` (113611–113655), deadZone `0x180c80a60` (113657–113743), structural slot initializer `0x180c80ce0` (113752–113972), name encoder `0x180c81390` (113974–114006).
- E-C = `ev_e8dd03488a043a96f2c19ef96e229bd1e7c239c5f922779afc0ad6f1e60b9801`, `UWPAdvance.009.c`, ResetView callback `0x180d16e70` lines76845–76854; Tournament callback `0x180d16a50`.
- E-M = `ev_b7bcc2da3eb329a704f93efab52acba24374640fba0b13173ac77a1ad8236dd5`, `UWPAdvance.003.c`: header/slot marshaling `0x1808e9590`/`0x1808e95f0`/`0x1808e9700`/`0x1808e9c20`/`0x1808e9e20`. Full layout and instruction-recovered marshaling tails: [spec-profile.md](spec-profile.md).
- E-S = `ev_b52f2e210711ee633e552e6b5b8742166e2fc57db085720350d4a92a4bd192b3`, Auto Reconnect `0x180d30a60`; E-P = `ev_63021b04bd745d1073996c0b85b85c262c3ecf03217d9777af95af65b58f721d`, `0x180d34860`.
- Instruction cross-check, original image, GNU objdump2.46: `nix shell nixpkgs#binutils -c objdump -d --start-address=0x180c818d0 --stop-address=0x180c81cf0 reconstruction/original/UWPAdvance.dll`; curve allocation cross-check `0x180c80a60..0x180c80b20`. This is direct original-image disassembly, not newly returned REA Evidence. Shared Ghidra target left unchanged; parent reports MCP-client timeout, so no duplicate import attempted.

## Actual reset, not structural initialization

O E-C `0x180d16e70` calls `0x180c818d0(0/1/2)`, clears boxed config object+`0xb`, then disables reports through `0x180cadd80(0)`. Subsequent ordinary full-profile write+commit chain is documented in [spec-maintenance.md](spec-maintenance.md). No separate pairing-erase/reset opcode established. **Live correction:** the host's byte3 clear does not change device byte3 through opcode1; the receiver reset test and explicit zero-byte3 write both read back `0x18` after commit ([replacement-plan.md](replacement-plan.md), factory-reset test). Rust therefore preserves both device-managed header bytes.

**O `0x180c818d0` does not call `0x180c80ce0` or the name encoder.** It clears all six marker words, allocates a new zeroed32-byte name, installs structural mapping constants, and copies light/motion/deadZone defaults with zero markers. All three stored names are empty, not `Config 1/2/3` or `配置 1/2/3`. This deliberately marks slots/subrecords invalid; firmware interpretation remains U. Do not replace these zeros with valid magic merely because it seems more useful.

O alternate structural initializer `0x180c80ce0` installs `0x20201123` in each marker and encodes localized `配置 1/2/3` or `Config 1/2/3` using `0x180c81390` (UTF-16BE, padded to32 bytes). That path is retained for the existing selected-slot structural reset/demo, not exposed as factory reset. Likewise UI/invalid-record mapping builder `0x180c8bf00` maps L4/R4/P1/P2 to0, unlike actual reset mapping below.

Instruction observations at `0x180c819ff/0x180c81a24` clear slot0/keyMap markers; `0x180c81a02..0x180c81a1b` allocates32-byte name. `0x180c81a3c..0x180c81a5f` constructs motion bytes `00 00 00 00 18 00 05 00 00 00 00 00`. `0x180c81aa8..0x180c81abd` installs feature flag0/value`002b6d00`. Identical slot1/2 branches use object offsets+b0/+150. Light/deadZone helpers initialize their flag fields to0; reset never overwrites them with magic. Curve array length2 is explicit at `0x180c80a79` and `0x180c80ad2` despite missing decompiler call arguments.

## Complete output byte contract

Header:

|Wire|Rust reset output / Windows host behavior|Evidence|
|---|---|---|
|000–001|Rust `factory_defaults` repairs CRC; Windows host preserves stored CRC|O E-M header copy; replacement CRC integrity policy|
|002|Preserve runtime slot index, including0xff|O E-C/E-R never write boxed object+0xa; E-M copies value+2; live runtime-slot behavior|
|003|Rust preserves entire input byte, including unknown bits; Windows host cache clears it|O E-C object+0xb minus8-byte box header = managed+3 = wire003 via E-M; live opcode1/commit cannot change device byte3|

Tournament/polling rate and Auto Reconnect are **not reset by the ordinary profile write**. Their Windows UI meanings remain 0x10 clear250Hz/set1000Hz and 0x08 Auto Reconnect (E-C/E-S/E-P), not measured polling rates. Separate setting commands and their safety collision are documented in [spec-header-bits.md](spec-header-bits.md); reset sends none of them.

For each slot `S=004/124/244`, all `0x120` bytes are determined:

|Relative S|Bytes/value after actual factory reset|Evidence|
|---|---|---|
|000–003,024–027,078–07b,080–083,0f8–0fb,104–107|six u32 marker words=0|O E-R `0x180c818d0` plus helpers|
|004–023|32 zero bytes, empty name in every slot|O E-R `0x180c818d0`|
|028–077|20 u32 cells: `2,4,00200000,40000000,200,100,80,40,2000,1000,10,20,400,800,4000,8000,02000000,04000000,8,1`|O E-R `0x180c806e0`; order per spec-profile|
|07c–07f|u32`002b6d00` = bytes`00 6d 2b 00`|O E-R `0x180c818d0`|
|084–085|u16 effect4 Fixed|O E-R `0x180c80950`|
|086–087|u16 speed5|same|
|088–08f|u32 pair`0000f144,007afc44`|same|
|090–097|u32 pair`fff71d44,e100af44`|same|
|098–0f7|24×u32`0064fb44`|same|
|0fc–103|`18 00 05 00 00 00 00 00`|O E-R motion builder/inline reset constants|
|108–10f|`08 00 25 21 42 42 5f 64`|O E-R `0x180c80a60`|
|110–117|same8 bytes|same|
|118–11f|`02 02 00 00 ff ff 00 00`|same|

Motion feature0x18 = Left Stick, Hold, continuous/Z, decoded bit3 set; sensitivity raw5/display6; compensation0; enable button None. No independent motion-enable bit established. Clearing the button is observable; claiming firmware completely disables gyro from these bytes would be U. Motion reserve and deadZone reserve semantics remain unknown, but their reset bytes are proven0, not guessed. No wire bytes remain materially undetermined. Unknown firmware invalid-record/pairing/restart behavior must not be presented as tested.

## Rust/UI policy

`Profiles::factory_defaults(&self)` returns the recovered slot defaults with explicitly repaired CRC and both device-managed header bytes2/3 retained. Idempotent. No read/write transport involved. Existing selected-slot structural reset reuses the same payload defaults, then sets valid markers and English names. Tests cover every byte3 value and slot0/1/2/0xff preservation.

Settings → **Restore factory profiles** stages all3 slots, empty names/markers, motion button None and loss of staged edits; Tournament/polling rate and Auto Reconnect remain unchanged. **Save backup first** writes the last controller read (`Model::baseline`, not staged edits) to `~/ultimate-backup-<Unix-nanoseconds>.bin` through `protocol::save_backup` (exclusive creation,0600, sync); backup failure prevents staging. Successful confirmation only replaces staged profiles; ordinary **Save + verify** owns persistent transfer/commit/read-back. Demo never writes backups. No pairing-erase command is sent; pairing preservation is not a hardware-tested claim.

## Exact read-only diff: receiver-before-edit backup

Input `ultimate-3e-profiles-receiver-before-edit.bin`:868 bytes, SHA256`8afcec341e46bec6ce31053b3f56e5dd0dd0c73f2c82b3fad5bfc896b851183f`, header`33 1d ff 18`.

Corrected Rust reset preview: **141 changed bytes,72 maximal contiguous spans**, CRC`c002`, header`02 c0 ff 18`. Header byte3 is preserved; the old preview's `02 c0 ff 00` incorrectly promised a change firmware ignored. Windows' CRC-preserving host serialization still differs:140 changed bytes/header`33 1d ff 00`; it is not the receiver's read-back image. The table below describes corrected Rust output; body bytes agree with recovered Windows constants. CRC excludes both runtime bytes2/3.

|Wire span|Before|After|
|---|---|---|
|000–001|33 1d|02 c0|
|004–007|23 11 20 20|00 00 00 00|
|009|43|00|
|00b|6f|00|
|00d|6e|00|
|00f|66|00|
|011|69|00|
|013|67|00|
|015|20|00|
|017|31|00|
|028–02b|23 11 20 20|00 00 00 00|
|034|02|00|
|036|00|20|
|038|04|00|
|03b|00|40|
|06f|00|02|
|073|00|04|
|07c–07f|23 11 20 20|00 00 00 00|
|082|1b|2b|
|084–087|23 11 20 20|00 00 00 00|
|0fc–100|23 11 20 20 29|00 00 00 00 18|
|107–10c|02 23 11 20 20 09|00 00 00 00 00 08|
|110|41|42|
|112|5e|5f|
|114|00|08|
|116–11a|21 20 41 41 64|25 21 42 42 5f|
|11c–11d|00 ff|02 02|
|124–127|23 11 20 20|00 00 00 00|
|129|43|00|
|12b|6f|00|
|12d|6e|00|
|12f|66|00|
|131|69|00|
|133|67|00|
|135|20|00|
|137|32|00|
|148–14b|23 11 20 20|00 00 00 00|
|156|00|20|
|15b|00|40|
|18c|02|00|
|18f–190|00 04|02 00|
|193|00|04|
|19c–19f|23 11 20 20|00 00 00 00|
|1a4–1a7|23 11 20 20|00 00 00 00|
|21c–220|23 11 20 20 2d|00 00 00 00 18|
|227–22b|40 23 11 20 20|00 00 00 00 00|
|244–247|23 11 20 20|00 00 00 00|
|249|43|00|
|24b|6f|00|
|24d|6e|00|
|24f|66|00|
|251|69|00|
|253|67|00|
|255|20|00|
|257|33|00|
|268–26b|23 11 20 20|00 00 00 00|
|274|02|00|
|276|00|20|
|278|08|00|
|27b|00|40|
|2bc–2bf|23 11 20 20|00 00 00 00|
|2c2|1b|2b|
|2c4–2c7|23 11 20 20|00 00 00 00|
|33c–340|23 11 20 20 29|00 00 00 00 18|
|345|40|00|
|348–34b|23 11 20 20|00 00 00 00|
|34e|24|25|
|350|41|42|
|354|00|08|
|356–357|21 20|25 21|
|359–35a|41 64|42 5f|
|35d|05|02|
