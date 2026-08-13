# Kiri 标注画布行为规格(Annotation Canvas Behavior Spec)

> 本文是 Kiri 从 Swift/macOS 原生实现 1:1 复刻迁移到 Tauri(Rust + React,Web Canvas)时,标注画布(`AnnotationCanvasView`)的**像素级**权威行为规格。
> 目标读者:一个**从未读过 Swift 代码**、但熟悉 Web Canvas 2D API 的工程师,只凭本文即可实现视觉与交互一致的画布。
> 所有数字精确到像素/点(pt),代码标识符与 UI 字符串保留英文原文并用 `code` 或引号标注。
>
> 事实来源(优先级从高到低):
> 1. `Sources/KiriApp/AnnotationCanvasView.swift`(现行源码,权威)
> 2. `Sources/KiriCore/AnnotationHistory.swift`(撤销/重做,权威)
> 3. `Sources/KiriCore/SelectionGeometry.swift`(8 手柄几何,权威)
> 4. `Sources/KiriApp/SelectionOverlayController.swift`(工具栏/滑块/键盘,权威)
> 5. `Sources/KiriApp/CaptureUIStyle.swift`(accent 色值,权威)
> 6. `docs/plans/` 与 `docs/adr/0002`(设计意图与验收口径;与源码冲突时以源码为准)

---

## 1. 坐标系与画布几何

### 1.1 视图与翻转

- `AnnotationCanvasView` 是 `NSView` 子类,`isFlipped = true`:视图原点在**左上角**,x 向右、y **向下**。
- Web Canvas 2D 的默认坐标原点也在左上、y 向下,与 Swift 视图一致;但 Swift 位图上下文(导出时)原点在**左下、y 向上**,因此导出路径里有一次 y 翻转(见 §9)。

### 1.2 画布尺寸与图像放置

- 画布视图的 `frame` 被外部设置为**截图选区矩形**(以屏幕点 pt 为单位),即 `SelectionOverlayController.layoutAnnotationUI()` 中的 `annotationCanvas.frame = selection`。
- 画布持有一张 `image: CGImage`,是截图选区在**源图像像素分辨率**下的裁剪(`croppedSelection()`),即 Retina 下宽高是选区 pt 的 2 倍(scale = 2)。
- `imageRect` = 图像在画布内**等比缩放(Aspect-Fit)**后的矩形,计算式为(`AnnotationCanvasView.imageRect`):

  ```
  imageAspect = image.width / image.height
  viewAspect  = bounds.width / max(bounds.height, 1)
  若 imageAspect > viewAspect:
      height = bounds.width / imageAspect
      rect = (x:0, y:(bounds.height - height)/2, w:bounds.width, h:height)
  否则:
      width = bounds.height * imageAspect
      rect = (x:(bounds.width - width)/2, y:0, w:width, h:bounds.height)
  ```

- 因为裁剪比例与选区一致,正常情况下 `imageRect` 几乎铺满 `bounds`(仅因像素取整可能有 ±1pt 边)。
- **所有标注坐标都限制在 `imageRect` 内**(`clampedPoint`),不写入 letterbox 区域。
- `imageRect` 之外(letterbox 黑边)用 `NSColor(calibratedWhite: 0.08, alpha: 1)` 填充,即 `#141414`。

### 1.3 缩放因子(导出与马赛克共用)

```
scaleX = image.width  / imageRect.width
scaleY = image.height / imageRect.height
```

Retina 下通常 `scaleX ≈ scaleY ≈ 2`(每 pt 对应 2 物理像素)。

---

## 2. 数据模型

### 2.1 工具枚举 `AnnotationTool`

顺序即工具栏顺序(`CaseIterable`):

```
select, pen, rectangle, line, arrow, text, mosaic
```

对应快捷键:`V / P / R / L / A / T / M`(见 §8)。

### 2.2 颜色预设 `AnnotationColorPreset`

8 种,顺序即工具栏色板顺序。颜色使用 `NSColor(calibratedRed:green:blue:alpha:)`(sRGB,alpha=1),精确 8-bit 值如下:

| 预设 | 名称(UI 原文) | sRGB 小数 | HEX |
|---|---|---|---|
| `.violet`(默认) | `Violet` | (0.49, 0.41, 0.96) = `CaptureUIColors.accent` | `#7D69F5` |
| `.cherry` | `Cherry` | (0.98, 0.28, 0.43) | `#FA476E` |
| `.orange` | `Orange` | (1.00, 0.49, 0.18) | `#FF7D2E` |
| `.yellow` | `Yellow` | (1.00, 0.82, 0.16) | `#FFD129` |
| `.mint` | `Mint` | (0.16, 0.78, 0.56) | `#29C78F` |
| `.blue` | `Blue` | (0.16, 0.58, 1.00) | `#2994FF` |
| `.white` | `White` | `.white` | `#FFFFFF` |
| `.black` | `Black` | `calibratedWhite 0.08`(非纯黑) | `#141414` |

> 注意:`.black` 不是 `#000000`,而是校准白 0.08 的深灰 `#141414`;画布 letterbox 背景用的也是同一 `#141414`。

### 2.3 文字背景样式 `AnnotationTextBackgroundStyle`

| 样式 | UI 原文 | 背景色 |
|---|---|---|
| `.transparent`(默认) | `Transparent` | `nil`(无背景) |
| `.dark` | `Dark` | `black` alpha `0.72` → `rgba(0,0,0,0.72)` |
| `.light` | `Light` | `white` alpha `0.9` → `rgba(255,255,255,0.9)` |

### 2.4 马赛克强度 `MosaicIntensityPreset`

| 强度 | UI 原文 | 分段控件标签 | `viewBlockSize`(点 pt) |
|---|---|---|---|
| `.soft` | `Soft` | `1` | 7 |
| `.standard`(默认) | `Standard` | `2` | 12 |
| `.strong` | `Strong` | `3` | 20 |

强度**只**影响像素块大小(格子边长),**不**影响透明度、模糊或混合模式。值越大格子越大、越"糊"(粗像素化)。算法见 §7。

### 2.5 标注标记 `AnnotationMark`(私有枚举)

