# Ultimate 3E Windows UI specification

## Evidence and interpretation

**O** observed in shipped PRI/XML/assets/native pseudocode; not a screenshot/runtime observation. **I** inferred. Function citations use address + evidence alias, expanded to full `evidence_id` in §Evidence. `L:0x...` means a native string-literal address in `reconstruction/ir/strings.tsv`, **not a PRI/resx key**. Real PRI keys are written `/Resources/.../Text`. Most modern labels are inline EN/CN branches (`FUN_1808640d0`), not PRI strings.

Repository and controller remained read-only. PRI parsing executed read-only Node under REA process capture; no parser file, build, tests, hardware I/O, Windows target execution, or screenshots. Exact XAML geometry and CN text baked into compiled XAML without an inline pair are not established. Preserve these boundaries rather than inventing visible text.

## 1. Model routing and navigation

**O: 3E does not use ThreeTitle or UltimateTitle.** `0x180d33b20` [MENU], `reconstruction/ir/UWPAdvance.009.c:90948-91063`: current logical PID at model-table `+0x48` equals 3E table entry `+0x3c`; `FUN_180c81440()` returns current managed configuration.

|Condition|Actual title|Tabs|
|---|---|---|
|Managed configuration byte `+0x0b & 0x10 != 0` AND `FUN_180cc5fb0()==1`|`TitleView/L7Title_New.xaml`; type `DAT_1800390c0`|Back, Mapping, Sticks, Triggers, Vibration, RGB, Motion|
|Otherwise|`TitleView/L7Title_GIP.xaml`; type `DAT_180038ed0`|Back, Mapping, Sticks, Triggers, Vibration, RGB; **no Motion**|

Constructors/loaders: `0x180d6ef60/0x180d6f390`, `0x180d6f440/0x180d6f7e0` [TITLE], `.010.c:17534-17864`. `0x180cc5fb0` [HOST], `.009.c:40749-40802`, returns1 for `Windows.Desktop` and `UIViewSettings` interaction-mode0; **host UI interaction mode, not connection type**. Profile/header bit0x10 is also exposed by Tournament Mode/rate controls; an older PlatformController label says Xinput Mode. Keep raw bit/title gate exact; do not turn inconsistent UI names into a transport-mode claim. Offsets here are managed configuration offsets, not wire offsets.

**O home ownership:** `HomeController.xaml`, constructor `0x180d7f330`, loader `0x180d80890` [HOME], `.010.c:24618-25180`, owns device discovery/removal, config/version notifications, and child pages. `MainPage_New.xaml` constructor/loader `0x180c501a0/0x180c50620` [SHELL], `.008.c:77299-77456`, renders home. `0x180c4fe40` [SHELLSTYLE], `.008.c:77223-77296`, selects 3E model title/image:
- **Ultimate 3E Controller for XBOX / 猎户座 3E 无线手柄（XBOX）**, `L:0x18062f008/0x18062f060`.
- `/Assets/XboxWired/mapping/Mapping_bg_L7.png`.
- Home actions **Config**, **Firmware**, **Setting**, **Test**, `L:0x18062f2a8/2d0/2f8/320`. Exact CN home labels are not among the25 keyed PRI strings; do not invent their original wording.

**O config flow:** Home Config → three-slot manager → select/create profile → `SettingMenu/SettingMenuView.xaml` composite title/content/status frames; `0x180d337c0/0x180d338f0/0x180d33b20` [MENU], `.009.c:90880-91063`. Initial content `DAT_18003da08` = `Mappings/Mapping_View.xaml`. Navigation order by title declarations: **Mapping → Sticks → Triggers → Vibration → RGB → Motion**, final tab conditional above. **I:** left-to-right visual order follows declaration order; no pixel coordinates observed. Observed selection indices Mapping0 (`d33b20`), Sticks1 (`0x180d15760` [RATE], `.009.c:76208-76256`), Vibration3 (`0x180cf7600` family [TABS], `.009.c:62629`), Motion5 (`0x180cf7500` [TABS], `.009.c:62548-62570`). Do not equate UI index with a wire enum.

## 2. Complete keyed PRI string extraction

**O both files:** `mrm_pri2`; identical hierarchy **861 names,73 scopes,788 items**. `resources.en.pri`: **25 candidates, all25 string resources**. `resources.pri`: **869 candidates,25 local string resources**, remaining candidates not string values. Extraction evidence `ev_20a4e3325fb0b6c8574df17d762b98b7b06f44f14eaa55b2919027417ce446e9`.

Read-only parser followed TOC → `[mrm_hschemaex]`/`[def_hnamesx]` parent scopes and item IDs → map groups/iteminfos/decision candidate counts → `[mrm_dataitem]` string/blob offsets. It did not pair names/values by approximate position. EN8-byte candidate records at decimal **29788+8*i**, CN **39888+8*i**, `i=0..24` below; these locate metadata, not UTF16 payload starts. Format reference: https://github.com/chausner/PriTools/blob/master/Package%20Resource%20Index%20File%20Format.md (reverse-engineered format, corroborated against actual section data).

All keys expand to `/Resources/<key>/Text` in both files. **Complete English table:**

