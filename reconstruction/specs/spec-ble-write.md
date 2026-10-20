# Ultimate 3E BLE full-profile write — Android 2.00

Status: **static protocol recovered; BLE persistent writes NOT hardware-verified**. GUI Bluetooth profile reads are implemented; GUI read acceptance also awaits the parent probe. `gui::state::BLE_WRITE_VERIFIED = false` hides Save and rejects Save dispatch/worker writes on BLE. No BLE scan, connect or controller send was performed by this investigation. Receiver/profile/GIP semantics are unchanged.

## Identity and evidence

APK `8BitDo_Ultimate_Software_V2_Android_V2.00.apk`, SHA256 `7f225242d9c899b8b9c316ef3442c0679ecc0d6160cfe2e87fe89bb809e5f1e2`. REA `extract_artifact` materialized `lib/x86_64/libadvancelib.so`, SHA256 `5a0bb940a91c7f6eebb2c806faf13bd3e75cf982d06112177db690f087cf252c` (372496 bytes), extraction evidence `ev_06f8b8eb9a948b30fa8e33f0a8a5a0c84e95b851c8ed86bd82b5389aa1cfdd19`.

Durable canonical bundle: `reconstruction/ble-write-evidence.json`. Ghidra 12.1.2 imports this ELF at base `0x100000`; subtract that base for the original ELF virtual addresses below. No target native instructions were executed in this investigation.

| Original ELF address (Ghidra address) | Observed fact | Evidence |
|---|---|---|
| `0x30ff0` (`0x130ff0`) JNI `write3ECustomConfig` | CRC helper called with global image pointer and `0x364`; return stored at same pointer, then full-write channel started | `ev_581f6f89ab215f433714891da4a25c096a5c2d3cba4134574e9ac0b530a77eee` |
| `0x3aaa0` (`0x13aaa0`) `ANSI_CRC_16_sh` | Reflected CRC16, init `0xffff`, polynomial `0x8408`, no final xor; exact byte loop `[0,length)` | `ev_a9ebae27127a8374e3359964851b97ee759c59ff0a1ab5d04dbdee854361ae82` |
| `0x2aed0` (`0x12aed0`) `get3E_WriteCofigChannel` | Opcode 1, parameter 0, source global 3E image; data offset 0, total/remaining/end 868 | `ev_0d0a3140abe3037c4a170feec2b81cc4741dc484bf1cc3475de3c75a9a561bce` |
| `0x27280` (`0x127280`) `Generate_CustomDataCmd` | PID `0x202f` forces MTU 62, caps data to `62-19=43`; opcode 1 uses device destination offset; payload copied from source + advancing data offset | `ev_a9ebae27127a8374e3359964851b97ee759c59ff0a1ab5d04dbdee854361ae82` |
| `0x28080` (`0x128080`) `Send_CustomCmdPacket` | PID 3E frames `[81,length+1,channel]`, channel 4; pads full value to 62 bytes | `ev_0d0a3140abe3037c4a170feec2b81cc4741dc484bf1cc3475de3c75a9a561bce` |
| `0x28c90` (`0x128c90`) receive JNI | Header starts at RX byte 2; transport 1 count masked to low byte; kind 4 routes by command low byte and total, kinds 1/5 also have native routing | `ev_0d0a3140abe3037c4a170feec2b81cc4741dc484bf1cc3475de3c75a9a561bce` |
| `0x28810` (`0x128810`) `handleChannel` + `0x271a0` continuation | Counts advance pending channel; unfinished callback triggers next frame; completion triggers onfinished | `ev_a9ebae27127a8374e3359964851b97ee759c59ff0a1ab5d04dbdee854361ae82` |
| `0x30ed0` (`0x130ed0`) full-write onfinished | `isSendCoustom(6)` gates separate save channel; no implicit persistence assumed after chunks | `ev_a9ebae27127a8374e3359964851b97ee759c59ff0a1ab5d04dbdee854361ae82` |
| `0x2c0b0` (`0x12c0b0`) `getSaveCustomConfigChannel` | Opcode 6, parameter `0x0067`; count/total/offset 0 | `ev_a9ebae27127a8374e3359964851b97ee759c59ff0a1ab5d04dbdee854361ae82` |
| `0x34460` (`0x134460`) save onfinished | Calls Java `writeCustomConfigDone`; this is software completion, not proof of firmware persistence | `ev_0d0a3140abe3037c4a170feec2b81cc4741dc484bf1cc3475de3c75a9a561bce` |

