# Kiri 区域录屏行为规格（Swift 源码 → Tauri 复刻）

> 本文档从 Kiri 现有 Swift/macOS 源码逐行提炼，目标是让一个**没有读过 Swift 代码**的工程师，
> 用 **Tauri + scap（ScreenCaptureKit 封装）+ ffmpeg** 复刻出等效的录屏体验。
> 所有 UI 字符串与代码标识符保留英文原文并用引号引用；数值精确到帧/秒/像素/点。

**源码来源（本规格的唯一事实依据）**

| 文件 | 职责 |
|---|---|
| `Sources/KiriCore/RecordingPolicy.swift` | 录屏参数模型与纯逻辑 |
| `Sources/KiriApp/RegionRecorder.swift` | ScreenCaptureKit + AVFoundation 采集与写 MP4 |
| `Sources/KiriApp/RecordingCountdownController.swift` | 3-2-1 倒计时 |
| `Sources/KiriApp/RecordingControlPanelController.swift` | 录制中控制面板 |
| `Sources/KiriApp/RecordingClickHighlighterController.swift` | 点击涟漪 |
| `Sources/KiriApp/RecordingOptionsPopoverController.swift` | 录制选项弹窗 |
| `Sources/KiriApp/RecordingPreferences.swift` | 选项持久化 |
| `Sources/KiriApp/RecordingSegmentMerger.swift` | 暂停多段合并 |
| `Sources/KiriApp/AppModel.swift` | 录制状态机、编排、入库 |
| `Sources/KiriApp/SelectionOverlayController.swift` | 模式入口、选区、触发录屏 |
| `Sources/KiriApp/CaptureCoordinator.swift` | 显示器冻结截图、屏幕录制权限 |
| `Sources/KiriCore/ScreenCapturePermissionGate.swift` | 权限请求闸门 |
| `Sources/KiriCore/AssetLibrary.swift` | 本地素材库、文件命名 |
| `docs/adr/0002-native-media-recording-export.md`、`docs/plans/2026-08-03-v0-2-region-recording-design.md`、`docs/plans/2026-08-03-v0-2-region-recording.md`、`docs/plans/2026-08-01-kiri-screen-recording-permission-design.md` | 架构与设计背景 |

**最低系统版本**：macOS 14（`LSMinimumSystemVersion = 14.0`）。Swift 6 + SPM + 纯 Apple 框架，无第三方运行依赖，无 ffmpeg 二进制。

---

## 1. 录屏模式入口与完整流程

### 1.1 全局入口

- 唯一全局快捷键 **`Shift-Command-A`（`⇧⌘A`）**，由 `GlobalShortcutMonitor` 用 **CGEventTap**（`CGEvent.tapCreate(tap: .cgSessionEventTap, place: .headInsertEventTap)`，监听 `.keyDown`/`.keyUp`）实现。
- 命中判定（`isKiriCaptureEvent`）：`keyCode == kVK_ANSI_A`（值 `0`），且修饰键交集恰好为 `.maskCommand + .maskShift`（不允许 Control/Option）。自动重复（`keyboardEventAutorepeat != 0`）不触发。
- 触发 `AppModel.startCapture()`。

### 1.2 开始捕获（与截图共用）

1. `startCapture()` 先隐藏 Kiri 素材库窗口（仅隐藏 `level == .normal && styleMask 含 .titled` 的可见窗口），记录当前前台应用与「原始前台是否为 Kiri」。
2. 若隐藏了窗口，`await 120ms` 让桌面稳定。
3. 调 `CaptureCoordinator.captureActiveDisplay()`（**这一步完成屏幕录制权限闸门 + 冻结整屏**，见第 10 节）。失败则恢复窗口并展示错误/恢复按钮，不进入覆盖层。
4. 用 `SCScreenshotManager.captureImage` 拍下**整屏**静态图作为覆盖层背景（`showsCursor = false`，即覆盖层快照不含光标）。快照分辨率 = 显示器 point 尺寸 × `backingScale`（`backingScale = max(screen.backingScaleFactor, 1)`），以像素为单位。
5. 创建 `SelectionOverlayController`，`present(...)` 显示覆盖层。覆盖层 `NSWindow`：`.borderless`、`level = .screenSaver`、透明、无阴影、`collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]`、可成为 key、不可成为 main，光标设为 `crosshair`。

### 1.3 首级模式选择器

覆盖层顶部中央有一个三段模式选择器（`CaptureModeSegmentedControl`），顺序固定：

| 段 | 图标（SF Symbol） | 标题原文 |
|---|---|---|
| 0 | `camera.viewfinder` | `Screenshot` |
| 1 | `record.circle` | `Record` |
| 2 | `text.viewfinder` | `OCR` |

- 初始选中 `Screenshot`（`captureMode = .screenshot`）。
- 选择器载体 `NSVisualEffectView`：`.hudWindow` 材质、`.darkAqua` 外观、圆角 `13`、边框白 `0.14` 宽 1、阴影黑 `0.2` 半径 8 偏移 (0,3)。
- 定位：水平居中（`bounds.midX - w/2`），距屏幕**顶部 88pt**（`bounds.maxY - height - 88`）；尺寸 `max(220, fittingSize.width) × max(44, fittingSize.height)`。
- 单个段按钮：最小宽 92、高 32、圆角 10；选中态 = `accentStrong` 填充 + 白字 + 白边框 0.22 + 阴影；未选中 = 次级标签色，悬停 `hoverFill`。
- 标题原文（英文）/ 简体中文：`Record` = `录屏`；`Screenshot` = `截图`；`OCR` = `文字`。

