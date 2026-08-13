# Kiri 产品契约(迁移宪法)

> 本文是 Kiri 从 Swift/macOS 原生实现 1:1 复刻迁移到 Tauri(Rust + React,macOS + Windows)时必须遵守的权威产品契约与验收标准。
> 全文由以下必读文档提炼:AGENTS.md、ROADMAP.md、README.md、README_ZH.md、
> docs/adr/0001–0004、docs/plans/ 下的 7 份设计/交接文档。
> 引用原文时以英文原文关键句呈现,出处标注在括号内。

---

## 0. 权威优先级与阅读顺序

1. `AGENTS.md` 是仓库的"第一事实来源"("This file is the first source of truth for agents working in this repository")。
2. 历史计划(`docs/plans/`)描述旧版本的构建方式;当它们与当前源码、AGENTS.md、更新的 ADR 或当前交接文档冲突时,**更新的来源胜出**:

   > "Historical plans under `docs/plans/` describe how earlier versions were built. When they conflict with current source, this file, a newer ADR, or the current handoff, the newer source of truth wins."(AGENTS.md)

3. 明确的取代关系(迁移时必须按"最新"口径理解,不得复刻旧行为):
   - ADR-0001(Single Capture Session)已被 `2026-08-01-kiri-capture-flow-reset-design.md` 取代(ADR-0001 Status 字段)。
   - ADR-0003(Single-outline window hover)取代早期捕获计划中的"stacked hover-preview"样式("Supersedes: The stacked hover-preview styling in earlier capture plans")。
   - `README_JA.md` 未与 v0.2 同步,不作为契约依据("do not claim it is")。
4. 迁移复刻的目标是 **当前源码已实现的 v0.1 + v0.2 行为**,而不是 roadmap 中尚未完成的条目(见 §6 未完成项,不得当作已存在功能实现)。

---

## 1. 不可妥协清单(逐条 + 出处)

以下每一条都是迁移必须遵守的产品决策;任何一条被破坏即视为迁移失败。分类列出。

### 1.1 平台与技术基线

| # | 不可妥协项 | 出处(关键原文) |
|---|---|---|
| 1.1.1 | 最低平台 macOS 14;Swift 6 + Swift Package Manager,使用 Apple 框架而非第三方依赖。 | AGENTS.md:"Minimum platform is macOS 14; the project uses Swift 6 and Swift Package Manager with Apple frameworks rather than third-party dependencies." |
| 1.1.2 | 应用是"原生、本地优先"的 macOS 捕获工具("native, local-first macOS capture utility")。 | AGENTS.md |
| 1.1.3 | 分发后的应用**不得依赖** Homebrew 或任何外部可执行文件即可工作(拖入 Applications 即可用)。 | ADR-0002:"The distributed application must work after dragging Kiri.app into Applications without requiring Homebrew or another executable." |
| 1.1.4 | 媒体能力全部来自系统自带框架:ScreenCaptureKit(取帧)、AVFoundation(H.264 MP4 写入/抽帧)、ImageIO(动画 GIF 编码);"No external runtime dependency"。 | ADR-0002 |

> 迁移语义:1.1.3/1.1.4 的**本质约束**是"零外部运行时依赖、零 Homebrew/FFmpeg 依赖"。在 Tauri 落地上,应映射为"只依赖系统自带能力 + 随应用打包的 Tauri/WebView2 运行时",不得引入需用户另装的可执行文件;macOS 侧仍需 ScreenCaptureKit/AVFoundation 思路,Windows 侧映射见 §2。

### 1.2 全局快捷键与键盘

| # | 不可妥协项 | 出处 |
|---|---|---|
| 1.2.1 | 全局捕获快捷键**唯一**为 `⇧⌘A`(Shift-Command-A)。 | AGENTS.md:"The global capture shortcut is exclusively `Shift-Command-A` (`⇧⌘A`)." |
| 1.2.2 | `⇧⌘A` 是**唯一**捕获快捷键,须移除预设菜单("Make `⇧⌘A` the only capture shortcut and remove the preset menu.")。 | hotkey-conflict-design |
| 1.2.3 | 快捷键必须在**键盘事件流最前端**安装 active session event tap,并**吞掉**匹配的 key-down 与 key-up,使前台应用收不到该快捷键(一次按键不得触发两个动作)。 | hotkey-conflict-design:"Install an active session event tap at the head of the event stream. Consume matching key-down and key-up events so foreground applications do not receive the shortcut." |
| 1.2.4 | 忽略自动重复(autorepeat);对因超时/用户输入被禁用的 tap 要能重新启用。 | hotkey-conflict-design:"Ignore autorepeat and re-enable a tap disabled by timeout or user input." |
| 1.2.5 | 需要 Accessibility 权限;过滤器无法安装时提供**直接恢复动作**(recovery action)。 | hotkey-conflict-design:"Require Accessibility permission and provide a direct recovery action when the filter cannot be installed." |