|i|Key|EN|CN|
|---:|---|---|---|
|0|Cancel|Cancel|取消|
|1|DeadZore|Disable Deadzone|关闭摇杆中心死区|
|2|DirectionControl|Direction Control|方向控制|
|3|InvertX|Invert X|反转 X 轴|
|4|InvertY|Invert Y|反转 Y 轴|
|5|LeftShake|Left|左马达|
|6|LeftStick|Left Stick|左摇杆|
|7|LeftTrigger|Left Trigger|左扳机键|
|8|Menu_mapping|Mapping|键位|
|9|Menu_sticks|Sticks|摇杆|
|10|Menu_triggers|Triggers|扳机键|
|11|Menu_vibration|Vibration|振动|
|12|OK|OK|确定|
|13|Reset|Reset|复位|
|14|RestoreFactory|Factory Setting|恢复出厂设置|
|15|RightShake|Right|右马达|
|16|RightStick|Right Stick|右摇杆|
|17|RightTrigger|Right Trigger|右扳机键|
|18|SwapDpad|Swap D-pad and Left Stick|方向键与左摇杆交换|
|19|SwapSticks|Swap Sticks|左摇杆与右摇杆交换|
|20|SwapTriggers|Swap Triggers|“LT” 与 “RT”交换|
|21|Sync|Sync to Controller|同步到控制器|
|22|Synced|Synced|同步完毕|
|23|Syncing|Syncing...|正在同步...|
|24|TapMenu|Switch Tabs|切换菜单|

**O modern labels override legacy resources:** PRI `Menu_mapping` CN=`键位`; modern title CN=`映射`. Preserve page-specific wording. Fonts selected by language: `/Assets/GT.ttf#GT Pressura Pro` vs `微软雅黑`, `L:0x1805eeef8/0x1805eeed8`, [TITLE].

## 3. Page inventory

### Config / slot manager

**O** `Platform/PlatFormController_New.xaml`; rendering `0x180d2a930` [SLOTS], `.009.c:86077-86253`; actions `0x180d2b0e0` [SLOTS], `.009.c:86260-86289`.

|Control|Exact EN/CN key|Options / backing feature|
|---|---|---|
|3 selectable slot cards|Empty labels `ConfigSolt 1/2/3` / 配置槽1/2/3; `L:0x18063c280/3b0/4e0`, CN`c260/c390/c4c0`|Exactly3 slots; occupied cards decode stored profile names|
|TextBox + Rename action|Rename `L:0x18063c5b0`; CN baked text not recovered as a pair|Per-slot name; physical storage capacity16UTF16 code units, not proven TextBox rule; see profile spec|
|Per-slot menu action|Delete / 删除 `L:0x180634130/0x180634158`|Remove profile|
|Per-slot action|Activate / 启用 `L:0x18063c5d8/0x18063cac8`|Select active profile|
|Action/status|Sync `L:0x18063c590`; Sync Complete / 同步完成 `L:0x18063c600/0x18063c9b0`|Profile transfer/completion|
|Back|Back / 返回 `L:0x180632960/980`|Home|

Assets selected by d2a930: empty `/Assets/Xbox3E/platform{1,2,3}_Normal.png`; occupied/selected `/Assets/XboxWired/platform/platform{1,2,3}_Selection.png`. **ConfigSolt** is shipped misspelling, not a corrected translation.

**O second platform panel:** `Platform/PlatformController.xaml`; constructor `0x180d7ab50` [MODERN], `.010.c:22562-22681`; labels `0x180d3b510` [PLATFORM], `.009.c:94000-94070`. Profile1/2/3 / 配置1/2/3 (`L:0x1806341c8/1a0/178`, CN`0x180630c78/cc0/d08`), Delete; binary checkboxes **Xinput Mode / Xinput模式** (`L:0x1806335f8/628`), **Auto Reconnect / 自动启停** (`L:0x180633650/688`). For3E constructor reads managed header byte`+0xb` bit0x10 into one checkbox and bit8 into another. Other code labels/rate-controls bit0x10 as tournament; preserve this observed naming conflict rather than assume a connection change. Slot and raw-header semantics are in `local://spec-profile.md`, Header/PlatformController section.

### Mapping

**O** `Mappings/Mapping_View.xaml`; constructor `0x180d7c580`, loader`0x180d7cae0` [MAPPING], `.010.c:23364-23513`; geometry `0x180cdc970` front12 and `0x180cdea30` back8 [GEOMETRY]. **20 source hotspots total**, not just16 standard XInput inputs.

|Control|Visible EN/CN key|Options/backing feature|
|---|---|---|
|Front/back diagram selector|Front / 正面 `L:0x180633f48/f68`; Back / 背面 `L:0x180632960/0x180633f28`|Front/rear controller face; Back here means rear, not navigation|
|20 clickable physical-input hotspots|Icon labels, language-independent|LSB/L3,RSB/R3,L4,R4,Up,Down,Left,Right,A,B,X,Y,LB/L1,RB/R1,LT/L2,RT/R2,P1,P2,View/Select,Menu/Start|
|Remap popup|Pictogram options|Exactly17 targets in order **A,B,X,Y,L1,L2,L3,R1,R2,R3,Select,Start,Up,Down,Left,Right,Null**; no macro/keyboard/mouse-button target inferred|

Target array `0x180d0d130` [REMAP], `.009.c:72185-72300`, allocates0x11 entries for3E. Rear L4/R4/P1/P2 are **sources, not selectable targets** in this array. Other-model25-choice arrays must not widen3E support. Source geometry and complete mapping table corroborated in `local://spec-profile.md`, Mapping section.

**O numeric icon IDs**, `0x180d0c290` [REMAP], `.009.c:71776-71863`: `A=0x2000,B=0x1000,X=0x10,Y=0x20,L1=0x400,L2=0x4000,L3=2,R1=0x800,R2=0x8000,R3=4,Select=8,Start=1,Up=0x200,Down=0x100,Left=0x80,Right=0x40,Null=0`. These are app button tokens, not HID usages.

Assets: front `/Assets/XboxWired/mapping/Mapping_bg_L7.png`; rear `/Assets/XboxWired/mapping/Mapping_back_L7.png` selected on3E in `.009.c:88955-88967`; popup `/Assets/Xbox3E/MappingButton_Left.png`, `MappingButton_Right.png`, `MappingButton_Enter.png`. Generated stems `/Assets/Xbox3E/Mapping_<name>.png`, changed `_Change`, default `_Default`; renderers `0x180d0b6c0/0x180d0c4b0` [REMAP], `.009.c:71573-72025`.