### 1.4 选区交互（与截图的差异）

- 选区方式与截图**完全一致**：鼠标悬停窗口 → 显示单一紫色描边（无尺寸、无把手、无 tooltip）→ 单击选中该窗口；拖动创建自定义区域；选中后支持移动与**八个把手**缩放（最小边 `16pt`，把手命中半径 `10`）。这些由 `SelectionGeometry` / `WindowSelectionGeometry` 提供。
- 拖拽创建时位移阈值：移动超过 `3pt` 才判定为拖动（`distance >= 3`）。
- **与截图唯一的交互差异**：`Record` 模式下选中区域后，**不出现标注工具栏**，而是立即弹出录制选项弹窗（见第 2 节）。
- 触发点：
  - 在 `Record` 模式下 `mouseUp` 且存在有效选区 → `clearAnnotationUI()` + `presentRecordingOptions()`。
  - 在 `Record` 模式下按 **Return**（keyCode `36` 或 `76`）且存在有效选区 → `presentRecordingOptions()`（**只打开弹窗，不直接开始录制**）。
- 选区提示文案（`Record` 模式专用，英文/中文）：
  - 初始：`"Drag to choose a recording area   ·   Click a window   ·   Esc to cancel"` / `拖动选择录屏区域   ·   单击选择窗体   ·   Esc 取消`
  - 拖拽中：`"Release for recording settings"` / `松开以设置录屏`
  - 已选中：`"Adjust the region · Recording settings below"` / `调整区域 · 下方可设置录屏`
- **Esc**（keyCode `53`）在覆盖层任意阶段取消整个会话；**右键**在选择阶段也取消。Return 在截图模式=确认复制，在录屏模式=打开选项弹窗。

### 1.5 确认录制开始的位置与 UI

- 真正的「确认开始」是弹窗底部的 **`Start Recording`** 主按钮（见第 2 节）。点击（或 Return 触发该按钮）后：
  1. `RecordingPreferences.save(options)` 持久化选项。
  2. 关闭弹窗。
  3. `window?.orderOut(nil)` **隐藏覆盖层**（此时屏幕恢复到未加暗的原画面）。
  4. `onRecord(selection.standardized, options.normalized)` 回调 → `AppModel.beginRegionRecording(...)`。
- 传入的选区是**屏幕坐标系**（top-left 原点、相对 `screenFrame`、单位 point），已 `standardized`（负宽高已归一化）。

### 1.6 开始录制（`beginRegionRecording` 顺序）

```
guard regionRecorder == nil && !captureIsUnavailable
记录 sourceApplication / returnApplication
isRecordingStarting = true
effectiveOptions = options.normalized
if macOS < 15: effectiveOptions.capturesMicrophone = false
if capturesMicrophone: try ensureMicrophonePermission()        // 仅在开启麦克风时请求
if usesCountdown: countdown.run(screenFrame, region) → Bool    // Esc 返回 false → 中止
    (中止时: isRecordingStarting = false, 清 sourceApplication, return)
recordingConfiguration = (displayID, sourceRect, backingScale, options, screenFrame)
recordingSegments = []
recordingElapsedBeforeCurrentSegment = 0
prepareRecordingClickHighlighter(...)                          // 若 highlightsClicks
prepareRecordingControlPanel(screenFrame)                      // 控制面板先于采集显示
recorder = RegionRecorder()
try recorder.start(displayID, sourceRect, backingScale, options,
                   exceptedWindowIDs: clickHighlighter?.exceptedWindowIDs ?? [])
isRecordingStarting = false
isRecording = true ; isRecordingPaused = false ; isRecordingTransitioning = false
recordingElapsed = 0 ; recordingStartedAt = now
startRecordingClock() ; updateRecordingControlPanel()
clickHighlighter?.setActive(true)
activate(returnApplication)                                    // 焦点还给原应用
showNotice("Recording Started", "record.circle.fill")          // 2 秒后自动消失
```

- 倒计时期间 `isRecordingStarting == true`，因此 `captureIsUnavailable == true`（菜单栏与 Capture 命令被禁用，无法并发开始第二段捕获/录屏）。
- 失败路径：麦克风权限错误 → `resetRecordingSession()` + 错误文案 + `capturePermissionRecoveryAction = .openMicrophoneSettings`；其它错误 → `resetRecordingSession()` + `errorMessage`。**失败/取消不产生任何库条目。**

---

## 2. 录制选项弹窗（`RecordingOptionsPopoverController`）

### 2.1 弹窗属性

- `NSPopover`：`behavior = .transient`（点击外部即关闭=放弃，不开始）、`animates = true`、`contentSize = 336 × 414`。
- 锚点：`captureModeControl`（顶部模式选择器），`preferredEdge = .maxY`（向下弹出）。
- 在 **macOS < 15** 上，初始选项被强制修正：`capturesMicrophone = false`、`highlightsClicks = false`。

### 2.2 视觉结构

- 内容宽 `336`，外边距 `20`，`VStack(alignment: .leading, spacing: 17)`。
- **头部**：`KiriSymbolMark(symbol: "record.circle.fill", size: 44)`（渐变圆角方块内白图标）+ 标题 `Record Region`（16pt bold rounded）/ 副标题 `MP4 · 30 fps · Saved locally`（11pt secondary）。
- **选项卡片**：`kiriSurface(radius: 16)`，内部 `VStack(spacing: 0)`，行间 `Divider().padding(.leading, 31)`。
- **底部主按钮**：`Start Recording`（图标 `record.circle`），`KiriPrimaryButtonStyle()`（渐变背景、圆角 11、白字、按下缩放 0.97），绑定 `keyboardShortcut(.return, modifiers: [])`。
- **底部脚注**：`Saved locally · Never uploaded`（图标 `lock.fill`，10pt medium secondary 居中）。

