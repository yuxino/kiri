# Kiri 素材库 / OCR / GIF 行为规格（Swift 源码 → Tauri 迁移）

> 本文档从以下源码逐行提取，目标读者是未读过 Swift 的工程师，用于在 Tauri（Rust 存储 + React UI）中 1:1 复刻 Kiri 的库体验。
> 规则约定：代码标识符（类型/方法/属性）与 UI 文案保留英文原文；凡涉及 UI 文案处同时给出简中翻译（来自 `zh-Hans.lproj/Localizable.strings`）。所有数字精确到像素/秒。
> 来源文件：`Sources/KiriCore/AssetLibrary.swift`、`Sources/KiriCore/CaptureAsset.swift`、`Sources/KiriCore/RecordingPolicy.swift`、`Sources/KiriApp/LibraryView.swift`、`Sources/KiriApp/AppModel.swift`、`Sources/KiriApp/KiriDesignSystem.swift`、`Sources/KiriApp/CaptureUIStyle.swift`、`Sources/KiriApp/OCRResultPanel.swift`、`Sources/KiriApp/TextRecognizer.swift`、`Sources/KiriApp/GIFExporter.swift`、`Sources/KiriApp/SelectionOverlayController.swift`、`Sources/KiriApp/KiriApp.swift`、`Tests/KiriCoreTests/*` 及 `docs/plans/` 相关设计文档。

---

## 0. 设计系统常量（贯穿全文的数值来源）

以下常量在 `KiriDesignSystem.swift` / `CaptureUIStyle.swift` 定义，是迁移到 React 的 design token 依据。

### 间距 `KiriUI.Spacing`
| 名称 | 值 (pt) |
|---|---|
| `tight` | 6 |
| `compact` | 10 |
| `standard` | 14 |
| `roomy` | 20 |
| `page` | 24 |

### 圆角 `KiriUI.Radius`
| 名称 | 值 (pt) |
|---|---|
| `control` | 11 |
| `badge` | 9 |
| `preview` | 14 |
| `card` | 18 |
| `surface` | 24 |

### Header / Card / Motion
| 常量 | 值 |
|---|---|
| `KiriUI.Header.searchWidth` | 228 pt |
| `KiriUI.Header.sectionPickerWidth` | 176 pt |
| `KiriUI.Header.controlHeight` | 36 pt |
| `KiriUI.Card.thumbnailHeight` | 184 pt |
| `KiriUI.Card.padding` | 12 pt |
| `KiriUI.Card.actionSpacing` | 8 pt |
| `KiriUI.Card.metadataSpacing` | 7 pt |
| `KiriUI.Motion.hover` | 0.14 s |
| `KiriUI.Motion.feedback` | 0.20 s |

### 配色 `CaptureUIColors`（sRGB）
| 名称 | 值 | 用途 |
|---|---|---|
| `accent` | (0.49, 0.41, 0.96) | 主强调色（紫） |
| `accentStrong` | (0.39, 0.31, 0.86) | 强调色加深（主按钮填充） |
| `blossom`/`coral` | (1.0, 0.50, 0.66) | 破坏性/珊瑚色（删除确认） |
| `cyan` | (0.31, 0.75, 0.94) | 渐变辅助色 |
| `accentSoft` | (0.67, 0.58, 1.0) | 浅强调色 |
| `canvas` | light `0xFFFFFF` / dark `0x15131D` | 页面底色 |
| `card` | light `0xFFFFFF` / dark `0x1E1B28` | 卡片底色 |
| `elevated` | light `0xFFFFFF` / dark `0x282334` | 抬高面底色 |
| `surfaceBorder` | light `0xE5DFF0` / dark `0x40394E` | 描边色 |
| `groupFill` | light `0xF3EFF9` / dark `0x302A3D` | 分组填充 |

`brandGradient` = `LinearGradient(accentStrong → accent → cyan, topLeading → bottomTrailing)`；主按钮用此渐变。

---

## 1. 存储模型（`AssetLibrary`）

`AssetLibrary` 是 `actor`（Swift 并发隔离），迁移时对应 Rust 侧的存储模块，所有读写在单线程/锁内串行执行。

### 1.1 目录布局

根目录（rootURL）默认由 `defaultRootURL()` 得到：

```
~/Library/Application Support/kiri/          # 根 rootURL
├── Assets/                                   # 资产原始文件
├── Thumbnails/                               # 缩略图目录（预留，当前版本从不写入）
└── library.json                              # 索引（元数据）
```

- `assetsURL = rootURL/Assets`
- `thumbnailsURL = rootURL/Thumbnails`
- `indexURL = rootURL/library.json`

构造 `AssetLibrary(rootURL:)` 时，先 `createDirectory(at: assetsURL, withIntermediateDirectories: true)`，再同样创建 `thumbnailsURL`。若 `library.json` 存在则读取解码为 `[CaptureAsset]`，否则 `index = []`。

**Debug 覆盖**：`AppModel.makeLibrary()` 在 `#if DEBUG` 下读取环境变量 `KIRI_LIBRARY_ROOT`，若非空则用它作为 rootURL。Rust 迁移保留等价机制（例如 Tauri 的 env 覆盖 / 测试注入）。

### 1.2 资产文件名规则

`importData` / `importFile` 生成的 `filename` 规则完全相同：

```
{yyyyMMdd-HHmmss}-{uuid小写}.{ext}
```

- 时间戳格式：`DateFormatter`，`dateFormat = "yyyyMMdd-HHmmss"`，`locale = "en_US_POSIX"`（必须固定 POSIX，避免本地化数字/日历）。
- `createdAt` 先归一化到毫秒精度（见 1.3）：`Date(timeIntervalSince1970: (t*1000).rounded()/1000)`，即时间戳保留 3 位小数秒。
- UUID：`UUID()`，写入时 `.uuidString.lowercased()`（小写、含连字符的标准 UUID 字符串）。
- 扩展名 `safeExtension`：`trimmingCharacters(in: CharacterSet(charactersIn: "."))`（去掉首尾所有 `.`）→ `.lowercased()`；随后校验 `!safeExtension.isEmpty && safeExtension.allSatisfy({ $0.isLetter || $0.isNumber })`，否则抛 `AssetLibraryError.invalidFilename`（文案 `"The capture filename is invalid."` / `"这项内容的文件名无效。"`）。即扩展名只能是纯字母/数字（不含 `-`、`_`）。
- 文件写入路径：`assetsURL/filename`（`data.write(to:fileURL, options: [.atomic])`；`importFile` 用 `copyItem`，保留源文件不删除）。

### 1.3 元数据 schema（`library.json`）

顶层是 JSON 数组 `[CaptureAsset]`。`CaptureAsset` 字段（Codable，字段顺序无关，JSON key 即 Swift 属性名）：

| 字段 | 类型 | 说明 |
|---|---|---|
| `id` | `UUID` → 字符串 | 资产唯一 ID |
| `kind` | `CaptureKind` → 字符串 | `"image"` / `"video"` / `"gif"` |
| `createdAt` | `Date` → 数字（毫秒时间戳） | 创建时间 |
| `filename` | `String` | 见 1.2 |
| `pixelWidth` | `Int` | 像素宽 |
| `pixelHeight` | `Int` | 像素高 |
| `duration` | `TimeInterval?`（可空 Double） | 时长秒；仅 video/gif 有值，image 为 `null`/缺省 |
| `sourceApplication` | `String?` | 来源应用名（截图/录屏时的前台应用 localizedName） |
| `isFavorite` | `Bool` | 收藏标记，默认 `false` |
| `trashedAt` | `Date?`（可空） | 进回收站时间；`null` 表示未删除 |