**O PL/PR inconsistency:** `0x180d0e0b0` [REMAP] maps02000000→PR,04000000→PL; motion icon resolver `0x180d49e90` [MOTION] reverses these labels. Internal names should retain numeric P1/P2 identity; do not silently swap persisted masks.

### Sticks

**O** `Sticks/StickController_New.xaml`; constructor`0x180d76110`, labels`0x180d78160`, loader`0x180d787c0` [STICKS], `.010.c:20506-21472`. Left/right selection, live stick plot, threshold controls, editable curve graph. These are custom progress/buttons/images, not proven stock sliders.

|Control|EN/CN key|Range/options/backing feature|Evidence|
|---|---|---|---|
|Stick selector|L Stick / 左摇杆 `L:0x180641da0/0x180632780`; R Stick / 右摇杆 `L:0x180641dc8/0x180632730`|Left/right independent settings|d78160 [STICKS]|
|Dual-ended progress|Deadzone / 死区 `L:0x180641e48/0x180638e90`|Inner/outer input thresholds; raw progress0..255 converted to percent; stored curveP0.x/P3.x|d06a00 [STICKVALUES], `.009.c:69182-69255`, computes trunc(raw/255*100)|
|Dual-ended progress|Anti-Dead Zone / 反死区 `L:0x1806327a0/7d8`|Output compensation endpoints; raw0..255→percent; storedP0.y/P3.y|d059f0 [STICKVALUES], `.009.c:68859-68908`|
|Binary switches|Invert X / 反转 X 轴; Invert Y / 反转 Y 轴; PRI InvertX/InvertY; CN literals`L:0x180641ae0/b08`|Independent selected-side axis inversions|d76110/d78160 [STICKS]|
|Binary switch|Swap LS/RS / 交换左 / 右摇杆 `L:0x180641f78/0x180641df0`|Swap sticks; mutually exclusive with LS/D-pad swap|d78160 [STICKS]; d05840/d058d0 [STICKVALUES]|
|Binary switch|Swap LS/D-pad / 交换左摇杆 / 方向键 `L:0x180641fa8/0x180641e18`|Swap LS/D-pad; exclusive with LS/RS|d05690/d057b0 [STICKVALUES]|
|Shape popover|Stick Shape Settings / 摇杆形状设置 `L:0x180633d40/d80`|**3E values0 Circular/圆形,1 Square/方形,2 Default/默认**|d76110 [STICKS], `.010.c:20850-20913`; d08f10 [CURVE]|
|Curve presets + editable graph|Stick Curve / 摇杆曲线 `L:0x180633da8/dd8`; Custom / 自定义 `L:0x1806330f0/118`|Default,Dynamic/动态,Precision/精准,Fast/快速,Digital/数字,Stable/稳定; two interior control points|d08780 [CURVE], `.009.c:70143-70265`; graph update d07230 [STICKVALUES]|
|Discrete slider|Stick Sensitivity / 防抖动 `L:0x180633df8/e30`|JitterSettingView, **integer0..5**, six positions; constructor initial5; stored factory default2 is a different fact|d79040 [STICKS], `.010.c:21684-21764`; d46a40→cff7c0 [LEVELS/PROGRESS], `.009.c:99534-99573,66400-66412`|
|Help buttons|Dead Zone,Anti-DeadZone,Stick Shape,Anti-Jitter|Explanations/images below|d01390 [HELP], strings.tsv:9719-9781|

**O curve geometry:** native graph width276,height120; `0x180d07230` [STICKVALUES] computes `x=trunc(pixelX*100/276)`, `y=trunc((120-pixelY)*100/120)`. Stored points are percentages0..100, not0..255. Interior drag enforces increasing neighboring coordinates (`0x180d001f0/0x180d003c0` [PROGRESS]); authored Digital preset has equal interiorx, so this drag rule must not reject imported preset points. Structural defaults vs UI Default preset differ; preserve actual curve points (profile spec contains exact preset coordinates).

**O help discrepancy:** EN Anti-Jitter text `L:0x180638af0` says default level5; CN`L:0x180638628` says 默认值为2级. Help says “Select ‘Off’ to use the raw input signal”; slider zero renders numeric0, not proof of a seventh option. EN full text: “Filters out unwanted signals caused by minor stick jitter around the center position. The default level is 5. Higher levels provide stronger filtering but may slightly reduce fine control accuracy. Select “Off” to use the raw input signal.”

Other exact help text: “A lower deadzone improves control precision but may prevent the stick from returning to center.” (`L:0x180641ea0`); “* Use the D-pad to adjust values.” (`L:0x180641fd8`). Dead Zone title/explanation `L:0x180639098/0c0`, CN`0x180638e90/eb0`; Anti-DeadZone `L:0x180639340/370`, CN explanation`0x180638fb8`. Language-selected images `/Assets/Xbox3E/DeadZone_Tisp{,_en}.png`, `DeadZone_Tisp1{,_en}.png` at `.009.c:67165-67224` [HELP]. Packaged `AntiDeadZone_Tisp.png` exists but that alone does not prove use by this handler. Common plots/cursors under `/Assets/XboxWired/sticks/`.

### Triggers

**O** `Triggers/TriggerController_New.xaml`; constructor`0x180d6ca60`, labels`0x180d6d4a0`, loader`0x180d6d5f0` [TRIGGERS], `.010.c:16284-16581`.

