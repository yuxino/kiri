# Selection Overlay 行为规格（Swift → Tauri 迁移）

> 本规格从 Kiri Swift/AppKit 源码 1:1 提取，目标是让一位没有读过 Swift 代码的工程师能在 Web Canvas（Tauri/Rust + React）上复刻出像素级一致的交互。
>
> 提取来源（仅这些文件及其直接引用的类型）：
> - `Sources/KiriApp/SelectionOverlayController.swift`
> - `Sources/KiriCore/SelectionGeometry.swift`
> - `Sources/KiriCore/CaptureShortcut.swift`
> - `Sources/KiriCore/ScreenCapturePermissionGate.swift`
> - `Sources/KiriApp/CaptureUIStyle.swift`（`CaptureUIColors`、`CaptureActionButton`、`AnnotationColorSwatchButton`、`CaptureModeSegmentedControl`、`CaptureDividerView`、`CaptureHintLabel`、`CaptureToolGroupView`、`CaptureTrackingSlider`）
> - `Sources/KiriApp/AnnotationCanvasView.swift`（`AnnotationTool`、`AnnotationColorPreset`、`MosaicIntensityPreset`、各工具绘制/编辑语义）
> - `Sources/KiriApp/AppModel.swift`、`Sources/KiriApp/CaptureCoordinator.swift`（权限门调用、完成/取消流程、全局快捷键注册）
> - `Sources/KiriApp/KiriDesignSystem.swift`（`KiriUI.Motion`）
> - `Sources/KiriApp/RecordingOptionsPopoverController.swift`、`Sources/KiriCore/RecordingPolicy.swift`、`Sources/KiriApp/RecordingPreferences.swift`
> - `docs/adr/0003-manual-region-selection.md`、`docs/adr/0001-single-capture-session.md`、`docs/plans/2026-07-29-kiri-capture-workflow-design.md`、`docs/plans/2026-08-03-capture-escape-cancel-design.md`、`docs/plans/2026-08-03-brush-mosaic-resizable-selection.md`、`docs/plans/2026-08-03-immediate-toolbar-size-sliders.md`、`docs/plans/2026-08-03-compact-single-row-toolbar-design.md`

## 0. 前置约定

### 0.1 坐标系

- 覆盖层 `CaptureSessionView` 是 `NSView` 且 `isFlipped = true`：**原点在左上角，y 轴向下**。所有鼠标坐标、选区、窗口矩形、toolbar 定位都以此坐标系为准。
- 覆盖层窗口的 `contentRect = capture.screenFrame`，但 `bounds.origin` 恒为 `(0,0)`、`bounds.size = screenFrame.size`（单位：点 point，1 pt = backingScale px）。所以 `bounds` 就是"被截取显示器"的左上原点点坐标系。
- `CapturedDisplay.windowRectsFrontToBack` 来自 `CGWindow`，也是**左上原点**坐标系，并已相对显示器做了平移（`visible.minX - displayBounds.minX` 等），因此直接落在 `bounds` 坐标系内。
- 最终导出到像素时用 `SelectionGeometry.pixelRect(forTopLeftRect:canvasSize:imageSize:)` 把左上原点矩形换算到图像像素（见 §4）。

### 0.2 全局常量总表（先给结论，后文展开）

| 常量 | 值 | 出处 |
|---|---|---|
| 强调色（紫）`CaptureUIColors.accent` | calibratedRGB (0.49, 0.41, 0.96, 1) ≈ `#7D69F5` | `CaptureUIStyle.swift` |
| 强调色深 `accentStrong` | (0.39, 0.31, 0.86, 1) ≈ `#634FDB` | 同上 |
| 强调色浅 `accentSoft` | (0.67, 0.58, 1.0, 1) ≈ `#AB94FF` | 同上 |
| 选区判定最小边长 `isValid(minimumSide:)` 默认 | **3 pt**（宽、高都 ≥3） | `SelectionGeometry.swift` |
| 点击 vs 拖动阈值 | **3 pt**（`hypot(dx,dy) >= 3`） | `SelectionOverlayController.mouseDragged` |
| 手柄命中半径（overlay 实际使用） | **10 pt** | `mouseDown` / `updateCursor` |
| `SelectionGeometry.hitTest` 默认半径 | 8 pt（overlay 一律显式传 10） | `SelectionGeometry.swift` |
| 缩放最小边长（overlay 实际使用） | **16 pt** | `mouseDragged .resizing(minimumSide: 16)` |
| `SelectionGeometry.resized` 默认最小边长 | 8 pt（overlay 显式传 16） | `SelectionGeometry.swift` |
| 窗口候选最小可见边长 | **8 pt**（宽、高都 ≥8） | `WindowSelectionGeometry.candidate` |
| 全屏变暗（无选区、无悬停） | 黑 alpha **0.25** | `draw()` |
| 悬停窗口变暗 | 黑 alpha **0.34** | `draw()` |
| 已选区域变暗 | 黑 alpha **0.48** | `draw()` |
| 窗口悬停描边 | 线宽 **2**，`accent` alpha **0.92**，单条 | `draw()` |
| 选区描边（selecting 阶段） | 白 3 pt + 紫 1.5 pt 双层 | `draw()` |
| 选区描边（annotating 阶段） | 白 4 pt + 紫 2 pt 双层 | `draw()` |
| 悬停提示动画时长 | `KiriUI.Motion.hover` = **0.14 s** | `KiriDesignSystem.swift` |
| 录制帧率 | **30 fps**；倒计时 **3 s** | `RecordingPolicy.swift` |
| 全局快捷键 | **`⇧⌘A`**（`Shift-Command-A`） | `CaptureShortcut.swift` |

### 0.3 与产品契约的差异说明

任务描述把首层按钮写作"截图/录制两个按钮"，但**当前源码实际是三个分段**：`Screenshot` / `Record` / `OCR`。本规格以源码为准，完整记录三段式选择器，并单独标注 OCR 段。迁移时若只做两段，请按产品决策裁剪，但本文件不裁剪。

---

## 1. 初始覆盖层

### 1.1 覆盖层窗口

`SelectionOverlayController.present(...)` 创建 `CaptureOverlayWindow`：

- `contentRect = capture.screenFrame`（整块被捕获显示器）。
- `styleMask = .borderless`（无边框无标题栏）。
- `level = .screenSaver`（覆盖在最上层，高于普通窗口，与 macOS 系统截屏同级）。
- `backgroundColor = .clear`、`isOpaque = false`、`hasShadow = false`。
- `acceptsMouseMovedEvents = true`。
- `collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]`（跨 Space、可覆盖全屏 App）。
- `canBecomeKey = true`、`canBecomeMain = false`。
- 打开流程：`NSApplication.shared.activate(ignoringOtherApps: true)` → `makeKeyAndOrderFront` → `makeFirstResponder(sessionView)` → `NSCursor.crosshair.set()`。
- 覆盖层 `contentView` 是 `CaptureSessionView`，它 `draw()` 先把 `capture.image`（冻结屏幕截图，Retina 像素）用 `NSImage(cgImage: image, size: bounds.size).draw(in: bounds)` 铺满，**不做任何滤镜**——用户看到的是静止的原屏幕。

### 1.2 模式选择器（首层 UI）

`viewDidMoveToWindow` 里调用 `prepareCaptureModeControl()` 创建模式选择器（只在首次创建）。它是 `NSVisualEffectView` 容器 + `CaptureModeSegmentedControl`：

- 容器材质：`.hudWindow`，`blendingMode = .withinWindow`，`state = .active`，外观 `.darkAqua`。
- 容器圆角 **13**（`cornerCurve = .continuous`），边框宽 **1**、色 `white alpha 0.14`，阴影黑 alpha **0.2**、半径 **8**、偏移 `(0, 3)`。
- 内边距：selector 距容器四边各 **6 pt**（Auto Layout）。
- 分段（`CaptureModeSegmentedControl`）：`NSStackView` 水平、`spacing = 2`。每个分段按钮：
  - 高 **32**、宽 ≥ **92**，圆角 **10**，图标 `pointSize 12 / semibold`，标题字体 12 / semibold，图标在标题左侧（`imageLeading`）。
  - 三段（顺序、图标、标题、accessibilityLabel、toolTip）：