标注是**内存对象**,Swift 端没有持久化/序列化格式(注释只存在于当前会话,导出时烧录进像素)。迁移到 Tauri 需自定义序列化,建议字段等价映射如下(每种 case 即一个 discriminated union):

```ts
type AnnotationMark =
  | { kind: "pen";       points: [x,y][]; color: ColorPreset; width: number }
  | { kind: "rectangle"; rect: Rect;         color: ColorPreset; width: number }
  | { kind: "line";      start: [x,y]; end: [x,y]; color: ColorPreset; width: number }
  | { kind: "arrow";     start: [x,y]; end: [x,y]; color: ColorPreset; width: number }
  | { kind: "text";      text: string; rect: Rect; color: ColorPreset;
                          background: "transparent"|"dark"|"light"; fontSize: number }
  | { kind: "mosaic";    points: [x,y][]; brushDiameter: number; intensity: "soft"|"standard"|"strong" }
```

Swift 原始 case 签名(字段顺序即上述顺序):

```
pen([CGPoint], AnnotationColorPreset, CGFloat)                         // width
rectangle(CGRect, AnnotationColorPreset, CGFloat)                      // width
line(CGPoint, CGPoint, AnnotationColorPreset, CGFloat)                 // start, end, width
arrow(CGPoint, CGPoint, AnnotationColorPreset, CGFloat)                // start, end, width
text(String, CGRect, AnnotationColorPreset, AnnotationTextBackgroundStyle, CGFloat) // fontSize
mosaic([CGPoint], CGFloat, MosaicIntensityPreset)                      // brushDiameter
```

所有几何均为**画布点坐标**(相对 `imageRect`,x/y 向下),存储时不做像素转换。

### 2.6 撤销/重做 `AnnotationHistory<Element>`

泛型容器,`Element = AnnotationMark`。内部结构:

```
private var visibleElements: [Element]          // 当前可见的标注数组(按 z-order)
private var undoSteps: [Step]                   // 撤销栈
private var redoSteps: [Step]                   // 重做栈

struct Step {
    let before: [Element]       // 操作前的完整可见数组快照
    let after: [Element]        // 操作后的完整可见数组快照
    let undoResult: Element?    // undo 时返回给 UI 的"受影响元素"
    let redoResult: Element?    // redo 时返回给 UI 的"受影响元素"
}
```

**精确语义(必须逐条复刻):**

1. **`append(element)`**(新建标注):
   - `before = visibleElements`;`visibleElements.append(element)`。
   - 记录 `Step(before, after: 新的 visibleElements, undoResult: element, redoResult: element)`。
   - 清空 `redoSteps`。
2. **`replace(at index, with element)`**(移动/缩放/改字号/文字二次编辑):返回被替换的旧元素。
   - 仅当 `index` 在 `visibleElements.indices` 内有效,否则返回 `nil` 且不记录。
   - `before = visibleElements`;`visibleElements[index] = element`。
   - 记录 `Step(before, after, undoResult: element(新), redoResult: element(新))`。
   - 清空 `redoSteps`。
3. **`remove(at index)`**(删除标注):返回被删除元素。
   - `before = visibleElements`;`removed = visibleElements.remove(at: index)`。
   - 记录 `Step(before, after, undoResult: removed, redoResult: removed)`。
   - 清空 `redoSteps`。
4. **`undo()`**:`step = undoSteps.popLast()`;`visibleElements = step.before`;把 step push 进 `redoSteps`;返回 `step.undoResult`。栈空返回 `nil`。
5. **`redo()`**:`step = redoSteps.popLast()`;`visibleElements = step.after`;把 step push 进 `undoSteps`;返回 `step.redoResult`。栈空返回 `nil`。
6. **`clear()`**:清空三者(`removeAll(keepingCapacity: true)`)。
7. **`canUndo = !undoSteps.isEmpty`;`canRedo = !redoSteps.isEmpty`**。

**关键结论:**

- 每次 undo/redo 恢复的是**整份数组快照**(不是单元素反操作),因此顺序天然正确。
- **任何新变更(append/replace/remove)都会清空 redo 栈**(`recordStep` 内 `redoSteps.removeAll`)。
- **删除标注的路径**:`deleteSelection()` → `history.remove(at: selectedMarkIndex)`;被删元素存进 undo 栈,undo 会按快照完整还原它。
- 一次拖拽移动/缩放只产生**一条** replace 历史(在 `mouseUp` 提交,拖拽过程只改预览不写历史,见 §6)。

---

## 3. 画布状态与默认值

`AnnotationCanvasView` 的可变属性及**默认值**:

| 属性 | 默认值 | 说明 |
|---|---|---|
| `tool` | `.select` | 当前工具 |
| `colorPreset` | `.violet` | 当前画笔/文字颜色 |
| `textBackgroundStyle` | `.transparent` | 当前文字背景 |
| `mosaicIntensity` | `.standard` | 当前马赛克强度 |
| `penWidth` | `3` | 画笔线宽(pt) |
| `shapeWidth` | `3` | 矩形/直线/箭头线宽(pt) |
| `textFontSize` | `18` | 文字字号(pt) |
| `mosaicBrushDiameter` | `36` | 马赛克笔刷直径(pt) |

**工具栏滑块范围**(`SelectionOverlayController`):

| 控件 | 最小值 | 最大值 | 默认值 | 单位(UI) |
|---|---|---|---|---|
| Stroke size(画笔 `pen`) | 1 | 24 | 3 | `px` |
| Stroke size(形状 `rectangle/line/arrow`) | 1 | 16 | 3 | `px` |
| Text font size | 12 | 64 | 18 | `pt` |
| Mosaic brush size | 12 | 120 | 36 | `px` |
| Mosaic strength | — | — | `.standard` | 分段 `1/2/3` |

> 滑块 `isContinuous = true`(拖动过程中连续回调),但值在回调里被 `rounded()` 取整,所以实际粒度是 1pt。字号滑块拖动**预览实时变化**、松手合并为一条历史(见 §6.6)。

---

## 4. 渲染管线与 z-order

`draw(_ dirtyRect)` 每次**全量重绘**(无按标注缓存),顺序固定:

1. 用 `#141414` 填充整个 `bounds`。
2. 把 `image` 画到 `imageRect`(视图内使用 `NSImage(cgImage:size:).draw`,默认插值)。
3. **先**遍历绘制所有 **mosaic** 标注(按 `history.elements` 顺序,`index` 小者先画)。
4. **再**遍历绘制所有 **非 mosaic** 标注(按顺序)。
5. 画"进行中"的草稿预览(pen/mosaic/rectangle/line/arrow,见 §5)。
6. 若 `tool == .mosaic`,画笔刷游标圆(§7.4)。
7. 若 `tool == .select` 且有选中,画选中框/手柄(§6.4)。

**z-order 结论(迁移必须精确):**

- **mosaic 作为整体永远位于所有其它标注之下**(不论创建先后)。
- mosaic 之间按创建顺序(后创建在上);非 mosaic 之间按创建顺序(后创建在上)。
- **新标注追加到数组末尾,因此在各自分组内"新标注在上"。**
- 正在被文字编辑的那个标注(`index == editingTextMarkIndex`)**隐藏不画**(编辑框顶替它显示)。

### 抗锯齿与插值

- 形状(pen/rectangle/line/arrow)与文字使用 AppKit 默认**抗锯齿**渲染(Web 侧即 Canvas 2D 默认抗锯齿)。
- 直线/箭头/画笔使用 `.round` 线帽(`lineCapStyle = .round`),画笔额外用 `.round` 连接(`lineJoinStyle = .round`)。
- 矩形、直线、箭头**只描边、不填充**。
- 马赛克绘制时强制 `imageInterpolation = .none`(最近邻),以保持像素块锐利。
- 导出时底图使用 `imageInterpolation = .high`。

---

## 5. 各工具行为(绘制 + 捕获 + 导出)

### 5.1 画笔 `pen`

- **采样**:`mouseDown` 时 `draftPoints = [point]`;`mouseDragged` 时,仅当 `hypot(point - last) >= 0.5`(0.5pt)才追加新点;`mouseUp` 先调用一次 `mouseDragged` 补最后一个点。
- **平滑算法**:`NSBezierPath`,从首点 `move(to:)`,其余点逐个 `line(to:)`。**就是折线 polyline,没有任何贝塞尔/曲线插值**;视觉上的"平滑"来自 `.round` 线帽 + `.round` 连接 + 0.5pt 采样阈值。Web 侧 `ctx.lineTo` + `lineCap="round"` + `lineJoin="round"` 即可。
- **提交条件**:`draftPoints.count > 1` 才写入历史(单次点击只产生 1 个点,不提交)。
- **绘制**:`lineWidth = penWidth`,`strokeStyle = color`。
- **导出**:`lineWidth = max(1, penWidth * min(scaleX, scaleY))`。

### 5.2 矩形 `rectangle`

- 捕获:`mouseDown` 记 `dragStart = dragCurrent = point`;`mouseDragged` 更新 `dragCurrent`;`mouseUp` 提交。
- 矩形归一化(`rect(from:to:)`):`x = min(x1,x2), y = min(y1,y2), w = |x2-x1|, h = |y2-y1|`。
- **无最小尺寸阈值**:单击(零拖动)也会产生 0×0 的圆角矩形标注(与直线/箭头不同,见下)。
- **绘制(视图)**:`NSBezierPath(roundedRect: rect, xRadius: 2, yRadius: 2)`,只描边,`lineWidth = shapeWidth`。圆角半径 **2pt**。
- **导出**:圆角矩形圆角半径 **3px**(注意与视图 2pt 不一致,是现有实现的原样细节),`lineWidth = max(1, shapeWidth * min(scaleX, scaleY))`。

### 5.3 直线 `line`

- 捕获:`dragStart → dragCurrent`,提交条件 `hasVisibleLength(from:to:)` = `hypot(end - start) >= 3`(**3pt**),短于 3pt 的拖动被丢弃(防止误触小点)。
- **绘制**:`lineWidth = shapeWidth`,`lineCap = .round`。
- **无角度约束**:当前源码**没有任何 Shift 吸附/角度约束逻辑**——直线完全自由角度。迁移不要实现 Shift 吸附(除非产品另有决定)。
- **导出**:`lineWidth = max(1, shapeWidth * min(scaleX, scaleY))`。

### 5.4 箭头 `arrow`

- 提交条件与直线相同:`hypot(end - start) >= 3`。
- **绘制** = 先画直线(同 `line`),再画箭头头。
- **箭头头几何(精确公式)**:

  ```
  angle       = atan2(end.y - start.y, end.x - start.x)   // 指向 end 的方向
  headLength  = max(12, width * 4)                        // 箭头头长:min 12pt,随线宽 4 倍
  left  = (end.x - headLength * cos(angle - π/6), end.y - headLength * sin(angle - π/6))
  right = (end.x - headLength * cos(angle + π/6), end.y - headLength * sin(angle + π/6))
  head 路径 = move(to: left) → line(to: end) → line(to: right)
  head.lineWidth = width; head.lineCap = .round; head.lineJoin = .round; head.stroke()
  ```

  - 箭头头是**开放折线**(两条线段 left→end→right),**不填充**;由于 round cap/join,呈现为空心"V/人字形"箭头。
  - 箭头头两侧与主轴夹角均为 **π/6 = 30°**(总张角 60°)。
  - `headLength = max(12, width*4)`:`width=3` 时 = 12pt;`width>=4` 时 = `4×width`。
- **导出**:同直线,`lineWidth = max(1, shapeWidth * min(scale))`,箭头头几何同式(在导出坐标下计算)。

### 5.5 文字 `text`