|Control|EN/CN key|Range/options / backing feature|
|---|---|---|
|Left dual-ended progress|Left Trigger / 死区 - LT `L:0x180638200/0x180640f10`|LT lower/upper actuation thresholds|
|Right dual-ended progress|Right Trigger / 死区 - RT `L:0x180638230/0x180640f38`|RT lower/upper thresholds|
|Binary switch|Swap Triggers / 交换扳机键 `L:0x180640cb0/ce0`|SwapLT/RT|
|Live arcs/trigger pictograms|LT/RT|Live input/threshold visualization, not additional settings|

**O range:** both controls initialized by `cfcc10(control,0xff,0x22)`. Raw0..255 clamp `0x180cfe4a0` [PROGRESS], `.009.c:65714-65738`; visible labels normalized0..100 from track width305 or271 host-layout units (`0x180cfcca0` and following helpers [PROGRESS], `.009.c:65264-65707`). Raw threshold callbacks`0x180cf3030/0x180cf2cd0` [TRIGGERVALUES], `.009.c:61190-61300`. Do not display raw0..255 as percentage. Exact firmware acceptance/crossing invariants are not established by the graphical-handle clamp alone.

3E diagram `/Assets/XboxWired/Triggers/Triggers_bg_L7.png`, `.010.c:16365-16373`; packaged `/Assets/Xbox3E/LT_Normal.png`, `LT_Enter.png`, `RT_Normal.png`, `RT_Enter.png`. `Triggers_bg_Three.png` belongs another model route.

### Vibration

**O** `Vibration/Vibration_New.xaml`; constructor`0x180d688c0`, loader`0x180d68fc0` [VIBRATION], `.010.c:14643-14825`; labels`0x180cefc50` [VIBLABEL], `.009.c:59769-59810`.

|Control|Exact EN/CN literal key|Backing feature|
|---|---|---|
|Discrete progress|Grip Vibration - Left / 握把振动-左 `L:0x180636d10/e28`|Left grip intensity|
|Discrete progress|Grip Vibration - Right / 握把振动-右 `L:0x180636d50/e50`|Right grip intensity|
|Discrete progress|Trigger Vibration - Left / 扳机振动-左 `L:0x180636d98/e78`|Left impulse intensity|
|Discrete progress|Trigger Vibration - Right / 扳机振动-右 `L:0x180636de0/ea0`|Right impulse intensity|
|Binary switch|Impulse Trigger Sync with Rumble / 扳机振动跟随握把振动 `L:0x180633570/5c8`|Impulse follows grip|

**O:** `0x180cedce0` [SIXLEVEL], `.009.c:58696-58835`, clamps level0..5 and displays0..5. Six discrete positions, **not percent and not arbitrary255**. Actual packed-feature encoding belongs profile codec spec. Exact test prompt: “*Hold LT/RT to test grip vibration.\nHold PL+LT / PR+RT to test trigger vibration.” (`L:0x180636ec8`, dcefc50). Diagram `/Assets/XboxWired/mapping/Mapping_bg_L7.png`, `.010.c:14720-14728`; checkbox `/Assets/XboxWired/sticks/check_Normal.png`, `check_Selection.png`; packaged `/Assets/Xbox3E/Vibration_Normal.png`.

### RGB / Lighting

**O reachable in both applicable3E titles.** `Light/RGB_New.xaml`, constructor`0x180d7cbf0`, loader`0x180d7d140` [MAPPING], `.010.c:23550-23678`. Buttons **Effect / 效果**, disable **Close / 关闭**, speed control **Speed**; swatches/foreground/background controls are conditionally displayed by effect. EN keys `L:0x180642808` Effect,`0x1805e2800` Close,`0x180642830` Speed; CN Close`0x18063bac0`. Constructor's selected-effect state3 is not evidence of persisted device default.

|Control|EN/CN key|Observed options/domain|Function evidence|
|---|---|---|---|
|Effect popup|`View/EffectSettingView.xaml`|**1 Close/关闭,2 Light-tracing/追光,3 Fire Ring/风火轮,4 Fixed/常亮**|0x180d1f560/0x180d203b0/0x180d20bc0 [EFFECT]; Fixed0x180d21260 [FIXED]|
|Speed progress|Speed; exact CN label bakedXAML not recovered here|**1..10**, reset5|0x180c8cb80 [LIGHT],0x180c80950 [RESET]|
|Color popup / foreground/background selectors|`View/ColorSettingView.xaml`; Foreground Color / 前景色 `L:0x18063bc48/bc08`; Background color / 背景色 `L:0x18063bc80/bc28`|FireRing2 colors, Light-tracing2 colors; index0background,index1foreground; Fixed stores24 colors and UI uniform edit fillsall24|0x180d3fb40 [RGB];0x180c80950 [RESET]|
|Hue slider|Hue / 色调 `L:0x18063bba0/bb20`|0..255, six42.5 segments red→yellow→green→cyan→blue→magenta→red; channel truncation|0x180d1ff50 [EFFECT]|
|Saturation slider|Saturation / 饱和度 `L:0x18063bb70/bb00`|**UI uses max(R,G,B),0..255**, display trunc(v*100/255)%; **not standard HSV saturation**|0x180d1f760/0x180d1fb70 [EFFECT]|
|Brightness slider|Brightness / 亮度 `L:0x18063bb40/bae0`|Stored color byte0,0..255, display trunc(v*100/255)%|0x180d1f760/0x180d1fb70 [EFFECT]|

Color representation observed `0x180d3fb40` [RGB]: packed u32 brightness | R<<8 | G<<16 | B<<24, so bytes `[brightness,R,G,B]`; do not assume byte0 is fixed alpha255. Legacy RGBController labels it transparency; modern RGB_New labels brightness. Effect labels Fixed`L:0x18063b830/0x18063bcd8`, FireRing`0x18063b850/0x18063bcb8`, Light-tracing`0x18063b878/0x18063bbe8`. Full byte palette/default behavior and cross-slot decoder quirk: `local://spec-profile.md`, Lighting section. No unobserved LED positions or per-LED editor inferred from24stored colors.