编码（`JSONEncoder.kiri`）：
- `outputFormatting = [.prettyPrinted, .sortedKeys]`（pretty 打印 + key 按字典序排序）
- `dateEncodingStrategy = .millisecondsSince1970`（Date 序列化为整数毫秒时间戳）

解码（`JSONDecoder.kiri`）：`dateDecodingStrategy = .millisecondsSince1970`。

**兼容性**：`CaptureKind.init(from:)` 遇到旧值 `"longImage"` 时映射为 `.image`（不会抛错）；遇到其他未知值抛 `DecodingError.dataCorruptedError("Unknown capture kind: …")`。

### 1.4 索引加载 / 保存机制

- **加载**：`init` 时一次性 `Data(contentsOf: indexURL)` + `JSONDecoder.kiri.decode([CaptureAsset].self, from: data)`，全量载入内存 `index`。
- **保存**：`persist()` 把整个 `index` 重新 `JSONEncoder.kiri.encode` 后 `.atomic` 写回 `library.json`。**每次结构变更都全量重写文件**（无增量、无事务文件）。触发 persist 的写操作：`importData`、`importFile`、`setFavorite`、`moveToTrash`、`restore`、`permanentlyDelete`、`emptyTrash`。`replaceData` 只写资产字节，不触发 persist（元数据不变）。
- **写失败回滚**：
  - import：`index.append` 后若 `persist()` 抛错 → `removeItem(at: fileURL)` 删掉已写文件 + `index.removeAll { $0.id == id }`，然后 rethrow。
  - `update(id:mutation:)`（favorite/trash/restore 共用）：先改内存副本，persist 失败则恢复原值 `index[position] = previous` 再抛。

### 1.5 损坏文件容错（重点）

- **`library.json` 损坏/不可解码**：`init` 直接 `throw`，**不做逐条跳过容错**。真正的容错在 `AppModel.makeLibrary()`：捕获 init 错误后回退到 `FileManager.default.temporaryDirectory + "kiri-library"` 作为新库，并设置警告 `L10n.format("Using a temporary library: %@", error.localizedDescription)`（`"正在使用临时素材库：%@"`）显示在错误横幅。若临时库也创建失败，`preconditionFailure("kiri could not create its local capture library")` 直接崩溃。
- **单个资产文件缺失/损坏**：索引里条目仍在；UI 缩略图加载失败会显示 fallback 系统图标（见 4.5）；`Copy` 失败抛 `"The capture file is unavailable."`。
- **文件删除失败被吞掉**：`permanentlyDelete` / `emptyTrash` 中对资产文件和缩略图文件的 `removeItem` 全部 `try?`（静默忽略），但元数据已先 persist 移除，因此 UI 上该条目已消失。
- **`Thumbnails/{id}.jpg`**：当前代码从不生成缩略图文件；`permanentlyDelete`/`emptyTrash` 仍会 `try?` 删除 `Thumbnails/{uuid小写}.jpg`（防御性清理）。迁移时可省略实际写缩略图，但保留目录与清理语义。

### 1.6 排序与查询 API

- `allAssets(includeTrashed: Bool = false)`：过滤（`includeTrashed || trashedAt == nil`）后按 `createdAt` **降序**排序（新的在前）。
- `search(_ query:, includeTrashed:)`：先 `query.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()`，再对 `allAssets` 过滤 `normalized.isEmpty || asset.searchableText.contains(normalized)`；同时校验回收站状态。注意：**UI 不使用这个方法**，UI 在 `AppModel.filteredAssets` 内自行过滤（见第 5 节）；`search` 仅存在于核心层与测试。
- `searchableText`（`CaptureAsset` 计算属性）= `[filename, sourceApplication, kind.rawValue].compactMap{$0}.joined(separator: " ").lowercased()`。

---

## 2. CaptureAsset 模型

- `CaptureKind`：`image`（PNG 截图）、`video`（MP4 录屏）、`gif`（由录屏转换而来）。`CaseIterable`、`Codable`。
- 类型与文件约定：
  - image → 扩展名 `png`（`AppModel.completeCapture` 用 `importData(..., fileExtension: "png", ...)`）。
  - video → 扩展名 `mp4`（`stopRecording` 用 `importFile(..., kind: .video, fileExtension: "mp4", ...)`）。
  - gif → 扩展名 `gif`（`convertToGIF` 用 `importFile(..., kind: .gif, fileExtension: "gif", ...)`）。
- `duration`（`TimeInterval?`）：image 为 `nil`；video 存实际录制时长；gif 存**源视频时长**（非 GIF 播放时长）。
- `pixelWidth`/`pixelHeight`：image 为 `CGImage.width/height`（Retina 物理像素）；video 为合并后 MP4 的像素；gif 为导出后的 GIF 尺寸（见第 9 节）。
- 文件路径约定（两个等价实现，返回同一 URL）：
  - `AssetLibrary.assetURL(for:) = assetsURL.appendingPathComponent(asset.filename)`
  - `AppModel.assetFileURL(asset) = libraryRoot/Assets/{filename}`

---

## 3. 回收站（Trash）

**关键事实：删除是"逻辑删除"，文件不移动。** 资产文件始终留在 `Assets/` 目录；只有 `library.json` 中的 `trashedAt` 字段标记删除状态。

| 操作 | 方法 | 精确行为 |
|---|---|---|
| 移到回收站 | `moveToTrash(id:at: Date = Date())` | `update { $0.trashedAt = date }`（默认当前时间），仅写元数据。文件/缩略图不移动。UI notice `"Moved to Trash"`（`"已移到回收站"`），symbol `trash`。 |
| 恢复 | `restore(id:)` | `update { $0.trashedAt = nil }`。UI notice `"Restored to Library"`（`"已恢复到素材库"`），symbol `arrow.uturn.backward`。 |
| 彻底删除（单个） | `permanentlyDelete(id:)` | ① `index.remove(at: position)` ② `persist()` ③ `try? removeItem(Assets/filename)` ④ `try? removeItem(Thumbnails/{uuid小写}.jpg)`。找不到 ID 抛 `assetNotFound`（`"The capture could not be found."`）。UI notice `"Deleted Permanently"`（`"已永久删除"`），symbol `trash.fill`。 |
| 清空回收站 | `emptyTrash()` | 取 `index.filter { $0.trashedAt != nil }`；空则直接 return（不重写文件）。否则 ① `index.removeAll { $0.trashedAt != nil }` ② `persist()` ③ 逐条 `try?` 删除资产文件与缩略图文件。UI notice `"Trash Emptied"`（`"回收站已清空"`），symbol `trash.slash`。 |

- **彻底删除时机**：**仅用户触发**——单条 `Delete Permanently`（回收站内卡片按钮/右键菜单）或 `Empty Trash`（回收站视图 header 按钮）。**没有任何自动清理策略**（无超时、无容量上限、无后台任务）。
- **确认对话框**：两类破坏性操作都需要二次确认（见 7 节文案）。
- 顺序保证：`permanentlyDelete`/`emptyTrash` 都是**先 persist 元数据、后删文件**；文件删除失败不影响元数据已删除的结果。