| 序号 | 图标 SF Symbol | 标题（accessibilityLabel） | toolTip | `CaptureMode` |
|---|---|---|---|---|
| 0 | `camera.viewfinder` | `"Screenshot"` | `"Screenshot"` | `.screenshot` |
| 1 | `record.circle` | `"Record"` | `"Record Region"` | `.recording` |
| 2 | `text.viewfinder` | `"OCR"` | `"Recognize Text"` | `.ocr` |

  英文文案均为 `L10n.text("…")`，英文原文见 `en.lproj/Localizable.strings`（"Screenshot"、"Record"、"OCR"、"Record Region"、"Recognize Text"、"Capture mode"）。选择器整体 `accessibilityLabel = "Capture mode"`。
  - 选中段：白字 + 背景 `accentStrong` + 边框 `white alpha 0.22` 宽 1 + 阴影 `accentStrong` alpha 0.24 半径 7 偏移 `(0,3)`。
  - 未选中段：字色 `secondaryLabelColor`，悬停背景 `accent alpha 0.10`（`hoverFill`）。
- **定位**（`layoutCaptureModeControl`）：容器尺寸 `size = (max(220, fittingWidth), max(44, fittingHeight))`；`frame` 水平居中（`bounds.midX - width/2`），`y = bounds.maxY - height - 88`。由于坐标 flipped，**这就是"屏幕底部居中、距下边缘 88 pt"**。
- 默认选中段 = `captureMode.segmentIndex`，初始 `captureMode = .screenshot`，即默认截图模式。

> 关键：模式选择器**全程可见、从不隐藏**（源码里没有任何 `captureModeControl.isHidden = true`）。选区确定后、进入标注后它仍停留在屏幕底部居中。

### 1.3 屏幕变暗

`draw()` 的变暗逻辑（在画完冻结截图之后）：

```
activeRect = 有有效选区 ? selection.standardized : hoveredWindowSelection
fillAlpha = activeRect == nil ? 0.25 : (有有效选区 ? 0.48 : 0.34)
fillColor = black.withAlpha(fillAlpha)
if activeRect != nil: dimOutside(activeRect)   // 只变暗选区/窗口之外的四块
else: bounds.fill()                            // 全屏变暗 0.25
```

`dimOutside(rect)` 填充 rect 上/下/左/右四块（上块 `(0,0,w,rect.minY)`、下块、左块、右块），即"挖空"选区/窗口。

### 1.4 初始提示（hover hint pill）

`drawInitialHint()` 在 `phase == .selecting`、无有效选区且无悬停窗口时绘制一个胶囊提示：

- 文案（`L10n.text(...)`）按模式：
  - screenshot: `"Drag to choose a capture area   ·   Click a window   ·   Esc to cancel"`
  - recording: `"Drag to choose a recording area   ·   Click a window   ·   Esc to cancel"`
  - ocr: `"Drag to choose text to recognize   ·   Esc to cancel"`
- 字体 system **12 / medium**、白色；内边距 `padding = (15, 9)`；胶囊圆角 = `height/2`（完整胶囊）；背景黑 alpha **0.72**。
- 定位：水平居中；`desiredY = modeControl.frame.minY - textHeight - 2*padding.height - 10`（即模式选择器上方 10 pt），取 `max(12, desiredY)`（离顶至少 12 pt）。

### 1.5 放大镜（loupe）

`drawLoupe()` 在 `phase == .selecting` 且满足以下任一条件时绘制：
- （无有效选区 **且** 无悬停窗口），或
- `selectionInteraction == .creating`（正在拖拽创建区域）。

即：**悬停窗口时不显示放大镜**（遵守 ADR 0003）。参数：
- 采样：以悬停点为圆心、半边长 **5.5**（即 11×11 像素源矩形，`.integral` 后与图像求交），`imageInterpolation = .none`（最近邻放大）。
- 显示：正方形边长 **88**，圆角 **6**，边框白 **2**，居中十字线白 alpha **0.8** 线宽 **1**。
- 定位：初始 `(hover.x + 18, hover.y + 18)`；若右溢出（`origin.x + 88 > bounds.maxX - 8`）翻到左侧 `hover.x - 88 - 18`；若下溢出翻到上方；最终 clamp 到 `[8, bounds.maxX - 88 - 8]`。

### 1.6 Esc / Return 行为（选择阶段）

见 §5 完整键盘表。要点：
- **Esc**（keyCode 53）：无论任何阶段，立即 `onCancel`，关闭覆盖层，**无淡出动画**（`orderOut` + `close`，见 §5.4）。
- **Return**（keyCode 36 或 76）：仅当 `phase == .selecting` 且存在有效选区时触发：screenshot → `complete(.copy)`（复制到剪贴板）；recording → `presentRecordingOptions()`（弹出录制选项）；ocr → 完成/弹出 OCR 面板。
- **右键**（`rightMouseDown`）：`phase == .annotating` 时返回选择阶段（`returnToSelection`），否则 `onCancel`。

---

## 2. 窗口悬停

### 2.1 命中规则（多窗口重叠）

`mouseMoved`（及 `mouseEntered`）时，当 `captureMode != .ocr`、无有效选区、且当前无拖拽交互时：

```
hoveredWindowSelection = WindowSelectionGeometry.candidate(
    at: point,
    windowsFrontToBack: windowRectsFrontToBack,
    within: bounds)
```

`WindowSelectionGeometry.candidate` 算法（精确）：
1. 遍历 `windowsFrontToBack` **按数组顺序（前 → 后）**；
2. 对每个窗口 `visible = window.standardized.intersection(displayBounds)`；
3. 跳过 `visible.isNull`、`visible.width < 8`、`visible.height < 8`、或 `!visible.contains(point)` 的窗口；
4. **返回第一个命中的 `visible`**（即数组中最靠前的窗口）。

数组顺序由 `CaptureCoordinator.captureActiveDisplay` 产生：`SCShareableContent.windows` 过滤 `isOnScreen && windowLayer == 0 && owningApplication.processID != 自身PID`，取 `window.frame.standardized.intersection(displayBounds)`，且可见 ≥8×8 才纳入；坐标为 `(minX-displayBounds.minX, minY-displayBounds.minY, w, h)`。窗口矩形只用于悬停/点选命中，会话结束即丢弃。

OCR 模式下 `hoveredWindowSelection` 恒为 `nil`（不做窗口悬停）。

### 2.2 视觉样式（单一紫色描边）

`draw()` 对 `hoveredWindowSelection`（无有效选区时）：

- 变暗：除该窗口外区域用黑 alpha **0.34** 填充。
- 描边：**单条**，`NSBezierPath(rect:)`，线宽 **2**，颜色 `accent.withAlphaComponent(0.92)`。
- **无手柄、无尺寸文本、无白边叠层、无放大镜、无跟随 tooltip**（ADR 0003 强制）。

### 2.3 悬停状态生命周期

- 进入：`mouseEntered` → `mouseMoved` 逻辑，设置 `hoverPoint`、`hoveredWindowSelection`，`updateCursor`，`needsDisplay`。
- 移动：`mouseMoved` 持续更新候选。
- 离开：`mouseExited` → `hoverPoint = nil`、`hoveredWindowSelection = nil`。
- 点击：`mouseDown` 里 `hoveredWindowSelection = nil`，`pendingWindowSelection = windowSelectionCandidate(at: point)`（先记住候选）；若 `mouseUp` 时未发生 ≥3 pt 移动，则把 `pendingWindowSelection` 赋给 `selection`（即"点选窗口"）。见 §3.1。
- OCR 模式下候选恒 nil，悬停不产生任何高亮。

---

## 3. 手动区域选择

### 3.1 点击窗口 vs 拖拽区域的判定（精确）

状态：`selectionInteraction`（`.creating / .moving(original:) / .resizing(handle:original:)`）、`dragStart`、`interactionMoved`、`pendingWindowSelection`。