- **创建**:`tool == .text` 时,`mouseDown` 清空选中并在点击处打开内联编辑器(不先产生标注);提交(Return / 失焦 / 切工具)后才写入历史。空文本(去空白后)不写入(§6.6)。
- **字体**:`NSFont.systemFont(ofSize: fontSize, weight: .semibold)`;默认 `fontSize = 18`。
- **文字颜色** = 所选 `colorPreset`;背景样式见 §2.3,默认透明。
- **换行/测量**:绘制用 `NSString.draw(with: rect, options: [.usesLineFragmentOrigin, .usesFontLeading])`,即在给定 `rect` 宽度内**自动换行**;不设行数上限。文字矩形宽度决定换行位置,导出与编辑用同一矩形,保证"编辑/预览/导出"换行一致。
- **绘制(视图)**:先画背景(若有),再画文字。
  - 背景:若有 `backgroundColor`,`backgroundRect = rect.insetBy(dx: -5, dy: -3)`(即外扩 5pt 水平、3pt 垂直),圆角矩形半径 `5`,填充。透明样式则**完全不画背景**。
  - 文字:以 `fontSize` 字号、颜色绘制,`paddingScale = 1`。
- **导出**:`fontSize = max(1, fontSize * min(scaleX, scaleY))`;背景 padding 与圆角半径都乘 `paddingScale = min(scaleX, scaleY)`(即水平 padding = 5·scale,垂直 = 3·scale,圆角 = 5·scale)。
- **实时字号调节**:见 §6.6。
- **内联编辑器**:见 §6.6 / §7。

### 5.6 马赛克 `mosaic`

- **连续画笔**:采样规则与 pen 完全相同(`mouseDown` 记 1 点,`mouseDragged` 间隔 ≥0.5pt 追加)。单击(1 点)也会提交一个**圆形**马赛克点。
- **两个参数**:
  - `mosaicBrushDiameter`(直径,pt):控制笔画覆盖宽度(笔刷圆/描边宽度),默认 36,范围 12–120。
  - `mosaicIntensity`(强度):控制像素块大小(格子边长),见 §7。
- 算法精确步骤见 §7。每个马赛克标注**存储自己**的 `brushDiameter` 与 `intensity`,后续修改参数不会改变已存在标注。
- **导出**:裁剪到笔画外接框,按 scale 换算,diameter 乘 `min(scaleX, scaleY)` 做 clip,块大小按 §7 用源像素计算,保证预览与导出视觉强度一致。

---

## 6. 选择与编辑(V 工具)

### 6.1 命中检测 `annotationMarkIndex(at:)`

从 `history.elements` **逆序**(最上层优先)找第一个 `hitTest(point) == true` 的标注。各类型命中规则(`hitTest`):

| 类型 | 命中规则(精确) |
|---|---|
| `pen` | 到折线任一线段的距离 `<= max(7, width/2 + 4)` |
| `rectangle` | 点在 `rect.standardized` 外扩 `max(6, width)` 的矩形内(**含内部整体**,不只是描边) |
| `line`/`arrow` | 点到线段距离 `<= max(7, width/2 + 4)` |
| `text` | 点在 `rect` 外扩 `(dx: -7, dy: -6)` 内(水平外扩 7,垂直外扩 6,含内部) |
| `mosaic` | 到折线任一线段距离 `<= diameter/2 + 4` |

- `polyline` 判定:单点 → 到该点距离 ≤ tolerance;多点 → 到任一线段距离 ≤ tolerance。
- 点到线段距离 `distance(from:toSegmentFrom:to:)`:标准投影钳制到 `[0,1]` 后求欧氏距离;线段退化(长度 0)时退化为到端点的距离。

### 6.2 手柄交互判定 `selectionInteraction`

- `rectangle`:用 `SelectionGeometry.hitTest(point, selection: rect, radius: 9)` 命中 8 个手柄之一(手柄中心 9pt 半径内),返回 `.resizingRectangle(handle)`。
- `line`/`arrow`:到 `start` 距离 `<= 10` → `.movingEndpoint(isStart: true)`;到 `end` 距离 `<= 10` → `.movingEndpoint(isStart: false)`。
- `pen`/`text`/`mosaic`:**无手柄**,只能整体移动。

**8 个手柄**(`SelectionHandle.allCases` 顺序):`topLeft, top, topRight, right, bottomRight, bottom, bottomLeft, left`。手柄中心点(`handlePoint`):

```
topLeft=(minX,minY)  top=(midX,minY)  topRight=(maxX,minY)
left=(minX,midY)                       right=(maxX,midY)
bottomLeft=(minX,maxY) bottom=(midX,maxY) bottomRight=(maxX,maxY)
```

### 6.3 选中/移动/缩放交互流程(`mouseDown/mouseDragged/mouseUp`)

`mouseDown`(tool == `.select`):

1. `handleInteraction = selectionInteraction(at: point)`(对当前已选标注的手柄命中)。
2. `index = handleInteraction == nil ? annotationMarkIndex(at: point) : selectedMarkIndex`。
3. 无有效 index → **清空选择**(`selectedMarkIndex = nil` 等),返回(点击空白清除选中)。
4. `selectedMarkIndex = index`。
5. 若 `event.clickCount >= 2` 且该标注是文字 → `beginTextEditing(markIndex:)`,返回(双击二次编辑)。
6. 否则进入拖拽:`dragStart = dragCurrent = point`;`selectionDragOriginalMark = 当前标注`;`selectionDragPreviewMark = nil`;`selectionInteraction = handleInteraction ?? selectionInteraction(at:point, for:mark) ?? .moving`;光标 `closedHand`。

`mouseDragged`(tool == `.select`):

- 更新 `dragCurrent = point`。
- `selectionDragPreviewMark`(预览,不写历史):
  - `.moving` → `original.translated(by: point - dragStart, within: imageRect)`。
  - `.resizingRectangle(handle)` → `original.resizedRectangle(using: handle, to: point, within: imageRect)`。
  - `.movingEndpoint(isStart)` → `original.movingEndpoint(isStart:, to: point)`。

`mouseUp`(tool == `.select`):

- 先 `mouseDragged` 补终态;若 `hypot(end - start) >= 1`(1pt)且有 preview,则 `history.replace(at: selectedMarkIndex, with: preview)`(**一条历史**),`publishHistoryState()`。
- 清空拖拽临时状态,光标回 `arrow`。

**钳制规则(clamp):**