### 2.3 五个选项（精确文案、状态与交互）

| 行 | 图标 | 标题原文（en / zh） | 绑定字段 | 默认值 | 交互规则 |
|---|---|---|---|---|---|
| 1 | `timer` | `3-second countdown` / `3 秒倒计时` | `usesCountdown` | **true** | 普通开关 |
| 2 | `speaker.wave.2.fill` | `System audio` / `系统声音` | `capturesSystemAudio` | **false** | 普通开关 |
| 3 | `mic.fill` | `Microphone` / `麦克风` | `capturesMicrophone` | **false** | macOS ≥15 普通开关；macOS <15 **禁用**，下方 detail 显示 `Requires macOS 15` / `需要 macOS 15` |
| 4 | `cursorarrow` | `Show pointer` / `显示鼠标指针` | `showsCursor` | **true** | 关掉它时**自动把 `highlightsClicks` 置为 false** |
| 5 | `cursorarrow.click.2` | `Highlight clicks` / `显示点击轨迹` | `highlightsClicks` | **false** | 仅当 `showsCursor == true` 时可开关；否则禁用 |

- 行布局：图标 12pt medium 宽 22；标题 12pt medium；可选 detail 9pt secondary；右侧 `Toggle`（`.switch` 样式、`.mini` 尺寸、`labelsHidden`）。行高：无 detail `minHeight 35`，有 detail `minHeight 40`。禁用行整体 `opacity 0.58`。
- **每次开关变化**：先 `normalized`（若 `!showsCursor` 则强制 `highlightsClicks = false`），再 `RecordingPreferences.save(...)` 即时持久化。

### 2.4 默认值与归一化（`RecordingOptions`）

```
public init(usesCountdown: true, capturesSystemAudio: false, capturesMicrophone: false,
            showsCursor: true, highlightsClicks: false)
normalized: if !showsCursor { highlightsClicks = false }
```

### 2.5 持久化（`RecordingPreferences`）

- `UserDefaults` key = **`"recording.options.v1"`**，值 = `RecordingOptions` 的 JSON `Data`（`JSONEncoder`/`JSONDecoder`）。
- 读失败/无数据 → 返回 `RecordingOptions()`（默认值）并 `normalized`。

---

## 3. `RecordingPolicy` 全部参数（含精确数值）

### 3.1 录屏相关

| 常量 | 值 | 说明 |
|---|---|---|
| `framesPerSecond` | **30** | 帧率，用于 `minimumFrameInterval = CMTime(1/30)` 与 H.264 `AVVideoExpectedSourceFrameRateKey` |
| `countdownSeconds` | **3** | 倒计时秒数 |
| `evenDimension(_ value)` | `max(2, value)`，奇数则 `-1` | 保证 H.264 需要偶数尺寸 |
| `pixelDimension(points:backingScale:)` | `evenDimension(Int((points * max(1, backingScale)).rounded()))` | point→像素（Retina 规则，见下） |
| `highQualityBitRate(width:height:)` | `min(40_000_000, max(4_000_000, width*height*8))` bit/s | 仅 Legacy H.264 路径使用（见第 7/9 节） |
| `elapsedLabel(_ duration)` | 取 `Int(duration.rounded(.down))` 秒；有小时 → `"%d:%02d:%02d"`（H:MM:SS）；否则 `"%02d:%02d"`（MM:SS） | 控制面板/菜单栏时长显示 |

**Retina 规则**：输出像素 = `points × backingScale`（`backingScale` 取显示器的 `backingScaleFactor`，至少 1），四舍五入后再向下取偶。例如 2x Retina 屏上 100×100pt 区域 → 200×200px；1x 屏 → 100×100px。录制始终按**物理像素**（`captureResolution = .best`、`scalesToFit = false`），不降采样。

**最小区间**：`sourceRect` 宽、高都须 **≥ 2pt**，否则抛 `invalidRegion`（文案 `The recording region is too small.` / `录屏区域太小。`）。

**时长限制 / 磁盘空间保护**：**源码中不存在**——没有最大录制时长上限，也没有剩余磁盘空间预检。规格上应如实声明为「无显式限制」。（唯一相关限制是 GIF 转换的 15 秒，见 3.2，与录屏本身无关。）

### 3.2 GIF 相关（同在 `RecordingPolicy` 中，但**不属于录屏管线**）

| 常量 | 值 |
|---|---|
| `maximumGIFDuration` | **15** 秒 |
| `gifFramesPerSecond` | **12** |
| `maximumGIFLongEdge` | **720** px |
| `isGIFEligible(duration)` | `duration > 0 && duration <= 15` |
| `gifFrameCount(duration)` | `max(1, Int(ceil(duration * 12)))` |

### 3.3 文件命名 / 时间戳格式

- 临时文件（`RegionRecorder`）：`FileManager.default.temporaryDirectory` + `kiri-recording-<UUID 小写>.mp4`。
- 合并临时文件（`RecordingSegmentMerger`）：同目录 + `kiri-recording-merged-<UUID 小写>.mp4`。
- 库最终文件名（`AssetLibrary.importFile`）：`<yyyyMMdd-HHmmss>-<UUID 小写>.mp4`。
  - 时间戳 `DateFormatter`，`locale = en_US_POSIX`，`dateFormat = "yyyyMMdd-HHmmss"`。
  - `createdAt` 默认取**导入时刻**（即停止录制之后），并四舍五入到毫秒（`(t * 1000).rounded() / 1000`）。
  - 因此文件名时间戳反映**停止/入库时间**，而非开始录制时间。