- `mouseDown`（仅 `phase == .selecting`）：
  - 记 `dragStart = point`（`clampedPoint` 裁剪到 bounds），`interactionMoved = false`，`hoveredWindowSelection = nil`。
  - 若已有有效选区：
    - `SelectionGeometry.hitTest(point, selection:, radius: 10)` 命中手柄 → `.resizing(handle, original: selection)`。
    - 否则 `selection.contains(point)` → `.moving(original: selection)`。
    - 否则（点在选区外）→ `.creating`，拆除标注 UI 与 OCR 面板，`pendingWindowSelection = windowSelectionCandidate(at: point)`，`selection = .null`（重新开始）。
  - 若无有效选区 → `.creating`，拆除标注 UI/OCR 面板，`pendingWindowSelection = windowSelectionCandidate(at: point)`，`selection = .null`。
- `mouseDragged`：
  - `distance = hypot(current.x - dragStart.x, current.y - dragStart.y)`。
  - `distance >= 3` → `interactionMoved = true`。
  - 未达到阈值：仅更新 `hoverPoint`，不产生选区。
  - 达到阈值后按交互类型更新 `selection`：
    - `.creating`：`hoveredWindowSelection = nil; pendingWindowSelection = nil`；`selection = clamped(normalized(from: dragStart, to: current), to: bounds)`。
    - `.moving`：`selection = moved(original, by: (dx,dy), within: bounds)`。
    - `.resizing`：`selection = resized(original, using: handle, to: current, within: bounds, minimumSide: 16)`。
  - 若 toolbar 已存在则实时 `layoutAnnotationUI()`（toolbar 跟随选区移动）。
- `mouseUp`：
  - 先再执行一次 `mouseDragged(with: event)`（把最终位置结算进去）。
  - 若 `interaction == .creating` **且** `!isValid(selection)` **且** `pendingWindowSelection != nil` → `selection = pendingWindowSelection`（**点选窗口**）。
  - 若仍无有效选区 → `selection = .null`，重新显示模式选择器。
  - 清空 `dragStart/selectionInteraction/interactionMoved/hoveredWindowSelection/pendingWindowSelection`。
  - 若有有效选区，按模式：
    - screenshot → `prepareSelectionToolbar()`（立刻出现 toolbar，见 §7）。
    - recording → `clearAnnotationUI()`；若未弹出过则 `presentRecordingOptions()`。
    - ocr → `clearAnnotationUI()`；`presentOCRPanel()`。

**判定结论**：按下后累计位移 `< 3 pt` 且落点在窗口上 → 选中该窗口（矩形取窗口可见区域与 bounds 的交集）；位移 ≥ 3 pt → 拖出矩形区域（窗口候选被丢弃）。点选得到的窗口矩形与手拖矩形后续行为完全一致（可移动/可八向缩放）。

### 3.2 选中区域的视觉样式

`draw()` 中 `phase == .selecting` 且存在有效选区时（`SelectionGeometry.isValid(selection)` 默认最小边 3 pt）：

1. **变暗**：选区外黑 alpha **0.48**（`dimOutside`）。
2. **描边**（`NSBezierPath(rect: activeRect)`，双描）：
   - 第一层：线宽 3（selecting）或 4（annotating），`white alpha 0.92`。
   - 第二层：线宽 1.5（selecting）或 2（annotating），`accent`。
3. **尺寸徽章**（`drawDimensions`，仅 selecting 阶段）：
   - 文本 `"\(Int(selection.width)) × \(Int(selection.height))"`（注意是 `×` 乘号，两侧空格），等宽字体 **11 / medium** 白色。
   - 徽章尺寸 = `textSize + (14, 8)`；圆角 = `height/2`（胶囊）；背景黑 alpha **0.76**；边框白 alpha **0.16** 线宽 **1**；文本居中。
   - 定位：初始 `(selection.minX, selection.minY - badgeHeight - 6)`（选区上方 6 pt）；`x` clamp 到 `[6, bounds.maxX - badgeWidth - 6]`；若 `origin.y < 6` 则改放 `selection.minY + 6`（选区内侧顶部）。
4. **八个手柄**（`drawSelectionHandles`，仅 selecting 阶段）：
   - 每个 `SelectionHandle` 的几何中心由 `handlePoint(for:in:)` 决定（§4.3）：四角 = 四顶点，四边中点 = 四边中点。
   - 外圆：以中心为圆心，`(center.x-5, center.y-5, 10, 10)` 的椭圆，**白色填充**。
   - 内圆：外圆 `insetBy(dx:2, dy:2)`（6×6），**accent 填充**。
   - 即外白内紫的同心圆，直径 10 pt / 6 pt。
5. **操作提示**（`drawHint`，仅当 `toolbar == nil` 时绘制）：
   - 若 `selectionInteraction == .creating`（正在拖）：文案按模式 `"Release to show tools"` / `"Release for recording settings"` / `"Release to recognize text"`。
   - 否则：`"Drag handles to resize · Drag inside to move"`（recording 为 `"Adjust the region · Recording settings below"`，ocr 为 `"Release to recognize text"`）。
   - 徽章：system 11 / medium 白色；`badgeSize = textSize + (16, 9)`；圆角 = `height/2`；背景黑 alpha **0.76**；边框白 alpha **0.16** 线宽 1；定位初始 `(selection.maxX - badgeWidth, selection.maxY + 7)`（右下角外侧），`x` clamp 到 `[6, bounds.maxX - badgeWidth - 6]`，若下溢出则改放 `selection.maxY - badgeHeight - 7`（内侧底部）。

### 3.3 手势逻辑（移动 + 八向缩放）

全部在 `mouseDown/mouseDragged/mouseUp` 中，几何纯函数见 §4。命中优先级：**手柄 > 选区内部移动 > 外部新建**。

- **移动**：`moved(original, by: translation, within: bounds)`——整体平移并 clamp 到 bounds 内（不改变尺寸）。拖拽过程中光标为 `closedHand`，静止悬停在选区内部为 `openHand`。
- **缩放**：`resized(original, using: handle, to: current, within: bounds, minimumSide: 16)`——拖到哪边就改哪边的 min/max，另一侧固定，最小边 16 pt；四角手柄同时改两边。拖拽过程中光标：上/下边 `resizeUpDown`，左/右边 `resizeLeftRight`，四角 `crosshair`。
- **移动/缩放期间** toolbar 实时 `layoutAnnotationUI()` 跟随；OCR 面板实时 `layoutOCRPanel()` 跟随。
- 每次 `mouseUp` 后（screenshot）`prepareSelectionToolbar()` 会**按新 crop 重建隐藏的标注画布**（丢弃旧画布，但保留颜色/字号/宽度等设置，见 §7.1）。

### 3.4 最小尺寸

- "有效选区"判定：宽、高都 ≥ **3 pt**（`isValid` 默认）。拖拽中若不足 3 pt，`draw()` 视为无选区（不画描边/手柄，回到全屏 0.25 变暗）。
- 缩放最小边：**16 pt**（overlay 传 `minimumSide: 16`）。
- 窗口候选最小可见：**8 × 8 pt**。
- 创建拖拽不设最小边（用 `normalized + clamped` 原样），靠 `isValid` 的 3 pt 阈值过滤。

### 3.5 吸附

**无磁吸/无吸附**。窗口点选只是"用窗口矩形作为初始选区"，之后没有把选区再吸附到窗口边缘的逻辑。几何里也没有任何 snap 代码。

### 3.6 跨显示器行为

- 覆盖层只覆盖 `capture.screenFrame`（按下快捷键时鼠标所在的那块显示器，`CaptureCoordinator` 用 `NSEvent.mouseLocation` 选屏，回退 `NSScreen.main`）。**不会跨屏创建多显示器覆盖层**。
- 所有 clamp 都以本显示器 `bounds` 为界；选区、窗口矩形、toolbar 都不会越出本显示器。
- `capture.image` 按该显示器的 `backingScale`（Retina）以 `display.width * backingScale` 采样，`bounds` 用点坐标，二者通过 `pixelRect` 换算（§4.4）。

---

## 4. SelectionGeometry（纯几何算法）