---

## 4. LibraryView 库界面布局

### 4.1 窗口尺寸 / 位置

- `Window("Kiri", id: "library")`，内容 `LibraryView`：
  - `minWidth: 820`，`minHeight: 540`
  - `maxWidth/maxHeight: .infinity`（可任意放大）
  - `defaultSize(width: 960, height: 640)`
- 窗口位置由系统管理（无自定义记住位置逻辑；首次按 defaultSize 居中）。应用是常规 Dock 应用（`LSUIElement` 已移除，`setActivationPolicy(.regular)`），库窗口是主窗口。

### 4.2 整体结构（`LibraryView.body`）

```
VStack(spacing: 0)
├── header          （顶部工具栏）
├── errorBanner     （可选，橙色错误横幅）
└── Group（占据剩余空间，maxWidth/maxHeight .infinity）
    ├── 若 !hasLoadedLibrary         → loadingState
    ├── 若 filteredAssets.isEmpty    → emptyState
    └── 否则 ScrollView { LazyVGrid(columns) { ForEach(filteredAssets) { CaptureCard } } }
        .padding(page=24)
        .id(model.showingTrash)   ← 切换 Library/Trash 时强制重建 ScrollView 以重置滚动位置
```

整体 `background(Color.kiriCanvas)`、`tint(CaptureUIColors.accent)`、`.frame(maxWidth/maxHeight: .infinity, alignment: .top)`。

顶层 `.overlay(alignment: .top)`：若有 `model.notice` 显示 `LibraryNoticeView`（见 4.7），`padding(.top: 78)`，`.transition(.move(edge: .top).combined(with: .opacity))`，`.zIndex(10)`，动画 `easeOut(0.20)`。

### 4.3 Header

`ViewThatFits(in: .horizontal)` 在 `wideHeader` 与 `compactHeader` 间自适应（宽度不足时切换为两行紧凑布局）。

- 外层：`.padding(.horizontal, page=24)`、`.padding(.vertical, 15)`、`.background(.regularMaterial)`，底部 1pt 分隔线 `Rectangle().fill(KiriUI.Palette.border.opacity(0.8)).frame(height: 1)`。

**wideHeader**（单行 `HStack(spacing: standard=14)`）：
`titleBlock (layoutPriority 1)` + `Spacer(minLength: 0)` + `searchField(frame width 228)` + `sectionPicker` + [回收站视图时 `emptyTrashButton`] + `captureActions`。

**compactHeader**（`VStack(spacing: standard)` 两行）：
- 行 1：`titleBlock(layoutPriority 1)` + `Spacer` + [emptyTrashButton] + `captureActions`
- 行 2：`searchField(maxWidth .infinity)` + `sectionPicker`

**titleBlock**（`HStack(spacing: compact=10)`）：
- `KiriBrandMark(size: 38)`（38×38，圆角 `38*0.3≈11.4`，app 图标，无图标时用 `viewfinder` 渐变占位）。
- 竖排：标题 `Text(L10n.text(showingTrash ? "Trash" : "Library"))`（`system 17 bold rounded`，即 `"Library"`/`"Trash"`，简中 `"素材库"`/`"回收站"`）；下方 `sectionSummary`（`.caption`，`.secondary`，`.contentTransition(.numericText())`）。
- `sectionSummary` = `L10n.format(count == 1 ? "%d capture" : "%d captures", count)`（英文单复数两套；简中统一 `"%d 项内容"`）。`count = sectionAssets.count`，`sectionAssets` 按 `showingTrash` 过滤 `model.assets`。

**sectionPicker**（`Picker("Section", selection: $model.showingTrash)`，`.segmented`，`.labelsHidden()`，`.controlSize(.large)`，`.frame(width: 176)`）：
- `Label(L10n.text("Library"), systemImage: "photo.on.rectangle").tag(false)`
- `Label(L10n.text("Trash"), systemImage: "trash").tag(true)`
- `.onChange(of: showingTrash) { model.searchQuery = "" }` —— **切换分区时清空搜索**。
- accessibilityLabel `"Library section"`。

**emptyTrashButton**（仅 `showingTrash` 时显示）：
- `Label(L10n.text("Empty Trash"), systemImage: "trash.slash")`，`.font(.system(size: 12, weight: .medium))`，`.labelStyle(.titleAndIcon)`，`.buttonStyle(.bordered)`，`.tint(.red)`。
- `.disabled(!model.assets.contains { $0.trashedAt != nil })`（无回收站内容时禁用）。
- `.help(L10n.text("Permanently delete all captures in Trash"))`；accessibilityLabel `"Empty Trash"`。
- 点击 → `confirmsEmptyTrash = true` 弹出确认 sheet（见 7 节）。

**captureActions**（主按钮）：
- label：`HStack(spacing: 7)`：`isCaptureStarting` 时 `ProgressView().controlSize(.small)`，否则 `Image(systemName: "viewfinder")`；文字 `L10n.text(isCaptureStarting ? "Preparing…" : "Capture")`（`"Preparing…"`/`"Capture"`，简中 `"正在准备…"`/`"截图 / 录屏"`）；非 starting 时追加快捷键文本 `model.captureShortcutLabel`（`.caption.monospacedDigit()`，`.foregroundStyle(.white.opacity(0.78))`）。
- `.buttonStyle(KiriPrimaryButtonStyle())`；`.disabled(model.captureIsUnavailable)`；`.help(L10n.text("Capture or record a region, with optional annotation tools"))`。
- `captureShortcutLabel = CaptureShortcut.kiriCapture.displayLabel = "⇧⌘A"`（key `"a"`，modifiers `[.shift, .command]`）。

### 4.4 网格

```
columns = [GridItem(.adaptive(minimum: 210, maximum: 280), spacing: KiriUI.Spacing.roomy = 20)]
LazyVGrid(columns: columns, spacing: 20) { ForEach(model.filteredAssets) { CaptureCard } }
.padding(KiriUI.Spacing.page = 24)
```

- 自适应列：列宽范围 **210–280 pt**，列间距 **20 pt**，行间距 **20 pt**，网格四周留白 **24 pt**。
- 使用 `LazyVGrid`（懒加载，仅渲染可见卡片）——React 侧用虚拟化列表/网格保证同等滚动性能。
- **无日期分组**：资产平铺排列，**仅按 `createdAt` 降序**（`allAssets` 已排好序，`filteredAssets` 保序）。没有按天/周/月分组标题。

### 4.5 条目卡片 `CaptureCard`

结构（`VStack(alignment: .leading, spacing: standard=14)`，`padding = Card.padding = 12`，背景 `Color.kiriCard`，圆角 `Radius.card = 18`）：

