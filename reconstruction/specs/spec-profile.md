# Ultimate 3E profile codec contract

O = observed in original Windows native code or immutable backup bytes; I = inference, explicitly identified. Byte offsets hex, inclusive ranges. Integers little-endian unless stated. `S = 4 + slot*0x120`, slot=0,1,2; whole wire exactly `0x364 = 868` bytes. Managed value starts at the first header byte (boxed objects add +8); managed slot `M = 8 + slot*0xa0`; whole managed value size0x1e8. **Never apply managed offsets directly to wire.**

Evidence aliases (every table reference resolves here):
- E-M = `ev_b7bcc2da3eb329a704f93efab52acba24374640fba0b13173ac77a1ad8236dd5`, UWPAdvance.003.c; 0x1808e8cd0 wire→managed, 0x1808e8d30 slot decode; 0x1808e9590 managed→wire, 0x1808e95f0 slot encode; subrecord funcs0x1808e8e40/0x1808e9700 deadZone,0x1808e92f0/0x1808e9c20 light,0x1808e94e0/0x1808e9e20 keyMap.
- E-RESET = `ev_4f2cf40de1612289b89c2c2d3cfdefc0cb480302ce2f02723b27aef21f160553`, UWPAdvance.008.c:0x180c806e0 default mapping,0x180c80920 default motion,0x180c80950 default light,0x180c80a60 default deadZone,0x180c80ce0 reset one slot,0x180c81390 name encoder.
- E-SAVE = `ev_39b0c322e175c84dc4960238d244bd1606504337470bdca4d905deab3f3afd8d`,0x180c887c0 whole-profile write factory.
- E-REFLECT = `ev_01cdbc690636532bbdcfa0b8257f3d16193ea450a07bda138053c3bc380eac47`,0x180b9eda0/0x180b9f3e0 and0x180ba17a0; reflected L7Advance structs. Field-name literals strings.tsv8244–8299 (including original misspelling `featrue`).
- E-NAME = `ev_b52f2e210711ee633e552e6b5b8742166e2fc57db085720350d4a92a4bd192b3`,0x180d31d60/0x180d31f10 rename handlers.
- E-LIGHT = `ev_c735e6a708c02d2bfc1892748c7a47c79a7375421254fa6d8d755248418bd681`,0x180c8a710 light encoder,0x180c8cb80 light decoder,0x180c8aed0 reset UI editing state.
- E-RGB = `ev_5692e29e6e37f1b5d16935baff04f4d6eaa16314de6672480961875504b3dd50`,0x180d3fb40 RGB packing,0x180d3ff60 RGB callback.
- E-EFFECT = `ev_7c1b07f8bb94eacac39855981b9110889346caf7056e72923ae04baf13ec8a91`,0x180d1f560/0x180d203b0/0x180d20bc0 effect selectors;0x180d1f760 RGB→UI,0x180d1ff50 hue→RGB. Fixed selector0x180d21260 = `ev_dc088566f316c976f0c12d168ab4cd2b78d1c1ab867671af93306a00044dcf42`.