`SelectionGeometry` 与 `WindowSelectionGeometry` 均为 `CoreGraphics` 纯函数，坐标统一为左上原点（flipped）点坐标。以下为**逐函数精确算法**。

### 4.1 `normalized(from:to:) -> CGRect`

```
x = min(start.x, end.x)
y = min(start.y, end.y)
width = abs(end.x - start.x)
height = abs(end.y - start.y)
```
即把任意两点（含反向拖拽）归一成左上原点、非负宽高的矩形。

### 4.2 `clamped(_ rect:to bounds:) -> CGRect`

```
return rect.standardized.intersection(bounds)
```
先标准化（`standardized` 校正负宽高），再与 bounds 求交集。越界部分被裁掉。

### 4.3 `isValid(_ rect:, minimumSide: = 3) -> Bool`

```
!rect.isNull && rect.width >= minimumSide && rect.height >= minimumSide
```
`.null` 矩形（`CGRect.null`，无穷大/invalid）视为无效。默认阈值 3。

### 4.4 `pixelRect(forTopLeftRect:canvasSize:imageSize:) -> CGRect`

把左上原点矩形换算到图像像素：
```
guard canvasSize.width > 0 && canvasSize.height > 0 else { return .null }
scaleX = imageSize.width / canvasSize.width
scaleY = imageSize.height / canvasSize.height
return CGRect(x: rect.minX*scaleX, y: rect.minY*scaleY,
              width: rect.width*scaleX, height: rect.height*scaleY).integral
```
`.integral` 把浮点像素矩形取整（四边向外扩到整数）。overlay 用它时再与 `(0,0,image.width,image.height)` 求交（`croppedSelection()`），保证不越图像。

### 4.5 `pixelRect(forScreenRect:displayFrame:scale:) -> CGRect`

屏幕矩形（左上原点、含显示器原点偏移）→ 像素：
```
x = (rect.minX - displayFrame.minX) * scale
y = (displayFrame.maxY - rect.maxY) * scale   // 翻转为图像原点在上的像素坐标
width = rect.width * scale
height = rect.height * scale
.integral
```

### 4.6 `handlePoint(for:in:) -> CGPoint`（八手柄中心）

`rect = selection.standardized`：

| handle | 中心点 |
|---|---|
| `.topLeft` | `(minX, minY)` |
| `.top` | `(midX, minY)` |
| `.topRight` | `(maxX, minY)` |
| `.right` | `(maxX, midY)` |
| `.bottomRight` | `(maxX, maxY)` |
| `.bottom` | `(midX, maxY)` |
| `.bottomLeft` | `(minX, maxY)` |
| `.left` | `(minX, midY)` |

### 4.7 `hitTest(_ point:, selection:, radius: = 8) -> SelectionHandle?`

```
guard isValid(selection), radius >= 0 else { return nil }
return SelectionHandle.allCases.first { handle in
    hypot(point.x - center.x, point.y - center.y) <= radius
}
```
按枚举顺序（topLeft, top, topRight, right, bottomRight, bottom, bottomLeft, left）取**第一个**距离 ≤ radius 的手柄。overlay 显式传 `radius: 10`。

### 4.8 `resized(_ selection:, using handle:, to point:, within bounds:, minimumSide: = 8) -> CGRect`

精确算法：
```
rect = selection.standardized
limits = bounds.standardized
minimum = max(1, minimumSide)
clampedPoint = (min(max(point.x, limits.minX), limits.maxX),
                min(max(point.y, limits.minY), limits.maxY))
minX, maxX, minY, maxY = rect.minX, rect.maxX, rect.minY, rect.maxY

// X 轴
switch handle {
case .topLeft, .left, .bottomLeft:  minX = min(clampedPoint.x, maxX - minimum)
case .topRight, .right, .bottomRight: maxX = max(clampedPoint.x, minX + minimum)
case .top, .bottom: break   // 不动 X
}

// Y 轴
switch handle {
case .topLeft, .top, .topRight:      minY = min(clampedPoint.y, maxY - minimum)
case .bottomLeft, .bottom, .bottomRight: maxY = max(clampedPoint.y, minY + minimum)
case .left, .right: break            // 不动 Y
}

minX = max(minX, limits.minX); maxX = min(maxX, limits.maxX)
minY = max(minY, limits.minY); maxY = min(maxY, limits.maxY)
return CGRect(x: minX, y: minY, width: maxX-minX, height: maxY-minY)
```
边界条件：
- 反向拖拽（例如左上角手柄被拖到右下角之外）会被 `min(...)/max(...)` 约束，保证 `width/height >= minimum`。
- 最后再 clamp 到 bounds（若 bounds 比 minimum 还小可能得到略小结果，但 overlay 的 bounds 是整屏，通常无此问题）。
- overlay 调用时 `minimumSide = 16`。

### 4.9 `moved(_ selection:, by translation:, within bounds:) -> CGRect`

```
rect = selection.standardized
limits = bounds.standardized
guard rect.width <= limits.width && rect.height <= limits.height else {
    return clamped(rect, to: limits)   // 选区比 bounds 大时直接裁剪
}
x = min(max(rect.minX + translation.width, limits.minX), limits.maxX - rect.width)
y = min(max(rect.minY + translation.height, limits.minY), limits.maxY - rect.height)
return CGRect(origin: (x,y), size: rect.size)
```
即整体平移并完全 clamp 在 bounds 内，尺寸不变。

### 4.10 `WindowSelectionGeometry.candidate(at:windowsFrontToBack:within:minimumSide: = 8) -> CGRect?`

见 §2.1 精确算法。关键点：按数组顺序取第一个满足 `visible.contains(point)` 且可见 ≥8×8 的窗口；返回的是窗口与 bounds 的交集矩形。

---

## 5. 快捷键

### 5.1 全局快捷键 `⇧⌘A`

- 定义（`CaptureShortcut.kiriCapture`）：`key = "a"`，`modifiers = [.shift, .command]`。显示标签 `displayLabel` = 修饰键 glyph（按 `control/option/shift/command` 顺序拼） + `key.uppercased()` = `"⇧⌘A"`。
- 修饰键 glyph 映射：`control="⌃"`、`option="⌥"`、`shift="⇧"`、`command="⌘"`。
- **事件来源**：`GlobalShortcutMonitor`（`AppModel` 私有类）用 `CGEvent.tapCreate(tap: .cgSessionEventTap, place: .headInsertEventTap, options: .defaultTap, eventsOfInterest: keyDown|keyUp, callback:)` 注册**会话级前置事件 tap**，加入主 RunLoop `.commonModes`。
- **按下时机**：回调 `isKiriCaptureEvent(event)` 判定：`keyboardEventKeycode == kVK_ANSI_A`（A 键），且 `flags` 与 `[maskCommand, maskShift, maskControl, maskAlternate]` 的交集**恰好等于** `[maskCommand, maskShift]`（必须无 Ctrl/Option）。仅 `keyDown` 且 `keyboardEventAutorepeat == 0`（忽略按住重复）时，`Task { @MainActor in monitor.performAction() }` 触发 `startCapture()`。回调返回 `nil` 表示**吞掉该事件**（拦截，不传给系统）。
- **冲突处理**：
  - 前置 tap 拦截后**不再透传**，因此 `⇧⌘A` 会被 Kiri 独占（这也是产品契约 "exclusively reserve ⇧⌘A" 的意图）。
  - tap 被系统禁用时（`tapDisabledByTimeout` / `tapDisabledByUserInput`）回调里重新 `tapEnable`。
  - 注册权限：先 `CGPreflightListenEventAccess() || CGRequestListenEventAccess()`（输入监控）；tap 创建失败再 `AXIsProcessTrustedWithOptions(prompt: true)`（辅助功能）。对应错误与恢复见 §6.3。

### 5.2 覆盖层内键盘（`CaptureSessionView.keyDown`）

`CaptureOverlayWindow.sendEvent` 在 `keyDown` 且 `keyCode == 53`（Esc）时**在派发前**直接调用 `onEscape` 并返回——保证任何子控件/文本框都无法先吞掉 Esc（见 `2026-08-03-capture-escape-cancel-design.md`）。窗口还实现 `cancelOperation(_:)`（语义 Esc）同样走 `onEscape`。