---

## 4. 倒计时（3-2-1）

### 4.1 窗口与行为

- 窗口 `RecordingCountdownWindow`：`.borderless`、`level = .screenSaver`、`backgroundColor = .clear`、`isOpaque = false`、无阴影、`collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]`、`canBecomeKey = true`、`canBecomeMain = false`。
- 窗口 `contentRect` = **所选区域**，坐标从「屏幕相对、top-left」转为「AppKit 屏幕绝对、bottom-left」：
  ```
  x = screenFrame.minX + region.minX
  y = screenFrame.maxY - region.maxY
  w = region.width ; h = region.height   (标准化)
  ```
- 显示前 `NSApplication.shared.activate(ignoringOtherApps: true)`，`makeKeyAndOrderFront`，使窗口能接收 Esc。
- **倒计时期间选区绝不加暗**：覆盖层已在弹窗确认时 `orderOut` 移除；倒计时窗口本身背景透明、只覆盖选区矩形，中心画一个 badge，选区及其画面保持原样（不叠加任何暗色遮罩）。这是硬性要求。

### 4.2 数字节奏

- 循环 `for value in stride(from: 3, through: 1, by: -1)`：先 `updateLabel(value)`，再 `try? await Task.sleep(for: .seconds(1))`。
- 即显示 `3` → 等 **1 秒** → `2` → 等 1 秒 → `1` → 等 1 秒 → 立即开始录制。**每拍精确 1 秒，总时长约 3 秒。**
- 结束时 `finish(startRecording: true)`；**Esc**（keyCode `53`，或 `cancelOperation(_:)`）→ `cancel()` → `finish(startRecording: false)`，AppModel 中止录制并复位。

### 4.3 数字视觉样式

- **badge（圆形容器）**：
  - 直径 `badgeSize = min(96, max(68, min(frame.width, frame.height) - 16))`（即 68 ≤ 直径 ≤ 96，选区越小编制越大到 68 为止）。
  - `cornerRadius = badgeSize / 2`（正圆）、`cornerCurve = .continuous`。
  - 背景 `NSColor(calibratedRed: 0.10, green: 0.08, blue: 0.16, alpha: 0.92)`（近黑紫）。
  - 边框 `1.5`，`accentSoft` 带 alpha 0.92。
  - 阴影：黑、`shadowOpacity 0.32`、`shadowRadius 20`、`shadowOffset (0, -5)`。
  - badge 在窗口内**水平、垂直均居中**。
- **数字 label**：
  - 初始文本 `"3"`；`font = .monospacedDigitSystemFont(ofSize: min(46, badgeSize * 0.48), weight: .semibold)`，白色，居中，中心相对 badge 中心上移 `6pt`。
  - 每次切数字动画：`alphaValue 0 → 1`、`scale 0.76 → 1`（`CATransform3DMakeScale`），`NSAnimationContext.duration = 0.22`、`CAMediaTimingFunction(.easeOut)`。
- **提示 label**：`Esc to cancel`（`按 Esc 取消`），`font 9 medium`，白 alpha `0.68`，居中，距 badge 底 `-12`；**当 badgeSize < 80 时隐藏**。

### 4.4 结束后如何瞬间开始

`finish(startRecording: true)` 恢复 continuation → `shouldStart == true` → `beginRegionRecording` 紧接创建 `RegionRecorder` 并 `await recorder.start(...)`。倒计时窗口先 `orderOut`+`close` 移除，之后采集开始；由于采集在倒计时**之后**才建立，倒计时画面永不进入视频。

---

## 5. 录制控制面板（`RecordingControlPanelController`）

### 5.1 面板属性与位置

- `NSPanel`：`styleMask = [.borderless, .nonactivatingPanel]`，尺寸 **296 × 64**，`level = .statusBar`，`isFloatingPanel = true`，`hidesOnDeactivate = false`，透明背景，`hasShadow = true`，`collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]`，标题/无障碍标签 `Recording Controls`。
- 定位（相对**屏幕**，非选区）：
  ```
  x = screenFrame.midX - 296/2        // 水平居中于当前显示器
  y = screenFrame.maxY - 64 - 18      // 距屏幕顶部 18pt
  ```
- `orderFrontRegardless()` 显示。**没有基于鼠标悬停的显隐规则——录制/暂停期间面板始终可见**；它是 `.nonactivatingPanel`，永不抢占键盘焦点。

### 5.2 内容（SwiftUI `RecordingControlBar`）

`HStack(spacing: 10)` + 内边距（横 14 / 纵 10），背景 `.regularMaterial` 圆角 `18`，叠加 1pt `border.opacity(0.9)` 描边，阴影黑 `0.18` 半径 14 偏移 y 6，外层再 `padding(4)`。

从左到右：