Ghidra pseudocode is not original source. Indirect callbacks and several tail calls have provider flow-model limitations listed in the evidence. Byte fields above are supported by the recovered factories/framer; actual BLE write acknowledgment acceptance/persistence remains unobserved.

## CRC reconciliation: two real, incompatible coverage rules

JNI assembly at `0x31025..0x31032` (loaded `0x131025..0x131032`) moves the **unadjusted** global pointer to RDI, moves `0x364` to ESI, calls the helper, then stores AX at `[R12]`. The helper sets CRC to `0xffff`, byte index to 0; each iteration reads `pointer[index]`, runs eight reflected steps with polynomial `0x8408`, increments index, and continues until `index == length`. Thus Android's full-write preparation consumes **all 868 bytes, including the old CRC word and runtime/header bytes 2/3**, then replaces bytes 0/1. `ultimate_config.py` is correct about this APK path. This is not a different polynomial, initialization, reflection, or xorout.

Observed offline checks against every root `ultimate-3e-profiles*.bin` (all 868 bytes):

| File | Stored LE word | CRC `[4,868)` | Android CRC `[0,868)` |
|---|---:|---:|---:|
| `ultimate-3e-profiles.bin` | `a1c4` | `a1c4` | `adee` |
| `ultimate-3e-profiles-verified.bin` | `a1c4` | `a1c4` | `adee` |
| `ultimate-3e-profiles-before-native-clutch-1790102027225.bin` | `6fad` | `6fad` | `663f` |
| `ultimate-3e-profiles-receiver-1.bin` | `1d33` | `1d33` | `224b` |
| `ultimate-3e-profiles-receiver-prewrite.bin` | `1d33` | `1d33` | `1f62` |
| `ultimate-3e-profiles-receiver-before-edit.bin` | `1d33` | `1d33` | `1f62` |
| `ultimate-3e-profiles-receiver-20261015-2209.bin` | `4876` | `4876` | `720f` |

The separately named earlier BLE backup `ultimate-3e-appimage-profiles.bin` is byte-identical to `ultimate-3e-profiles.bin`: same `a1c4/a1c4/adee`. BLE `ultimate-3e-profiles.bin` / `-verified.bin` SHA256 `1d4f63d398241405f8dbceb07b7cc2557fd8b376a31827b23906c07d6dc00d8d`; before-native-clutch SHA256 `b512842c1af3f42caa022b269910c526da0fabf6ce807337696dda4e55dd4a8d`.

Result: **none** matches Android whole-image CRC; **all** match the body-only CRC. Receiver saves already verified the body-only checksum on hardware. Difference between receiver-1 and prewrite whole-image CRC despite equal body CRC also demonstrates why runtime/header bytes must not be folded into stored-image validation.

Unknown: firmware may ignore/recompute the Android-supplied CRC, or this vendor path may have a bug. Neither explanation is proven. Do not invent a checksum exception or rewrite Python evidence. Parent approved the lossless Rust API: `Connection::write_profiles(&[u8;868])` validates existing `profile::verify_crc` (body-only) and transfers **exactly the supplied image**, without Android's contradictory pre-send rewrite. This preserves identity-write/read-back semantics and is a deliberate difference in image preparation, not a claim of byte-identical JNI behavior. BLE firmware acceptance is gated on the parent probe.

## Frame and acknowledgment contract

TX: 62-byte ATT value, zero-padded. `[81,LEN,04]`, then little-endian `<HHIII>` at byte 3: opcode, parameter, count, total, destination offset; payload starts at 19. `LEN=17+payload_length`.

Sequence implemented: settle after subscription; disable reporting (7/0) and wait for application acknowledgment; 21 opcode-1 frames at offsets `0,43,...,817,860`, counts `43` twenty times then `8`, total `868`; acknowledgment required before advancing; commit opcode 6/parameter `0x0067` and wait; enable reporting (7/`0x0110`) **on success, malformed acknowledgment, timeout, send failure and cancellation**, ignoring cancellation during cleanup. Restoration failures explicitly report uncertainty and recommend reconnecting. No retry or guessed fallback opcode.