`keyDown` 精确逻辑（keyCode 常量）：

| 按键 | keyCode | 阶段 | 条件 | 行为 |
|---|---|---|---|---|
| Esc | 53 | 任意 | — | `onCancel()`（结束会话） |
| Return | 36 或 76 | selecting | 有有效选区 | 按模式：screenshot → `complete(.copy)`；recording → `presentRecordingOptions()`；ocr → 有面板则 `finishOCR`，无则 `presentOCRPanel()` |
| Return | 36 或 76 | annotating | — | `complete(.copy)` |
| Delete（退格） | 51 或 117 | annotating | — | `annotationCanvas?.deleteSelection()`（删除选中的标注） |
| ⌘C | — | annotating | `modifierFlags.contains(.command)`，字符 `"c"` | `complete(.copy)` |
| ⌘S | — | annotating | 同上，字符 `"s"` | `complete(.save)` |
| ⌘Z | — | annotating | 字符 `"z"` 且 **无** shift | `undo()` |
| ⇧⌘Z | — | annotating | 字符 `"z"` 且 **有** shift | `redo()` |
| V/P/R/L/A/T/M | — | 任意（`captureMode == .screenshot` 且存在有效选区） | 修饰键与 `[.command,.control,.option]` 交集为空（即无 cmd/ctrl/opt，shift 允许） | 对应工具：V→select、P→pen、R→rectangle、L→line、A→arrow、T→text、M→mosaic（`useSelect()` 等 → `selectTool(_:)`） |

未命中以上分支则 `super.keyDown`（交还系统）。

### 5.3 标注画布内键盘（`AnnotationCanvasView.keyDown`）

- `tool == .select` 时 keyCode 51（退格）或 117（前向删除）→ `deleteSelection()`。
- 文本框内（`InlineAnnotationTextView`）：`insertNewline`（Return）→ 提交文本；`cancelOperation`（Esc）→ 取消文本编辑（覆盖层 Esc 已在窗口层拦截，文本框 Esc 走 `doCommand` 的 `cancelOperation` 分支）。

### 5.4 取消是否淡出 / Return 确认流程

- **取消无淡出动画**：`SelectionOverlayController.close()` 依次 `NSCursor.arrow.set()` → 清空 `window.onEscape` → `window.orderOut(nil)` → `window.close()` → `window = nil`。没有任何 `NSAnimationContext`。覆盖层打开时也无淡入。
- **取消后的会话清理**（`AppModel.onCancel` → `cancelCapturePresentation`）：若按下快捷键时 Kiri 在最前，恢复 Kiri 库窗口并 `activate(ignoringOtherApps:)`；否则保持库窗口隐藏并 `activate(returnApplication)` 把焦点还给原 App。
- **Return 确认（copy）流程**（`complete(.copy)`）：
  1. `isCompleting` 防重入；先 `window.orderOut(nil)`（立刻隐藏覆盖层，让"完成"即时反馈）。
  2. `Task { @MainActor; await Task.yield() }` 让出一帧后调用 `canvas.renderedImage()` 渲染标注结果；渲染失败则 `isCompleting=false` 并重新显示窗口。
  3. 成功则 `onComplete(rendered, .copy)` → `AppModel.finishCapturePresentation`（copy 动作激活 returnApplication）+ `completeCapture`：先 `writeToClipboard`（`NSPasteboard.general.clearContents()` + `writeObjects([NSImage])`），成功显示 2 秒通知 `"Copied to Clipboard"`（`checkmark.circle.fill`），失败置 `errorMessage = "Could not copy the capture to the clipboard."`；随后**无论 copy/save 都异步编码 PNG 并 `library.importData` 持久化到库**，copy 动作到此结束。
- **Return 确认（save）流程**：`complete(.save)` 同样渲染 → `onComplete(rendered, .save)` → `finishCapturePresentation` **不**激活 returnApplication → `completeCapture` **跳过剪贴板** → 入库后 `perform(.save)` → `saveToChosenLocation`：`NSSavePanel`（`allowedContentTypes = [.png]`，默认文件名 `"kiri-<时间戳>.png"`，`activate(ignoringOtherApps:)` 后模态运行），用户选路径后原子写入，成功通知 `"Saved"`。
- 其它完成动作：`.pin` → `pin(nsImage)`（置顶面板）；`.edit` → `presentEditor`（打开完整编辑器窗口）。

---

## 6. 权限门

### 6.1 Screen Recording（截图与录制共用）

`ScreenCapturePermissionGate`（`KiriCore`，纯状态机，`check(preflight:request:)`）：

```
if preflight() { cache = nil; return .authorized }
if let cached = cache { return cached }        // 已请求过且未恢复：不再重复弹系统提示
let outcome = request() ? .restartRequired : .settingsRequired
cache = outcome
return outcome
```

`CaptureCoordinator.captureActiveDisplay` 调用：
- `preflight = CGPreflightScreenCaptureAccess`、`request = CGRequestScreenCaptureAccess`。
- `.authorized` → 继续采集。
- `.restartRequired` → 抛 `permissionRestartRequired`（用户刚在系统弹窗里点了允许，需重启生效）。
- `.settingsRequired` → 抛 `permissionSettingsRequired`（被拒绝/关闭，需去系统设置）。

**缓存语义**：`permissionGate` 是 `CaptureCoordinator` 的成员，**同一进程/coordinator 生命周期内**，一旦得到"缺权限"结论就缓存，后续 `check` 直接返回缓存结论而**不再次弹出系统授权框**；一旦某次 `preflight` 通过则清缓存返回 `.authorized`（即用户去系统设置勾选后再次触发可恢复，但屏幕录制权限按 macOS 规则通常仍需重启 App）。

### 6.2 无权限时的 UI（弹窗文案 + 跳转）

无权限不会弹独立 alert，而是在**菜单栏图标的下拉菜单**里（`KiriApp.swift`）内联显示 `errorMessage` 文本 + 一个恢复按钮（`capturePermissionRecoveryLabel`）。文案（英文原文）：

| 场景 | `errorDescription`（菜单中显示） | 恢复按钮 label | 动作 |
|---|---|---|---|
| 屏幕录制：刚授予需重启 | `"Screen Recording access was granted. Quit and reopen Kiri once to finish enabling capture."` | `"Quit Kiri"` | `NSApplication.shared.terminate(nil)` |
| 屏幕录制：关闭 | `"Screen Recording is off. Enable Kiri in System Settings, then quit and reopen it once."` | `"Open Settings"` | 打开 `x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture` |
| 麦克风：关闭 | `"Microphone access is off. Enable it in System Settings to record your voice."` | `"Open Microphone Settings"` | 打开 `...?Privacy_Microphone` |
| 辅助功能：关闭 | `"Enable Kiri in Accessibility settings, then quit and reopen it to reserve ⇧⌘A exclusively."` | `"Open Accessibility Settings"` | 打开 `...?Privacy_Accessibility` |
| 输入监控：关闭 | `"Enable Kiri in Input Monitoring settings, then quit and reopen it to reserve ⇧⌘A exclusively."` | `"Open Input Monitoring Settings"` | 打开 `...?Privacy_ListenEvent` |
| 事件 tap 创建失败 | `"Kiri could not create the exclusive ⇧⌘A keyboard filter. Check Input Monitoring and Accessibility, then quit and reopen Kiri."` | （无按钮） | — |

其它无恢复动作的错误：`displayUnavailable` → `"The active display could not be captured."`。

### 6.3 权限恢复后的行为

- 无权限态**不阻塞菜单栏 UI**：错误信息常驻菜单，用户点恢复按钮后：
  - "Open Settings" 等跳系统设置 → 用户勾选 → 按 macOS 规则（屏幕录制）**需退出并重开 Kiri**；重开后 `CGPreflightScreenCaptureAccess` 为 true，gate 清缓存返回 `.authorized`。
  - 麦克风/辅助功能/输入监控在授予后通常无需重启，下次触发即生效（gate 缓存策略同理，只有"缺权限"才缓存）。