### Motion

**O conditional tab, only L7Title_New.** `Motion/Motion_New.xaml`, constructor/loader`0x180d7b010/0x180d7b690` [MODERN], `.010.c:22714-22904`; binder`0x180d4a0c0` [MOTION], `.010.c:745-852`.

|Control|EN/CN key|Exact UI options/domain/backing feature|
|---|---|---|
|Mapping popup|Map motion sensing to: / 将体感映射为： `L:0x180633388/3d0`|0 Left Stick (recommended for racing games) / 左摇杆（适用于赛车游戏） `L:0x1806334d8/540`; 1 Right Stick (recommended for shooter games) / 右摇杆（适用于射击游戏） `L:0x180633438/4a8`; 2 Mouse / 鼠标 `L:0x1806333f8/418`|
|Enable-method popup|Enable method / 开启方式 `L:0x180633c40/c70`|0 Press and hold to enable / 按住启用 `L:0x180633b88/bd0`; 1 Press once to enable/disable / 按下启用，再次按下关闭 `L:0x180633b08/b58`|
|Activation-button icon picker|Enable button / 开启按键 `L:0x180633bf0/c20`|Exactly21 choices: standard16 physical inputs, PL,PR,L4,R4,Null; no combos|
|Discrete slider|Sensivity / 灵敏度 `L:0x180633ac0/ae8`|Stored/backing **0..5**, displayed **1..6**; six positions|
|Progress slider|Deadzone Compensation / 死区补偿 `L:0x1806327f8/838`|Raw **0..255**; displayed **floor(v*100/255)%**|
|Live/controller diagram|Image|`/Assets/XboxWired/mapping/Mapping_bg_L7.png`|

Option binder evidence d4a0c0 [MOTION]. Button-array constructor`0x180d49440` [MOTION], `.010.c:412-434`, allocates0x15 values, exact order: `2000 A,1000 B,10 X,20 Y,400 L1,4000 L2,2 L3,800 R1,8000 R2,4 R3,8 Select,1 Start,200 Up,100 Down,80 Left,40 Right,02000000 PL,04000000 PR,00200000 L4,40000000 R4,0 None`. Icon resolver`0x180d49e90` [MOTION], `.010.c:646-721`, emits `/Assets/Xbox3E/Mapping_<button>.png`, `_Default` when requested; PL/PR conflict with mapping page noted above.

Sensitivity clamp/display`0x180d45f60` [LEVELS], `.009.c:99138-99249`; update globalDAT_1805c1b5c via`0x180d4a440` [MOTION]. Compensation clamp/display`0x180d48110/0x180d48250` [COMPENSATION], `.009.c:100375-100453`; callback`0x180d4a430` storesDAT_1805c1b60. **Do not reuse legacy MotionController's1..10 sensitivity or raw255display.** Hidden angular-velocity/acceleration and X/Z-axis fields exist in profile storage/legacy controller, but not exposed by recovered simplified new-page initializer; preserve imported bits rather than invent extra3E controls. Numeric IDs above are UI enums/button tokens, not protocol opcodes.

### Setting / Calibration / Reset

**O** `Setting/SettingController.xaml`, constructor`0x180d30a60`, loader`0x180d310c0`, initializer`0x180d30820` [SETTINGS], `.009.c:89200-89464`. Labels/actions Auto Reconnect/自动启停; Tournament Mode/赛事模式 (`L:0x18063cdd0/cdb0`); Calibration; Back; **Initialization Complete** (`L:0x18063ce08`). Constructor explicitly collapses element`+0x100`; declared labels therefore do **not** prove all rows visible. Initialization reads managed header byte`+0xb` bits0x10 and8 into switches`+0x108/+0x118`. Preserve exact visibility/rawbit behavior, not assumptions from labels.

Observed d30a60 explanation pairs:
- “*When Tournament Mode is enabled, the polling rate increases to 1000Hz for lower input latency, but reduces battery life.” / “*启用赛事模式时会提高回报率至1000Hz，可带来更低的输入延迟，但会降低续航时间。” `L:0x1806336a8/0x1806337b0` (CN original has spaces around1000 Hz). **UI prose, not a measured hardware rate.**
- “*When enabled, the controller will automatically power off and charge when placed on the dock, and automatically power on and reconnect when removed.” / “*开启后，当手柄放入底座，将自动进入关机充电状态；当手柄从底座上取出，将立即自动开机并进入连接状态。” `L:0x180633820/0x180633960`.

**O Calibration chooser:** `Setting/CalibrationView.xaml`, `0x180d182d0/0x180d188b0` [CALIBRATION], `.009.c:77418-77604`, buttons **Calibrate Joysticks**, **Calibrate \r\nTriggers**, **Calibrate Motion Controls**, Back. Host-mode branch changes fonts/margins, not model exclusion.

|Target page|Controls / English visible content|CN key / stages|
|---|---|---|
|Setting/CalibrationStickView.xaml|Back, left/right indicators, Start Calibration. “When ready, do not touch the joysticks. Select Start Calibration to begin.” `L:0x18063af78`|若准备就绪，请勿触碰摇杆，选择“开始校准” `L:0x18063af30`; completion“Joystick calibration complete” / 摇杆校准完成 `L:0x18063c070/c048`|
|Setting/CalibrationTriggerView.xaml|Back,left/right,Start Calibration. “When ready, do not touch the triggers and make sure the trigger mode switch is turned off. Select Start Calibration to begin.” `L:0x18063ad30`|若准备就绪，请勿触碰扳机并确保扳机模式开关未打开，选择“开始校准” `L:0x18063acd0`; completion“Trigger calibration complete.” / 扳机校准完成 `L:0x18063c0e8/c0c0`|
|Setting/CalibrationAxisView.xaml|Back,flat-surface diagram,Start Calibration. “Place the controller on a flat surface with the joystick side facing up.” `L:0x18063abb8`|Ready EN/CN`L:0x18063aa20/0x18063a9b8`; waiting“Calibrating. Do not touch the controller during calibration.” / 正在校准，在校准过程中请勿触碰手柄 `L:0x18063a8f0/a980`; success“Motion calibration complete.” / 体感校准完成 `L:0x18063a7f8/a7b0`|