RX: ignored prefix bytes 0/1; at byte 2 little-endian `<HHIII>` = kind, echoed command, count, total, offset. Payload would start at byte 18; write acknowledgments require no payload. Rust accepts only kind 4, exact command/count/total/offset; unrelated notifications ignored. Type-1 count's low byte is used as in native code. Native accepts additional kinds 1/5 but their write success/error semantics are unverified, so they fail matching writes rather than falsely completing. Exact echoed offset/count validation is a conservative integrity condition; actual firmware replies must be checked by the parent. Report and commit expect zero count/total/offset. Parameter is **not echoed**, so an application acknowledgment alone cannot establish report-enable state; parent must observe normal input afterwards.

Transport 1 adds CRC16 init `0xffff`, reflected polynomial `0xa001`, over each outgoing payload only, to count bits 16..31; transports 0/2 omit it. Empty report/commit payload CRC is `0xffff`. This is separate from the stored-image CRC.

Static recovered vectors (not radio captures; payload all zero), followed by zero padding to 62:

| Frame | Hex prefix |
|---|---|
| First write, type 2 | `813c04010000002b0000006403000000000000` |
| Second write, type 2 | `813c04010000002b000000640300002b000000` |
| Last write, type 2 | `8119040100000008000000640300005c030000` |
| First write, type 1 | `813c04010000002b0043546403000000000000` |
| Last write, type 1 | `811904010000000800400b640300005c030000` |
| Commit, type 2 | `81110406006700000000000000000000000000` |
| Commit, type 1 | `811104060067000000ffff0000000000000000` |
| Second write acknowledgment (RX shape) | `8111040001002b000000640300002b000000` |
| Commit acknowledgment (RX shape) | `811104000600000000000000000000000000` |

## GUI link policy and parent probes

GUI only scans following **Home → Connect Bluetooth**. Receiver discovery remains automatic. A present receiver must be unplugged before BLE configuration; no silently preferred second configuration link. Connect instruction: close Android app; hold front pixel-logo/profile button **3 seconds** until configuration LEDs blink. `Connection::open` performs the existing `att::scan` discovery for 20 seconds, selects one `81HF01`, discovers/subscribes existing verified characteristic, then `read_profiles` and `Profiles::decode` feed the same editor pages/model. Refresh/Revert use the current BLE connection. Disconnects retain edits; explicit reconnect retains them until Revert. No BLE version query or invented XInput conversion. BLE physical packets are drained for lifecycle/disconnect detection, not mislabeled as receiver input.

Bluetooth host mapper needs exclusive ownership of this same configuration bearer. Starting it closes GUI BLE configuration; stop mapper and explicitly Connect Bluetooth again to edit device profiles. No auto BLE reconnect/scan.

Parent-only acceptance, in order:

1. Preserve current receiver backup; unplug receiver. Close mobile app and stop host mapper. Controller in Bluetooth gaming mode; hold profile button 3 seconds. Launch normal GUI, Home → Connect Bluetooth. Confirm link label Bluetooth, all 3 slot contents/pages agree with a fresh BLE CLI backup, Revert reloads, Save is absent and explains hardware gate, and ordinary controller input resumes after reads. Disconnect/reconnect and confirm staged edits survive; demo must never scan.
2. **Separately authorize the persistent identity write. Leave `BLE_WRITE_VERIFIED=false`.** Use the direct API only in a parent-controlled probe: `Connection::open(None, Duration::from_secs(20), stop)` → `read_profiles()` → convert exact `[u8;868]` → `protocol::save_backup(new_private_path, &image)` → require `profile::verify_crc(&image)` → `write_profiles(&image)` → `read_profiles()` → require all 868 bytes equal. Do not recompute CRC or change slot/motion/other fields for this test. Close connection on success and failure. Capture every request/ack: disable, all 21 chunk offsets/counts, commit parameter `0x0067`, report enable. On timeout/mismatch stop; do not try alternate checksums/opcodes automatically.
3. Confirm ordinary gaming input after the identity write and after an intentional read-only disconnect. Reconnect/read (and, if authorized, power-cycle/read) against the preserved image. Identity equality is not proof that a changed setting persists; do not claim that. Only parent may decide to flip the gate after this evidence; any later edited save still uses existing Save + verify flow.

Tests: pure static frame/ack vectors and malformed/duplicate/out-of-order acknowledgments; Unix socket peer exercises ack-gated full transaction and reporting restoration after success, malformed ack and cancellation; GUI state tests cover explicit scan command, loaded BLE link, local edits, disabled save, re-read, retained edits and retry. These tests never open Bluetooth hardware.