- 恢复后**没有任何自动重试提示**：下次按 `⇧⌘A` 或点"Capture"重新走完整流程。

---

## 7. 标注工具栏

### 7.1 出现时机

- screenshot 模式下，`mouseUp` 判定有效选区后立即 `prepareSelectionToolbar()`：
  1. 用 `croppedSelection()`（`pixelRect(forTopLeftRect:...)` 与图像求交后 `image.cropping`）裁剪出选区像素。
  2. 若已有旧 `AnnotationCanvasView`，**继承**其 `colorPreset / textBackgroundStyle / mosaicIntensity / penWidth / shapeWidth / textFontSize / mosaicBrushDiameter`（换区不丢设置）。
  3. 创建新 `AnnotationCanvasView(image: cropped)`，`isHidden = true`（先隐藏，选工具才显示），`addSubview(canvas, positioned: .below, relativeTo: toolbar)`，移除旧画布。
  4. 若 `toolbar == nil` 才创建 toolbar（`makeToolbar()`），加入子视图。
  5. `clearToolSelectionForAdjustingRegion()`（把所有工具按钮设为 `.select` 选中、隐藏三条上下文行和颜色组），更新颜色选中、`updateHistoryControls(canUndo:false, canRedo:false)`、`layoutAnnotationUI()`。
- 因此：**截图模式选区一确定，toolbar 立即出现，且选区仍可移动/缩放**（phase 仍是 `.selecting`，画布隐藏）。只有真正点选一个工具（或按工具快捷键）才 `activateAnnotationTool` 进入 `.annotating` 阶段、显示画布、锁定选区。
- recording / ocr 模式不建 toolbar。

### 7.2 布局（单行）

`makeToolbar()` 用 `NSVisualEffectView` 容器：
- 材质 `.hudWindow`、`.withinWindow`、`.active`、`.darkAqua`；圆角 **13** continuous；边框白 alpha **0.14** 宽 **1**；阴影黑 alpha **0.24** 半径 **12** 偏移 `(0, 5)`。
- 内容为 `NSStackView actions`：**水平、`alignment = .centerY`、`spacing = 4`、edgeInsets(top:6, left:7, bottom:6, right:7)**（单行 dock）。
- 从左到右的排列顺序（`addArrangedSubview` 顺序）：

```
[xmark 取消] | [V][P][R][L][A][T][M] | [stroke 行(隐)][text 行(隐)][mosaic 行(隐)] | [8 色板组] | [undo][redo] | [checkmark 完成] | [ellipsis.circle 更多]
```

  其中 `|` 是 `CaptureDividerView(height: 24)`（宽 1 pt，颜色 `separatorColor alpha 0.55`）。工具组内部 `spacing = 1`，颜色组内部 `spacing = 1`。

### 7.3 按钮规格（`CaptureActionButton`）

- 默认 `preferredSize = 32 × 32`（`showsTitle = false` 时），图标 SF Symbol `pointSize 13 / semibold`，`imageOnly`，圆角 **10** continuous。
- 三种 `Style`：
  - `.tool`（工具按钮 V/P/R/L/A/T/M）：未选中 tint = `labelColor`，悬停背景 `hoverFill`(accent 0.10)，**选中** tint = `accent`、背景 `selectedFill`(accent 0.18)、边框 `accent alpha 0.32` 宽 **1**；按下背景 `label alpha 0.16`。
  - `.secondary`（取消/undo/redo/更多）：tint = `secondaryLabelColor`，悬停 `hoverFill`，按下 `label alpha 0.16`；titleColor = `labelColor`。
  - `.primary`（完成）：tint 白、背景 `accentStrong`（悬停 `highlight +0.1`，按下 `shadow +0.14`），边框白 alpha **0.22** 宽 **1**，阴影 `accentStrong` alpha **0.25** 半径 7 偏移 `(0,3)`。
- 按下时整体 `CATransform3DMakeScale(0.94, 0.94, 1)`（缩小到 94%）。
- 禁用（undo/redo 初始）：tint/title 用 `tertiaryLabelColor`，背景 clear。
- 每个按钮 `toolTip = label`、`accessibilityLabel = label`。

### 7.4 工具按钮逐个规格（图标 / label / hoverHint / 快捷键）

| 工具 | SF Symbol | label（accessibilityLabel） | hoverHint | 快捷键 | 行为摘要 |
|---|---|---|---|---|---|
| Select | `cursorarrow` | `"Select (V)"` | `"Select and edit annotations (V)"` | `V` | 选中/编辑标注：单击选中、拖动移动、矩形标注八手柄缩放、线/箭头拖端点、双击文本进入编辑、删除键删除选中项 |
| Pen | `pencil.tip` | `"Pen (P)"` | `"Pen (P) — Draw freehand"` | `P` | 自由手绘；拖拽期间采样点间距 ≥0.5 pt 才追加；每笔（一次 mouseDown→mouseUp）一条可撤销记录，用 `penWidth` + `colorPreset` |
| Rectangle | `rectangle.dashed` | `"Rectangle (R)"` | `"Rectangle (R) — Draw a box"` | `R` | 拖拽画矩形，`rect(from:to:)` 取拖拽起止；用 `shapeWidth` + `colorPreset` |
| Line | `line.diagonal` | `"Line (L)"` | `"Line (L) — Connect two points"` | `L` | 拖拽画线段；仅当起止点有可见长度（`hasVisibleLength`）才提交 |
| Arrow | `arrow.up.right` | `"Arrow (A)"` | `"Arrow (A) — Point something out"` | `A` | 拖拽画箭头（起止点）；仅当有可见长度才提交 |
| Text | `textformat` | `"Text (T)"` | `"Text (T) — Click the image, type, then press Return"` | `T` | 单击落点内联文本编辑器（placeholder `"Type something…"`），输入后 Return 提交、Esc 取消；字号/背景/颜色随当前设置 |
| Mosaic | `square.grid.3x3.fill` | `"Mosaic (M)"` | `"Mosaic (M) — Hide sensitive content"` | `M` | 连续圆形笔刷：拖拽采样点，每笔一条记录，用 `mosaicBrushDiameter` + `mosaicIntensity`；绘制圆形笔刷光标，实时预览像素化 |

- Undo 按钮：`arrow.uturn.backward`，label `"Undo (⌘Z)"`，hoverHint `"Undo the last annotation · ⌘Z"`，初始禁用。
- Redo 按钮：`arrow.uturn.forward`，label `"Redo (⇧⌘Z)"`，hoverHint `"Redo the last annotation · ⇧⌘Z"`，初始禁用。
- 完成按钮（primary）：`checkmark`，label `"Done (Return)"`，hoverHint `"Done — Copy to clipboard · Return"`。
- 更多按钮：`ellipsis.circle`，label `"More Actions"`，hoverHint `"More — Save, pin, edit, or clear"`。
- 取消按钮：`xmark`，label `"Cancel (Esc)"`，hoverHint `"Cancel capture · Esc"`。

### 7.5 上下文行（随工具显示/隐藏）

三行创建后默认 `isHidden = true`，`updateContextualControls(selected:)` 按当前工具切换：

| 当前工具 | stroke 行 | text 行 | mosaic 行 | 颜色组 |
|---|---|---|---|---|
| `.select` | 隐 | 隐 | 隐 | **隐** |
| `.pen` | **显** | 隐 | 隐 | 显 |
| `.rectangle` / `.line` / `.arrow` | **显** | 隐 | 隐 | 显 |
| `.text` | 隐 | **显** | 隐 | 显 |
| `.mosaic` | 隐 | 隐 | **显** | **隐** |

- stroke 行 = `contextIcon("lineweight", "Stroke size")` + 滑块 + 数值标签（见 §8）。
- text 行 = `contextIcon("character.textbox", "Text options")` + 字号滑块 + 数值标签 + 背景三段控件。
- mosaic 行 = `contextIcon("square.grid.3x3.fill", "Mosaic brush")` + 笔刷尺寸滑块 + 数值标签 + 强度三段控件。
- `contextIcon`：SF Symbol `pointSize 11 / semibold`，tint `accent`，尺寸 **16 × 16**。
- 颜色组（`CaptureToolGroupView` 包裹）：圆角 **11**，背景 `groupFill`，边框 `surfaceBorder alpha 0.55` 宽 1，内容四边 inset **2**；内部 `NSStackView` 水平 `spacing = 1`。