- `translated(by:within:)`:位移量在 x/y 上分别 `clamp` 到 `[bounds.minX - markBounds.minX, bounds.maxX - markBounds.maxX]`,保证整体不越出 `imageRect`。
- `resizedRectangle`:调用 `SelectionGeometry.resized(rect, using: handle, to: point, within: bounds, minimumSide: 8)`——先把 point clamp 到 bounds,再按手柄方向调整 minX/maxX/minY/maxY,**最小边 8pt**,最后再 clamp 回 bounds(详见 `SelectionGeometry.swift` 第 98–147 行)。
- `movingEndpoint`:端点取事件里已 `clampedPoint` 的坐标,即端点也被钳制在 `imageRect` 内。
- 事件坐标在进入处理前都经 `clampedPoint` 钳制到 `imageRect`。

### 6.4 选中视觉样式

`drawSelectionOutline(for:)`:

- **line / arrow**:只画**两个端点手柄**(`drawSelectionHandle` at start 与 end),**不画虚线外框**。
- **其余类型**:画**虚线圆角外框** + (仅 rectangle)8 个手柄:
  - 外框 = `selectionBounds` 外扩 5pt(`insetBy(dx: -5, dy: -5)`),圆角半径 `6`。
  - 描边两遍:第一遍 `lineWidth 1.5`、虚线 `[4, 3]`、`white alpha 0.96`;第二遍 `lineWidth 1`、`CaptureUIColors.accent`(即 `#7D69F5`),虚线保持 `[4, 3]`(setLineDash 未重置)。
  - 仅 rectangle 追加 8 个手柄。

`drawSelectionHandle(at:)`(手柄几何):

- 外圆:`(x-5, y-5, 10, 10)`,填充白色。
- 内圆:外圆 `insetBy(dx: 2, dy: 2)` 即 `(x-3, y-3, 6, 6)`,填充 `accent` `#7D69F5`。
- 即:白色 10pt 圆 + 居中紫色 6pt 圆。

`selectionBounds`(外框依据):

```
pen       → pointBounds(points) 外扩 max(1, width/2)
rectangle → rect.standardized
line/arrow→ pointBounds([start,end]) 外扩 max(1, width/2)
text      → rect.standardized
mosaic    → pointBounds(points) 外扩 diameter/2
```

### 6.5 删除 Delete / Forward Delete

- `AnnotationCanvasView.keyDown`:`tool == .select` 且 keyCode `51`(Delete/Backspace)或 `117`(Forward Delete)→ `deleteSelection()`。
- 上层 `SelectionOverlayController.keyDown` 在 `.annotating` 阶段对 `51`/`117` 也直接调 `annotationCanvas.deleteSelection()`。
- `deleteSelection()`:仅在 `tool == .select` 且 `selectedMarkIndex` 有效时 `history.remove(at:)`(可撤销),清空选中与拖拽状态,`publishHistoryState`。
- **无选中时 Delete 什么都不做**(不产生历史)。

### 6.6 内联文字编辑器(核心)

`InlineAnnotationTextView`(NSTextView 子类):

- `placeholder = "Type something…"`;仅当 `string.isEmpty` 且 `markedRange().location == NSNotFound`(即无 IME 组合态)时在 `textContainerInset` 处用 `systemFont(18, .semibold)` + `placeholderTextColor` 绘制占位符。
- 配置:`isRichText=false`、`importsGraphics=false`、自动引号/破折号/替换全关、水平/垂直不可 resize、`textContainerInset = (8, 5)`、`textContainer.widthTracksTextView=true`、`heightTracksTextView=false`、`drawsBackground=true`、`layer.cornerRadius=7`(continuous)、`borderWidth=1`。
- 视觉联动(跟随当前色/背景/字号):`textColor=color`、`insertionPointColor=color`、`backgroundColor=背景色(或 clear)`、`layer.borderColor=color.withAlphaComponent(0.8)`。

**新建位置(无既有矩形)**:

```
width  = min(180, max(96, imageRect.width))
origin = (x: min(point.x, imageRect.maxX - width), y: min(point.y, imageRect.maxY - 34))
frame  = (origin, size: (width, 34))
```

即:初始宽度 96–180pt(受 imageRect 宽限制,上限 180),高度固定 34pt,起点不越出 imageRect 右下。

**二次编辑位置(有既有矩形 `rect`)**:

```
frame = (rect.minX - 8, rect.minY - 5, rect.width + 16, rect.height + 10)   // 外扩 textContainerInset (8,5)
```

**实时尺寸自适应 `resizeTextEditor()`**(在 `textDidChange` 与字号变化时调用):

```
horizontalPadding = 8 * 2 = 16; verticalPadding = 5 * 2 = 10
maximumWidth  = max(96, imageRect.maxX - frame.minX)
measuredText  = string.isEmpty ? placeholder : string
textBounds    = measuredText.boundingRect(width: max(1, maximumWidth - 16), height: ∞,
                    options: [.usesLineFragmentOrigin, .usesFontLeading], attrs: [.font: font])
width  = min(maximumWidth, max(120, ceil(textBounds.width) + 16 + 2))
maximumHeight = max(34, imageRect.maxY - frame.minY)
height = min(maximumHeight, max(34, ceil(textBounds.height) + 10 + 2))
setFrameSize(width, height)
```

即:最小 120×34pt,随文本/字号增长,受 imageRect 右/下边界钳制;宽高**自动**按测量结果,无手动拖拽调整手柄。

**提交 `commitTextEditing()`**:

1. `text = editor.string.trimmingCharacters(in: .whitespacesAndNewlines)`(**去除首尾空白与换行**)。
2. `textRect = (frame.minX + 8, frame.minY + 5, max(1, frame.width - 16), max(1, frame.height - 10))`。
3. 关闭编辑器(discard)。
4. 若 `text` 为空:若在编辑既有标注 → `history.remove(at: markIndex)`(可撤销,且 `selectedMarkIndex=nil`);否则不产生标注。
5. 非空:构造 `.text(text, textRect, color, background, fontSize)`。
   - 编辑既有:若新 mark 与原 mark `==`(**未变化**)→ 只保留选中、**不写历史**(避免 no-op 历史项);否则 `history.replace`(一条历史)。
   - 新建:`history.append`。