### 1.3 捕获流程与焦点

| # | 不可妥协项 | 出处 |
|---|---|---|
| 1.3.1 | 初始 overlay **第一级**即提供 Screenshot 与 Record 两个模式("The initial overlay offers Screenshot and Record at the first level.")。 | AGENTS.md |
| 1.3.2 | 截图完成是 **剪贴板优先(clipboard-first)**,并把焦点**归还给原应用**;每次捕获后**不得**打开 Kiri 素材库。 | AGENTS.md:"Screenshot completion is clipboard-first and returns focus to the original application. Do not open the Kiri library after every capture." |
| 1.3.3 | `Esc` 取消捕获**和**倒计时;`Return` 确认(复制)截图。 | AGENTS.md:"Escape cancels capture and countdown; Return confirms a screenshot.";README:"Esc — cancel capture / Return — copy capture" |
| 1.3.4 | 完成捕获时,先写入原始 PNG + 元数据记录到素材库,**再**复制到剪贴板(保证即使后续剪贴板/保存失败仍可恢复)。 | kiri-design:"Completing a capture writes an original PNG and a metadata record into the library before copying it to the clipboard." |
| 1.3.5 | 选区与标注在**同一个无边框 overlay 窗口**内完成,不切换应用上下文(单捕获会话理念;其早期 ADR 虽被取代,但"inline annotation + 无上下文切换"已固化为现行行为)。 | ADR-0001 / AGENTS.md |
| 1.3.6 | 空选区或越界选区**必须拒绝且不写入任何 asset**。 | kiri-design:"Empty or off-screen selections are rejected without writing an asset." |

### 1.4 窗口悬停与选区

| # | 不可妥协项 | 出处 |
|---|---|---|
| 1.4.1 | 悬停可识别窗口时**恰好显示一条**克制的紫色描边(outline)。 | AGENTS.md:"Window hover shows exactly one restrained violet outline…";ADR-0003:"Hovering an eligible window displays exactly one violet outline." |
| 1.4.2 | 悬停**不显示**:手柄(handles)、尺寸(dimensions)、堆叠边框(stacked borders)、放大镜(loupe)、跟随指针的 tooltip。 | AGENTS.md:"without handles, dimensions, stacked borders, or a following tooltip.";ADR-0003:"Hover does not display handles, dimensions, a stacked white border, a loupe, or a pointer-following tooltip." |
| 1.4.3 | 单击选中高亮的最前窗口;拖动创建自定义区域;在现有区域内拖动 = 移动;拖动八个手柄中的任意一个 = 缩放。 | AGENTS.md:"A click selects that window; a drag creates a custom region. Both selections remain movable and resizable with eight handles.";ADR-0003 |
| 1.4.4 | `CaptureCoordinator` 负责收集可见窗口矩形,用于 hover 与 click 命中测试。 | ADR-0003:"`CaptureCoordinator` collects visible-window rectangles for hover and click hit testing." |

### 1.5 标注

| # | 不可妥协项 | 出处 |
|---|---|---|
| 1.5.1 | 区域选定后**立即**出现标注工具("Annotation tools appear immediately after region selection.")。 | AGENTS.md |
| 1.5.2 | 已存在的文字与图形**保持可选中、可编辑**;尺寸控件**实时更新**("Existing text and shapes remain selectable and editable; size controls update live.")。 | AGENTS.md |
| 1.5.3 | 文字背景**默认透明**("Text backgrounds default to transparent.")。 | AGENTS.md |
| 1.5.4 | 马赛克是**连续画笔(continuous brush)**,可调直径与强度("Mosaic is a continuous brush with adjustable diameter and intensity.")。 | AGENTS.md |
| 1.5.5 | 支持画笔(pen)、矩形、直线、箭头、文字、马赛克;撤销/重做;选中/移动/改形/缩放/删除既有标注。 | ROADMAP v0.1、README:"P / R / L / A / T / M — pen / rectangle / line / arrow / text / mosaic" |
| 1.5.6 | 删除是移动到**应用管理的可恢复 Trash**,不立即销毁源文件("Deletion moves an item to an app-managed trash and does not immediately destroy the source.")。 | kiri-design |

### 1.6 录制