Native marshal caveat: Ghidra loses loop tails after indirect branch imports (`_DAT_1806b0308`, etc.), labels later blocks unreachable. Bounded original-image objdump disassembly0x1808e8d30..0x1808e9eb0 recovered those blocks (artifact://59 and preceding bounded result). Layout rows below are instruction-observed, not extrapolated from truncated pseudocode; E-M identifies the original functions whose omitted native instructions were inspected. Original DLL SHA2561f2325902b5d3ca2cc7b44405243247d3f276c39f99a66c97b193545be038352.

## Complete wire/managed byte table

Header:
| Wire | Size | Managed | Field / behavior | Evidence |
|---|---:|---|---|---|
|000–001|2|000–001|`crc:u16`; both marshal directions copy unchanged; whole-profile factory copies value then marshals, no CRC recomputation in that factory|O 0x1808e8cd0/0x1808e9590 E-M;0x180c887c0 E-SAVE|
|002|1|002|`profileIdx:u8`; current slot, observed values0/1/2 in code and0xff in backup; preserve raw 0xff instead of indexing slots with it|O0x1808e8cd0/0x1808e9590 E-M; backup B0/B1 below|
|003|1|003|`profileBit:u8`; includes tournament bit0x10; remaining bit semantics detailed below; preserve unknown bits|O0x1808e8cd0/0x1808e9590 E-M; reflected member0x180b9eda0 E-REFLECT|

The following table repeats byte-exactly for S=004/124/244. Managed offsets relative to M; array offsets denote managed reference fields, **not inline data**.
| Wire relative S | Size | Managed relative M | Field / storage | Evidence |
|---|---:|---|---|---|
|000–003|4|000–003|slot `flag:u32`, initialized0x20201123|O0x1808e8d30/0x1808e95f0 E-M;0x180c80ce0 E-RESET|
|004–023|32|ref008|`name:[u8;32]`; UTF-16BE code units, zero-filled remainder; physical capacity16 code units|O0x1808e8d30/0x1808e95f0 E-M;0x180c81390 E-RESET|
|024–027|4|010–013|keyMap `flag:u32`, initialized0x20201123|O0x1808e94e0/0x1808e9e20 E-M;0x180c80ce0 E-RESET|
|028–077|80|ref018|`mapping:[u32;20]`, table below|O0x1808e94e0/0x1808e9e20 E-M;0x180c806e0 E-RESET|
|078–07b|4|020–023|feature-record `flag:u32`, initialized0x20201123|O0x1808e8d30/0x1808e95f0 E-M;0x180ba17a0 E-REFLECT|
|07c–07f|4|024–027|packed `featrue:u32`; default0x002b6d00; byte/bit interpretation below|O0x1808e8d30/0x1808e95f0 E-M;0x180c80ce0 E-RESET|
|080–083|4|028–02b|light `flag:u32`, initialized0x20201123|O0x1808e92f0/0x1808e9c20 E-M;0x180c80ce0 E-RESET|
|084–085|2|02c–02d|light `effect:u16`; enum1off/2light-tracing/3fire-ring/4fixed|O0x1808e92f0/0x1808e9c20 E-M;0x180c8a710 E-LIGHT|
|086–087|2|02e–02f|light `gear:u16` speed, UI1..10, reset5|O0x1808e92f0/0x1808e9c20 E-M;0x180c80950 E-RESET|
|088–08f|8|ref030|light `hotwheel:[u32;2]`, each packed bytes brightness/R/G/B|O0x1808e92f0/0x1808e9c20 E-M;0x180d3fb40 E-RGB|
|090–097|8|ref038|light `follow:[u32;2]`, same representation|O0x1808e92f0/0x1808e9c20 E-M;0x180d3fb40 E-RGB|
|098–0f7|96|ref040|light `single:[u32;24]`, same representation; Fixed UI writes selected color to all24|O0x1808e92f0/0x1808e9c20 E-M;0x180d3fb40 E-RGB|
|0f8–0fb|4|048–04b|motion `flag:u32`, initialized0x20201123|O0x1808e8d30/0x1808e95f0 E-M;0x180c80ce0 E-RESET|
|0fc|1|04c|motion `featrue:u8` bitfield|O0x1808e8d30/0x1808e95f0 E-M;0x180c80920 E-RESET|
|0fd|1|04d|motion `reserve:u8`; unknown semantics; retained, reset0|O0x1808e8d30/0x1808e95f0 E-M;0x180c80920 E-RESET|
|0fe|1|04e|motion `sensitivity:u8`, reset5 in structural reset|O0x1808e8d30/0x1808e95f0 E-M;0x180c80920 E-RESET|
|0ff|1|04f|motion `deadZoneCompensation:u8`, reset0|O0x1808e8d30/0x1808e95f0 E-M;0x180c80920 E-RESET|
|100–103|4|050–053|motion `openKey:u32`, physical-button mask, reset0|O0x1808e8d30/0x1808e95f0 E-M;0x180c80920 E-RESET|
|104–107|4|058–05b|deadZone `flag:u32`, initialized0x20201123|O0x1808e8e40/0x1808e9700 E-M;0x180c80ce0 E-RESET|
|108–10b|4|ref060|`joyCurve1:[u16;2]`; each pair physically `[input_x:u8,output_y:u8]`|O0x1808e8e40/0x1808e9700 E-M;0x180c80a60 E-RESET|
|10c–10f|4|ref068|`joyCurve2:[u16;2]`; same byte pairs|O0x1808e8e40/0x1808e9700 E-M;0x180c80a60 E-RESET|
|110–113|4|ref070|`joyCurve3:[u16;2]`; same byte pairs|O0x1808e8e40/0x1808e9700 E-M;0x180c80a60 E-RESET|
|114–117|4|ref078|`joyCurve4:[u16;2]`; same byte pairs|O0x1808e8e40/0x1808e9700 E-M;0x180c80a60 E-RESET|
|118–119|2|ref080|`joyFilterLev:[u8;2]`, left/right filter, reset[2,2]|O0x1808e8e40/0x1808e9700 E-M;0x180c80a60 E-RESET|
|11a–11b|2|ref088|`triggerUp:[u8;2]`, left/right lower threshold, reset[0,0]|O0x1808e8e40/0x1808e9700 E-M;0x180c80a60 E-RESET|
|11c–11d|2|ref090|`triggerDown:[u8;2]`, left/right upper threshold, reset[255,255]|O0x1808e8e40/0x1808e9700 E-M;0x180c80a60 E-RESET|
|11e–11f|2|ref098|`reserve:[u8;2]`; unknown semantics, reset[0,0], preserve unchanged|O0x1808e8e40/0x1808e9700 E-M;0x180c80a60 E-RESET|

Coverage: header000..003; slot0 004..123; slot1 124..243; slot2 244..363. **No trailing bytes.** Unknown-semantic spans are motion reserve S+fd and deadZone reserve S+11e..11f; profile flag upper bits/feature reserved bits retain raw storage as specified below. Managed-only alignment gaps M+4..7,M+14..17,M+54..57 are not wire bytes.

## Offline backup cross-check / CRC

Read-only byte inspection, no device access:
- B0 `ultimate-3e-profiles.bin`, `ultimate-3e-profiles-verified.bin`, `ultimate-3e-appimage-profiles.bin`: all868 bytes, identical SHA256`1d4f63d398241405f8dbceb07b7cc2557fd8b376a31827b23906c07d6dc00d8d`; header`c4 a1 01 18`.
- B1 `ultimate-3e-profiles-before-native-clutch-1790102027225.bin`:868 bytes SHA256`b512842c1af3f42caa022b269910c526da0fabf6ce807337696dda4e55dd4a8d`; header`ad 6f ff 18`.
- O: each slot's six record markers atS+0,+24,+78,+80,+f8,+104 contains`23 11 20 20` (u32LE0x20201123). Name bytes decode UTF-16BE`Config 1`,`Config 2`,`Config 3`.
- O offline calculation: reflected CRC16 polynomial0x8408, init0xffff, no xorout, over bytes`[4,868)` returns0xa1c4(B0) and0x6fad(B1), matching u16LE header exactly. Including bytes2..3 does **not** match (0x5a85/0xd713). This identifies stored CRC in these samples; it does not establish controller validation policy.
- O Windows marshal0x1808e9590 and factory0x180c887c0 copy CRC, do not recompute it (E-M,E-SAVE); existing `ultimate-windows-save-analysis.json` separately records native-executed exact roundtrip and unchanged CRC. Do not conflate other-model CRC routines0x180c95580/0x180c9ee80 with the3E path. A Rust stored-image CRC utility may recompute for explicit integrity repair, but normal Windows-compatible save behavior preserves existing CRC unless new hardware evidence requires otherwise.
- B0 motion tuples(feature,reserve,sensitivity,compensation,openKey): slot0(0x2d,0,5,0,0x40000000); slot1(0x29,0,5,0,0x40000000); slot2(0x2d,0,5,0,0x02000000). Wire motion record bases0xfc/0x21c/0x33c; openKey0x104/0x224/0x344.
- B1 motion tuples:slot0(0x29,0,5,0,0x02000000);slot1(0x2d,0,5,0,0x40000000);slot2(0x29,0,5,0,0x00004000).
- O B0 feature word all slots0x002b6d00; B1 slot0/2=0x001b6d00,slot1=0x002b6d00. B0 slot0 joyCurve3 starts[7,0],slot1/2[8,0]; B1 slot0/2 curve3[0,0],filter[2,5]. These are user settings, **not defaults**.

## Slot names

O0x180c81390 E-RESET swaps each UTF-16LE managed char's high/low bytes into a2*length byte array → wire UTF-16BE, no BOM. O rename handlers0x180d31f10(slot0),0x180d31d60(slot1), neighboring analogous slot2 handler clear exactly32 bytes before copying encoded name (E-NAME). I safe codec/UI capacity16 UTF-16 code units (not16 Unicode scalar values); preserve embedded/remaining zero bytes on raw decode and do not silently normalize unknown malformed names. O reset0x180c80ce0 uses localized`配置 1/2/3` or`Config 1/2/3`; slot-selector labels`配置槽 1/2/3` are UI labels, not forced stored names.

## Lighting storage and controls

| Control | Wire | Encoding/range/default | Evidence |
|---|---|---|---|
|Effect / 效果|S+84 u16LE|1=`Close`/关闭,2=`Light-tracing`/追光,3=`Fire Ring`/风火轮,4=`Fixed`/常亮; reset4|O0x180c80950 E-RESET;0x180d1f560/0x180d203b0/0x180d20bc0 E-EFFECT;0x180d21260 ev_dc088566f316c976f0c12d168ab4cd2b78d1c1ab867671af93306a00044dcf42|
|Speed|S+86 u16LE|1..10; reset5; decoder clamps to1..10|O0x180c8cb80 E-LIGHT;0x180c80950 E-RESET|
|Fire Ring colors|S+88,S+8c|two packed4-byte colors; index0 background,index1 foreground; reset0x0000f144/0x007afc44|O0x180c80950 E-RESET;0x180d3fb40 E-RGB|
|Light-tracing colors|S+90,S+94|same background/foreground order; reset0xfff71d44/0xe100af44|O0x180c80950 E-RESET;0x180d3fb40 E-RGB|
|Fixed color|S+98..f7|24 colors; UI uniform-color edit fills all24; reset every entry0x0064fb44|O0x180c80950 E-RESET;0x180d3fb40 E-RGB|
|Color representation|each color|u32LE brightness + R<<8 + G<<16 + B<<24 (non-overlapping bits); byte0 displayed as Brightness, bytes1..3 RGB. Older RGBController labels byte0透明度(Transparency), new RGB_New labels亮度(Brightness). Preserve literal bytes rather than assume alpha255|O0x180d1f760 E-EFFECT;0x180d3fb40 E-RGB|
|Hue / 色调|UI-derived, not stored|hue axis0..255, six42.5 segments through red→yellow→green→cyan→blue→magenta→red; channels truncated toward zero|O0x180d1ff50 E-EFFECT|
|Saturation / 饱和度|UI-derived, not stored|UI uses max(R,G,B) as0..255 amplitude; label percent trunc(value*100/255); not conventional HSV saturation|O0x180d1f760 and0x180d1fb70 E-EFFECT|
|Brightness / 亮度|color byte0|0..255, displayed trunc(value*100/255)%; sample/reset68|O0x180d1fb70 E-EFFECT;0x180c80950 E-RESET|

O light encoder0x180c8a710 copies effect,gear and all color arrays into selected slot. **Observed decoder quirk:** slots1/2 read their own gear and palettes but effect from slot0's object+0x3c, not slot-specific0xdc/0x17c (0x180c8cb80 E-LIGHT). Preserve separate encoded slot values; do not accidentally reproduce this cross-slot read bug in the typed codec.

## Mapping / physical-button table

O table order/values: encoder0x180c8bbf0,decoder0x180c8d2c0,UI/fallback builder0x180c8bf00 (E-LIGHT); label decoder0x180c08730 E-BUTTON=`ev_4549b9769cd00a47b5e174ecc704ee100107a32764e7317741299a3290e57816`; structural reset0x180c806e0 E-RESET. These citations apply to **every row** below. Sources implicit by index; persisted u32 cell holds target mask, not HID usage/picker index. Two distinct reset paths must not be conflated.

|Index decimal|Cell relative S|Source label|Physical mask|Structural reset0x180c80ce0|UI/invalid-record fallback0x180c8bf00|
|---:|---|---|---|---|---|
|0|28|LSB / L3|00000002|00000002|00000002|
|1|2c|RSB / R3|00000004|00000004|00000004|
|2|30|L4|00200000|00200000|0|
|3|34|R4|40000000|40000000|0|
|4|38|Up|00000200|00000200|00000200|
|5|3c|Down|00000100|00000100|00000100|
|6|40|Left|00000080|00000080|00000080|
|7|44|Right|00000040|00000040|00000040|
|8|48|A|00002000|00002000|00002000|
|9|4c|B|00001000|00001000|00001000|
|10|50|X|00000010|00000010|00000010|
|11|54|Y|00000020|00000020|00000020|
|12|58|LB / L1|00000400|00000400|00000400|
|13|5c|RB / R1|00000800|00000800|00000800|
|14|60|LT / L2|00004000|00004000|00004000|
|15|64|RT / R2|00008000|00008000|00008000|
|16|68|P1|02000000|02000000|0|
|17|6c|P2|04000000|04000000|0|
|18|70|View / Select|00000008|00000008|00000008|
|19|74|Menu / Start|00000001|00000001|00000001|

O3E picker0x180d0d130 E-MAP=`ev_a0c7578348ab36bd379e24fdcd1641ad588f25e016290ef42af36b5ea6854bbd`: **17 legal targets**, exact order A2000,B1000,X10,Y20,LB400,LT4000,LS2,RB800,RT8000,RS4,View8,Menu1,Up200,Down100,Left80,Right40,Null0. L4/R4/P1/P2 are sources, not targets. No OR/combo builder; imported arbitrary raw u32 preserved by decoder.

O generic label decoder0x180c08730 E-BUTTON recognizes Home/Xbox0x20000,Swap0x08000000,LS virtual Up0x10000000/Down0x20000000/Left0x00080000/Right0x00100000,RS virtual Up0x08000001/Down0x08000002/Left0x08000004/Right0x08000008. **None offered by3E17-target picker**. RS values are encoded tokens, not independent physical bits.

O PL/PR inconsistency:0x180d0e0b0 E-MAP uses images02000000→PR,04000000→PL; motion resolver0x180d49e90 E-MOTIONUI below uses02000000→PL,04000000→PR. Prefer numeric P1/P2 internally; do not silently swap identities.

O Mapping_View0x180d7c580/labels0x180d7c970 (`ev_b461008a7c56b4a11a55a73f1ddc105b8ae03991638e8fa25348620112b3d384`) labelsFront/Back,正面/背面. Geometry0x180cdc970(front12),0x180cdea30(back8), `ev_de366c57126de24bc5aa2936d3e6041a84dab1f31894403389966ae2b7d37feb`, covers20 sources. Click0x180d0cdb0→MappingSettingController_New→callback0x180d0ce60→global pair update0x180d0d030 (E-MAP), selected-slot saver0x180c9b950 (`ev_f126ef59b8d6ff94afec91a97ca5fcbaab736a1dd8bc13bf8258009ed9f0ca3d`). Generic MappingController/BackMappingController constructors0x180d81780/0x180d820c0 (`ev_05e30a69d4f1f17aeaa9a32711aa8947d99864eb880368d5ca49b9cee15c38b1`) are distinct paths, not grounds to widen3E choices.

O no keyboard/mouse-button/wheel/macro targets in complete3E17-target array/schema; bounded strings/resources searches found no macro editor/keyboard table. This establishes absent recovered app-visible3E support, not firmware impossibility. Motion→Mouse is separate. Evidence0x180d0d130 E-MAP;keyMap reflection0x180b791d0 (`ev_b35f9e7c8829a60c4164c05673302118ad4b5a0cf03983ee80a181cb9d7a6151`). Other-model+0x30 D-pad picker has25 choices; legacy picker0x180d4db80 (`ev_613aa128ebb91cf9f121fca977e044983a67990272e2a2d9462bffe6cbb2a1c9`) sometimes addsSwap; neither belongs3E.

## Packed feature word / stick, trigger, vibration controls

Evidence aliases:
- E-STICK=`ev_ede68e6258948136c4f8c18451e208e168ffd977d1fdc20be63ac7511c192036`.
- E-STICKUI=`ev_95b46a1a071025c2a277fa810b6519a093544be7f61268f18274777141260fa2`.
- E-SHAPE=`ev_a1d6db0ef9cf34e49b2449a9d4d84c0a67cb5f02133d8a3d1cd1aca074b7c0fd`.
- E-CURVEUI=`ev_b94508b556289abcfe336422c3f4af88efe673943dd354f14d5aca16e4639f14`.
- E-CURVEBOUND=`ev_6d52fdb9eeabc2119326652170e1e9930a5faade49cd99bea2cd829870a2355b`.
- E-TRIGGER=`ev_060bfc288df5370d90a3b90b0ae849b5e0aea886e986d9e0585dabc471bc5ad4`.
- E-TRIGGERUI=`ev_dd9d6382ab8f87fba66340a55e0380eae6e76b9b29653c1c574fb5ef2651d04d`.
- E-VIBUI=`ev_4d052f7d138ac155a4f2dde52fb589196237a5a821b18bf4dff8b7db0ce83ed9`.
- E-VIB=`ev_5847cc7002766b2f802e2c0eb50a9a04c37cf5a4950c50936dffaba95447f5b6`.
- E-VIBLABEL=`ev_e4f9af7cf57362b54fbc5235e095bc60534ef9f3aa1a6a16c58f0e150b49e8c6`.

O encoding all rows:0x180c8a430 E-LIGHT constructs word from zero. Raw codec preserves unknowns; explicit Windows-style feature rebuild drops unknown high bits.
|Mask/shift in u32 S+7c|Meaning|UI domain/reset|Additional evidence|
|---|---|---|---|
|1|Invert left X|bool,false|O0x180d059a0 E-STICK|
|2|Invert left Y|bool,false|O0x180d05950 E-STICK|
|4|Invert right X|bool,false|O0x180d059a0 E-STICK|
|8|Invert right Y|bool,false|O0x180d05950 E-STICK|
|10|Swap LS/D-pad|bool,false; exclusive LS/RS swap|O0x180d05690/0x180d057b0 E-STICK|
|20|Swap LS/RS|bool,false; exclusive LS/D-pad swap|O0x180d05840/0x180d058d0 E-STICK|
|40|Swap Triggers|bool,false|O0x180d6ca60 E-TRIGGERUI|
|80|Impulse Trigger Sync with Rumble|bool,false|O0x180d688c0 E-VIBUI|
|700,shift8|Grip Vibration Left|u3 level0..5,reset5|O0x180cedce0 E-VIB|
|3800,shift11|Grip Vibration Right|u3 level0..5,reset5|O0x180cedce0 E-VIB|
|1c000,shift14|Trigger Vibration Left|u3 level0..5,reset5|O0x180cedce0 E-VIB|
|e0000,shift17|Trigger Vibration Right|u3 level0..5,reset5|O0x180cedce0 E-VIB|
|300000,shift20|Stick shape|3E enum0Circular,1Square,2Default(reset);raw3 decodesSquare|O0x180d053a0/0x180d05500 E-STICK;0x180d76110 E-STICKUI;0x180c908a0 E-SHAPE|
|ffc00000|reserved/unassigned|no UI, raw-preserve; feature rebuildzeros|O0x180c8a430 E-LIGHT|

|UI control|Wire / domain / encoding|Reset / labels|Evidence|
|---|---|---|---|
|Left curve|S+108..10f=four [input_x,output_y] byte pairs,percent0..100|P0(8,0),P1(37,33),P2(66,66),P3(95,100)|O0x180c80a60 E-RESET;0x180d07230 E-STICK|
|Right curve|S+110..117,same|same|O0x180c80a60 E-RESET;0x180d07230 E-STICK|
|Deadzone|P0.x/P3.x;trunc(sliderPosition/255*100)|8/95;Deadzone|O0x180d06a00 E-STICK;0x180d78160 E-STICKUI|
|Anti-Dead Zone / 反死区|P0.y/P3.y,same scaling|0/100|O0x180d059f0 E-STICK;0x180d78160 E-STICKUI|
|Custom interior points|P1/P2;x=trunc(pixelX*100/276),y=trunc((120-pixelY)*100/120)|graph276×120|O0x180d07230 E-STICK|
|JitterSettingView|S+118left,S+119right;integer0..5,no scale|2,2;Stick Sensitivity/防抖动|O0x180cff7c0 E-CURVEBOUND;0x180d07890 E-STICK;0x180c80a60 E-RESET|
|StickModeSelect|feature shape enum above|Default/默认,Circular/圆形,Square/方形;other models reverse0/2 labels|O0x180d08f10 E-CURVEUI;0x180d76110 E-STICKUI|
|Triggers|S+11aLTlower,+11bRTlower,+11cLTupper,+11dRTupper;raw0..255|lower0/upper255;Left Trigger,Right Trigger,Swap Triggers/交换扳机键|O0x180cf3030/0x180cf2cd0 E-TRIGGER;0x180d6d4a0 E-TRIGGERUI;0x180c80a60 E-RESET|
|Four vibration controls|feature u3 fields above,0..5,notfloat/percent255|5each;Grip Vibration - Left/Right,Trigger Vibration - Left/Right|O0x180d688c0 E-VIBUI;0x180cedce0 E-VIB;0x180cefc50 E-VIBLABEL|
|Impulse Trigger Sync with Rumble|feature80|false;扳机振动跟随握把振动|O0x180cefc50 E-VIBLABEL;0x180c8a430 E-LIGHT|

O user curve drag/D-pad enforces strict increasingx/y between neighbors0x180d001f0/0x180d003c0 E-CURVEBOUND; Digital preset equalx is nonetheless authored, so do not reject imported equalx. I lower≤upper sensible trigger UI invariant; exact firmware rejection/crossing behavior not established. O deadZone encoder0x180c8aa40 E-LIGHT builds reserve[0,0]; raw codec preserves it.

Preset labels0x180d08060 E-CURVEUI; authored graph endpoints(22,120),(262,0); endpoint reset0x180d078b0 E-STICK restores wire(8,0),(95,100). Interior bytes I mechanically derive with observed0x180d07230 truncation.
|Preset|Authored graphP1,P2|Derived wireP1,P2|Setter/evidence|
|---|---|---|---|
|Default|(102,80),(182,40)|(36,33),(65,66)|O0x180d1f340 E-EFFECT|
|Dynamic / 动态|(102,100),(182,20)|(36,16),(65,83)|O0x180d20f50 E-EFFECT|
|Precision / 精准|(102,100),(202,40)|(36,16),(73,66)|O0x180d1f170 E-EFFECT|
|Fast / 快速|(42,100),(102,40)|(15,16),(36,66)|O0x180d1efa0 E-EFFECT|
|Digital / 数字|(82,100),(82,40)|(29,16),(29,66)|O0x180d1edd0 E-EFFECT|
|Stable / 稳定|(142,60),(202,40)|(51,50),(73,66)|O0x180d1ebb0 `ev_94b344795cbeeac843e5ad7e9404a8bd2f0294b5747e4b82be243c31d63e7e26`|

**Factory reset differs from UI Default preset** by one on interiorx37/66 vs36/65; persist actual points, not an invented preset enum. Custom/自定义 is UI state inferred from points. Warning: lower deadzone improves precision but may prevent returning center(0x180d78160 E-STICKUI). Curve_View sets y label`y : Stick Input` despite it representing output, an app typo(0x180d07fd0 E-CURVEUI).

O3E routes StickController_New0x180d76110 E-STICKUI,TriggerController_New0x180d6ca60 E-TRIGGERUI,Vibration_New0x180d688c0 E-VIBUI. Older SticksController/TriggersController/VibrationController_new use other records; legacy vibrationfloat((max<<8)|min) at0x180c89660 E-SAVE **not3E868-byte storage**. Vibration test prompt only: holdLT/RT grip,PL+LT/PR+RT triggers(0x180cefc50 E-VIBLABEL); no test executed.

## Motion controls

Evidence aliases:
- E-MOTIONUI=`ev_85b2d3ea49bf1742d8ab5832af81d8b7592d3a499b11af77eeb2fa5086d1c4a9`.
- E-MOTIONOLD=`ev_754b7e13d3a28077868bff354bdf5dbd60763d196a1471fbaedbad2b269d9303`.
- E-SENS=`ev_c722a339db1dc45dd36ba2cf753d94686a2da2c8e379a500b33e775caf5df18d`.
- E-COMP=`ev_548623ec1cb109c735b1daf49f48018a2cf2d37679948e1ebee5eca5b2f79d70`.
- E-MOTIONFACT=`ev_27d2a4015dc3325dade755fd6b22448e047f5cf59ff12a58ce059eb889d1d16e`.
- E-POPUP=`ev_04269b9cf98a012c2446012d4f65822a2836914da082aee92d2f7ab7381ba500`.
- E-LEGACYSLIDER=`ev_613aa128ebb91cf9f121fca977e044983a67990272e2a2d9462bffe6cbb2a1c9`.

O encoder0x180c8a050 E-LIGHT constructs `map==1 ? 1 : map==2 ? 2 : 0`, OR toggle?4:0, **always OR8**, continuous?0x10:0,axisX?0x20:0. Every bit table row cites this encoder and decoder0x180c8c840 E-LIGHT; additional UI bindings below. Raw byte decode may preserve non-UI values; do not expose them as supported options.

|Field|Wire|Encoding / UI label|Structural reset|Invalid-magic fallback|Additional UI evidence|
|---|---|---|---|---|---|
|Mapping|S+fc bits0..1|0Left Stick(recommended for racing games),1Right Stick(recommended for shooter games),2Mouse;3 no UI|0|1|O0x180d4a0c0 E-MOTIONUI;0x180d1ba30 E-POPUP|
|Enable method|S+fc bit2|0Press and hold to enable/按住启用;1Press once to enable/disable/按下启用，再次按下关闭|0|0|O0x180d1c030 E-POPUP|
|Unlabeled decoded flag|S+fc bit3|no UI setter established;encoder forces1|1|decodedstate0,encoderforces1|O0x180c8a050/0x180c8c840 E-LIGHT|
|Motion mode|S+fc bit4|0即时模式（角速度）instantaneous/angularvelocity;1持续模式（加速度）continuous/acceleration|1|0|O0x180d50db0 E-MOTIONOLD|
|Axis|S+fc bit5|0Z轴,1X轴|0|1|O0x180d4f640/0x180d4f6a0 E-MOTIONOLD|
|Reserved feature bits|S+fc bits6..7|no UI;rawpreserve,encoderzeros|0|0|O0x180c8a050 E-LIGHT|
|Reserved byte|S+fd|no UI;motion edit preserves it|0|notloaded|O0x180c8a090 E-LIGHT|
|Sensitivity|S+fe|**3E new stored0..5,display1..6**; labelSensivity(typo)/灵敏度|5(display6)|0(display1)|O0x180d45f60 E-SENS;0x180d48d80/0x180d4a440 E-MOTIONUI|
|Deadzone Compensation / 死区补偿|S+ff|raw0..255,displayfloor(raw*100/255)%;button±5 with clamp,drag truncation|0|0|O0x180d48250/0x180d48370/0x180d48490 E-COMP;0x180d4a430 E-MOTIONUI|
|Enable button / 开启按键|S+100u32|single selected physical mask;picker below;0None|0|0|O0x180d49360/0x180d49440 E-MOTIONUI|

O structural reset0x180c80920/0x180c80ce0 E-RESET gives **valid** motion marker,feature0x18,reserve0,sensitivity5,compensation0,openKey0. Invalid-marker fallback0x180c8c840 E-LIGHT instead encodes0x29 when saved. They are distinct; sample0x29/0x2d is user configuration, not factory reset.

O enable-button picker0x180d49440/labels0x180d49e90 E-MOTIONUI has21 choices in exact order (hex):`2000 A,1000 B,10 X,20 Y,400 L1,4000 L2,2 L3,800 R1,8000 R2,4 R3,8 Select,1 Start,200 Up,100 Down,80 Left,40 Right,02000000 PL,04000000 PR,00200000 L4,40000000 R4,0 None`. It writes u32 directly0x180d49360; no combination builder. Home/Swap not choices.

O3E tab0x180cf7500 (`ev_c77107d0210604c81bbb0c5f738e8f05735a662bfda3afced40a03c9d4d4e564`)→Motion_New factory0x180d7b010 E-MOTIONFACT→initializer0x180d4a0c0 E-MOTIONUI. New simplified page exposes mapping/method/button/sensitivity/compensation; does not expose axis/mode menus in recovered initializer. Preserve those imported/reset bits even if hidden. Sensitivity-to-angular-speed gain curve not established; do not invent multiplier.

O legacy Motion controller0x180d4f2e0 E-MOTIONOLD Mouse selection setsmap2/mode0,Right0x180d4f3a0 setsmap1/axisX/mode0,Left0x180d4f4a0 setsmap0/axisZ/mode1; these legacy side effects not proven for newer3E popup. Legacy SensitivityProgress0x180d4e500 E-LEGACYSLIDER usesstored0..9/display1..10, not3E sixlevels; legacy DeadZoneProgress0x180d4e100 same evidence displays raw0..255 rather than percent. `Small`/`Large` labels0x180d7bc00 (`ev_b461008a7c56b4a11a55a73f1ddc105b8ae03991638e8fa25348620112b3d384`). Distinguish versions rather than widening3E UI range.

## Header / PlatformController / slots

Evidence aliases:
- E-PLAT=`ev_63021b04bd745d1073996c0b85b85c262c3ecf03217d9777af95af65b58f721d`.
- E-SETTINGS=`ev_b52f2e210711ee633e552e6b5b8742166e2fc57db085720350d4a92a4bd192b3`.
- E-RATE=`ev_e8dd03488a043a96f2c19ef96e229bd1e7c239c5f922779afc0ad6f1e60b9801`.
- E-OFF=`ev_dc088566f316c976f0c12d168ab4cd2b78d1c1ab867671af93306a00044dcf42`.
- E-SLOTS=`ev_1d31aa13fc6d1c2ca5e453b3b60c46b2bd02f1674f59ec326bd2aaf36b196ac3`.

|Header field/control|Encoding / range / label|Behavior / default boundary|Evidence|
|---|---|---|---|
|Current slot byte2|u8 index0/1/2|0xff seenB1; do not index unchecked. PlatformController reads/writes selected index|O0x180d34cf0/0x180d34de0 E-PLAT;0x180c8a090 E-LIGHT|
|profileBit byte3 bit10|clear250Hz,set1000Hz|Tournament Mode/赛事模式;UI local selection1clear,2set;2000/4000Hz belongs other-model+38,not3E|O0x180d16a50 E-RATE;0x180d681c0 E-VIBUI|
|profileBit bit08|Auto Reconnect / 自动启停|dock placement off/charge;removal on/reconnect;UI claim,not hardware-observed|O0x180d30a60 E-SETTINGS;0x180d34860 E-PLAT|
|profileBit bits01/02/04|unknown lower flags|preserved in known toggles; no semantic labels established|O0x180d1af30 E-POPUP;0x180d21770 E-OFF|
|profileBit bits20/40/80|unknown high flags|rawpreserve; several Windows toggles drop them|O0x180d21770 E-OFF;0x180d34860 E-PLAT|
|Slot availability/name|slotflag0x20201123 gates valid/displayedstoredname|invalidplaceholders配置槽1/2/3;English`ConfigSolt 1/2/3` typo,not stored resetnames|O0x180d2a930 E-SLOTS|
|Empty slot|invalidslot UI marker|Empty / 未使用配置|O0x180d2e220/0x180d2e320 `ev_bfeeb44dccaf09bbf1998f3e22f24c1bbbeca811eb47433d5ab24c0eba5ac8f8`|

O tournament enable0x180d21120 E-EFFECT ORs0x10; disable0x180d21770 E-OFF reconstructs **only bits0..3**, drops all higher flags, not simply`&~0x10`. Dock clear0x180d34860 E-PLAT reconstructs onlybits0..2, also clearingtournament0x10; docksetORs0x08. UI rate-setting/tournament buttons may issue immediate command selector0x0a throughfactory0x180c87b50 (`ev_8023db373ce5c5f936d7316790bab2182785b085f1fd57dd68cad289db929220`); fulltransport contract belongs spec-transport. Distinguish raw codec preservation from explicit Windows action reconstruction.

O tournament explanatory label0x180d30a60 E-SETTINGS:`When Tournament Mode is enabled, the polling rate increases to 1000Hz for lower input latency, but reduces battery life.` Not a measured latency/battery fact. Reset helper0x180c85c60 (`ev_e70a9da6317c68dc6ca1145adaeec51f1282b8987d885bd4818cb43f916e3d71`) zeros headerprofileBit; firmware power-on default unestablished.

O name decoder0x180cc78e0 (`ev_e0bcaeecae7f5b4a1bed83fd2d70a6b4a54ea469207876ba8e8b307139f85bd9`) reads(byte[2i]<<8)|byte[2i+1],stops0000; full32-byte name need not have terminator. Rename0x180d31bb0(slot2),0x180d31d60(slot1),0x180d31f10(slot0) E-SETTINGS clear32 then copy; exceeding capacity bounds-checks rather than silentlytruncates. Exact textbox max-character validator unestablished; capacity16UTF16units is codec fact, not proventextbox rule.

O PlatformController factory0x180d7ab50 E-MOTIONFACT exposes slot/state/dock/tournament controls for3E/+38; loader0x180d7af60→Platform/PlatformController.xaml. Thirdslot creation0x180d38690 E-PLAT selectsindex2,writesvalidmarker,labelsProfile3/配置3. 3E title routing0x180d33b20 (`ev_7b5115a5544e546984915d1b84ee69393cebd4c482b7ab16cd2906071ae612b3`) selectsL7Title_New whenprofileBit10set and0x180cc5fb0()==1(desktop mouse mode),otherwiseL7Title_GIP. Both haveRGB; onlyNew hasMotion. Constructors0x180d6ef60/0x180d6f440 (`ev_f4323834a29b612a1778bc7bf3834476a3e70f0bafd42073073c3f842ce29608`).

## SOCD and unsupported profile features

O SOCD is **not a3E868-byte field**. Navigation0x180cbf0c0 (`ev_09cd9be6ae389a71445a1beaad751e87659f693a6e5fdbaf6b479812ed221a1e`), IR.009:38411–38422, selectsSocdController typeDAT_18003b218 only for modeltableindex0xc(offset30),not3Eindex0xf(offset3c). Saver0x180c9b790 (`ev_f126ef59b8d6ff94afec91a97ca5fcbaab736a1dd8bc13bf8258009ed9f0ca3d`) writes different singleton0x180c88560+9c;3E singleton0x180c81440 differs. Do not overlaythat+9c onto3E lightpalette.

For completeness, other-model SOCD UI enums0x180d0a5f0 E-MAP;value3setter0x180d0e6d0 (`ev_4f1b4543ad6798baa002ca6c06d81065a3331f86d338161d4586586b5d8d0664`):0Default/默认,1Absolute Up Priority/向上优先,2Last Input Priority/后覆盖,3First Input Priority/前覆盖. No3E UI or3E default inferred.

## Codec rules and explicit unknowns

- O all868 bytes accounted for above; no trailingpadding. Raw roundtrip includes reservedbytes,unknownfeaturebits,arbitrarytargetmasks,unknownheaderindices.
- O integer field widths/arraycounts are marshal-observed; UTF16BE names are not host-endian. Curves store percentbytepairs; triggersraw255; rumble3-bitlevels; motion newstored0..5. These scales are intentionally different.
- O explicit feature/deadZone/motion edits rebuild assignedfields; Windows zerosfeature reservedbits anddeadZone reserve,forcesmotionbit3,retainsmotionreservebyte. Rawcodec should not silently perform those semantic edits.
- Unknowns: lowerprofileBit flags01/02/04; highprofileBit20/40/80; featurewordbits22..31; motionreservebyte; deadZonereservepair; exactfirmwareacceptanceof malformed/out-of-domainvalues; sensor gaincurve; hardwareCRCvalidation. They are not license to discard bytes.
- Byte-level defaults grounded in structuralreset0x180c80ce0 E-RESET, not guessed from genericfallback or userbackups. UI reset/fallback0x180c8aed0 E-LIGHT differs in mappingextras,triggerlower5vs0,motiondefaults; preserve distinction.
- Offline analysis only; no writes torepo,backups,controller; no build/tests executed. Supplemental focused reports:local://profile-mappings.md,local://profile-sticks.md,local://profile-motion.md.