**提交触发点**:Return(编辑器 `doCommand` 拦截 `insertNewline` → commit + 完成截图)、失焦 `textDidEndEditing`、切换工具(`tool` 的 `didSet` 里 oldValue == `.text` 且新值非 text → commit)、undo/redo 前(`commitTextEditing()` 先行)、导出前(`renderedImage()` 先 commit)。

**Esc / 取消**:编辑器 `doCommand` 拦截 `cancelOperation` → `onCancel?()` = `discardTextEditing()` + `onCancelRequested?()` → **取消整个截图会话**(丢弃未提交文本并退出捕获)。即:文字编辑中的 Esc 不是"只退出文本框",而是**取消整个捕获**。

**IME 输入**:NSTextView 使用 AppKit 原生文本输入(含 marked text 组合,中文/日文输入法);组合过程中 `textDidChange` 会触发 → `resizeTextEditor()` 实时自适应;占位符在存在 marked text 时隐藏。Return 提交语义由输入法先行确认组合,再由 `insertNewline` 触发 commit。

**实时字号调节(滑块)**:

- 滑块 `onTrackingBegan` → `beginTextFontSizeAdjustment()`:先 commit 文本框,记录 `textSizeAdjustmentMarkIndex = selectedMarkIndex`、`originalMark`(要求选中项是文字)。
- 拖动中 `changeTextFontSize(value)` → `updateTextFontSize(value)`:
  - `textFontSize = value`。
  - 若选中是文字:用 `textMark(source, changingFontSizeTo:)` 计算新 mark;拖动期间更新 `textSizeAdjustmentPreviewMark`(预览,不写历史);若未在拖动态且确实变化则直接 `replace`(键盘调节场景,立即写历史)。
- `textMark(source, changingFontSizeTo:)` 重算文字矩形(保持 `top-left` 不动):
  ```
  maximumWidth  = max(1, imageRect.maxX - rect.minX)
  measured      = text.boundingRect(width: maximumWidth, height: ∞,
                     options: [.usesLineFragmentOrigin, .usesFontLeading],
                     attrs: textAttributes(fontSize, color))
  maximumHeight = max(1, imageRect.maxY - rect.minY)
  newRect = (rect.minX, rect.minY,
             min(maximumWidth, max(1, ceil(measured.width) + 2)),
             min(maximumHeight, max(1, ceil(measured.height) + 2)))
  ```
- 滑块 `onTrackingEnded` → `endTextFontSizeAdjustment()`:若 preview != original → `history.replace`(**整个拖拽合并为一条历史**),`selectedMarkIndex = markIndex`。
- 因此:字号变化**保留 text、颜色、背景样式、左上角位置**;undo/redo 针对"完成的那一次调整"而非每个中间值。

### 6.7 光标(cursorUpdate)

- `mosaic` → `crosshair`。
- `select` → 命中手柄 → `crosshair`;命中标注 → `openHand`;否则 `arrow`。
- 其它工具 → `arrow`。

---

## 7. 马赛克算法(像素级)

### 7.1 笔画外接框 `mosaicStrokeBounds(points, diameter)`

```
radius = diameter / 2
bounds = (x: minX - radius, y: minY - radius,
          w: (maxX - minX) + diameter, h: (maxY - minY) + diameter)
```

再与 `imageRect` 求交;交集 `width/height < 1` 则不绘制。

### 7.2 像素化裁剪 `pixelatedCrop(for: viewRect, intensity:)`

1. `clipped = viewRect.standardized ∩ imageRect`(要求 ≥1×1)。
2. 换算到源像素坐标并取整、钳制到图像:
   ```
   cropRect = (x:(clipped.minX - imageRect.minX)*scaleX, y:(clipped.minY - imageRect.minY)*scaleY,
               w:clipped.width*scaleX, h:clipped.height*scaleY).integral ∩ (0,0,image.width,image.height)
   cropped = image.cropping(to: cropRect)
   ```
3. **块大小(格子边长,源像素)**:
   ```
   blockSize = intensity.viewBlockSize * max(scaleX, scaleY)
   ```
   (`viewBlockSize` = soft 7 / standard 12 / strong 20,见 §2.4;取 `max` 而非 `min`)
4. 缩小采样:
   ```
   smallSize = (max(1, ceil(cropRect.width / blockSize)), max(1, ceil(cropRect.height / blockSize)))
   ```
   把 `cropped` 以 `imageInterpolation = .none`(最近邻)缩绘到 `smallSize`,得到每个"格子一个像素"的小图。

### 7.3 绘制(视图 `drawMosaicStroke` / 导出 `drawForExport`)

1. 计算笔画外接框(视图坐标)→ 与 `imageRect` 求交 → 得到 `pixelatedCrop` 小图。
2. 设置裁剪区 `clipToMosaicStroke(points, diameter)`:
   - 1 个点 → `addEllipse`(圆心 point,直径 diameter 的圆)。
   - 多点 → polyline,`lineWidth = diameter`,`lineCap/lineJoin = .round`,`replacePathWithStrokedPath()` 后 `clip()`。
3. `imageInterpolation = .none`,把小图 `draw(in: 外接框, from: 小图满幅, operation: .copy)` 放大回原尺寸。

**结果**:马赛克区域 = 原始截图被按 `blockSize` 分块、每块取一代表像素(最近邻)后放大回填,形成纯色像素块;块越大越糊。**无模糊、无透明度、无混合模式**。

**强度(格子大小)精确定量**(Retina `scaleX ≈ scaleY = s` 时):

- 每个格子覆盖的源像素 ≈ `viewBlockSize * s`(soft ≈ 14px,standard ≈ 24px,strong ≈ 40px @2x)。
- 显示到屏幕上每个格子的点尺寸 ≈ `viewBlockSize`(soft 7pt / standard 12pt / strong 20pt)。

### 7.4 笔刷游标 `drawMosaicBrushCursor(at:)`

- 圆 = 以游标点为中心、直径 `mosaicBrushDiameter`。
- 描边两遍:黑 `alpha 0.72`、`lineWidth 3`;再白 `alpha 0.95`、`lineWidth 1.5`。
- 游标点 = `dragCurrent ?? hoverPoint`;仅 `tool == .mosaic` 时绘制。