| # | 不可妥协项 | 出处 |
|---|---|---|
| 1.6.1 | 录制是 Retina 级(Retina-scale)、高质量 MP4(H.264),使用最佳捕获分辨率与"有界的高质量码率策略"。 | AGENTS.md:"Recording is Retina-scale, high-quality MP4.";v0-2 handoff:"Retina-scale recording dimensions, best capture resolution, and a bounded high-quality bitrate policy." |
| 1.6.2 | Kiri 自身的录制控件与**暂停时的时间(画面)不得出现在导出视频中**。 | AGENTS.md:"Kiri's recording controls and paused time must not appear in the exported video.";v0-2 handoff:"Kiri application windows excluded from recordings, preventing the floating control/pause UI from entering exported frames." |
| 1.6.3 | 暂停/恢复以 MP4 分片实现,最终合并为**一个** asset。 | v0-2 handoff:"Pause/resume implemented as MP4 segments merged into one final asset." |
| 1.6.4 | 可选系统音频、麦克风、指针与点击反馈(麦克风仅 macOS 15+;其余路径支持 macOS 14)。 | ROADMAP v0.2;v0-2 handoff:"Microphone recording is enabled only on macOS 15 or later; other v0.2 capture paths support macOS 14." |
| 1.6.5 | 短录屏可转优化 GIF。 | ROADMAP v0.2;ADR-0002(ImageIO 编码 GIF) |
| 1.6.6 | 录制完成后走"后台保存"行为(clipboard-first 的对应面)。 | v0-2 handoff:"Clipboard-first capture completion and background recording save behavior." |

### 1.7 倒计时与点击涟漪

| # | 不可妥协项 | 出处 |
|---|---|---|
| 1.7.1 | 3-2-1 倒计时居中、紧凑;**不得使所选录制区域变暗**("The 3-2-1 countdown is centered and compact; it must not dim the selected recording region.")。 | AGENTS.md |
| 1.7.2 | 可选紫色点击涟漪(click ripple):**实时可见,且同样被录制进视频**("The optional violet click ripple is visible live and is also captured.")。 | AGENTS.md;v0-2 handoff:"Live violet click ripple that is also included in the recording." |

### 1.8 本地优先与隐私

| # | 不可妥协项 | 出处 |
|---|---|---|
| 1.8.1 | 捕获永远留在本机;未经明确产品决策与隐私文档,**绝不**加入上传、分析(analytics)、账号、或任何网络行为。 | AGENTS.md:"Captures stay local. Never add uploads, analytics, accounts, or network behavior without an explicit product decision and privacy documentation." |
| 1.8.2 | 捕获不会自动离开设备("Captures never leave the device automatically.")。 | kiri-design |
| 1.8.3 | OCR 在**本机**完成(基于 macOS Vision),不上传。 | README.md:"OCR — local text recognition powered by macOS Vision." |
| 1.8.4 | 可恢复 Trash;永久删除必须是**显式**操作。 | kiri-design:"Trash is recoverable from the library; permanent deletion is explicit." |

### 1.9 视觉系统(Kawaii-professional)

ADR-0004 确立了唯一视觉系统,迁移时 React 组件库必须复刻同一套设计 token,不得各自引入散落的紫/蓝/灰值。

| # | 不可妥协项 | 出处 |
|---|---|---|
| 1.9.1 | 白色画布 + 白色抬升表面(浅色);plum-charcoal 表面(深色)。 | ADR-0004:"a clean white canvas with white elevated surfaces in light mode and plum-charcoal surfaces in dark mode" |
| 1.9.2 | 主操作色 = lavender;sky blue 表清新;peach pink 仅用于温暖强调或**破坏性状态**。 | ADR-0004:"lavender as the primary action color, sky blue for freshness, and peach pink only for warm emphasis or destructive states" |
| 1.9.3 | 圆角几何、细边框、柔和阴影,同时保留原生 macOS material 与控件。 | ADR-0004:"rounded geometry, fine borders, and soft shadows while retaining native macOS materials and controls" |
| 1.9.4 | 彩色 chibi 女孩 app 图标(紫蓝色头发 + 取景框母题);同一 chibi 形象作为应用内品牌标记,而非通用截图图标。 | ADR-0004:"a colorful chibi-girl app icon with violet-blue hair and a capture-frame motif; the same chibi-girl artwork as the in-app brand mark" |
| 1.9.5 | 捕获与 OCR overlay 用紧凑深色 material,保证在任意屏幕内容上可读。 | ADR-0004:"compact dark materials for capture and OCR overlays so they remain legible over arbitrary screen content" |
| 1.9.6 | OCR 结果面板为浅色、深色可编辑文字,保证对比度。 | ADR-0004:"a light OCR result panel with dark editable text to guarantee contrast" |
| 1.9.7 | 永久删除用**自定义**确认 sheet,不用视觉上无关的系统 action sheet。 | ADR-0004:"custom in-app confirmation sheets for permanent deletion" |
| 1.9.8 | **破坏性动作不得绑定 Return**,以降低误确认风险。 | ADR-0004:"Destructive actions are intentionally not bound to Return" |
| 1.9.9 | 可爱细节集中在图标/品牌标记/渐变/空状态;密集工作区优先可读性,不用装饰字符画。 | ADR-0004:"Cute details remain concentrated… Dense working surfaces prioritize legibility and do not use decorative character art." |
| 1.9.10 | 设计 token 集中在 `KiriDesignSystem.swift` / `CaptureUIStyle.swift`;新 UI 必须复用,不得引入孤立色值。 | ADR-0004 |