1. **状态圆点**：`Circle` 10×10。暂停时填充 `coral`（= `CaptureUIColors.blossom`），录制中填充 `Color.red`；当「未暂停且不 busy」时额外画一个 `17×17`、`stroke red opacity 0.35`、线宽 4 的圆环（提示正在录制）。
2. **文本**：暂停时显示 `Paused`（coral）；否则显示 elapsed 时长（`elapsedLabel`）。字体 12pt semibold monospaced，前景 `.primary`（暂停时 coral）。`minWidth 58`，左对齐。
3. **分隔线**：`Divider().frame(height: 22)`。
4. **暂停/恢复按钮（或进度）**：
   - busy（`isRecordingStarting || isRecordingTransitioning || isRecordingFinalizing`）时：`ProgressView().controlSize(.small)`（32×30），help `Preparing recording`。
   - 否则：`Button` 28×28，图标 `pause.fill`（录制中）/ `play.fill`（暂停），背景 `accent.opacity(0.14)` 圆角 9，前景 `accent`；help/无障碍 `Pause Recording` / `Resume Recording`。
5. **停止按钮**：`stop.fill`（11pt bold 白色），28×28，背景 `Color.red` 圆角 9；busy 时禁用；help `Stop and Save Recording`，无障碍 `Stop Recording`。

### 5.3 状态机与状态显示

- `RecordingControlState`（`@Published`）：`elapsed`（初始 `"00:00"`）、`isPaused`、`isBusy`。
- `update(elapsed:isPaused:isBusy:)` 由 AppModel 每 250ms（时钟）及状态切换时刷新。
- **面板不会出现在导出视频里**的机制（见第 6/9 节过滤）：面板是 Kiri 进程窗口，`SCContentFilter` 用 `excludingApplications: [Kiri]` 整体排除 Kiri；点击涟漪面板通过 `exceptingWindows` 例外重纳入。

---

## 6. 点击涟漪（`RecordingClickHighlighterController`）

### 6.1 面板

- `NSPanel`：**58 × 58**、`[.borderless, .nonactivatingPanel]`、`level = .statusBar`、透明、无阴影、`ignoresMouseEvents = true`（不拦截点击）、`hidesOnDeactivate = false`、`collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary]`。
- 初始锚点 = **选区中心**（`selectedFrame.midX/midY`，屏幕绝对坐标）；`rippleView.primeForCapture()` 后 `orderFrontRegardless()`。
- `exceptedWindowIDs = [CGWindowID(panel.windowNumber)]` → 传给 `RegionRecorder`，使此面板**被录进视频**（这是唯一被例外的 Kiri 窗口）。

### 6.2 事件来源（重要更正）

- **不是 CGEventTap**。点击监听使用 **`NSEvent.addGlobalMonitorForEvents(matching: [.leftMouseDown, .rightMouseDown])`** 全局监视器（仅鼠标按下，不含键盘，因此不需要额外权限）。
- 每次事件：读 `NSEvent.mouseLocation`（全局 AppKit 屏幕坐标，**bottom-left 原点**），把面板 `setFrameOrigin` 到 `(point.x - 29, point.y - 29)`，`orderFrontRegardless()`，`rippleView.play()`。
- `setActive(true)` 在录制开始/恢复时启用监听；`setActive(false)` 在暂停/停止时移除监听并复位。**即涟漪只在真正录制中的段内显示，暂停期间无涟漪。**

### 6.3 涟漪视觉（三层 CAShapeLayer，面板中心 (29,29)）

| 层 | 形状 | 颜色/描边 | 缩放 | 峰值不透明度 | 时长 |
|---|---|---|---|---|---|
| `haloLayer` | 椭圆 42×42（半径 21） | 描边 `accent` α0.30，线宽 **6** | 0.45 → 1.12 | 0.72 | **0.46s** |
| `ringLayer` | 椭圆 30×30（半径 15） | 填充 `accent` α0.12 + 描边 `accent` α0.95，线宽 **2.5** | 0.58 → 1.0 | 1 | **0.34s** |
| `centerLayer` | 椭圆 7×7（半径 3.5） | 填充白 α0.95 + 描边 `accent`，线宽 **1.5** | 0.72 → 1.0 | 1 | **0.24s** |

- 关键帧曲线（三层统一，`CAAnimationGroup`，`CAMediaTimingFunction(.easeOut)`，key `"kiri-click-ripple"`）：
  - `transform.scale`：`values = [from, to, to]`，`keyTimes = [0, 0.68, 1]`
  - `opacity`：`values = [0, peak, peak*0.82, 0]`，`keyTimes = [0, 0.12, 0.68, 1]`
- 最大可见半径 ≈ halo `21 × 1.12 ≈ 23.5pt`（面板 58pt 内）。
- `accent` = `CaptureUIColors.accent` = `(R 0.49, G 0.41, B 0.96, A 1)` 紫罗兰。

### 6.4 涟漪被录制 + 多显示器坐标

- **涟漪必须被录制**：`exceptedWindowIDs` 使涟漪面板成为 `SCContentFilter(display:, excludingApplications:[Kiri], exceptingWindows:[涟漪窗口])` 中的「例外」，从而出现在视频中；Kiri 其余窗口（控制面板、倒计时、覆盖层）全部被排除。
- 多显示器：涟漪跟随全局鼠标位置（可跨显示器移动，`.stationary` 保证其在 Space 切换时驻留）；但录制只捕获**所选显示器上的所选区域**，落在录制区域之外的涟漪自然不会被录到。

---

## 7. 暂停 / 恢复与分段合并

### 7.1 采集后端选择（`RegionRecorder.start`）

```
if #available(macOS 15.0, *), options.capturesMicrophone {
    backend = ModernRegionRecordingBackend()   // SCRecordingOutput（可含麦克风）
} else {
    backend = LegacyRegionRecordingBackend()   // AVAssetWriter（屏幕 + 系统音频）
}
```