Text setters `.009.c:75655-75659,76017-76021,76428-76432`. Start Calibration / 开始校准 `L:0x18063a848/0x18063aa00`; CN completion action重新校准 `L:0x18063a7d8`. Exact punctuation/spacing is retained in strings.tsv. Trigger stage prompts: “Please press the right trigger to the bottom twice.” / 请将右扳机按至底部两次 `L:0x180631b50/bd0`; left equivalent`0x180631c00/c80`. Stick stage variants clockwise2or3 rotations exist (`L:0x180631f80,0x180632070,0x180632160,0x180632250`); maintenance flow spec determines applicable stage, not arbitrary choice.

Packaged assets: `/Assets/Xbox3E/CalibrationAxis.png`, `CalibrationStick.png`, `CalibrationStickTisp.png`, `CalibrationTrigger.png`, `CalibrationTriggerTisp.png`, `CalibrationSure.png`, `Calibration_Enter.png`, `Calibration_Loading.png`. **O existence only** where no explicit code reference ties exact file to visual.

**O Reset:** `View/ResetView.xaml`, constructor`0x180d17370` [CALIBRATION], `.009.c:76988-77067`: Factory Reset; “Factory Reset This will erase the following data and restart the device. Please confirm:”; “1.All configuration profiles\n2.Bluetooth pairing information\n3.All other settings”; Cancel/Ok. `L:0x18063b198/b1c8/b290`. Older `StatusBar/RestoreFactoryController.xaml` separately has “Restore to default factory settings” / 是否恢复到出厂设置 `L:0x18063c8a8/c908`, Yes/确定,No. Do not merge legacy/new dialogs into one claimed screenshot. Detailed maintenance applicability/opcodes are separate, not guessed from UI.

### Firmware

**O** `Update/UpdateController_New.xaml`, constructor`0x180d6ad60` [TRIGGERS], `.010.c:15548-15746`; 3E model name/image branch `.010.c:15676-15686`. Check Update / 检查更新 `L:0x1806328b8/8e8`; Current firmware version / 当前版本 `L:0x180633088/0d0`; Update to / 更新至 `L:0x180633038/068`; Update Details / 更新内容 `L:0x180632f18/f50`. `View/UpdateView_New.xaml` literal`L:0x18063fb98`; completion Update Success`L:0x18063fb60`. Release notes and version strings are network/dynamic data, not fixed content to invent. Firmware flash not exercised.

### Test

**O** `Test/TestController.xaml`, constructor`0x180d711d0` [STICKS], `.010.c:18804-19038`; live buttons/sticks/triggers; Back. “* Unable to test the XBOX, Square, Star, Share, and Profile buttons.” `L:0x1806415d0`; “Press and hold              to return” `L:0x180641670` (gap contains an icon, not a missing word to guess). Exact CN variants not among25PRI strings or recovered inline pairs. Do not confuse diagnostic display with vibration test actions. **No Headset tab** in either model-applicable title, despite global Headset root classes and physical USB audio interfaces.

## 4. Dialog / notification text catalogue

**O exact literals;** existence does not guarantee every error reachable on every transport. Sources `reconstruction/ir/strings.tsv:9243-9302,9898-10075`; owning pages/functions above. Address pairs are native string keys; standard buttons use §2 PRI keys or inline variants.

|Context|English|Chinese|EN/CN keys|
|---|---|---|---|
|Discovery|Please connect XBOX licensed 8BitDo device|请连接八位堂 XBOX 版设备|0x1806423f0 / 0x1806423b8|
|Home prompt|Click the software window and move a joystick to retrieve device information|No recovered inlineCN pair|0x18062f340; d501a0[SHELL]|
|Calibration error|Calibration failure, please try again!|校准失败，请重试|0x1806324a0 / 0x180632508|
|Calibration success|Calibration complete|设备校准已完成|0x180632530 / 0x180632570|
|Write error|Write failed, please try again!|写入失败，请重试|0x180632598 / 0x1806325f0|
|Deadzone warning|ATTENTION: Disable Deadzone may cause drift. Use at your own risk. |关闭摇杆中心死区可能会导致漂移，请谨慎操作。|0x180632618 / 0x1806326b8|
|Firmware info error|Error in retrieving firmware information.|获取固件信息异常|0x180632a90 / 0x180632af8|
|Already latest|You're already using the latest version.|已是最新版本|0x180632b20 / 0x180632b88|
|Network error|Unstable network connection, please try again later.|网络连接异常，请稍候再试|0x180632bb0 / 0x180632c30|
|Update warning|Do not exit the software or shut down the computer during the update process.|更新完成前请不要关闭软件、断开设备连接或关闭设备电源。|0x180632c60 / 0x180632d10|
|Update error|Update failed, please try again.|更新失败，请重试|0x180632d60 / 0x180632db8|
|Download error|Download failed.|下载固件错误|0x180632de0 / 0x180632e18|
|Latest variant|Your firmware is up-to-date.|已经是最新版本固件|0x180632e40 / 0x180632e90|
|Download status|Downloading...|下载固件中...|0x180632eb8 / 0x180632ef0|
|New version|New Firmware Available|手柄发现新版本|0x180632f70 / 0x180632fb8|
|Update status|Updating...|正在更新...|0x180632fe0 / 0x180633010|
|Sync status|Syncing...|正在同步...|0x18063c6a8 / 0x18063c6d8|
|Sync success|Sync Complete|同步完成|0x18063c600 / 0x18063c9b0|