1. **缩略图区 `CaptureThumbnail`**（`.frame(maxWidth: .infinity)`，`.frame(height: 184)`）：
   - 内层 ZStack 底色：`RoundedRectangle(cornerRadius: Radius.preview = 14)` 填充 `LinearGradient(accent.opacity(0.075) → cyan.opacity(0.04), topLeading → bottomTrailing)`。
   - 图片：`Image(decorative: cgImage).resizable().interpolation(.high).scaledToFit().clipShape(RoundedRectangle(cornerRadius: Radius.control = 11)).padding(5)`（等比缩放 fit，四周 5pt 内边距）。
   - 加载失败：`Image(systemName: fallbackSystemImage)`（`.system(size: 30, weight: .medium)`，`.tertiary`）。
   - 加载中：`ProgressView().controlSize(.small)`。
   - **缩略图来源**（`CaptureThumbnailLoader.load`，非磁盘缓存）：
     - 扩展名 `mp4`/`mov`：`AVURLAsset` + `AVAssetImageGenerator`，`appliesPreferredTrackTransform = true`，`maximumSize = CGSize(width: 640, height: 640)`，取 `image(at: .zero)`（首帧）。
     - 其他（png/gif 等）：`CGImageSourceCreateThumbnailAtIndex`，options `kCGImageSourceCreateThumbnailFromImageAlways=true`、`kCGImageSourceCreateThumbnailWithTransform=true`、`kCGImageSourceShouldCacheImmediately=true`、`kCGImageSourceThumbnailMaxPixelSize=640`。
     - `.task(id: reloadToken)` 依赖 `model.libraryRevision` 变化时重载（资产字节被 `replaceData` 更新后刷新）。
   - **悬停覆盖**（`.overlay`，仅 `isHovered && asset.trashedAt == nil` 时）：主操作按钮 `Label(primaryActionTitle, systemImage: primaryActionSymbol)`，`.callout.weight(.semibold)`，`KiriPrimaryButtonStyle()`，`.controlSize(.large)`，`.transition(.scale(scale: 0.96).combined(with: .opacity))`。
     - `primaryActionTitle = asset.kind == .image ? "Copy" : "Open"`（image→`"Copy"`/`"复制"`，video/gif→`"Open"`/`"打开"`）。
     - `primaryActionSymbol = image ? "doc.on.doc" : "play.fill"`。
   - **kind 徽章**（`.overlay(alignment: .topLeading)`，`padding(compact=10)`）：`Label(kindTitle, systemImage: iconName)`，`.caption2.weight(.semibold)`，`.foregroundStyle(accent)`，`.padding(.horizontal, 7)`，`.frame(height: 24)`，`.background(.regularMaterial, in: RoundedRectangle(cornerRadius: Radius.badge = 9))`，描边 `Color.primary.opacity(0.12)`。
     - `kindTitle`：image→`"Image"`/`"图片"`，video→`"Video"`/`"视频"`，gif→`"GIF"`（无本地化）。
     - `iconName`：image→`"photo"`，video→`"video"`，gif→`"sparkles.rectangle.stack"`。
   - `.onTapGesture(count: 2) { model.open(asset) }` —— **双击打开**。`.help(L10n.text("Double-click to open"))`（`"双击打开"`）。

2. **元数据区**（`VStack(alignment: .leading, spacing: Card.metadataSpacing = 7)`）：
   - 行 1（`HStack(alignment: .firstTextBaseline, spacing: compact=10)`）：
     - `Text(asset.createdAt, format: .dateTime.month(.abbreviated).day().hour().minute())`，`.subheadline.weight(.medium)`，`.lineLimit(1)`。格式示例 `"Aug 1, 3:30 PM"`（缩写月 + 日 + 时 + 分；迁移时按系统 locale 的等价格式，含小时分钟、不含年份/秒）。
     - `Spacer(minLength: 0)`；若 `asset.isFavorite` → `Image(systemName: "star.fill")`，`.foregroundStyle(.yellow)`（隐藏于无障碍树）。
   - `metadataLine`（`HStack(spacing: metadataSpacing=7)`，`.secondary`，`.frame(maxWidth: .infinity, alignment: .leading)`）：
     - `pixelSize = "\(pixelWidth) × \(pixelHeight)"`（如 `"1920 × 1080"`，**× 为 U+00D7**），`.caption.monospacedDigit()`。
     - 若 `asset.duration != nil` → 分隔符 `"·"`（`.tertiary`）+ `RecordingPolicy.elapsedLabel(duration)`（`.caption.monospacedDigit()`）。
     - 若 `sourceApplication` 非空 → 分隔符 + `Text(source)`（`.caption`，`.lineLimit(1)`，`.truncationMode(.middle)`）。
   - `elapsedLabel`：`totalSeconds = Int(duration.rounded(.down))`；`h = totalSeconds/3600`，`m = (totalSeconds%3600)/60`，`s = totalSeconds%60`；`h>0` → `"%d:%02d:%02d"`，否则 `"%02d:%02d"`（即 `MM:SS` 或 `H:MM:SS`）。

3. **操作行**（`HStack(spacing: Card.actionSpacing = 8)`）：
   - **非回收站**（`trashedAt == nil`）：
     - 主操作按钮：`Label(primaryActionTitle, systemImage: primaryActionSymbol)`，`.buttonStyle(.bordered)`，`.tint(accent)`，`.controlSize(.small)`，`.help(primaryActionTitle)`。
     - `Spacer()`。
     - 收藏图标按钮：`iconButton(asset.isFavorite ? "star.slash" : "star", help: asset.isFavorite ? "Remove Favorite" : "Favorite")` → `model.toggleFavorite(asset)`。
     - 删除图标按钮：`iconButton("trash", help: "Move to Trash", role: .destructive)` → `model.moveToTrash(asset)`。
     - `actionMenu`（ellipsis，见 7 节）。
   - **回收站**（`trashedAt != nil`）：
     - `Label(L10n.text("Restore"), systemImage: "arrow.uturn.backward")`（`"Restore"`/`"恢复"`），`.bordered`，`.small`。
     - `Spacer()`。
     - 永久删除按钮：`Image(systemName: "trash.fill")`，`.borderless`，`.frame(width: 28, height: 26)`，`.help("Delete Permanently")`，`role: .destructive` → `confirmsPermanentDelete = true`。
   - `iconButton` 通用：`Image(systemName:)`，`.borderless`，`.frame(width: 28, height: 26)`，`.contentShape(Rectangle())`，`.help(help)`。

4. **卡片外观状态**（`@State isHovered`，`onHover { isHovered = $0 }`）：
   - 圆角 `Radius.card = 18`，描边 `RoundedRectangle`：hover 时 `accent.opacity(0.52)` 线宽 **1.25**，否则 `Palette.border` 线宽 **1**。
   - 阴影两层：hover 时 `accent.opacity(0.12)` radius 16 y 7 + `black.opacity(0.08)` radius 10 y 4；非 hover 时 `black.opacity(0.035)` radius 5 y 4。
   - `.offset(y: isHovered ? -1 : 0)`（hover 上浮 1pt）。
   - 动画 `.easeOut(duration: Motion.hover = 0.14)`，value `isHovered`。
   - **没有单选/多选状态**：卡片只有 hover 态与双击，不存在"选中态"。点击一次（非双击）不产生选中。

5. **拖出**：`.onDrag { NSItemProvider(contentsOf: model.assetFileURL(asset)) ?? NSItemProvider() }` —— 从卡片拖出即把**资产文件**作为文件提供者（用于拖到 Finder 等）。

6. **确认 sheet**（`confirmsPermanentDelete`）→ `KiriDestructiveConfirmationView`，见 7 节。

### 4.6 空状态（`emptyState` 三分支，仅当 `hasLoadedLibrary && filteredAssets.isEmpty`）