### 7.6 窄区域与屏幕边缘的 toolbar 定位算法（`layoutAnnotationUI`）

```
toolbarSize = toolbar.fittingSize
width  = max(1, toolbarSize.width)
height = max(42, toolbarSize.height)
origin.x = selection.midX - width/2
origin.y = selection.maxY + 10                 // 默认在选区下方 10 pt
origin.x = min(max(origin.x, 8), max(8, bounds.maxX - width - 8))
if origin.y + height > bounds.maxY - 8:        // 下方放不下
    origin.y = selection.minY - height - 10    // 改放选区上方 10 pt
origin.y = min(max(origin.y, 8), max(8, bounds.maxY - height - 8))
```

- **不缩放 toolbar**：宽度固定为 fittingSize（所有按钮+上下文行宽之和），窄选区或屏幕边缘时**只做平移 clamp（8 pt 边距）**，不会压缩按钮、不会换行、不会缩放。
- 上下翻转条件：默认放选区下侧；若下侧会越过屏幕底边（距底 <8）则翻到选区上侧；最终 y 再 clamp 到 `[8, bounds.maxY - height - 8]`。
- 选区移动/缩放拖拽期间每次 `mouseDragged` 都重算，toolbar 实时跟随。

### 7.7 hover 提示 pill（toolbar 相关）

- 每个 `CaptureActionButton` 的 `mouseEntered/mouseExited` 触发 `onHoverHintChange`，`showHoverHint` 在 `hoverHintLabel`（`CaptureHintLabel`，初始 alpha 0）上显示 hint，动画 0.14 s（`KiriUI.Motion.hover`）淡入/淡出。
- `CaptureHintLabel`：system 11 / medium 白字，圆角 **9**，背景黑 alpha **0.76**，边框白 alpha **0.16** 宽 1；intrinsic 尺寸 = 文本 + (20, 9)。
- 定位（`layoutHoverHint`）：宽 = `min(intrinsic.width, bounds.width - 16)`；`origin.y = toolbar.frame.maxY + 8`（toolbar 下方 8 pt）；若下溢出则 `toolbar.frame.minY - height - 8`（上方）；`origin.x` 相对按钮中心，clamp 到 `[8, bounds.maxX - width - 8]`。

### 7.8 工具切换与阶段转换

- 选区确定后（selecting 阶段）toolbar 已显示但所有工具钮显示 `.select` 选中、上下文行隐藏、画布隐藏；此时**点工具或按工具快捷键** → `selectTool(tool)` → `activateAnnotationTool(tool)`：
  - `phase = .annotating`、`canvas.isHidden = false`、`canvas.tool = tool`、光标 `arrow`、`layoutAnnotationUI()`、`makeFirstResponder(self)`。
  - 之后 `selectTool` 直接改 `annotationCanvas?.tool`（不再切换 phase）。
- 返回选区（`returnToSelection`）：`phase = .selecting`、`selection = .null`、拆掉标注 UI 与 OCR 面板、重新显示模式选择器、光标 `crosshair`。入口：annotating 阶段右键，或 More 菜单 "Reselect Region"。

---

## 8. 工具栏尺寸滑块

### 8.1 出现 / 消失

- 滑块只在对应工具的上下文行里，上下文行又只在选中对应工具时显示（§7.5）。工具未选中 / `.select` / 换工具时对应行 `isHidden = true`。
- 三条行在 `clearAnnotationUI()` 时整体销毁，下次 `makeToolbar()` 重建为隐藏态。

### 8.2 滑块控件规格（`makeSizeSlider`）

- `CaptureTrackingSlider`（`NSSlider` 子类）：`isContinuous = true`（拖动过程中**连续**回调），`controlSize = .mini`，`widthAnchor = 76`。
- 数值标签：`NSTextField(labelWithString: "\(Int(value))")`，字体 `monospacedDigitSystemFont(ofSize: 9, weight: .medium)`，色 `secondaryLabelColor`，右对齐，`widthAnchor = 28`。
- 三组参数：

| 行 | 初始值 | 最小 | 最大（动态） | accessibilityLabel | 数值单位 |
|---|---|---|---|---|---|
| stroke（笔画/图形） | 3 | 1 | 16（形状） / 24（画笔） | `"Annotation line width"` | `px` |
| text（字号） | 18 | 12 | 64 | `"Text font size"` | `pt` |
| mosaic（笔刷） | 36 | 12 | 120 | `"Mosaic brush size"` | `px` |

- **stroke 行最大值的动态切换**（`updateContextualControls`）：当前工具为 `.pen` 时 slider 范围 `1...24`、显示 `canvas.penWidth`；否则（矩形/线/箭头）范围 `1...16`、显示 `canvas.shapeWidth`。

### 8.3 拖拽调节的交互（实时预览）

- stroke：`changeStrokeSize` → 取整 → `canvas.tool == .pen ? canvas.penWidth = value : canvas.shapeWidth = value`，更新数值标签（`Int(value)`，toolTip `"\(value) px"`），`makeFirstResponder(self)`。由于 `isContinuous`，**拖动即实时生效**（影响下一次绘制；已存在的标注不变，因为每个标注已存储其绘制时的尺寸）。
- text 字号：`changeTextFontSize` → 取整 → `annotationCanvas.updateTextFontSize(value)`，更新标签（单位 `pt`）。滑块的 `onTrackingBegan/Ended` 包裹 `beginTextFontSizeAdjustment()`/`endTextFontSizeAdjustment()`：**若当前有选中的文本标注，拖动字号会实时修改该标注**（live preview，见 `AnnotationCanvasView.updateTextFontSize`）。
- mosaic 笔刷：`changeMosaicBrushSize` → 取整 → `mosaicBrushDiameter = value`，更新标签（`px`）；实时改变圆形笔刷光标直径与下一次笔画的直径。
- 数值显示规则：标签始终显示 `Int(value.rounded())`；滑块 `doubleValue` 也被强制回写为取整值。

### 8.4 相邻的分段控件

- 文本背景（`NSSegmentedControl`，`.capsule`、`.mini`、字体 9/medium，段宽 `[26,26,26]`）：三段图标 `square.dashed`（透明）/`moon.fill`（深）/`sun.max.fill`（浅），toolTip 分别 `"Transparent"`/`"Dark"`/`"Light"`；选择回调 `selectTextBackgroundOption` → 设 `textBackgroundStyle`（`.transparent/.dark/.light`）并切到 text 工具。
- mosaic 强度（段宽 `[24,24,24]`）：标签 `"1"/"2"/"3"`，toolTip 对应 `"Soft"/"Standard"/"Strong"`；回调 `selectMosaicIntensity` → 设 `mosaicIntensity` 并切到 mosaic 工具。强度实际映射为像素块大小：soft=7、standard=12、strong=20（`MosaicIntensityPreset.viewBlockSize`）。

---

## 9. 确认条（取消 / 完成 / 更多）——"打勾/取消/重录"

> 说明：源码里**没有独立的"重录"按钮**。确认相关的三个固定按钮是"取消 / 完成 / 更多"；"重录（重新选区）"位于"更多"菜单里（`"Reselect Region"`），等价入口还有 annotating 阶段右键。

### 9.1 布局与文案

在单行 toolbar 内，顺序（§7.2）：取消、工具组、上下文行、颜色、undo、redo、完成、更多。三个关键按钮：

| 按钮 | SF Symbol | label | style | hoverHint |
|---|---|---|---|---|
| 取消 | `xmark` | `"Cancel (Esc)"` | `.secondary` | `"Cancel capture · Esc"` |
| 完成 | `checkmark` | `"Done (Return)"` | `.primary` | `"Done — Copy to clipboard · Return"` |
| 更多 | `ellipsis.circle` | `"More Actions"` | `.secondary` | `"More — Save, pin, edit, or clear"` |

undo/redo 位于完成按钮之前，中间以 divider 分隔。