## 5. UWP root types and asset inventory

**O** `reconstruction/original/UWPAdvance.xr.xml:330-551` defines custom roots, not per-model route eligibility. Relevant roots:
- `MainPage_New`; `Mappings.Mapping_View`; `Mappings.Controller.MappingSettingController_New`.
- `Sticks.StickController_New`, `Sticks.StrickProgress_New`, `Sticks.Curve_View`, `Sticks.JitterSettingView`.
- `Triggers.TriggerController_New`, `Triggers.ArcImage_New`; `Vibration.Vibration_New`, `Vibration.VibrationSettingView_New`.
- `Motion.Motion_New`, `Motion.SensitivityProgress_New`, `Motion.DeadZoneProgress_New`.
- `Platform.PlatFormController_New`, `Platform.PlatformController`, `SettingMenu.SettingMenuView`, `Setting.SettingController`, all3new calibration pages.
- `TitleView.L7Title_New`, `TitleView.L7Title_GIP`; `Update.UpdateController_New`; `Test.TestController`; `View.SwitchButton`; lighting RGB/ColorPicker/AlphaProgress/ColorProgress/SpeedProgress/LightSetting roots.
- Legacy ThreeTitle/UltimateTitle/Headset classes also exist; **root presence is not model support**.

XML has no stock Slider or ToggleSwitch root; custom progress and image-backed SwitchButton supply those roles. ComboBox/ComboBoxItem roots exist elsewhere, not proof every modern popup is a native ComboBox. TextBox.MaxLength is exposed in XML but exact3E rename limit is not recovered from that declaration alone.

**O glob-observed `/Assets/Xbox3E/` families**, exact case/spelling:
- `Mapping_{A,B,X,Y,Up,Down,Left,Right,L1,L2,L3,L4,R1,R2,R3,R4,PL,PR,Select,Start,Null}.png`, respective `_Change` and `_Default` states; `Mapping_Swap.png`, `Mapping_Swap_Change.png` only (do not synthesize Swap_Default).
- `MappingButton_Left.png`, `MappingButton_Right.png`, `MappingButton_Enter.png`.
- `Switch_Close.png`, `Switch_Open.png`, `Switch_Cursor.png`; code selections `.009.c:62795,62930`.
- `LT_Normal.png`, `LT_Enter.png`, `RT_Normal.png`, `RT_Enter.png`.
- `StatusBar_A.png`, `StatusBar_B.png`, `StatusBar_X.png`, `StatusBar_Y.png`, `StatusBar_Menu.png`, `StatusBar_View.png`.
- `platform1_Normal.png`, `platform2_Normal.png`, `platform3_Normal.png`, `Vibration_Normal.png`, `check_Selection.png`.
- `DeadZone_Tisp.png`, `DeadZone_Tisp_en.png`, `DeadZone_Tisp1.png`, `DeadZone_Tisp1_en.png`, `AntiDeadZone_Tisp.png`.
- Calibration assets in §3; `Connect_2_4.png`, `Connect_USB.png`, `Loading.png`, `Warnning.png` (original spelling).

**O actual3E diagrams are largely outsideXbox3E:** `/Assets/XboxWired/mapping/Mapping_bg_L7.png`, `mapping/Mapping_back_L7.png`, `Triggers/Triggers_bg_L7.png`. `*_Three.png` files are another model route. Generic tabs `/Assets/XboxWired/tabBar_New/{Mapping,Stick,Trigger,Vibration,RGB,Motion}_{Normal,Enter}.png`, references `.009.c:61459-61945` [TABS]. Common progress `/Assets/XboxWired/Progress/Cursor_Normal.png`, `Cursor_Enter.png`, `ProgressBtn_Normal.png`, `ProgressBtn_Enter.png`; checkbox `/Assets/XboxWired/sticks/check_Normal.png`, `check_Selection.png`.

## 6. Reconstruction boundaries and implementation gotchas

- Source-backed inventory is not pixel-perfectXAML or live Windows screenshot. Exact bakedCN text without inline pair remains unknown. No substitute translations claimed observed.
- Managed offsets vs wire offsets differ. UI option/button IDs are not opcodes; codecs must use profile/transport contracts.
- Deadzone progressraw255→storedpercent100; triggerraw255→visiblepercent100; rumble/jitter0..5; motion sensitivitystored0..5→visible1..6; compensationraw255→percent100. Do not standardize away distinct domains.
- EN/CN discrepancies and typos are evidence: ConfigSolt,Sensivity,Stick Sensitivity vs防抖动,anti-jitterhelpdefault5vs2,PL/PRimage identity disagreement. Stored defaults and user-sample settings are not constructor/UIhelp defaults.
- Both L7 titles exposeRGB; only New exposesMotion; no Headset tab. Do not add SOCD/macros/keyboard targets from other models.
- Lighting Saturation is nonstandardmaxRGB amplitude; Brightness is byte0, not assumedalpha255.24storedFixed colors do not prove a24-node editableLED layout.
- No checks were run; this is a read-only investigation. Parent owns implementation integration, hardwareI/O, and consolidated verification.

## Evidence aliases

All addresses below have image prefix`0x180`; abbreviated functionnames in tables refer to those full addresses. IR filenames are under `reconstruction/ir/`.