1. **有搜索词**（`hasSearchQuery = !searchQuery.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty`）：
   - `LibraryStatusView(systemImage: "magnifyingglass", title: "No matching captures", message: "Try a different search, or clear the current one.")` + 动作按钮 `Button("Clear Search") { searchQuery = "" }`。
2. **回收站空**（`showingTrash`）：
   - `LibraryStatusView(systemImage: "trash", title: "Trash is empty", message: "Captures you delete stay recoverable here.")`（无动作按钮）。
3. **首次运行 onboarding**：
   - 居中卡片，`.padding(.horizontal, 40)`、`.padding(.vertical, 34)`，`.kiriSurface(radius: 24, elevated: true)`（背景 `card` + 渐变 `accent.opacity(0.09) → clear → cyan.opacity(0.06)`）。
   - 内容（`VStack(spacing: 20)`）：
     - `KiriBrandMark(size: 72)` + 右上角 `sparkles`（14 bold，coral，`.thinMaterial` 圆形，offset(8,-7)）。
     - 标题 `"Ready for your first capture"`（`system 22 bold rounded`，简中 `"开始第一次截图或录屏"`）；副标题 `"Choose a capture mode, then select the region you need."`（`.callout`，`.secondary`，简中 `"选择一种截取方式，再框出你需要的区域。"`）。
     - 主按钮 `Label("Capture", systemImage: "viewfinder").frame(minWidth: 150)`，`KiriPrimaryButtonStyle()`。
     - 快捷键提示 `L10n.format("or press  %@", "⇧⌘A")`（`.caption.monospacedDigit()`，`.secondary`）。
     - 分割线 `Rectangle().fill(border).frame(width: 400, height: 1)`。
     - 三步（`HStack(spacing: 18)`，`chevron.right` 分隔）：`OnboardingStep(number, title, detail)`：
       - 1 `"Mode"` / `"Screenshot · Record · OCR"`（简中 `"模式"` / `"截图 · 录屏 · 识字"`）
       - 2 `"Select"` / `"Choose a region"`（`"框选"` / `"选择区域"`）
       - 3 `"Finish"` / `"Copy or save"`（`"完成"` / `"复制或保存"`）
     - `OnboardingStep`：序号（`.caption.semibold`，accent，22×22 圆，accent 12% 底）+ 竖排 title（`.caption.semibold`）/detail（`.caption2`，secondary）。
- `LibraryStatusView` 通用：图标（`.system(size: 28, weight: .medium)`，`.tertiary`）+ title（`.headline`）+ message（`.callout`，secondary）+ actions，`.multilineTextAlignment(.center)`，`.padding(32)`。

### 4.7 顶部通知 `LibraryNoticeView` 与错误横幅

**notice**（操作反馈，自动消失）：
- 外观：`HStack(spacing: 8)`：`Image(systemName: notice.symbol)`（accent）+ `Text(notice.title)`（`.callout.weight(.medium)`）+ 关闭 `xmark`（`.caption.semibold`，secondary）。
- `.padding(.horizontal, 12)`、`.frame(height: 36)`、`.background(.regularMaterial, in: Capsule())`、`Capsule` 描边 `Color.primary.opacity(0.12)`、阴影 `black.opacity(0.12)` radius 12 y 5。
- 定位：`.overlay(alignment: .top)` 内 `.padding(.top: 78)`。
- 生命周期：`AppModel.showNotice` 设置后 `Task.sleep(2 秒)` 自动清除（若期间生成了新 notice，旧任务因 id 不匹配而失效）。`dismissNotice()` 手动清除。

**errorBanner**（`.padding(.top: compact=10)`，位于 header 下）：
- `HStack(spacing: 9)`：`exclamationmark.triangle.fill`（orange）+ `Text(errorMessage)`（`.callout`）+ `Spacer()` + 可选 `capturePermissionRecoveryLabel` 恢复按钮（`.borderedProminent` `.small`）+ 关闭 `xmark`（`.plain`，help `"Dismiss"`）。
- 容器：`.padding(.horizontal, 14)`、`.padding(.vertical, 11)`、背景 `Color.orange.opacity(0.10)` 圆角 `Radius.control=11`，描边 `orange.opacity(0.22)`；外层 `.padding(.horizontal, page=24)`。

---

## 5. 搜索

### 5.1 UI 交互（`searchField`）

- `HStack(spacing: 7)`：`magnifyingglass`（`.secondary`）+ `TextField(L10n.text("Search captures"), text: $model.searchQuery)`（`.textFieldStyle(.plain)`，`.focused($searchIsFocused)`，`.onSubmit { searchIsFocused = false }`）+ 清空按钮（`searchQuery` 非空时显示 `xmark.circle.fill`，`.tertiary`，`.plain`，`.help("Clear Search")`）。
- 容器：`.padding(.horizontal, 10)`、`.frame(height: 36)`、`.background(Color.kiriElevated)`、圆角 `11`；描边：聚焦时 `accent.opacity(0.58)` 否则 `Palette.border`；聚焦时额外阴影 `accent.opacity(0.10)` radius 7。
- accessibility：`accessibilityElement(children: .contain)`，label `"Search captures"`。

### 5.2 匹配逻辑（在 `AppModel.filteredAssets`，非核心层）

```
query = searchQuery.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
result = assets.filter {
    stateMatches = showingTrash ? trashedAt != nil : trashedAt == nil
    return stateMatches && (query.isEmpty || asset.searchableText.contains(query))
}
```

- **匹配字段**：`searchableText = filename + " " + sourceApplication + " " + kind.rawValue`（小写）。即**文件名（含扩展名）、来源应用名、类型关键字（image/video/gif）**。**不匹配 OCR 文本**（OCR 结果从不入库、不关联资产）、不匹配像素尺寸、不匹配时间。
- **大小写不敏感**：query 与 searchableText 均 lowercased。
- **清空行为**：① 搜索框 xmark 按钮清空；② `sectionPicker` 切换 Library/Trash 时 `onChange` 强制 `searchQuery = ""`。
- **空结果状态**：搜索词非空但无匹配 → `"No matching captures"` 空态 + `"Clear Search"` 按钮（见 4.6）。
- `assets` 来源：`refresh()` 每次用 `library.allAssets(includeTrashed: true)`（已按 createdAt 降序）全量替换，因此搜索结果天然按时间降序。
- 核心层 `AssetLibrary.search` 与 UI 过滤等价（同样 trim + lowercase + `searchableText.contains`），但 UI 不调用它。

### 5.3 ⌘F 快捷键

- 在 `KiriCommands`（`CommandGroup(after: .textEditing)`）中定义菜单项 `"Find in Library"`（简中 `"在素材库中查找"`），`.keyboardShortcut("f", modifiers: .command)`，`.disabled(focusLibrarySearch == nil)`。
- 触发动作：`focusLibrarySearch?()` → 通过 `FocusedValue(\.focusLibrarySearch)` 设置 `searchIsFocused = true`（聚焦搜索框）。菜单在无焦点视图时禁用。

---

## 6. 收藏（Favorite）

- **切换**：`AppModel.toggleFavorite(_ asset:)` → `library.setFavorite(!asset.isFavorite, id:)` → `refresh()`。入口有三处：
  1. 卡片操作行图标按钮：`star`（未收藏，help `"Favorite"`）/ `star.slash`（已收藏，help `"Remove Favorite"`）。
  2. 右键菜单：`"Favorite"` / `"Remove Favorite"`。