> 注意：只有 **macOS 15 且开启麦克风** 时走现代后端；macOS 15 关麦克风、以及 macOS 14 全部走 Legacy 后端。

### 7.2 两后端的共同采集参数（`SCStreamConfiguration`）

- `sourceRect = sourceRect.standardized`
- `width` / `height` = `RecordingPolicy.pixelDimension(...)`（Retina 像素，见 3.1）
- `minimumFrameInterval = CMTime(value: 1, timescale: 30)`
- `queueDepth = 6`
- `pixelFormat = kCVPixelFormatType_32BGRA`
- `captureResolution = .best`、`scalesToFit = false`
- `showsCursor = options.showsCursor`、`showMouseClicks = false`
- `capturesAudio = options.capturesSystemAudio`、`excludesCurrentProcessAudio = true`
- `sampleRate = 48_000`、`channelCount = 2`
- 现代后端额外 `captureMicrophone = options.capturesMicrophone`

### 7.3 Legacy 后端（AVAssetWriter，H.264）

- 输出 `.mp4`；视频 `AVAssetWriterInput`：
  - `AVVideoCodecKey = .h264`，`width/height`
  - `AVVideoAverageBitRateKey = highQualityBitRate(w,h)`（= clamp(w·h·8, 4M, 40M) bit/s）
  - `AVVideoExpectedSourceFrameRateKey = 30`
  - `AVVideoMaxKeyFrameIntervalKey = 60`（= 30×2）
  - `AVVideoProfileLevelKey = H264HighAutoLevel`、`AVVideoAllowFrameReorderingKey = false`
  - `expectsMediaDataInRealTime = true`
- 系统音频（若开启）`AVAssetWriterInput`：`AAC`、`48_000` Hz、`2` 声道、`192_000` bit/s、real-time。
- 流输出：`.screen`（+ `.audio` 若系统音频）挂在 `sampleQueue`；`firstTimestamp` 在**首个完整帧**时 `startWriting` + `startSession`；逐帧 `append` 并记录 `lastTimestamp`。
- `stop()`：`stopCapture` → `markAsFinished`（视频+音频）→ `finishWriting` → 时长 = `lastTimestamp - firstTimestamp`。
- 音频同步：靠各 `CMSampleBuffer` 自身 PTS 交给 `AVAssetWriter` 编排，**无人工重同步/修复逻辑**（ADR-0002 亦写明「Audio synchronization requires a later extension」）。

### 7.4 现代后端（macOS 15 + 麦克风，`SCRecordingOutput`）

- `SCRecordingOutputConfiguration`：`outputURL` 临时 mp4、`outputFileType = .mp4`、`videoCodecType = availableVideoCodecTypes.contains(.hevc) ? .hevc : .h264`。
- 屏幕+系统音频+麦克风由 `SCRecordingOutput` 原生合流（音视频同步由系统保证）。
- `stop()`：`stopCapture` → 等待 `recordingOutputDidFinishRecording` 回调 → 用 `AVURLAsset.load(.duration)` 读时长。

### 7.5 暂停 / 恢复流程（`AppModel`）

**暂停**（`pauseRecording`，须 `isRecording && !isRecordingTransitioning`）：
1. `isRecording = false`；`isRecordingTransitioning = true`；点击高亮 `setActive(false)`；停止时钟；面板进入 busy（进度圈）。
2. `await recorder.stop()` → 得到 `RecordedMedia`，`recordingSegments.append(media)`。
3. `recordingElapsedBeforeCurrentSegment = recordingSegments.reduce(0) { $0 + $1.duration }`（**累计已录时长，不含暂停墙钟**）。
4. `isRecordingPaused = true`；`isRecordingTransitioning = false`；面板刷新；`showNotice("Recording Paused", "pause.circle.fill")`。
5. 失败 → `failRecordingSession`（删除已生成片段文件 + 复位 + 错误文案）。

**恢复**（`resumeRecording`，须 `isRecordingPaused && !isRecordingTransitioning`）：
1. 用**同一份** `recordingConfiguration`（displayID、sourceRect、backingScale、options、screenFrame）新建 `RegionRecorder`，`exceptedWindowIDs` 同样取涟漪面板。
2. `isRecordingTransitioning = true` → `await recorder.start(...)` → 成功后 `isRecordingPaused = false`、`isRecording = true`、`recordingStartedAt = now`、重启时钟、点击高亮 `setActive(true)`、`showNotice("Recording Resumed", "play.circle.fill")`。
3. **恢复不重复倒计时。**
4. 恢复失败：保持 `isRecordingPaused = true`、复位 transitioning、`errorMessage`（不清空既有片段，用户可再次尝试）。

**时钟**（`startRecordingClock`）：每 **250ms** 刷新 `recordingElapsed = recordingElapsedBeforeCurrentSegment + (now - recordingStartedAt)`，并 `updateRecordingControlPanel()`。

### 7.6 分段合并（`RecordingSegmentMerger.merge(_:)`）

输入 `[RecordedMedia]`（`RecordedMedia = {fileURL, pixelWidth, pixelHeight, duration}`）：

- 空 → `noSegments`（`No recording segments are available.`）。
- 仅 1 段 → **直接返回该段，不重编码**。
- ≥2 段：`AVMutableComposition` 顺序拼接所有 video/audio 轨道，`insertionTime` 从 0 累加每段时长；导出 `AVAssetExportPresetHighestQuality` → `.mp4`，输出临时 `kiri-recording-merged-<UUID>.mp4`；返回尺寸取第一段、时长为累加和（`CMTimeGetSeconds(insertionTime)`）。导出失败删除输出并抛 `exportFailed`。