### 9.2 行为

- 取消：`onCancel()`（Esc 同路径，见 §5.4，无淡出）。
- 完成：`finishCapture` → `complete(.copy)`（复制到剪贴板 + 入库，见 §5.4）。
- 更多（`showMoreActions`）弹出 `NSMenu`（`autoenablesItems = false`），`popUp` 定位在按钮左下方 `(sender.bounds.minX, sender.bounds.maxY + 4)`。菜单项顺序：

| 菜单项 | SF Symbol | 动作 |
|---|---|---|
| `"Reselect Region"` | `crop` | `returnToSelection`（重录/重选） |
| —— separator —— | | |
| `"Save As…"` | `square.and.arrow.down` | `complete(.save)` |
| `"Pin on Screen"` | `pin` | `complete(.pin)` |
| `"Open in Editor"` | `slider.horizontal.3` | `complete(.edit)` |
| —— separator —— | | |
| `"Clear Annotations"` | `trash` | `clearAnnotations`（仅当 `undoButton` 可用时 enabled） |

### 9.3 复制 vs 保存路径

见 §5.4。核心差异：`complete(.copy)` 激活 returnApplication 并写剪贴板；`complete(.save)` 不激活 returnApp、跳过剪贴板、弹 `NSSavePanel` 保存。**两条路径都会先把 PNG 持久化进 Kiri 库**（`library.importData`），`perform(action, on: stored)` 再执行后续动作。

---

## 10. 颜色选择器

### 10.1 预设色板（精确色值）

`AnnotationColorPreset`（8 个，`CaseIterable` 顺序即 UI 顺序）。每个是 `AnnotationColorSwatchButton`：

| 顺序 | case | name（toolTip/accessibility） | 精确色值（calibratedRGB / 近似 hex） |
|---|---|---|---|
| 1 | `.violet` | `"Violet"` | accent = (0.49, 0.41, 0.96, 1) ≈ `#7D69F5` |
| 2 | `.cherry` | `"Cherry"` | (0.98, 0.28, 0.43, 1) ≈ `#FA476E` |
| 3 | `.orange` | `"Orange"` | (1.00, 0.49, 0.18, 1) ≈ `#FF7D2E` |
| 4 | `.yellow` | `"Yellow"` | (1.00, 0.82, 0.16, 1) ≈ `#FFD129` |
| 5 | `.mint` | `"Mint"` | (0.16, 0.78, 0.56, 1) ≈ `#29C78F` |
| 6 | `.blue` | `"Blue"` | (0.16, 0.58, 1.00, 1) ≈ `#2994FF` |
| 7 | `.white` | `"White"` | 白 `#FFFFFF` |
| 8 | `.black` | `"Black"` | calibratedWhite 0.08 ≈ `#141414` |

默认色 = `.violet`。`accessibilityLabel = "Annotation color: <name>"`。

### 10.2 色块按钮（`AnnotationColorSwatchButton`）

- 尺寸 **22 × 28**，圆角 **8** continuous。
- 内容：中心圆形色点，直径 **10**（选中时 **12**），外描边黑 alpha **0.18** 线宽 **0.75**，画在 16×16 图像里。
- 背景：未选中无背景（悬停 = `hoverFill`）；选中 = 该色 `alpha 0.2`。
- 边框：未选中无边框；选中 = 该色 `alpha 1` 线宽 **1.5**。
- `setColorSelected` 控制选中态；点击 → `selectAnnotationColor` → `annotationCanvas.colorPreset = preset` + 刷新所有色块 + `makeFirstResponder(self)`。

### 10.3 自定义颜色 / 最近使用

**当前源码不提供自定义取色器，也不提供"最近使用"颜色。** 只有上述 8 个预设色块。颜色组在 `.select` 与 `.mosaic` 工具时隐藏（§7.5）。迁移时若需要自定义/最近色，属新增功能，须另立产品决策。

---

## 附录 A：模式选择器三段（含 OCR）与模式切换

- 三段选择器见 §1.2。切换回调 `changeCaptureMode(toSegment:)`：
  - 相同段直接返回。
  - 关掉录制弹窗、OCR 面板。
  - 切到 OCR：拆标注 UI、`selection = .null`、清悬停/候选、显示模式选择器、`makeFirstResponder(self)`。
  - 切到其它模式且已有有效选区：若 `annotationCanvas != nil` 则 `suspendAnnotationUI()`（隐藏画布/toolbar/hover hint），否则 `clearAnnotationUI()`；screenshot → 恢复/新建 toolbar；recording → `presentRecordingOptions()`。
  - 无有效选区 → `clearAnnotationUI()` + 显示模式选择器。
- `suspendAnnotationUI` 是"切换模式但保留画布与标注"的机制；`resumeAnnotationUI` 切回截图时恢复；若挂起期间选区被改动，`invalidateAnnotationUI`（`mouseDragged` 时 `annotationUISuspended` 为真）直接清掉画布（crop 已失效）。

## 附录 B：录制选项弹窗（选区确定后）

- recording 模式 `mouseUp` 有效选区 → `presentRecordingOptions()`：`RecordingOptionsPopoverController` 的 `NSPopover`（`behavior = .transient`、`animates = true`、`contentSize = 336 × 414`），`show(relativeTo: captureModeControl, preferredEdge: .maxY)`（锚定底部模式选择器，向上弹出）。
- 内容（SwiftUI）：标题 `"Record Region"` + 副标题 `"MP4 · 30 fps · Saved locally"`；五个开关行：`"3-second countdown"`(timer)、`"System audio"`、`"Microphone"`（macOS <15 显示 `"Requires macOS 15"` 且禁用）、`"Show pointer"`、`"Highlight clicks"`（依赖 Show pointer）；主按钮 `"Start Recording"`（`record.circle`，绑定 Return 快捷键）；底部 `"Saved locally · Never uploaded"`。
- `RecordingOptions` 默认：`usesCountdown=true, capturesSystemAudio=false, capturesMicrophone=false, showsCursor=true, highlightsClicks=false`；`normalized` 保证 `!showsCursor => highlightsClicks=false`。macOS <15 强制 `capturesMicrophone=false, highlightsClicks=false`。变更即时 `RecordingPreferences.save`（UserDefaults key `recording.options.v1`）。
- "Start Recording" → `recordRegion`：保存选项、关弹窗、`window.orderOut(nil)`、`onRecord(selection.standardized, options.normalized)` → `AppModel.beginRegionRecording`（倒计时 3 s → `RegionRecorder` 30 fps MP4，录制控件与暂停时间不入导出）。

## 附录 C：OCR 面板（第三段，简述）

- OCR 模式有效选区 → `presentOCRPanel()`：`OCRResultPanel` 定位类似 toolbar（`panelWidth` 固定，`origin.x = selection.midX - width/2` 居中、默认 `selection.maxY + 12`，下溢出翻到 `selection.minY - height - 12`，y clamp 到 `[8, bottomLimit - height]`，其中 `bottomLimit = modeControl.frame.minY - 12`）。
- 后台 `TextRecognizer.recognizeText(in: cropped)`（`ocrRecognitionToken` 防过期）；结果 trim 后空则 `.empty`，否则 `.text(text)`；Copy → `finishOCR` → `onRecognizeText(text)` → `AppModel.copyRecognizedText`（剪贴板写字符串，通知 `"Text Copied"`）。

## 附录 D：数值/文案速查

- 关键几何阈值：选区有效 ≥3、点击阈值 3、手柄命中 10、缩放最小 16、窗口候选 ≥8；变暗 0.25/0.34/0.48；描边窗口 2pt / 选区 3+1.5（selecting）或 4+2（annotating）；toolbar 圆角 13、模式选择器圆角 13、按钮圆角 10、色块圆角 8、上下文组圆角 11、hint 圆角 9、loupe 88×88 圆角 6。
- 滑块范围：Pen 1–24、Shape 1–16、Text 12–64、Mosaic 12–120；默认 3/3/18/36。
- 全部 UI 字符串英文原文见 `Sources/KiriApp/Resources/en.lproj/Localizable.strings`（本规格引用的字符串均与之一致）。