- **显示**：收藏项在卡片元数据行右侧显示黄色 `star.fill`（`.foregroundStyle(.yellow)`，accessibilityHidden）。
- **重要：没有独立的"收藏过滤视图"**。当前版本只有 `Library` / `Trash` 两个分区（`sectionPicker`），收藏仅作为卡片上的星标标记，不改变排序、不置顶、不提供筛选。迁移时若要"完全一致"，则不要添加收藏筛选标签。

---

## 7. 右键 / 悬停操作（逐项精确行为与文案）

### 7.1 卡片右键菜单（`contextMenu`）

**非回收站资产**（`trashedAt == nil`，自上而下）：
1. `Copy`（`systemImage: "doc.on.doc"`）—— **仅 `kind == .image` 时显示**。→ `model.copy(asset)`。
2. `Open`（`"arrow.up.right.square"`）→ `model.open(asset)`。
3. `Show in Finder`（`"folder"`）→ `model.reveal(asset)`。
4. `Convert to GIF`（`"sparkles.rectangle.stack"`）—— **仅 `kind == .video` 时显示**；`.disabled(!canConvertToGIF || isConvertingToGIF)`。→ `model.convertToGIF(asset)`。
5. `Favorite` / `Remove Favorite`（`"star"` / `"star.slash"`）→ `toggleFavorite`。
6. `Divider()`。
7. `Move to Trash`（`"trash"`，`role: .destructive`）→ `moveToTrash`。

**回收站资产**（`trashedAt != nil`）：
1. `Restore`（`"arrow.uturn.backward"`）→ `restore`。
2. `Divider()`。
3. `Delete Permanently`（`"trash.fill"`，`role: .destructive`）→ `confirmsPermanentDelete = true`（弹确认）。

> **注意**：库内右键菜单**没有"另存为 / Save As…"**。`"Save As…"`（`"另存为…"`）只存在于**截图标注编辑器**的 More 菜单，属于标注流程，不属于库。迁移时不要在库的右键菜单加入 Save As。

### 7.2 ellipsis "更多" 菜单（`actionMenu`，卡片操作行的 `ellipsis` 图标）

- `Menu` label：`Image(systemName: "ellipsis")`（`.frame(width: 28, height: 26)`，`.menuStyle(.borderlessButton)`，`.menuIndicator(.hidden)`，help `"More Actions"`）。
- 内容（仅非回收站资产显示该菜单）：
  1. 若 `kind == .video`：`Convert to GIF` 或（转换中）`"Converting to GIF…"`，`.disabled(!canConvertToGIF || isConvertingToGIF)` + `Divider()`。
  2. `Open`。
  3. `Show in Finder`。
- 回收站资产不显示 ellipsis 菜单（操作行只有 Restore + 永久删除）。

### 7.3 各操作精确行为

| 操作 | 方法 | 精确行为 / 文案 |
|---|---|---|
| 复制（仅 image） | `AppModel.copy(_:)` | `NSImage(contentsOf: assetFileURL)` 失败 → error `"The capture file is unavailable."`；写剪贴板 `NSPasteboard.general.clearContents()` + `writeObjects([image])` 失败 → `"Could not copy the capture."`；成功 → notice `"Copied to Clipboard"`（`"已复制到剪贴板"`），symbol `checkmark.circle.fill`。**注意：剪贴板写的是 NSImage 对象，不是文件引用。** |
| 打开 | `open(_:)` | `NSWorkspace.shared.open(assetFileURL(asset))` —— 用系统默认应用打开（PNG→预览，MP4→QuickTime，GIF→浏览器/预览）。 |
| 在访达中显示 | `reveal(_:)` | `NSWorkspace.shared.activateFileViewerSelecting([assetFileURL(asset)])` —— 打开 Finder 并选中该文件。 |
| 转换为 GIF | `convertToGIF(_:)` | 见第 9 节。 |
| 移到回收站 | `moveToTrash(_:)` | 见第 3 节；notice `"Moved to Trash"` / `"已移到回收站"`，symbol `trash`。 |
| 恢复 | `restore(_:)` | 见第 3 节；notice `"Restored to Library"` / `"已恢复到素材库"`，symbol `arrow.uturn.backward`。 |
| 永久删除（单条） | `permanentlyDelete(_:)` | 先弹确认 sheet（见下），确认后执行，notice `"Deleted Permanently"` / `"已永久删除"`，symbol `trash.fill`。 |
| 清空回收站 | `emptyTrash()` | 先弹确认 sheet，notice `"Trash Emptied"` / `"回收站已清空"`，symbol `trash.slash`。 |
| 双击 | `open(asset)` | 同"打开"。 |
| 拖出卡片 | `.onDrag` | 导出资产文件（拖到 Finder/其他应用）。 |

### 7.4 破坏性确认对话框 `KiriDestructiveConfirmationView`

（sheet 形式，`.frame(width: 370)`，`.padding(26)`，背景 `canvas` + 顶部 coral 径向渐变 `coral.opacity(0.08) → clear`，endRadius 220）

- 顶部图标：`RoundedRectangle(cornerRadius: 18).fill(KiriUI.warmGradient.opacity(0.16))` 58×58，内 `Image(systemName: "trash.fill")`（23 semibold，coral）。
- 标题（`system 18 bold rounded`，居中）+ 消息（`system 12.5`，secondary，居中）。
- 按钮行（`HStack(spacing: compact=10)`）：
  - `Cancel`（`"取消"`，`.bordered`，`.controlSize(.large)`，`.keyboardShortcut(.cancelAction)`）。
  - 确认按钮（`role: .destructive`，`.borderedProminent`，`.large`，`.tint(coral)`，文字 `.frame(minWidth: 118)`）。

**两组文案**：
1. 单条永久删除：
   - title `"Delete this capture permanently?"`（`"要永久删除这项内容吗？"`）
   - message `"This cannot be undone."`（`"此操作无法撤销。"`）
   - confirmTitle `"Delete Permanently"`（`"永久删除"`）
2. 清空回收站：
   - title `"Empty Trash?"`（`"清空回收站？"`）
   - message `"All captures in Trash will be permanently deleted. This cannot be undone."`（`"回收站中的所有内容将被永久删除，此操作无法撤销。"`）
   - confirmTitle `"Empty Trash"`（`"清空回收站"`）

---

## 8. OCR（文字识别）

### 8.1 入口与触发时机

OCR 是**第三个捕获模式**（非库内操作）。进入截屏浮层后，顶部/底部模式选择器 `CaptureModeSegmentedControl` 有三段：

| 段 | symbol | title | accessibilityLabel / toolTip |
|---|---|---|---|
| 0 | `camera.viewfinder` | `Screenshot`（`"截图"`） | `Screenshot` |
| 1 | `record.circle` | `Record`（`"录屏"`） | `Record Region` |
| 2 | `text.viewfinder` | `OCR`（`"文字"`） | `Recognize Text`（`"识别文字"`） |