---

## 2. 跨平台迁移等价映射

> 约定:**必须遵守** = 该映射背后承载了 §1 的不可妥协语义,迁移必须落地为对应行为;
> **需产品确认** = 具体键位/位置/机制需产品拍板,本文给出建议但不得擅自定死。

| # | macOS 行为 | Windows 迁移建议 | 性质 | 说明 |
|---|---|---|---|---|
| 2.1 | 全局快捷键 `⇧⌘A` | `Shift+Ctrl+A`(建议) | 需产品确认键位;必须遵守排他性 | 键位本身需确认;但"唯一、无预设菜单、事件流最前端吞掉按键、忽略 autorepeat、可恢复"这些 §1.2 约束必须遵守。Windows 用 `RegisterHotKey` 或 `WH_KEYBOARD_LL` 低级钩子实现。 |
| 2.2 | `Esc` 取消 / `Return` 确认 | `Esc` 取消 / `Enter` 确认 | 必须遵守 | 语义一致。 |
| 2.3 | `⌘C` 复制(Command-C copies) | `Ctrl+C` 复制 | 必须遵守 | 剪贴板优先契约的快捷键面。 |
| 2.4 | 剪贴板图片(PNG) | Windows 剪贴板 `CF_DIB`/PNG | 必须遵守 | "clipboard-first"必须保持;图片格式 PNG 应保持一致。 |
| 2.5 | 素材库 reveal / Finder 定位("reveal") | 文件资源管理器 `explorer /select,"<path>"` 选中文件 | 必须遵守 | "reveal"语义(定位并在文件管理器中选中)必须保留。 |
| 2.6 | 菜单栏 app + `MenuBarExtra` | 系统托盘(tray)图标 | 需产品确认 | 菜单栏常驻 → 托盘是自然映射;是否同时保留任务栏存在需确认。 |
| 2.7 | Dock 存在(`.regular` 激活策略,非 `LSUIElement` 代理模式) | 任务栏 + 托盘 | 需产品确认 | macOS 上"像普通应用一样出现在 Dock"是明确决策(dock-presence-design);Windows 无 Dock,应映射为"有任务栏窗口 + 托盘常驻",具体组合需确认。 |
| 2.8 | Screen Recording 权限(TCC) | 无对等物(见 §4.3) | 需产品确认 | Windows 桌面(Win32)无逐应用录屏授权;用 Windows Graphics Capture API / DXGI 取帧,无等价隐私弹窗。需产品决定是否要引导用户、是否加隐私声明。 |
| 2.9 | Input Monitoring(全局键盘事件 tap) | 无权限弹窗;`RegisterHotKey`/`WH_KEYBOARD_LL` | 需产品确认 | 语义等价物是全局键盘钩子,但没有对等的用户授权 UI。 |
| 2.10 | Accessibility(事件 tap / 窗口几何) | `SetWinEventHook` / UI Automation | 需产品确认 | 用于读取可见窗口矩形与全局键盘;Windows 侧无等价权限概念。 |
| 2.11 | ScreenCaptureKit 取帧 | Windows Graphics Capture API | 必须遵守(能力面) | 承载 1.6 的录制契约。 |
| 2.12 | AVFoundation H.264 MP4 | Windows Media Foundation H.264 MFT | 必须遵守(能力面) | 承载"系统自带、零外部依赖"约束。 |
| 2.13 | ImageIO GIF | Windows Imaging Component(WIC) | 必须遵守(能力面) | 承载 GIF 编码契约。 |
| 2.14 | macOS Vision OCR(本机) | Windows.Media.Ocr(Windows OCR)或打包 Tesseract | 需产品确认 | 必须保持"本机 OCR、不上传";引擎选型需确认(优先系统自带以守 1.1.3)。 |
| 2.15 | `⌘F` 搜索 / `⌘Z`/`⇧⌘Z` 撤销重做 / `Delete` 删除选中标注 | `Ctrl+F` / `Ctrl+Z`·`Ctrl+Y` / `Delete` | 必须遵守 | 快捷键语义一致映射。 |
| 2.16 | `/Applications/Kiri.app` 固定安装路径 | `%ProgramFiles%\Kiri\` 或 `%LocalAppData%\Programs\Kiri` | 需产品确认 | 详见 §5;固定规范安装路径的**原则**必须遵守。 |
| 2.17 | `.dmg` 分发 | `.msi`(WiX)或 `.exe`(NSIS) | 需产品确认 | 见 §5。 |
| 2.18 | 资源/库存储 `~/Library/Application Support/kiri/` | `%APPDATA%\kiri\`(或等价) | 需产品确认 | 本地持久化边界(`AssetLibrary`)必须保留;具体目录需确认。 |

---

## 3. UI 验收清单(AGENTS.md 逐条展开为可测试项)

AGENTS.md "UI acceptance checklist" 的每一条展开为可测试(可写进 Tauri 端自动化/手动 QA)条目。

### 3.1 初始 overlay 的两种模式
- [ ] 从 `⇧⌘A` 唤起后,第一级同时可见 **Screenshot** 与 **Record** 两个选项(不得藏到二级菜单)。
- [ ] 点击/快捷键在两种模式间切换,选中态清晰。
- [ ] 模式选择后,才进入对应选区绘制(录制模式:先选模式、再画区域 — polish 计划明确"recording mode is selected before drawing its region")。

### 3.2 单描边悬停 + 点击选中 + 手动区域
- [ ] 指针悬停在可识别窗口上时,只出现**一条**紫色描边。
- [ ] 悬停时**不出现**:八手柄、尺寸数字、白色堆叠边框、放大镜、跟随 tooltip(逐项断言为"无")。
- [ ] 单击高亮的最前窗口 → 该窗口被选中。
- [ ] 从空白处拖动 → 创建自定义区域。
- [ ] 在已有区域内拖动 → 整体移动。
- [ ] 拖动八个手柄(四角 + 四边中点)的每一个 → 正确缩放(方向、比例正确)。
- [ ] 选中/移动/缩放后仍可继续编辑标注。

### 3.3 Esc/Return 与焦点归还
- [ ] 选区阶段按 `Esc` → 取消并关闭 overlay,原应用仍在前台。
- [ ] 倒计时阶段按 `Esc` → 取消倒计时。
- [ ] 截图完成按 `Return` → 复制到剪贴板。
- [ ] 截图完成后,**焦点回到捕获前的前台应用**。
- [ ] 截图完成后**不自动打开** Kiri 素材库窗口。

### 3.4 工具栏在窄区域与各屏幕边缘
- [ ] 选区极窄(如 40px 高/宽)时,工具栏仍完整可见、不遮挡内容、可操作。
- [ ] 选区贴近屏幕四个角/四条边时,工具栏与手柄不被裁剪、能自动换位或约束在屏内。
- [ ] 多显示器、混合缩放(Retina + 非 Retina)下工具坐标正确。

### 3.5 文字标注
- [ ] 可创建文字标注。
- [ ] 支持 **IME 输入**(中日/简中输入法候选窗口正常,不被吞键)。
- [ ] 对既有文字**二次编辑**可进入编辑态并保存。
- [ ] 修改字号时**实时**更新渲染尺寸。
- [ ] 背景样式:默认透明;切换背景样式即时生效。

### 3.6 马赛克与既有标注编辑
- [ ] 马赛克为**连续画笔**(拖一笔连续绘制,而非单点)。
- [ ] 可调**直径**与**强度**,调节即时生效。
- [ ] 已存在的任意标注(文字/图形/马赛克)仍可选中、移动、改形、缩放、删除。

### 3.7 录制帧检查
- [ ] 抽取录制**开始**附近的帧:清晰(Retina 分辨率,无马赛克模糊)。
- [ ] 抽取**点击涟漪**时刻的帧:涟漪可见且被录制。
- [ ] 抽取**暂停/恢复**前后帧:导出视频不含 Kiri 的任何控件、不含暂停时画面(暂停期间的静态帧不得以 Kiri UI 形式出现)。
- [ ] 抽取**停止**附近的帧:清晰、无 Kiri 控件。
- [ ] 确认导出为 H.264 MP4,暂停/恢复合并为单个文件。

### 3.8 QA 资产清理
- [ ] QA 期间**不把捕获留在用户素材库**。
- [ ] 只把 agent 创建的测试资产移动到 Kiri Trash(可恢复)。
- [ ] **未经同意绝不永久清空 Trash**。

---

## 4. 隐私与签名

### 4.1 稳定签名身份对 macOS 权限的含义(必须理解)

- macOS 把 Screen Recording 授权与应用的**代码身份(code identity)**绑定,而不只是可见名称或 bundle id:
  > "macOS associates Screen Recording consent with an app's code identity, not only its visible name or bundle identifier."(stable-privacy-identity-design)
- 因此**更换签名身份 = 破坏隐私授权**。ad-hoc 签名会改变隐私身份,导致 Screen Recording / Input Monitoring 授权失效:
  > "Do not silently use ad-hoc signing because it changes the privacy identity and can invalidate Screen Recording/Input Monitoring permissions."(AGENTS.md)
- 打包脚本**必须按固定顺序**选择稳定身份,且**绝不静默回退到 ad-hoc**:
  1. 显式 `KIRI_CODESIGN_IDENTITY`;
  2. 已安装的 Apple Development 证书;
  3. 已安装的 Developer ID Application 证书;
  4. 本项目现用的 `mimi Local Development` 证书。
  > "If none exists, packaging fails with an explanation. Ad-hoc signing remains an explicit development escape hatch through `KIRI_ALLOW_ADHOC_SIGNING=1`; it is never silent."
- 每次打包都启用 hardened runtime,并打印所选身份("Every package enables hardened runtime and prints the chosen identity")。
- 一次性权限迁移:只有 `io.yuxino.kiri` 的过期 Screen Recording 记录可用 `tccutil` 重置,**需用户明确确认**;不碰其他应用的隐私记录。长期应换 Apple 签发的 Apple Development / Developer ID 证书,该迁移本身也需要重新授予一次 Screen Recording。
- 固定 bundle id:`io.yuxino.kiri`(stable-privacy-identity / stable-install-permissions 中反复出现)。

### 4.2 Windows 上的等价(签名/安装身份)

| macOS 概念 | Windows 等价 | 说明 |
|---|---|---|
| 代码身份(code identity)与授权绑定 | Authenticode 签名身份 + 应用安装路径 | Windows 没有 TCC 式的逐应用录屏授权,但签名身份影响 SmartScreen 信誉、Defender、防火墙规则、未来"麦克风/摄像头隐私"对 UWP 的约束。**稳定签名身份同样是必须的**,不换签、不做自签名静默回退。 |
| Apple Development / Developer ID | 代码签名证书(推荐 EV / OV) | 对应 macOS 的"稳定签名身份"要求;发布版需 Authenticode 签名。 |
| notarization | SmartScreen 信誉 + (可选)Microsoft 签名提交 | 保证下载分发不被拦截;对应"签名并公证的发布构建"(ROADMAP:"Signed and notarized release builds")。 |
| `KIRI_ALLOW_ADHOC_SIGNING` 逃生舱 | 显式开发自签名开关 | 保持"绝不静默 ad-hoc"的同构约束。 |
| bundle id `io.yuxino.kiri` | MSI `UpgradeCode` / `ProductCode` / AppIdentity | 稳定安装身份,升级/重复实例检测依赖它(对应 4.4 的"唯一进程"约束)。 |

### 4.3 Input Monitoring / Screen Recording 在 Windows 的对应物

- **Screen Recording(TCC 录屏授权)**:Windows 桌面(Win32)应用**无逐应用录屏授权弹窗**。Windows 10 1803+ 的"屏幕录制同意"仅约束 UWP/Graphics Capture 的 UWP 路径。Win32 用 Windows Graphics Capture API / DXGI 取帧通常无需该弹窗。→ 无 1:1 等价物,建议(需产品确认):保持"捕获内容不上传"的本地语义,并在首次使用时显示本地隐私说明;不伪造一个假权限流程。
- **Input Monitoring(全局键盘事件 tap)**:对应 Windows 的全局快捷键注册(`RegisterHotKey`)或低级键盘钩子(`WH_KEYBOARD_LL`),同样没有逐应用的授权 UI。→ 语义等价物是钩子本身,授权面需产品确认。
- **Accessibility(事件 tap / 窗口几何)**:对应 `SetWinEventHook` / UI Automation,无权限概念。
- **麦克风**:macOS 15+ 才启用麦克风录制;Windows 侧麦克风访问受"设置 > 隐私 > 麦克风"约束(对 UWP 逐应用;Win32 不受逐应用开关限制)。→ 需产品确认是否提示、是否做隐私声明。
- **共同不可妥协语义**:无论平台,授权缺失时**必须给出清晰的系统设置引导**(kiri-design:"Missing Screen Recording permission opens a clear system-settings guide.");Windows 上应映射为"能力不可用/被禁用时的明确提示与恢复引导"。

### 4.4 单一实例与固定安装路径

- 唯一规范 bundle 名 `Kiri.app`;用户可见构建只从 `/Applications/Kiri.app` 安装运行:
  > "Produce one canonical bundle name: `Kiri.app`. Install and run the user-facing build only from `/Applications/Kiri.app`."(stable-install-permissions-design)
- 替换安装前先停止正在运行的 Kiri 进程(原子替换,避免旧可执行文件驻留)。
- 启动时终止其它同 bundle id 的进程;运行中观察应用启动并终止后续重复副本(先优雅终止,仅对该重复副本在必要时强制终止);另有 1 秒重复扫描兜底。
- 每次构建使用同一非 ad-hoc 签名身份。
- 迁移到 Windows 必须保留**"单一实例 + 唯一规范安装路径 + 稳定身份"**这组不变量:同一 app 身份不得同时多实例运行,更新前结束旧进程,原子替换。

---

## 5. 发布流程(打包、签名、安装路径 + Tauri 对应)

### 5.1 现有 macOS 发布流程(契约来源)

- `scripts/package-app.sh`:release 构建 + 稳定代码签名(AGENTS.md 仓库地图)。
- `scripts/install-app.sh`:打包并安装到 `/Applications/Kiri.app`(AGENTS.md)。
- 发布构建需签名并公证(ROADMAP v0.1:"Signed and notarized release builds")。
- 分发形态:GitHub Releases 下载,解压后把 `Kiri.app` 拖入 Applications(README:"Download the latest build … unzip it, and move `Kiri.app` to Applications.")。
- 安装路径约定:`/Applications/Kiri.app`(固定,QA/用户统一)。

### 5.2 Tauri 版本应如何对应

| macOS 现状 | Tauri 对应(macOS) | Tauri 对应(Windows) | 性质 |
|---|---|---|---|
| `/Applications/Kiri.app` | Tauri 产物 `Kiri.app`,安装到 `/Applications` | — | 必须遵守(路径约定) |
| `.dmg` 分发 | `.dmg`(tauri bundler 支持 `dmg`) | `.msi`(WiX)或 `.exe`(NSIS) | 需产品确认(Windows 具体安装器格式) |
| — | `.app` | 安装到 `%ProgramFiles%\Kiri\`(系统级)或 `%LocalAppData%\Programs\Kiri`(每用户) | 需产品确认(目录) |
| `scripts/package-app.sh` / `install-app.sh` | 对应 Tauri CLI `tauri build` + 签名/公证步骤 | `tauri build --bundles msi/nsis` + Authenticode 签名 | 必须遵守(打包须含稳定签名) |
| 稳定签名身份 + 硬运行时 | Apple 证书 + hardened runtime + notarization | Authenticode 签名 + 稳定证书身份 | 必须遵守 |
| 单实例 + 规范路径 | 单实例插件 + 固定 bundle id | 单实例锁(如 mutex)+ 固定安装身份 | 必须遵守 |
| `Info.plist` 无 `LSUIElement`、激活策略 `.regular`(Dock 存在) | 保持普通应用(不设 agent 模式) | 任务栏窗口 + 托盘(见 §2.7) | 需产品确认 |

> 关键迁移约束:发布产物必须沿用"稳定签名身份、签名后分发、固定规范安装路径、更新前结束旧进程"的完整链条;Tauri 侧不得用默认 ad-hoc/自签名静默发布。

---

## 6. 明确禁止的行为(全文汇总)

以下行为在任何情况下都**禁止**(来自各文档的显式禁令或强约束):

1. **不清空回收站**:Kiri Trash 可恢复;未经用户同意绝不永久删除("never empty Trash without consent";"must not be permanently deleted without consent")。
2. **不联网/不上传/不分析**:不加上传、analytics、账号或任何网络行为("Never add uploads, analytics, accounts, or network behavior");捕获不自动离开设备("Captures never leave the device automatically")。
3. **不静默 ad-hoc 签名**:打包绝不静默回退到 ad-hoc 签名(会改变隐私身份、破坏权限)。
4. **不换签名身份当排障捷径**:禁止"替换签名身份"作为排障手段("Do not … replace signing identities as a troubleshooting shortcut.")。
5. **不重置隐私权限当排障捷径**:禁止"重置隐私权限"作为排障手段;`tccutil` 重置需用户明确确认且只针对 `io.yuxino.kiri`。
6. **不删除捕获数据**:禁止删除用户的捕获数据("Do not delete capture data…")。
7. **不直接操纵用户素材库**:QA 期间绝不直接操纵用户素材库("never manipulate a user's library directly during QA");离屏快照脚本不得读取用户捕获库("it must not read the user's capture library")。
8. **不提交敏感内容**:禁止把私有捕获、凭据、个人绝对路径、`~/Library/Application Support/kiri/` 内容提交进仓库。
9. **不擅自做 Git/发布动作**:未经用户要求,不新建分支、不 commit/merge/push/tag/发布("Do not create a new branch, commit, merge, push, tag, or publish a release unless the user asks.")。
10. **不复刻未完成功能为已存在**:以下仅为 roadmap 未完成项,迁移时不得当成现有功能实现——Blur 标注、混合缩放多显示器验收、全屏录制、MP4 剪辑、内联视频/GIF 播放、录制时长与文件大小保护、Tags、可选同步、长截图(v0.3 滚动拼接)。(ROADMAP v0.1/v0.2/Later;v0-2 handoff"Known constraints";polish 计划"Long Screenshot is absent")。
11. **不破坏现有行为/文件**:不要 reset/discard/覆盖/重排无关工作("Never reset, discard, overwrite, or reformat unrelated work")。

---

## 7. 语言与文档义务

### 7.1 本地化

- 用户界面当前支持 **English 与 Simplified Chinese 两种**,并跟随 macOS 首选语言:
  > "User-facing UI currently supports English and Simplified Chinese and should follow the macOS preferred language."(AGENTS.md)
- English 与 Simplified Chinese 是**唯一被声明支持的 UI 本地化**("English and Simplified Chinese remain the only claimed UI localizations")。
- 两个语言的 key 集合必须**完全一致**("Keep English and Simplified Chinese key sets identical.")。
- 用户可见 AppKit/SwiftUI 字符串必须经 `L10n.text`/`L10n.format` 包装。
- 校验 `.strings` 文件(打包脚本或 `plutil -lint`)。
- 迁移到 Tauri:等价义务 = React 侧 `i18n` 资源必须**恰好**覆盖 en + zh-Hans 且 key 集一致;系统语言跟随逻辑需在 Tauri/前端保留;**zh-Hans 完整性是验收项**(每个 UI 字符串都有对应简体中文,无漏译、无占位)。
- 快捷键清单等可见文案两语需同步(README 中英文快捷键表已逐条对应)。

### 7.2 文档义务

- 用户可见行为变更时,`README.md` 与 `README_EN.md` **必须同步更新**("Update `README.md` and `README_EN.md` together for user-visible behavior.")。(注:仓库实际存在 `README_ZH.md`;迁移后应保证英文与简体中文两份 README 内容对等。)
- `README_JA.md` 未与 v0.2 同步,**不得声称其已同步**("`README_JA.md` is currently not synchronized with v0.2; do not claim it is.")。
- README 声明必须与源码一致:Long Screenshot 不存在、OCR 需要拖动一个区域、录制模式在画区域之前选择("Long Screenshot is absent, OCR requires a dragged region, and recording mode is selected before drawing its region.")。
- 持久的交互变更应写成**新 ADR**,而非无解释地改写旧历史("Record durable interaction changes as a new ADR instead of rewriting old history without explanation.")。

---

## 附录 A:关键常量速查

| 常量 | 值 | 出处 |
|---|---|---|
| 全局快捷键 | `⇧⌘A`(Shift-Command-A) | AGENTS.md / README |
| Bundle ID | `io.yuxino.kiri` | stable-privacy-identity / stable-install-permissions |
| 规范应用名 | `Kiri.app` | stable-install-permissions |
| 安装路径 | `/Applications/Kiri.app` | README / AGENTS.md |
| 仓库源 | `github.com/yuxino/kiri` | README |
| 库存储目录 | `~/Library/Application Support/kiri/` | AGENTS.md / kiri-design |
| 最低系统 | macOS 14+(麦克风 15+) | AGENTS.md / v0-2 handoff |
| 语言栈 | Swift 6 / SPM / AppKit / SwiftUI / ScreenCaptureKit / AVFoundation / CoreMedia / CoreVideo / Carbon / ImageIO | v0-2 handoff |
| 支持语言 | English + Simplified Chinese | AGENTS.md |

## 附录 B:权威来源清单(本文件引用)

- `AGENTS.md`(第一事实来源)
- `ROADMAP.md`
- `README.md` / `README_ZH.md`(`README_JA.md` 不同步,不作依据)
- `docs/adr/0001-single-capture-session.md`(已取代,仅取"单会话/inline annotation"固化语义)
- `docs/adr/0002-native-media-recording-export.md`
- `docs/adr/0003-manual-region-selection.md`
- `docs/adr/0004-kawaii-professional-visual-system.md`
- `docs/plans/2026-08-04-kiri-v0-2-codex-handoff.md`
- `docs/plans/2026-08-11-kiri-ui-readme-polish.md`
- `docs/plans/2026-07-29-kiri-design.md`
- `docs/plans/2026-07-29-kiri-hotkey-conflict-design.md`
- `docs/plans/2026-08-01-kiri-stable-privacy-identity-design.md`
- `docs/plans/2026-08-03-stable-install-permissions-design.md`
- `docs/plans/2026-08-03-dock-presence-design.md`