### 7.5 叠加顺序

马赛克整体绘制在**所有**非马赛克标注之下(§4)。多个马赛克之间按创建顺序叠加(后者在上)。

---

## 8. 键盘快捷键(标注阶段)

`SelectionOverlayController.keyDown`(标注 overlay 的键盘分发;keyCode 为 macOS 虚拟键码):

| 键 | keyCode / 判定 | 行为 |
|---|---|---|
| `Esc` | keyCode `53` | **取消整个捕获会话**(`onCancel`),任何阶段(选择/标注/文字编辑)都生效 |
| `Return` / `Enter` | keyCode `36` 或 `76` | `.annotating` 阶段 → `complete(.copy)`(复制到剪贴板) |
| `Delete`(退格) | keyCode `51` | `.annotating` → `deleteSelection()` |
| `Forward Delete` | keyCode `117` | 同上 |
| `⌘C` | command + `"c"` | `complete(.copy)` |
| `⌘S` | command + `"s"` | `complete(.save)`(Save As…) |
| `⌘Z` | command + `"z"`(无 shift) | `undo()` |
| `⇧⌘Z` | command + `"z"` + shift | `redo()` |

**工具快捷键**(`captureMode == .screenshot` 且 `SelectionGeometry.isValid(selection)` 且**无** command/control/option 修饰键;按 `charactersIgnoringModifiers?.lowercased()` 匹配):

```
V → .select      P → .pen      R → .rectangle      L → .line
A → .arrow       T → .text     M → .mosaic
```

- 切换工具:`phase == .selecting` → `activateAnnotationTool`(同时锁定区域进入标注);否则 `annotationCanvas.tool = tool`。
- **工具切换锁定区域**:选中区域后选择任意绘制工具即"锁定"选区进入标注阶段。
- `AnnotationCanvasView.keyDown` 自身只处理 select 态下的 Delete(`51`/`117`)→ `deleteSelection()`,其余 `super.keyDown`。
- 无"Esc 取消选择"这一独立语义:Esc 一律取消整个截图(见上)。取消选择仅通过点击空白处实现。
- 文字编辑中的 Esc/Return 由内联编辑器拦截(§6.6):Return 提交+复制,Esc 丢弃+取消捕获。

---

## 9. 导出合成(`renderedImage()`)

流程(与视图绘制共享同一套绘制函数,但坐标系翻转):

1. 先 `commitTextEditing()`。
2. `outputSize = (image.width, image.height)` —— **输出保持源图像像素尺寸(Retina 不降采样)**。
3. 建 `NSBitmapImageRep`:宽高 = 源像素,`bitsPerSample=8`、`samplesPerPixel=4`(RGBA)、`hasAlpha=true`、`colorSpaceName=.deviceRGB`、`isPlanar=false`。
4. `context.imageInterpolation = .high`;把 `image` 以 `operation: .copy` 画满 `outputSize`(底图原样)。
5. 先遍历绘制所有 mosaic(`drawForExport`),再所有非 mosaic。
6. `flushGraphics()` 后返回 `bitmap.cgImage`。

**`drawForExport` 坐标转换**:

```
scaleX = image.width / imageRect.width
scaleY = image.height / imageRect.height
convert(point) = (
    x: (point.x - imageRect.minX) * scaleX,
    y: outputSize.height - (point.y - imageRect.minY) * scaleY   // y 翻转:视图 y 向下 → 位图 y 向上
)
```

**各类型导出参数**:

| 类型 | 线宽/字号 | 其它 |
|---|---|---|
| pen | `max(1, width * min(scaleX, scaleY))` | 圆帽圆接折线 |
| rectangle | `max(1, width * min(scaleX, scaleY))` | 圆角半径 **3**(px,注意与视图 2pt 不同) |
| line | `max(1, width * min(scaleX, scaleY))` | 圆帽 |
| arrow | 同 line | 箭头头几何同 §5.4(导出坐标下) |
| text | `max(1, fontSize * min(scaleX, scaleY))` | 背景 padding/圆角乘 `min(scaleX, scaleY)` |
| mosaic | clip 直径 `brushDiameter * min(scaleX, scaleY)` | 块大小按 §7 源像素 |

- 所有线宽/字号/内边距统一按 `min(scaleX, scaleY)` 缩放;马赛克块大小按 §7 的 `max(scaleX, scaleY)` 换算。
- **导出即最终图像**:标注被"烧录"进截图像素(`operation: .copy` 底图 + 覆盖绘制),之后不再保留标注数据。

---

## 10. 撤销/重做与截图重录(重选区域)时的重置

### 10.1 undo / redo / clear

```
undo(): commitTextEditing(); history.undo(); selectedMarkIndex = nil; publish; 重绘
redo(): commitTextEditing(); history.redo(); selectedMarkIndex = nil; publish; 重绘
clearAnnotations(): discardTextEditing(); 仅当 canUndo||canRedo 才 history.clear();
                    selectedMarkIndex = nil; publish; 重绘
```

- undo/redo **先提交进行中的文本编辑**,再操作历史,并**清空选中**(`selectedMarkIndex = nil`)。
- `clearAnnotations` 用的是 `discardTextEditing()`(**丢弃**未完成文本框,而非提交),且**无历史可清时是 no-op**。
- 历史状态通过 `onHistoryChange(canUndo, canRedo)` 回调驱动工具栏 undo/redo/clear 的可用态。

### 10.2 重录 / 重选区域

- `returnToSelection()`(重选区域 / More 菜单 "Reselect Region"):`phase = .selecting`、`selection = .null`、`tearDownAnnotationUI()` → **`clearAnnotationUI()` 销毁画布与工具栏**(`annotationCanvas = nil`、`toolbar = nil`、历史随之丢弃)。
- 重新选好区域后 `prepareSelectionToolbar()` **新建**一个 `AnnotationCanvasView`,传入新的裁剪图;`history` 全新(空),**标注不保留**。
- 工具/尺寸/颜色等设置:仅当存在"上一个画布"(捕获模式切换的 suspend/resume 场景,§10.3)时才拷贝;重录(画布已被销毁)后回到**默认值**(§3)。
- 选区在被 suspended 期间若发生改变,`invalidateAnnotationUI()` 会销毁该画布(因裁剪图已与当前区域不符)。