---

## 8. 结束（停止录制）与入库

### 8.1 停止流程（`stopRecording`）

```
guard (isRecording || isRecordingPaused) && !isRecordingTransitioning
activeRecorder = isRecording ? regionRecorder : nil        // 暂停中无活动 recorder
isRecording = false ; isRecordingPaused = false ; isRecordingFinalizing = true
clickHighlighter.setActive(false) ; stopRecordingClock() ; updateRecordingControlPanel()
activate(returnApplication)                               // 先归还焦点
segments = recordingSegments
if let activeRecorder { media = try await activeRecorder.stop(); segments.append(media) }
regionRecorder = nil
finalMedia = try await RecordingSegmentMerger.merge(segments)
defer { 删除所有 segment.fileURL + finalMedia.fileURL }    // 清空临时目录
library.importFile(at: finalMedia.fileURL, kind: .video, fileExtension: "mp4",
                   pixelWidth, pixelHeight, duration, sourceApplication)
refresh()                                                 // 刷新库
showNotice("Recording Saved", "video.fill")
resetRecordingSession()
```

- 停止后**仅入库，不打开编辑器、不写剪贴板**；焦点已归还给录制前的前台应用。
- 失败（`merge` / `importFile` 抛错）：删除所有 segment 文件，`errorMessage`，`resetRecordingSession()`。**失败不产生库条目。**

### 8.2 文件写入位置与入库（`AssetLibrary.importFile`）

- 库根：`~/Library/Application Support/kiri/`，结构 `Assets/`（文件）、`Thumbnails/`、`library.json`（索引）。
- `importFile` 用 `copyItem` **复制**（不是移动）到 `Assets/<yyyyMMdd-HHmmss>-<UUID>.mp4`，随后追加 `CaptureAsset(kind: .video, filename, pixelWidth, pixelHeight, duration, sourceApplication)` 并 `persist()`。
- `persist()` 失败 → 删除刚复制的文件、回滚索引、抛错（保证原子性）。
- 成功后 AppModel 的 `defer` 删除临时目录里的所有段文件与合并文件（临时文件不残留）。

### 8.3 失败清理汇总

| 场景 | 清理 |
|---|---|
| `recorder.start` 抛错 | RegionRecorder 自删临时文件 + `resetRecordingSession` |
| 暂停段 stop/写失败 | `failRecordingSession` 删除已生成 segment 文件 + 复位 |
| 停止时 merge/import 失败 | 删除所有 segment 文件 + 复位（库无新条目） |
| 音频/流中途失败 | 后端记录 `streamFailure`/回调错误，stop 时抛错走失败清理 |

---

## 9. 音频采集细节

### 9.1 系统声音（System audio，opt-in，默认关）