- 选择 OCR 模式（`changeCaptureMode(toSegment: 2)`）后：清空选区 `selection = .null`、禁用窗口悬停选中（`windowSelectionCandidate` 对 `.ocr` 返回 nil）、拆除标注 UI、保留模式选择器可见。
- **触发时机**：用户**拖动框选文字区域，松开鼠标（mouse-up）**即触发识别。指令文案 `"Release to recognize text"`（`"松开以识别文字"`）；初始提示 `"Drag to choose text to recognize   ·   Esc to cancel"`（`"拖动框选要识别的文字区域   ·   Esc 取消"`）。
- **自动重跑**：结果面板显示期间，选区框保持可编辑（可移动/缩放，把手命中半径 10，最小边长 16）。任何拖动结束（mouse-up）都会重新调用 `presentOCRPanel()` → `runOCRRecognition()` **重新识别**；拖动过程中 `layoutOCRPanel()` 实时跟随选区。新开始一次框选（mouseDown 在空白处）会先 `tearDownOCRPanel()`。
- **Return 键**：有结果面板时 → `finishOCR(with: panel.editedText)`（复制并结束）；无面板时 → `presentOCRPanel()`。Esc 取消整个浮层。

### 8.2 识别流程（`TextRecognizer.recognizeText`）

```
VNRecognizeTextRequest:
  recognitionLevel = .accurate
  usesLanguageCorrection = true
  recognitionLanguages = ["zh-Hans", "zh-Hant", "en-US", "ja-JP"]
VNImageRequestHandler(cgImage: image, options: [:]).perform([request])
结果 = observations.compactMap { $0.topCandidates(1).first?.string }.joined(separator: "\n")
```

- 输入：框选区域裁剪出的 `CGImage`（`croppedSelection()` 按物理像素矩形裁剪，Retina 全分辨率）。
- **每个 observation 取 top 1 候选**，**按 observation 顺序（自上而下阅读顺序）逐行用 `"\n"` 连接**，保留阅读顺序。
- 运行在 `Task.detached(priority: .userInitiated)`。
- **支持语言**：简体中文 `zh-Hans`、繁体中文 `zh-Hant`、英语（美国）`en-US`、日语 `ja-JP`。精准度级别：`.accurate`（准确模式），开启语言校正 `usesLanguageCorrection = true`。
- 返回空字符串 = 无文本；`try?` 捕获错误 → 面板进入 failed 态（异常统一按"失败"处理，不区分具体错误）。

### 8.3 结果面板 `OCRResultPanel`

- 尺寸：`panelWidth = 336`，`panelHeight = 224`（初始，高度随后动态调整）。
- 外观：`NSVisualEffectView`，`material = .popover`，`blendingMode = .withinWindow`，`state = .active`，`appearance = .aqua`；`cornerRadius = 16`，`cornerCurve = .continuous`；边框 `black 0.10` 宽 1；阴影 `black` opacity 0.2 / radius 20 / offset (0, 7)。
- 布局（垂直 `NSStackView`，spacing 10，上下左右边距 16/18）：
  1. **header**：`text.viewfinder` 图标（18×18，accent，15 semibold）+ 标题 `"Recognized Text"`（`"识别结果"`，15 semibold）。
  2. **contentWell**（圆角 10，背景 `(0.975, 0.968, 0.99)`，边框 `black 0.08`）：
     - `scrollView`（文本视图容器，top/bottom 6，leading/trailing 10）。
     - `textView`：`isRichText = false`、13pt、`textContainerInset = (2, 8)`、可垂直滚动不可水平、宽度跟随容器、关闭自动引号替换。**可编辑**（用户可改字）。
     - `statusStack`（居中，vertical，spacing 9）：spinner + statusSymbol + statusLabel + statusDetailLabel。
  3. **buttonRow**（horizontal，spacing 8，`distribution = .fill`）：`summaryLabel`（11 medium，secondary，可隐藏）+ 弹性 spacer + `cancelButton` + `copyButton`。
     - `copyButton`：`CaptureActionButton(symbol: "doc.on.clipboard.fill", label: "Copy", style: .primary, showsTitle: true)`。
     - `cancelButton`：`CaptureActionButton(symbol: "xmark", label: "Close", style: .secondary, showsTitle: true)`。

### 8.4 面板状态机（`State`）

| 状态 | 视觉 | 文案 | copy 按钮 |
|---|---|---|---|
| `.recognizing` | spinner 转，隐藏文本区 | `"Recognizing Text…"`（`"正在识别文字…"`），detail 空 | 禁用 |
| `.text(value)` | 显示可编辑文本，隐藏 status | 文本 + summary | `value` 去空白非空则启用 |
| `.empty` | 图标 `text.badge.xmark`（accent）+ 文字 | `"No Text Found"` / `"Try a larger region or clearer text"`（`"未找到文字"` / `"试试框选更大的区域，或选择更清晰的文字"`） | 禁用 |
| `.failed` | 图标 `exclamationmark.triangle`（systemOrange）+ 文字 | `"Text Recognition Failed"` / `"Adjust the region and try again"`（`"文字识别失败"` / `"调整识别区域后再试一次"`） | 禁用 |

- `statusSymbol` 尺寸 24（regular）；`statusLabel` 12.5 medium；`statusDetailLabel` 11.5 secondary 80% 透明。
- `.text` 态：`textView.string = value`，光标移到末尾，`scrollToBeginningOfDocument`。
- **summary**：`characterCount = value.filter { !$0.isWhitespace }.count`（非空白字符数）；`lineCount = max(1, value.split(separator: "\n", omittingEmptySubsequences: false).count)`（含空行的换行数）；文案 `L10n.format("%d lines · %d chars", lineCount, characterCount)`（`"%d 行 · %d 字"`）。编辑文本时 `textDidChange` 实时更新 summary 并重算 copy 可用性。

### 8.5 面板高度动态计算（`updatePanelSize`）

- `.text` 态：`textHeight = 文本 usedRect 高度`；`wellHeight = min(max(textHeight + 24, 56), 160)`（范围 56–160）。
- 其他态：`wellHeight = 104`。
- 总高 = `16 + headerHeight + 10 + wellHeight + 10 + buttonRowHeight + 16`。宽度固定 336。
- 面板位置（`layoutOCRPanel`）：默认在选区上方 `x = selection.midX - 168`，`y = selection.maxY + 12`；水平夹紧到 `[8, bounds.maxX - 336 - 8]`；若超出底部（模式选择器上方 12 或屏幕底 8）则翻到选区上方 `y = selection.minY - height - 12`，再夹紧到 `[8, bottomLimit - height]`。

### 8.6 结果关联与落地

- **OCR 结果不创建库资产、不入库、不与任何资产关联**。点击 Copy（或 Return）→ `finishOCR(with: panel.editedText)` → 关闭浮层 → `onRecognizeText(text)` → `AppModel.copyRecognizedText(text)`：
  - `trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)`，空则不复制。
  - `writeToClipboard(text)`：`NSPasteboard.general.clearContents()` + `setString(text, forType: .string)`（**纯文本**）。
  - 成功 → notice `"Text Copied"`（`"文字已复制"`），symbol `doc.on.clipboard.fill`；失败 → `"Could not copy the capture to the clipboard."`。
- 点击 Close（cancelButton）→ `onCancel?()` 取消整个浮层。

---

## 9. GIF 导出（`GIFExporter`）

### 9.1 入口