### 10.3 模式切换(screenshot ↔ record)的 suspend/resume

- 切换捕获模式时若已有标注画布:`suspendAnnotationUI()` 仅**隐藏**画布与工具栏(`annotationUISuspended = true`),**不销毁**;切回 screenshot 时 `resumeAnnotationUI()` 恢复显示,**标注与历史保留**。
- 若 suspended 期间选区被改(`mouseDragged` 里 `annotationUISuspended` 为真 → `invalidateAnnotationUI()`),则画布被销毁。
- 新建画布时若存在"上一个画布"引用,则拷贝其 `colorPreset / textBackgroundStyle / mosaicIntensity / penWidth / shapeWidth / textFontSize / mosaicBrushDiameter`(外观设置延续,历史不延续)。

---

## 11. 迁移实现清单(Web Canvas 2D 映射)

1. **坐标**:Canvas 2D 原点即左上、y 向下,与视图一致;只需在**导出位图**时对 y 做一次 `H - y` 翻转。维护 `imageRect`(Aspect-Fit)与 `scaleX/scaleY`。
2. **重绘模型**:每次状态变化全量重绘;顺序 = 底图 → 所有 mosaic(按序)→ 所有非 mosaic(按序)→ 草稿 → mosaic 游标 → 选中框。
3. **画笔**:`ctx.lineCap="round"`,`ctx.lineJoin="round"`,`lineWidth=penWidth`;采样阈值 `0.5pt`;`points.length > 1` 才提交。
4. **矩形**:`roundRect(rect, 2)`,`stroke`,无最小尺寸;导出圆角半径 3。
5. **直线/箭头**:提交阈值 `3pt`;无 Shift 吸附;箭头头 `headLength = max(12, width*4)`、张角 ±30°、空心折线(stroke,不 fill)。
6. **文字**:字体 `system-ui, -apple-system, semibold`(≈ macOS 系统 semibold);背景 transparent/dark(rgba(0,0,0,0.72))/light(rgba(255,255,255,0.9)),padding(5,3) 圆角 5;在矩形宽度内 `word-wrap` 自动换行;提交前 trim 首尾空白。
7. **马赛克**:`blockSize_px = viewBlockSize * max(scaleX, scaleY)`(soft 7 / standard 12 / strong 20);裁剪区 downscale 到 `ceil(w/blockSize) × ceil(h/blockSize)` 用最近邻,再 `imageSmoothingEnabled=false` 放大回填;clip 用 round 的描边路径;游标圆 = 直径 `brushDiameter`、黑(0.72,w3)+白(0.95,w1.5)双描边。
8. **命中检测**:按逆序命中;pen/line/arrow 用点到线段距离 `≤ max(7, width/2+4)`;rectangle 用外扩 `max(6,width)` 的矩形**含内部**;text 外扩 (7,6) 含内部;mosaic `≤ diameter/2+4`。
9. **选择**:8 手柄(radius 9 命中);line/arrow 只画两端点手柄;其余画外扩 5pt、圆角 6、虚线 `[4,3]` 的白(0.96,w1.5)+紫(w1)双描边;手柄 = 白 10pt 圆 + 紫 6pt 圆;移动/缩放最小步 1pt 才提交,矩形最小边 8pt,全部钳制到 imageRect。
10. **历史**:`append/replace/remove` 存 before/after 全量快照;任何变更清空 redo;拖拽结束只写一条 replace;导出前 commit 文本;重选区域销毁画布与历史。

---

## 附录 A:精确颜色值速查

| 名称 | HEX | 用途 |
|---|---|---|
| accent(violet 默认) | `#7D69F5` | 默认颜色、选中框内描边、手柄内圆、accentStrong 的浅色系 |
| cherry | `#FA476E` | 色板 |
| orange | `#FF7D2E` | 色板 |
| yellow | `#FFD129` | 色板 |
| mint | `#29C78F` | 色板 |
| blue | `#2994FF` | 色板 |
| white | `#FFFFFF` | 色板 / 选中框外描边(alpha .96)/ 手柄外圆 |
| black | `#141414` | 色板 / 画布 letterbox 背景 |
| dark 背景 | `rgba(0,0,0,0.72)` | 文字背景 |
| light 背景 | `rgba(255,255,255,0.9)` | 文字背景 |

## 附录 B:精确阈值/常量速查

| 常量 | 值 |
|---|---|
| 画笔/马赛克采样最小间距 | `0.5` pt |
| 直线/箭头可见长度阈值 | `3` pt |
| 选择拖拽提交最小位移 | `1` pt |
| 矩形缩放最小边 | `8` pt |
| 手柄命中半径(矩形) | `9` pt |
| 端点手柄命中半径(line/arrow) | `10` pt |
| pen/line/arrow 命中容差 | `max(7, width/2 + 4)` pt |
| rectangle 命中外扩 | `max(6, width)` pt |
| text 命中外扩 | `dx: 7, dy: 6` pt |
| mosaic 命中容差 | `diameter/2 + 4` pt |
| 选中框外扩 | `5` pt,圆角 `6`,虚线 `[4,3]` |
| 手柄几何 | 外圆 10pt 白 + 内圆 6pt 紫 |
| 箭头头长 | `max(12, width*4)`,张角 `π/6`(30°) |
| 矩形圆角(视图/导出) | `2` pt / `3` px |
| 文字背景 padding | 水平 `5`,垂直 `3`,圆角 `5` |
| 文字编辑器 inset | `(8, 5)`;圆角 `7`;边框 `1` |
| 新文字框初始尺寸 | 宽 `min(180, max(96, imageRect.width))`,高 `34` |
| 编辑器最小尺寸 | 宽 `120`,高 `34` |
| 马赛克块大小 | soft `7` / standard `12` / strong `20` pt |
| 默认值 | tool `.select`、色 `.violet`、文字背景 `.transparent`、强度 `.standard`、penWidth `3`、shapeWidth `3`、fontSize `18`、brushDiameter `36` |