|Alias|evidence_id|Principal functions|
|---|---|---|
|SHELL|ev_ebdc5977288a0e584a213e4a1087fdb677f89b960bfc5eb5d354cfc8510751f7|c501a0,c50620|
|SHELLSTYLE|ev_eb24534b10694e07513f3199072969fa0bd9f4773ffe06f575a25e5f5a7adae2|c4fe40|
|HOME|ev_7e4805ad42512bd0d9b63a03486e66e4baeab5f09e99fb8765c566c8126ba9e8|d7f330,d80890,d805a0|
|HOST|ev_fa2a3d9b17b01ea0b053e4eb81b6151e6bd92e8f2e68b0e01917e28416915e50|cc5fb0|
|MENU|ev_7b5115a5544e546984915d1b84ee69393cebd4c482b7ab16cd2906071ae612b3|d337c0,d338f0,d33b20,d33d90|
|TITLE|ev_f4323834a29b612a1778bc7bf3834476a3e70f0bafd42073073c3f842ce29608|d6ef60,d6f390,d6f440,d6f7e0|
|SLOTS|ev_1d31aa13fc6d1c2ca5e453b3b60c46b2bd02f1674f59ec326bd2aaf36b196ac3|d2a930,d2b0e0|
|PLATFORM|ev_63021b04bd745d1073996c0b85b85c262c3ecf03217d9777af95af65b58f721d|d3b510|
|MAPPING|ev_b461008a7c56b4a11a55a73f1ddc105b8ae03991638e8fa25348620112b3d384|d7c580,d7cae0,d7cbf0,d7d140|
|GEOMETRY|ev_de366c57126de24bc5aa2936d3e6041a84dab1f31894403389966ae2b7d37feb|cdc970,cdea30|
|REMAP|ev_a0c7578348ab36bd379e24fdcd1641ad588f25e016290ef42af36b5ea6854bbd|d0b6c0,d0c290,d0c4b0,d0d130,d0e0b0|
|STICKS|ev_95b46a1a071025c2a277fa810b6519a093544be7f61268f18274777141260fa2|d76110,d78160,d787c0,d79040,d711d0|
|STICKVALUES|ev_ede68e6258948136c4f8c18451e208e168ffd977d1fdc20be63ac7511c192036|d059f0,d06a00,d07230,d05690,d057b0,d05840,d058d0|
|PROGRESS|ev_6d52fdb9eeabc2119326652170e1e9930a5faade49cd99bea2cd829870a2355b|cfcca0,cfe4a0,cff7c0,d001f0,d003c0|
|CURVE|ev_b94508b556289abcfe336422c3f4af88efe673943dd354f14d5aca16e4639f14|d08780,d08f10|
|HELP|ev_9fba98bc1e7d498291cf789db6a937a2eb53f9fe73226cc8c818dddd99bb3d7e|d01390|
|TRIGGERS|ev_dd9d6382ab8f87fba66340a55e0380eae6e76b9b29653c1c574fb5ef2651d04d|d6ca60,d6d4a0,d6d5f0,d6ad60|
|TRIGGERVALUES|ev_060bfc288df5370d90a3b90b0ae849b5e0aea886e986d9e0585dabc471bc5ad4|cf3030,cf2cd0|
|VIBRATION|ev_4d052f7d138ac155a4f2dde52fb589196237a5a821b18bf4dff8b7db0ce83ed9|d688c0,d68fc0|
|VIBLABEL|ev_e4f9af7cf57362b54fbc5235e095bc60534ef9f3aa1a6a16c58f0e150b49e8c6|cefc50|
|SIXLEVEL|ev_5847cc7002766b2f802e2c0eb50a9a04c37cf5a4950c50936dffaba95447f5b6|cedce0,ceecb0|
|LEVELS|ev_c722a339db1dc45dd36ba2cf753d94686a2da2c8e379a500b33e775caf5df18d|d45f60,d46a40|
|MODERN|ev_27d2a4015dc3325dade755fd6b22448e047f5cf59ff12a58ce059eb889d1d16e|d7ab50,d7b010,d7b690|
|MOTION|ev_85b2d3ea49bf1742d8ab5832af81d8b7592d3a499b11af77eeb2fa5086d1c4a9|d49440,d49e90,d4a0c0,d4a430,d4a440|
|COMPENSATION|ev_548623ec1cb109c735b1daf49f48018a2cf2d37679948e1ebee5eca5b2f79d70|d48110,d48250|
|SETTINGS|ev_b52f2e210711ee633e552e6b5b8742166e2fc57db085720350d4a92a4bd192b3|d30820,d30a60,d310c0|
|CALIBRATION|ev_030024085343b462a4b9def7759197b7a31d9a33fff614d53dee21ff24c068e9|d182d0,d188b0,d17370|
|TABS|ev_c77107d0210604c81bbb0c5f738e8f05735a662bfda3afced40a03c9d4d4e564|cf3830,cf6e90,cf71b0,cf7500,cf7600|
|RATE|ev_e8dd03488a043a96f2c19ef96e229bd1e7c239c5f922779afc0ad6f1e60b9801|d15760|
|LIGHT|ev_c735e6a708c02d2bfc1892748c7a47c79a7375421254fa6d8d755248418bd681|c8a710,c8cb80|
|RESET|ev_4f2cf40de1612289b89c2c2d3cfdefc0cb480302ce2f02723b27aef21f160553|c80950|
|RGB|ev_5692e29e6e37f1b5d16935baff04f4d6eaa16314de6672480961875504b3dd50|d3fb40|
|EFFECT|ev_7c1b07f8bb94eacac39855981b9110889346caf7056e72923ae04baf13ec8a91|d1f560,d203b0,d20bc0,d1f760,d1fb70,d1ff50|
|FIXED|ev_dc088566f316c976f0c12d168ab4cd2b78d1c1ab867671af93306a00044dcf42|d21260|