- 卡片右键菜单 / ellipsis 菜单中的 `"Convert to GIF"`（仅 `kind == .video`）。
- 可用条件 `canConvertToGIF(asset)` = `asset.kind == .video && RecordingPolicy.isGIFEligible(duration: asset.duration)`，其中 `isGIFEligible` = `duration > 0 && duration <= 15`。
- 转换中：`gifConversionAssetIDs` 含该 id 时菜单显示 `"Converting to GIF…"`（`"正在转换为 GIF…"`）并禁用（防重入）。
- **没有进度条/进度 UI**。转换期间仅菜单文字变化 + 禁用；完成后 notice `"GIF Created"`（`"GIF 已生成"`），symbol `sparkles.rectangle.stack`。

### 9.2 转换参数（全部来自 `RecordingPolicy` / `GIFExporter`）

| 参数 | 值 |
|---|---|
| 最长时长 `maximumGIFDuration` | **15 s**（超过抛 `durationTooLong`） |
| 帧率 `gifFramesPerSecond` | **12 fps** |
| 长边上限 `maximumGIFLongEdge` | **720 px** |
| 循环次数 | `kCGImagePropertyGIFLoopCount = 0`（**无限循环**） |
| 单帧延迟 | `1 / 12` s（`kCGImagePropertyGIFDelayTime`） |
| 帧数 | `gifFrameCount = max(1, Int(ceil(duration * 12)))` |
| 采样时间 | 第 i 帧 = `min(duration - 0.001, i / 12)`，再 `max(0, …)`，`preferredTimescale = 600` |
| 帧时间容差 | `requestedTimeToleranceBefore = .zero`；`after = 1/12 s` |

### 9.3 尺寸缩放规则

```
naturalSize = 视频轨 naturalSize
transform = 视频轨 preferredTransform
transformedSize = naturalSize.applying(transform)
sourceWidth = abs(transformedSize.width); sourceHeight = abs(transformedSize.height)
（任一 ≤ 0 → videoTrackUnavailable）
scale = min(1, 720 / max(sourceWidth, sourceHeight))      // 只缩小，不放大
targetSize = (max(1, round(sourceWidth * scale)), max(1, round(sourceHeight * scale)))
```

- `AVAssetImageGenerator.appliesPreferredTrackTransform = true`，`maximumSize = targetSize`。

### 9.4 导出文件与入库

- 临时文件：`FileManager.temporaryDirectory + "kiri-gif-{uuid小写}.gif"`。
- 逐帧 `CGImageDestinationAddImage`，每帧附加 `kCGImagePropertyGIFDelayTime = 1/12`；`CGImageDestinationSetProperties` 设 loopCount 0。
- 失败清理：逐帧循环抛错或 `CGImageDestinationFinalize` 失败时 `try? removeItem(outputURL)` 后抛错。
- 成功后 `AppModel.convertToGIF`：
  - `importFile(at: exported.fileURL, kind: .gif, fileExtension: "gif", pixelWidth: exported.pixelWidth, pixelHeight: exported.pixelHeight, duration: exported.duration, sourceApplication: asset.sourceApplication)`。
  - `pixelWidth/Height` = **实际输出 GIF 尺寸**（`outputSize` 取最后一帧 `result.image` 的宽高）；`duration` = **源视频时长**；`sourceApplication` 继承自源视频。
  - `defer { try? removeItem(exported.fileURL) }` 删除临时文件；`refresh()`；notice `"GIF Created"`。

### 9.5 错误文案（`GIFExporterError`，均本地化）

| 错误 | 文案 (en) | 简中 |
|---|---|---|
| `videoTrackUnavailable` | `"The video track could not be read."` | `"无法读取视频轨道。"` |
| `durationUnavailable` | `"The video duration is unavailable."` | `"无法获取视频时长。"` |
| `durationTooLong` | `"GIF conversion currently supports videos up to 15 seconds."` | `"GIF 转换目前仅支持不超过 15 秒的视频。"` |
| `destinationUnavailable` | `"Kiri could not create the GIF file."` | `"Kiri 无法创建 GIF 文件。"` |
| `frameGenerationFailed` | `"Kiri could not extract a video frame."` | `"Kiri 无法提取视频画面。"` |
| `finalizeFailed` | `"The GIF could not be finalized."` | `"无法完成 GIF 文件。"` |

（注：`frameGenerationFailed` 枚举虽定义，但逐帧循环直接把底层错误 rethrow，实际路径多走 `finalizeFailed` / 底层错误；迁移时保留等价错误模型即可。）

---

## 10. 其他行为（汇总）

- **空库状态 UI**：见 4.6（loading / 搜索空 / 回收站空 / onboarding 四态）。
- **加载态**：`loadingState` = `ProgressView().controlSize(.small)` + `"Loading Library…"`（`"正在加载素材库…"`，`.callout`，secondary）。`hasLoadedLibrary` 在首次 `refresh()` 完成后置 true（防止 onboarding 闪屏）。
- **库窗口尺寸**：min 820×540，default 960×640（见 4.1）。
- **拖放导入**：**不存在**。库只支持**拖出**（`.onDrag` 导出文件），无 `.onDrop` 导入。资产入库只有三条路径：截图完成（`importData` png）、录屏停止（`importFile` mp4）、录屏转 GIF（`importFile` gif）。
- **双击打开**：卡片 `.onTapGesture(count: 2)` → 系统默认应用打开。
- **单进程 / Dock**（来自 `docs/plans/2026-08-03-dock-presence-design.md` 与 `KiriAppDelegate`）：应用是常规 Dock 应用，`LSUIElement` 已移除、`setActivationPolicy(.regular)`；启动后每秒 + 每次检测到同 bundle id 应用启动时，关闭其他 Kiri 实例（`closeOtherKiriInstances`，terminate 失败 350ms 后 forceTerminate）。这是 app 级行为，Tauri 单实例 + 常规窗口即可对齐。
- **本地优先**：所有数据留在本地（`~/Library/Application Support/kiri`），无上传/分析/账户/网络。

---

## 附：迁移关键提示（Rust 存储 + React UI）

1. **存储**：`library.json` 全量重写 + `.atomic`（写临时文件再 rename）即可对齐 `persist()`；时间戳用毫秒整数；JSON key 排序（`sortedKeys`）在功能上无关紧要，但若需字节级 diff 测试可保留。
2. **回收站是软删除**：不要移动文件；只需翻转 `trashedAt`，文件留在 `Assets/`。删除元数据先落盘、后删文件，文件删除失败不报错。
3. **时间戳**：`yyyyMMdd-HHmmss` 必须用 UTC/POSIX 语义（源码用 `en_US_POSIX` + 系统时区——`DateFormatter` 未设 timeZone，实际用**本地时区**）。注意文件名时间戳跟随设备本地时区，而 `createdAt` 存的是绝对毫秒时间戳；两者可能因时区变化不一致（迁移需复现"本地时区格式化文件名"这一行为）。
4. **网格无日期分组**：不要实现分组标题；纯 `createdAt` 降序 + 自适应列 210–280。
5. **无收藏过滤**、**库内无另存为**、**无拖放导入**、**GIF 无进度条**——这些都是"不存在的功能"，复刻时不要画蛇添足。
6. **缩略图**：即时生成（图片 max 640，视频取首帧 640×640），不落盘缓存；`Thumbnails/` 目录仅需保留清理语义。
7. **OCR 不入库**：OCR 是截图浮层的第三种模式，结果仅复制到剪贴板，不写 `library.json`。