- 开关 = `capturesSystemAudio`。开启时 `SCStreamConfiguration.capturesAudio = true`。
- **`excludesCurrentProcessAudio = true`**：排除 Kiri 自身进程发出的声音（例如「Recording Started」通知音效、Kiri 任何音频都不进视频）。
- 采样率 **48 000 Hz**、声道 **2（立体声）**。
- Legacy 后端：额外 `addStreamOutput(.audio)`，AAC 48kHz/2ch/**192 kbps**。现代后端由 `SCRecordingOutput` 处理。
- **无音量调节、无手动混音**；与屏幕轨靠各自时间戳由系统合流。

### 9.2 麦克风（Microphone，opt-in，默认关，仅 macOS 15+）

- 开关 = `capturesMicrophone`。仅 macOS 15+ 可用（`SCStreamConfiguration.captureMicrophone = true` 走现代后端 `SCRecordingOutput`）。
- 权限在**开始录制前、倒计时前**请求（`AVCaptureDevice.authorizationStatus(for: .audio)` / `requestAccess(for: .audio)`），见第 10 节。
- 采样/声道同上（48kHz 立体声）；无音量/增益控制。
- macOS 14 降级：开关禁用 + detail `Requires macOS 15`，并在 `beginRegionRecording` 内强制 `capturesMicrophone = false`（即使旧偏好里为 true 也不会请求权限或采集）。

### 9.3 音视频同步

- 现代后端：`SCRecordingOutput` 原生把屏幕+系统音频+麦克风合流到单一 MP4，同步由系统保证。
- Legacy 后端：视频与音频 `CMSampleBuffer` 各带 PTS 直接喂 `AVAssetWriter`；**无人工重同步/漂移修复**。若 ffmpeg 复刻 Legacy 路径，等价做法是把音、视频流按各自时间戳 mux，不做额外对齐修正。

---

## 10. 权限请求时机与文案

### 10.1 屏幕录制（Screen Recording）

- **时机**：每次 `startCapture()` 都会先跑 `CaptureCoordinator.captureActiveDisplay()`，其中 `ScreenCapturePermissionGate.check(preflight: CGPreflightScreenCaptureAccess, request: CGRequestScreenCaptureAccess)`。**这是在覆盖层出现之前、也即对截图与录屏都通用的一次性闸门**；录屏本身不再二次请求（覆盖层快照已触发过）。
- 闸门逻辑（进程内缓存，**不持久化**）：
  1. `preflight()` 已授权 → 清缓存，返回 `authorized`，继续。
  2. 未授权且本进程已请求过 → 返回缓存结果（不再弹系统请求）。
  3. 未授权且首次 → `request()`；返回 true → `restartRequired`（授权需重启生效）；false → `settingsRequired`。
  4. 之后某次 `preflight()` 成功会清空缓存。
- 结果映射到 UI：
  - `restartRequired` → 文案 `Screen Recording access was granted. Quit and reopen Kiri once to finish enabling capture.` / `屏幕录制权限已开启。请退出并重新打开 Kiri，以完成启用。`，恢复按钮 `Quit Kiri`（直接 `terminate`）。
  - `settingsRequired` → 文案 `Screen Recording is off. Enable Kiri in System Settings, then quit and reopen it once.` / `屏幕录制权限未开启。请在系统设置中启用 Kiri，然后退出并重新打开。`，恢复按钮 `Open Settings`。
  - 系统设置 URL：`x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture`。
  - Kiri **从不自动打开设置或退出**，除非用户点按钮。

### 10.2 麦克风（Microphone）

- **时机**：仅在 `capturesMicrophone == true` 时，于 `beginRegionRecording` 内、**倒计时之前**请求（`ensureMicrophonePermission`）：
  - `.authorized` → 继续。
  - `.notDetermined` → `AVCaptureDevice.requestAccess(for: .audio)`；拒绝则抛错。
  - `.denied / .restricted` → 直接抛错。
- 拒绝 → `RecordingAccessError.microphonePermissionDenied`，文案 `Microphone access is off. Enable it in System Settings to record your voice.` / `麦克风权限未开启。请在系统设置中允许 Kiri 使用麦克风。`，恢复按钮 `Open Microphone Settings`（URL `x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone`）；`resetRecordingSession()`，**不开始录制**。

### 10.3 Info.plist 用途说明（系统弹窗文案）

| Key | 英文原文（en） | 简体中文（zh-Hans） |
|---|---|---|
| `NSScreenCaptureUsageDescription` | `Kiri needs screen access to capture or record the region you choose.` | `Kiri 需要访问屏幕，才能截取或录制你选择的区域。` |
| `NSMicrophoneUsageDescription` | `Kiri records microphone audio only when you enable the Microphone switch before recording.` | `仅当你在录屏前打开“麦克风”开关时，Kiri 才会录制麦克风声音。` |
| `NSInputMonitoringUsageDescription` | `Kiri uses keyboard access only to reserve ⇧⌘A as its exclusive capture shortcut.` | `Kiri 仅使用键盘访问权限，将 ⇧⌘A 设为专属截图与录屏快捷键。` |

（`NSInputMonitoring` 用于全局 ⇧⌘A 快捷键的 CGEventTap，与录屏无直接关系；点击涟漪用的是鼠标全局监视器，不需要额外权限。）

---

## 附录 A：颜色/材质表

| 名称 | 值 |
|---|---|
| `CaptureUIColors.accent`（紫，主强调） | `(R 0.49, G 0.41, B 0.96, A 1)` |
| `CaptureUIColors.accentStrong` | `(R 0.39, G 0.31, B 0.86, A 1)` |
| `CaptureUIColors.accentSoft` | `(R 0.67, G 0.58, B 1.0, A 1)` |
| `CaptureUIColors.blossom`（=`KiriUI.Palette.coral`，暂停态珊瑚色） | `(R 1.0, G 0.50, B 0.66, A 1)` |
| `CaptureUIColors.cyan` | `(R 0.31, G 0.75, B 0.94, A 1)` |
| 倒计时 badge 背景 | `(R 0.10, G 0.08, B 0.16, A 0.92)` |
| 控制面板 `border` | `surfaceBorder`：浅色 `0xE5DFF0` / 深色 `0x40394E` |
| 停止按钮 / 录制圆点 | 系统 `Color.red` |

## 附录 B：Tauri + scap + ffmpeg 复刻映射要点

1. **采集**：用 scap（ScreenCaptureKit 封装）按 `sourceRect`（point）+ backingScale 输出像素尺寸，30fps、`showsCursor` 开关、`excludesCurrentProcessAudio` 排除自身进程音频、48kHz 立体声；macOS ≥15 且开麦克风时优先用 `SCRecordingOutput`（HEVC 优先，否则 H.264），否则自建 H.264+AAC 写入器。
2. **排除自身 UI**：`SCContentFilter` 排除 Kiri 应用，但用 `exceptingWindows` 重纳入「点击涟漪窗口」——等效于 Tauri 里把涟漪渲染进一个**独立且被重纳入**的窗口，而把控制面板/倒计时窗口排除。
3. **倒计时**：单独透明窗口覆盖选区，68–96pt 圆形 badge，3→2→1 各 1 秒，Esc 取消，绝不加暗选区。
4. **控制面板**：296×64、屏幕水平居中、距顶 18pt、常显、非激活（不抢焦点）、busy 态进度圈、暂停珊瑚色 `Paused`。
5. **涟漪**：58×58 面板跟随全局鼠标，三层椭圆动画（时长 0.46/0.34/0.24s，easeOut），紫罗兰 accent，被录制。
6. **暂停/恢复**：暂停即停止当前采集并落盘为一段，恢复用相同配置重启采集、不重倒数；计时只累加「已录时长」，排除暂停墙钟。
7. **合并**：单段直接入库；多段用 ffmpeg 等价「concat + 无重编码或最高质量导出」，时长累加。
8. **入库**：复制到库目录，文件名 `yyyyMMdd-HHmmss-<uuid>.mp4`（`en_US_POSIX`），索引 JSON 原子写；临时文件清理；失败不留库条目。
9. **权限**：屏幕录制在覆盖层前统一闸门（preflight/request 缓存，授权需重启，拒绝走系统设置）；麦克风仅在开关打开时、倒计时前请求。
