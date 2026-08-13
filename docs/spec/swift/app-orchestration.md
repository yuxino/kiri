# Kiri Swift 端应用编排行为规格

> 本文件是对 Kiri（Swift/macOS 截屏标注工具）应用编排层的行为规格，供 1:1 复刻迁移到 Tauri（Rust + React）使用。所有代码标识符、UI 字符串原文保留英文（以反引号或引号引用）。数值精确到像素/秒。坐标约定：AppKit 使用**左下原点**（bottom-left），CoreGraphics/Quartz 使用**左上原点**（top-left），凡涉及坐标换算处均已标注。
>
> 源码依据：`Sources/KiriApp/` 下的 `AppModel.swift`、`KiriApp.swift`、`EditorWindowController.swift`、`PinnedImageController.swift`、`CaptureCoordinator.swift`、`KiriDesignSystem.swift`、`CaptureUIStyle.swift`、`L10n.swift`，以及 `docs/adr/0004-kawaii-professional-visual-system.md`、`docs/plans/2026-08-04-kiri-v0-2-codex-handoff.md`。为精确覆盖"模式切换 / HoverData / 光标位置"等行为，补充引用了 `SelectionOverlayController.swift`、`RecordingPolicy.swift`（`KiriCore`）、`ScreenCapturePermissionGate.swift`、`SelectionGeometry.swift`、`CaptureShortcut.swift`、`RecordingPreferences.swift`、`AnnotationCanvasView.swift`、`AssetLibrary.swift`、`CaptureAsset.swift` 中的事实。

---

## 1. 应用生命周期

### 1.1 应用入口与场景结构

- `KiriApp`（SwiftUI `App`，`@main`）定义两个场景：
  1. `Window("Kiri", id: "library")` —— 素材库主窗口，内容为 `LibraryView(model:)`。
  2. `MenuBarExtra("Kiri", systemImage: "viewfinder")` —— 菜单栏常驻图标，内容为 `MenuBarView(model:)`。
- `AppModel` 以 `@StateObject` 持有（全局唯一）。
- 库窗口 frame 约束：`.frame(minWidth: 820, minHeight: 540)` + `.frame(maxWidth: .infinity, maxHeight: .infinity)`；默认尺寸 `.defaultSize(width: 960, height: 640)`。即**最小 820×540，默认 960×640，可最大化**。
- 库窗口 `.task { model.start(); await model.refresh() }`：窗口出现时注册快捷键并加载素材库。
- `MenuBarExtra` 内容末尾 `.task { model.start() }`：菜单栏图标出现时也触发 `start()`（`start()` 有 `hasStarted` 幂等保护，见 2.2）。

### 1.2 Dock 图标 / 激活策略

- `KiriAppDelegate.applicationDidFinishLaunching` 调用 `NSApplication.shared.setActivationPolicy(.regular)`：**Kiri 是常规应用，Dock 上有图标**（不是 `accessory`/`prohibited` 无窗口菜单栏应用）。
- 无"无窗口模式"：库窗口与菜单栏图标始终可用；菜单栏 `MenuBarView` 中的 `Open Library` 按钮通过 `openWindow(id: "library")` 并 `NSApplication.shared.activate(ignoringOtherApps: true)` 打开/激活库窗口。
- 无 `applicationShouldTerminateAfterLastWindowClosed` 覆盖：关闭库窗口**不会**退出应用，应用继续以菜单栏图标存活。

### 1.3 单实例

- `applicationDidFinishLaunching` 中：
  1. 立即调用 `closeOtherKiriInstances()`。
  2. 注册 `NSWorkspace.didLaunchApplicationNotification` 观察者（`queue: .main`），回调里 `Task { @MainActor in closeIfDuplicate(application) }`。
  3. 创建 `Timer(timeInterval: 1, repeats: true)`，加入 `RunLoop.main` 的 `.common` 模式，`duplicateScanTimer` 持有；每 **1 秒**重复调用 `closeOtherKiriInstances()`。
- `closeOtherKiriInstances()`：取 `Bundle.main.bundleIdentifier`，枚举 `NSRunningApplication.runningApplications(withBundleIdentifier:)`，对每个调用 `closeIfDuplicate`。
- `closeIfDuplicate(application)`（静态）：
  - 跳过 `processIdentifier == 自身 pid`；跳过 `bundleIdentifier != 自身 bundleIdentifier`。
  - 调用 `application.terminate()`；若返回 `false` 立即 `forceTerminate()` 并返回。
  - 否则 `Task { @MainActor in try? await Task.sleep(for: .milliseconds(350)); if !application.isTerminated { application.forceTerminate() } }`：**350 毫秒**宽限后仍未终止则强制终止。
- `applicationWillTerminate`：移除 `launchObserver`，`duplicateScanTimer?.invalidate()`。

### 1.4 退出逻辑

- 唯一退出入口：`NSApplication.shared.terminate(nil)`。
- 菜单栏 `MenuBarView` 末尾按钮 `Quit Kiri`（symbol `power`）→ `terminate`。
- `capturePermissionRecoveryAction == .quitKiri` 时 `performCapturePermissionRecovery()` 也调用 `terminate`（见 2.8）。
- 无"退出前确认"、无"退出时保存"额外逻辑；录音中直接退出的特殊收尾不在应用编排层处理（由录音控制器资源释放兜底）。

### 1.5 激活 / 失活处理

- 无 `applicationDidBecomeActive` / `applicationDidResignActive` 委托实现。Kiri **不监听自身的激活/失活事件**。
- 焦点归还逻辑集中在 `AppModel.startCapture()`（见 2.5），通过 `NSRunningApplication.activate(options: [])` 显式把焦点还给来源应用。
- 覆盖层窗口 `CaptureOverlayWindow`：`canBecomeKey = true`、`canBecomeMain = false`（可成为 key window 但不能成为 main window）。

---

## 2. AppModel 状态机

### 2.1 全部状态字段（`@Published`）

| 字段 | 类型 | 可写性 | 初始值 | 说明 |
|---|---|---|---|---|
| `assets` | `[CaptureAsset]` | `private(set)` | `[]` | 素材列表（含回收站时由 `refresh` 全量载入） |
| `hasLoadedLibrary` | `Bool` | `private(set)` | `false` | `refresh()` 置 `true` |
| `libraryRevision` | `Int` | `private(set)` | `0` | 每次 `refresh()` `&+= 1`，驱动视图刷新 |
| `isCaptureStarting` | `Bool` | `private(set)` | `false` | 截图会话正在启动（含权限/冻结/覆盖层准备） |
| `isRecordingStarting` | `Bool` | `private(set)` | `false` | 录制正在启动（含倒计时/编码器启动） |
| `isRecording` | `Bool` | `private(set)` | `false` | 正在录制 |
| `isRecordingPaused` | `Bool` | `private(set)` | `false` | 录制已暂停 |
| `isRecordingTransitioning` | `Bool` | `private(set)` | `false` | 暂停/恢复的中间态（segment 停止/启动进行中） |
| `isRecordingFinalizing` | `Bool` | `private(set)` | `false` | 停止后正在合并/入库/清理 |
| `recordingElapsed` | `TimeInterval` | `private(set)` | `0` | 当前已录制总时长（跨 segment 累加） |
| `gifConversionAssetIDs` | `Set<UUID>` | `private(set)` | `[]` | 正在转 GIF 的 asset id 集合 |
| `notice` | `AppNotice?` | `private(set)` | `nil` | 顶部短暂提示（2 秒自动消失） |
| `searchQuery` | `String` | 可写 | `""` | 库搜索框文本 |
| `showingTrash` | `Bool` | 可写 | `false` | 是否显示回收站视图 |
| `errorMessage` | `String?` | 可写 | 见下 | 错误文案；`didSet` 中若值变化则 `capturePermissionRecoveryAction = nil` |
| `capturePermissionRecoveryAction` | `CapturePermissionRecoveryAction?` | `private(set)` | `nil` | 错误对应的恢复按钮动作 |

- `errorMessage` 初始值：`init()` 里 `Self.makeLibrary()` 失败落到临时库时设置的 warning（`L10n.format("Using a temporary library: %@", …)`），成功时为 `nil`。
- `errorMessage` 的 `didSet` 行为：只要新值 `!= oldValue`，就把 `capturePermissionRecoveryAction` 清为 `nil`（因此错误一变，恢复按钮即消失）。

### 2.2 派生计算属性

- `filteredAssets`：`searchQuery.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()`；`showingTrash ? asset.trashedAt != nil : asset.trashedAt == nil`；且 `query.isEmpty || asset.searchableText.contains(query)`。`searchableText = [filename, sourceApplication, kind.rawValue].compactMap.joined(" ").lowercased()`。
- `captureShortcutLabel` = `CaptureShortcut.kiriCapture.displayLabel` = `"⇧⌘A"`（`key="a"`, `modifiers=[.shift,.command]`，glyph 顺序按 `allCases` `[control,option,shift,command]` 过滤后拼接 + `key.uppercased()`）。
- `recordingElapsedLabel` = `RecordingPolicy.elapsedLabel(recordingElapsed)`（见 2.6）。
- `hasRecordingSession` = `isRecording || isRecordingPaused || isRecordingTransitioning`。
- `captureIsUnavailable` = `isCaptureStarting || isRecordingStarting || isRecording || isRecordingPaused || isRecordingTransitioning || isRecordingFinalizing`（任一项为真即禁用截图/录屏入口）。
- `capturePermissionRecoveryLabel`：把 `capturePermissionRecoveryAction` 映射为文案，见 2.8。

### 2.3 `start()`（幂等初始化）

- `guard !hasStarted else { return }`，置 `hasStarted = true`，`try registerShortcut()`（即 `shortcutMonitor.start(shortcut: .kiriCapture) { self?.startCapture() }`）。
- 捕获 `GlobalShortcutError`：`errorMessage = error.localizedDescription`；`accessibilityPermissionRequired → .openAccessibilitySettings`；`inputMonitoringPermissionRequired → .openInputMonitoringSettings`；`eventTapCreationFailed` 不设 recovery action。
- 其它错误：`errorMessage = error.localizedDescription`。

### 2.4 模式切换（screenshot / record / OCR）

> 重要：`AppModel` 本身**没有 mode 字段**。"模式"属于覆盖层 `SelectionOverlayController` 内部的 `CaptureMode`（`screenshot`/`recording`/`ocr` 三种，`segmentIndex` 依次 0/1/2）。AppModel 只通过 `SelectionOverlayController.present` 的四个回调区分结果动作。

- 覆盖层 `CaptureSessionView` 顶部 `CaptureModeSegmentedControl`（3 段：`Screenshot`/`Record`/`OCR`，SF Symbol 依次 `camera.viewfinder`/`record.circle`/`text.viewfinder`），初始 `captureMode = .screenshot`。
- 切换 `changeCaptureMode(toSegment:)`：`nextMode == captureMode` 直接返回；否则改 `captureMode`、`phase = .selecting`、关闭录音选项浮层、`tearDownOCRPanel()`。切到 OCR：清标注 UI、`selection = .null`、清 hovered/pending 窗口选择、显示模式控件、重置 first responder。已有有效选区时：截图模式恢复/新建标注工具条，录屏模式 `presentRecordingOptions()`。
- 模式控件几何：`layoutCaptureModeControl()` 令控件尺寸 `width = max(220, fittingSize.width)`、`height = max(44, fittingSize.height)`，水平居中，垂直位于 `y = bounds.maxY - size.height - 88`（即距屏幕**顶部 88pt**）。
- `Record` 模式完成选区后弹出 `RecordingOptionsPopoverController`（`options = RecordingPreferences.load()`，`onChange` 持久化，`onStart` 调 `recordRegion`）。`recordRegion` 保存 options、关浮层、`window.orderOut(nil)`、`onRecord(selection.standardized, options.normalized)`。

### 2.5 `startCapture()` 完整流程 + 焦点归还

```
guard overlayController == nil, !captureIsUnavailable else { return }
isCaptureStarting = true
errorMessage = nil
initialFrontmostApplication = NSWorkspace.shared.frontmostApplication
isKiriFrontmost = (initialFrontmostApplication?.pid == 自身 pid)
hiddenWindows = hideKiriLibraryWindows()
Task {
  defer { isCaptureStarting = false }
  do {
    returnApplication = await resolveCaptureReturnApplication(initialFrontmostApplication, wasKiriFrontmost: isKiriFrontmost)
    sourceApplication = returnApplication?.localizedName
    if !hiddenWindows.isEmpty { try? await Task.sleep(for: .milliseconds(120)) }
    capture = try await captureCoordinator.captureActiveDisplay()
    controller = SelectionOverlayController(capture: capture)
    overlayController = controller
    controller.present(onComplete:, onRecord:, onRecognizeText:, onCancel:)
  } catch { ... }
}
```

- `hideKiriLibraryWindows()`：收集 `NSApplication.shared.windows` 中 `window.isVisible && window.level == .normal && window.styleMask.contains(.titled)` 的窗口，逐个 `orderOut(nil)`，返回数组。
- `resolveCaptureReturnApplication(_:wasKiriFrontmost:)`（异步）：
  - 若**不是** Kiri 在前台 → 直接返回 `initialApplication`（即截屏前的前台应用）。
  - 若是 Kiri 前台 → `NSApplication.shared.hide(nil)`；`sleep 100ms`；读 `NSWorkspace.shared.frontmostApplication`；`unhideWithoutActivation()`；若新前台是 Kiri 自身则返回 `nil`，否则返回该应用。
- 隐藏了库窗口则额外 `sleep 120ms`（让窗口真正离屏后再冻结画面，避免截到 Kiri 自身）。
- 四个回调的焦点/窗口处理：
  - `onComplete(image, action)`：`overlayController = nil`；`finishCapturePresentation`（`keepKiriLibraryHidden(hiddenWindows)`；若 `action == .copy` 则 `activate(returnApplication)`）；随后 `completeCapture(...)`。
  - `onRecord(region, options)`：`overlayController = nil`；`keepKiriLibraryHidden`；`activate(returnApplication)`；`beginRegionRecording(...)`。
  - `onRecognizeText(text)`：`overlayController = nil`；`keepKiriLibraryHidden`；`activate(returnApplication)`；`copyRecognizedText(text)`。
  - `onCancel`：`overlayController = nil`；`cancelCapturePresentation(initialApplication:, returnApplication:, hiddenWindows:)`。
- `cancelCapturePresentation`：
  - `wasKiriFrontmost`（由 `initialApplication.pid == 自身 pid` 判断）为真 → 对 `hiddenWindows` 逐个 `orderFront(nil)` 并 `NSApplication.shared.activate(ignoringOtherApps: true)`（把 Kiri 带回前台）。
  - 否则 → `keepKiriLibraryHidden(hiddenWindows)` 并 `activate(returnApplication)`（归还焦点给来源应用）。
- `activate(_ application:)`：`guard let application, !application.isTerminated else { return }; application.activate(options: [])`。
- 错误路径：`CaptureCoordinatorError` → `cancelCapturePresentation(initial, returnApplication: initialFrontmostApplication, hidden)` + `handleCaptureCoordinatorError`；其它错误 → 同上 + `errorMessage = error.localizedDescription`。

**焦点归还结论（截图后回到原应用）**：截图完成走 `Copy`（默认）时，先把来源应用 `activate` 回来，再异步把 PNG 写库（`completeCapture`）。即"剪贴板优先 + 归还焦点"，且**不自动打开 Kiri 素材库**。

### 2.6 从覆盖层到库的流转（`completeCapture` / `perform`）

`completeCapture(image, action, sourceApplication)`：

1. 若 `action == .copy`：立即 `nsImage(from: image)` 并 `writeToClipboard`；失败置 `errorMessage = CaptureExportError.clipboardWriteFailed`，成功 `showNotice(title: "Copied to Clipboard", symbol: "checkmark.circle.fill")`。（剪贴板写入在 PNG 编码入库**之前**，体现"clipboard-first"。）
2. 异步（`Task.detached(priority: .utility)`）把 `CGImage` 编码为 PNG（`NSBitmapImageRep.representation(using: .png, properties: [:])`）。失败：`errorMessage = "Could not encode the capture as PNG."` 并返回。
3. `library.importData(data, kind: .image, fileExtension: "png", pixelWidth: image.width, pixelHeight: image.height, sourceApplication:)`。
4. 构造 `StoredCapture(asset,image,data)`，`await refresh()`，`perform(action, on: stored)`。

`perform(action, on:)` 分发：
- `.copy`：无操作（已复制）。
- `.save`：`saveToChosenLocation(stored.data)`（`NSSavePanel`，`allowedContentTypes = [.png]`，`nameFieldStringValue = "kiri-<CaptureFilename.timestamp()>.png"`，`activate(ignoringOtherApps:true)`，模态 `runModal() == .OK` 后 `data.write(to: url, options: [.atomic])`，成功 `showNotice("Saved", "checkmark.circle.fill")`）。
- `.pin`：`pin(stored.nsImage)`。
- `.edit`：`presentEditor(for: stored)`。

### 2.7 录制状态机（AppModel 内）

- `beginRegionRecording`：`guard regionRecorder == nil, !captureIsUnavailable else return`；记录 `recordingSourceApplication`/`recordingReturnApplication`；`isRecordingStarting = true`；`errorMessage = nil`。
  - `effectiveOptions = options.normalized`；**macOS < 15**（`#unavailable(macOS 15.0)`）时强制 `capturesMicrophone = false`。
  - 若 `capturesMicrophone` → `ensureMicrophonePermission()`（`.authorized` 通过；`.notDetermined` 请求；`.denied/.restricted/unknown` 抛 `RecordingAccessError.microphonePermissionDenied`）。
  - 若 `usesCountdown` → `RecordingCountdownController.run(screenFrame:, region:)`（3-2-1，`RecordingPolicy.countdownSeconds = 3`）；返回 `false`（Esc 取消）时 `isRecordingStarting = false; recordingSourceApplication = nil; return`（不开始、不提示）。
  - 建 `RegionRecorder`，`recordingConfiguration` 记录 `displayID/sourceRect/backingScale/options/screenFrame`；`recordingSegments = []`、`recordingElapsedBeforeCurrentSegment = 0`。
  - `prepareRecordingClickHighlighter`（仅当 `highlightsClicks`）与 `prepareRecordingControlPanel`。
  - `recorder.start(...exceptedWindowIDs:)`；成功后 `isRecordingStarting=false, isRecording=true, isRecordingPaused=false, isRecordingTransitioning=false, recordingElapsed=0, recordingStartedAt=Date()`，`startRecordingClock()`，`updateRecordingControlPanel()`，`recordingClickHighlighterController?.setActive(true)`，`activate(recordingReturnApplication)`，`showNotice("Recording Started", "record.circle.fill")`。
  - 失败：`resetRecordingSession()` + `errorMessage`（麦克风错误额外设 `.openMicrophoneSettings`）。
- `pauseRecording()`：`guard isRecording, !isRecordingTransitioning, let recorder`；`isRecording=false, isRecordingTransitioning=true`；关高亮/时钟/面板；`recorder.stop()` → `recordingSegments.append(media)`；`recordingElapsedBeforeCurrentSegment = segments 时长和`；`recordingElapsed = 该和`；`regionRecorder=nil`；`recordingStartedAt=nil`；`isRecordingPaused=true, isRecordingTransitioning=false`；`showNotice("Recording Paused", "pause.circle.fill")`。失败 → `failRecordingSession`。
- `resumeRecording()`：`guard isRecordingPaused, !isRecordingTransitioning, let configuration`；新建 `RegionRecorder`，用 `configuration` 重新 `start`；成功 `isRecordingPaused=false, isRecordingTransitioning=false, isRecording=true, recordingStartedAt=Date()`，重启时钟，`activate(recordingReturnApplication)`，`showNotice("Recording Resumed", "play.circle.fill")`。失败回滚为 paused。
- `stopRecording()`：`guard (isRecording || isRecordingPaused), !isRecordingTransitioning`；`activeRecorder = isRecording ? regionRecorder : nil`；`isRecording=false, isRecordingPaused=false, isRecordingFinalizing=true`；关高亮/时钟/面板；`activate(recordingReturnApplication)`；把当前 segment stop 后追加，`regionRecorder=nil`，`RecordingSegmentMerger.merge(segments)`；`defer` 删除所有临时 segment 文件与最终文件；`library.importFile(at: finalMedia.fileURL, kind: .video, fileExtension: "mp4", pixelWidth:, pixelHeight:, duration:, sourceApplication:)`；`refresh()`；`showNotice("Recording Saved", "video.fill")`；`resetRecordingSession()`。失败：删除 segment 文件 + `errorMessage` + `resetRecordingSession()`。
- 时钟 `startRecordingClock()`：`Task` 每 **250ms** sleep 一次，`recordingElapsed = recordingElapsedBeforeCurrentSegment + Date().timeIntervalSince(recordingStartedAt)` 并 `updateRecordingControlPanel()`。
- `resetRecordingSession()` 清空：`regionRecorder/recordingCountdownController`、关控制面板/点击高亮、`isRecordingStarting/isRecording/isRecordingPaused/isRecordingTransitioning/isRecordingFinalizing=false`、`recordingElapsed=0`、`recordingElapsedBeforeCurrentSegment=0`、`recordingStartedAt=nil`、`recordingConfiguration=nil`、`recordingSegments=[]`、`recordingSourceApplication=nil`、`recordingReturnApplication=nil`。
- `recordingElapsedLabel`：`elapsedLabel` 规则 = `totalSeconds = max(0, Int(duration.rounded(.down)))`；`hours>0` → `"h:mm:ss"`，否则 `"mm:ss"`（`String(format: "%02d:%02d", …)`）。
- 控制面板 `updateRecordingControlPanel` 传入 `isBusy = isRecordingStarting || isRecordingTransitioning || isRecordingFinalizing`。

### 2.8 权限恢复动作

`CapturePermissionRecoveryAction` 枚举与 URL（`performCapturePermissionRecovery` 打开系统设置）：

| case | 触发 | URL（`x-apple.systempreferences:…`） |
|---|---|---|
| `.openSettings` | 截屏权限 `settingsRequired` | `com.apple.preference.security?Privacy_ScreenCapture` |
| `.quitKiri` | 截屏权限 `restartRequired` | `NSApplication.shared.terminate(nil)` |
| `.openAccessibilitySettings` | 快捷键 Accessibility 缺失 | `com.apple.preference.security?Privacy_Accessibility` |
| `.openInputMonitoringSettings` | 快捷键 Input Monitoring 缺失 | `com.apple.preference.security?Privacy_ListenEvent` |
| `.openMicrophoneSettings` | 麦克风权限被拒 | `com.apple.preference.security?Privacy_Microphone` |

恢复按钮文案 `capturePermissionRecoveryLabel`：`openSettings→"Open Settings"`、`quitKiri→"Quit Kiri"`、`openAccessibilitySettings→"Open Accessibility Settings"`、`openInputMonitoringSettings→"Open Input Monitoring Settings"`、`openMicrophoneSettings→"Open Microphone Settings"`。

### 2.9 库操作与提示（AppModel 的公开方法）

- `refresh()`：`assets = await library.allAssets(includeTrashed: true)`；`libraryRevision &+= 1`；`hasLoadedLibrary = true`。
- `toggleFavorite/moveToTrash/restore/permanentlyDelete/emptyTrash`：对应 `library` 调用 + `refresh()` + 成功提示（notice 见 8.3）。
- `copy(asset)`：`NSImage(contentsOf: assetFileURL)`；失败 `"The capture file is unavailable."`；`writeToClipboard` 失败 `"Could not copy the capture."`，成功 `showNotice("Copied to Clipboard", "checkmark.circle.fill")`。
- `open(asset)`：`NSWorkspace.shared.open(assetFileURL)`。`reveal(asset)`：`NSWorkspace.shared.activateFileViewerSelecting([assetFileURL])`。
- `canConvertToGIF`：`asset.kind == .video && RecordingPolicy.isGIFEligible(duration:)`（`duration > 0 && duration <= 15s`）。
- `convertToGIF`：`GIFExporter.export` → `importFile(kind: .gif, fileExtension: "gif", …)`，`showNotice("GIF Created", "sparkles.rectangle.stack")`；`gifConversionAssetIDs` 在过程中含该 id。
- `assetFileURL(asset)` = `libraryRoot/Assets/<filename>`（AppModel 侧路径；`AssetLibrary` 内部另有 `assetURL` 见 8.2）。
- `pin(image)`：建 `PinnedImageController`，存 `pinnedControllers[id]`，`onClose` 时移除。

---

## 3. CaptureCoordinator（截屏/冻结/几何收集）

### 3.1 数据结构与错误

- `CapturedDisplay { image: CGImage; screenFrame: CGRect; windowRectsFrontToBack: [CGRect]; displayID: CGDirectDisplayID; backingScale: CGFloat }`。
- `CaptureCoordinatorError`（`LocalizedError`）：`permissionRestartRequired`（文案 "Screen Recording access was granted. Quit and reopen Kiri once to finish enabling capture."）、`permissionSettingsRequired`（"Screen Recording is off. …"）、`displayUnavailable`（"The active display could not be captured."）。

### 3.2 权限检查（`ScreenCapturePermissionGate`）

`captureActiveDisplay(excludingWindowIDs: Set<CGWindowID> = [])`：

1. **DEBUG fixture**：环境变量 `KIRI_CAPTURE_FIXTURE == "1"` 或命令行 `--capture-fixture` 时返回合成画面（主屏尺寸、两个假窗口矩形、`displayID=0`、`backingScale=max(scale,1)`）。
2. `permissionGate.check(preflight: CGPreflightScreenCaptureAccess, request: CGRequestScreenCaptureAccess)`：
   - `preflight()` 真 → 清空缓存并返回 `.authorized`。
   - 已有缓存 → 直接返回缓存值（避免重复弹权限框）。
   - 否则 `request()` 真 → `.restartRequired`；假 → `.settingsRequired`；**缓存**该结果并返回。
3. `.authorized` 继续；`.restartRequired` 抛 `permissionRestartRequired`；`.settingsRequired` 抛 `permissionSettingsRequired`。

### 3.3 冻结（freeze）机制

- **没有真正的显示冻结 API**。Kiri 通过 `SCScreenshotManager.captureImage` 抓取当前活动显示器的一张**全屏静态快照**，把它作为覆盖层 `CaptureSessionView` 的背景 `CGImage` 绘制，从而"冻结"画面供框选。覆盖层窗口（`level = .screenSaver`，borderless、透明背景）盖在真实桌面上，用户看到的是快照。
- 快照 `SCStreamConfiguration`：`showsCursor = false`（**快照不含系统鼠标指针**；覆盖层自行用 `NSCursor.crosshair` 等绘制光标）。
- `width/height` = `display.width/height`（点）× `backingScale`（`max(screen.backingScaleFactor, 1)`），四舍五入取整、`max(1, …)` —— 即按**视网膜倍率**像素尺寸抓取，保证 Retina 屏上覆盖层清晰。

### 3.4 显示器选择与坐标

- `mouseLocation = NSEvent.mouseLocation`（AppKit 左下原点屏幕坐标）。
- `screen = NSScreen.screens.first { NSMouseInRect(mouseLocation, $0.frame, false) } ?? NSScreen.main` —— **取鼠标当前所在屏幕**，取不到则主屏。
- `displayNumber = screen.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")]` → `displayID`。
- `SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true)`。
- `display = content.displays.first { $0.displayID == displayID }`，否则抛 `displayUnavailable`。
- `displayBounds = CGDisplayBounds(displayID)`（Quartz 左上原点，全局坐标）。

### 3.5 可见窗口枚举（`windowRectsFrontToBack`）

对 `content.windows`（`SCWindow` 顺序为**前到后**）`compactMap`，仅保留同时满足：

1. `window.isOnScreen`；
2. `window.windowLayer == 0`（普通窗口层）；
3. `window.owningApplication?.processID != 当前进程 pid`（**排除 Kiri 自身窗口**）；
4. `visible = window.frame.standardized.intersection(displayBounds)` 非 `null` 且 `visible.width >= 8 && visible.height >= 8`（可见部分至少 8×8pt）。

返回的矩形转换为**显示器本地坐标**：`x = visible.minX - displayBounds.minX`、`y = visible.minY - displayBounds.minY`、宽高取 `visible` 宽高。结果即 `windowRectsFrontToBack`。

### 3.6 HoverData / 可见窗口命中规则（覆盖层）

- 覆盖层 `CaptureSessionView` 用 `WindowSelectionGeometry.candidate(at:windowsFrontToBack:within:minimumSide: 8)`：
  按数组顺序（前→后）遍历，返回**第一个**满足 `visible = window.standardized.intersection(displayBounds)` 非 null、宽高 ≥ 8、且 `visible.contains(point)` 的矩形。**即命中"最前面的"包含鼠标点的窗口**（ADR 0003：单击选中最前窗口）。
- `hoveredWindowSelection`：`mouseMoved` 中当 `captureMode != .ocr && 无有效选区 && selectionInteraction == nil` 时计算；OCR 模式下恒为 `nil`。
- 单击（`mouseDown` + `mouseUp` 未移动超过阈值）选中：`windowSelectionCandidate(at:)`（同 `WindowSelectionGeometry.candidate`）；`mouseUp` 时若 `interaction == .creating` 且选区无效且 `pendingWindowSelection` 存在，则 `selection = pendingWindowSelection`。
- 手动拖拽选区：`mouseDragged` 移动距离 `hypot(dx,dy) >= 3` 判定为拖拽（`interactionMoved = true`），否则视为 hover；创建时 `selection = SelectionGeometry.clamped(normalized(from:to:), to: bounds)`；`SelectionGeometry.isValid` 默认 `minimumSide = 3`（宽高 ≥ 3 有效）。
- 选区移动/缩放：`SelectionGeometry.moved`/`resized`（`resized` 在拖拽里 `minimumSide: 16`）；8 个手柄 `SelectionHandle`（`topLeft/top/topRight/right/bottomRight/bottom/bottomLeft/left`）；命中半径 `hitTest(..., radius: 10)`。

### 3.7 光标位置计算 / 多显示器

- 光标**全局位置**：`NSEvent.mouseLocation`（左下原点）→ 用于选屏（`NSMouseInRect`）。
- 光标**在覆盖层内位置**：`convert(event.locationInWindow, from: nil)`（覆盖层 `frame == capture.screenFrame`），再 `clampedPoint` 夹到 `bounds`。
- 放大镜 `drawLoupe` 像素采样：`scaleX = image.width / bounds.width`、`scaleY = image.height / bounds.height`；采样中心 `(hoverPoint.x * scaleX, hoverPoint.y * scaleY)`；采样 `sourceRect` 为以该点为中心 **11×11 像素**（`±5.5`），`integral` 后与图像范围求交；放大镜边长 **88pt**，画在鼠标点 `(+18, +18)` 处（越界翻到 `-18` 侧），夹在距边 8pt 内；白色 2pt 边框圆角 6 + 白色 80% 1pt 十字线。
- **多显示器**：只捕获鼠标所在的那一台显示器的画面；`screenFrame` 为该屏 AppKit frame；`displayBounds` 为该屏 Quartz bounds。窗口矩形转换为该屏本地坐标。录制时 `RegionRecordingConfiguration` 记住 `displayID/sourceRect/backingScale/screenFrame`。
- 录制区域坐标换算（`prepareRecordingClickHighlighter`，Quartz 左上 → AppKit 左下）：`x = screenFrame.minX + region.minX`、`y = screenFrame.maxY - region.maxY`，宽高不变，再 `.standardized`；点击高亮锚点为该框中心 `CGPoint(midX, midY)`。

### 3.8 内容过滤器（录制时排除自身）

`contentFilter(display:content:excludingWindowIDs:)`：
- `excludingWindowIDs` 为空 → `SCContentFilter(display: display, excludingWindows: [])`。
- 非空：找到 `content.applications` 中 `processID == 当前进程` 的 Kiri 应用；若存在 → `SCContentFilter(display:, excludingApplications: [kiri], exceptingWindows: 那些不排除的 Kiri 窗口)`；否则 → `SCContentFilter(display:, excludingWindows: 要排除的窗口)`。**镜像录制后端，保证浮动控制/暂停 UI 不进导出画面**。

---

## 4. EditorWindowController（编辑器窗口）

### 4.1 打开方式

- 入口：截图覆盖层"More"菜单 → `Open in Editor`（`CaptureSessionAction.edit`）→ `AppModel.presentEditor(for:)`。
- `presentEditor`：`EditorWindowController(image: stored.image, completion: { rendered, copy, saveURL in updateStoredCapture(...) }, onClose: { editorController = nil })`；`editorController = controller`；`controller.showWindow(nil)`；`NSApplication.shared.activate(ignoringOtherApps: true)`。

### 4.2 窗口尺寸 / 位置 / 外观

- `NSWindow(contentRect: 0,0,880,620, styleMask: [.titled, .closable, .miniaturizable, .resizable], backing: .buffered, defer: false)`。
- `title = "Kiri Editor"`；`titleVisibility = .hidden`；`titlebarAppearsTransparent = true`；`titlebarSeparatorStyle = .none`。
- `appearance = NSAppearance(named: .darkAqua)`（**强制深色**）。
- `backgroundColor = rgb(0.06, 0.055, 0.09)`（即 #0F0E17 附近，源值以浮点为准：`calibratedRed 0.06, green 0.055, blue 0.09`）。
- `minSize = 860×520`；`window.center()`（居中）。
- `window.delegate = self`；`windowWillClose` 触发 `onClose` 并置 `onClose = nil`（通知 AppModel 释放 `editorController`）。

### 4.3 工具栏布局（顶部，高 58pt）

- 工具栏容器 `toolbarSurface`：高 **58pt**，背景同 `rgb(0.06,0.055,0.09)`，`borderWidth = 1`、`borderColor = white alpha 0.10`。
- 内层 `NSStackView`（horizontal、centerY、`spacing = 5`、`edgeInsets = top9 left13 bottom9 right13`、`detachesHiddenViews = true`）。
- 工具按钮顺序（7 个，`CaptureActionButton`，style `.tool`，keyEquivalent 无修饰键）：
  | 工具 | SF Symbol | 标签 key | key |
  |---|---|---|---|
  | `.select` | `cursorarrow` | `Select (V)` | `v` |
  | `.pen` | `pencil.tip` | `Pen (P)` | `p` |
  | `.rectangle` | `rectangle.dashed` | `Rectangle (R)` | `r` |
  | `.line` | `line.diagonal` | `Line (L)` | `l` |
  | `.arrow` | `arrow.up.right` | `Arrow (A)` | `a` |
  | `.text` | `textformat` | `Text (T)` | `t` |
  | `.mosaic` | `square.grid.3x3.fill` | `Mosaic (M)` | `m` |
- 尺寸控件（`makeSizeControl`）：标题 `Line`（font 10 semibold）；`CaptureTrackingSlider(value:3, min:1, max:16)`，`isContinuous = true`、`controlSize = .small`、宽 **92pt**；值标签初值 `"3 px"`（`monospacedDigitSystemFont 9 medium`、右对齐、宽 **36pt**）。滑块开始/结束回调：文字工具时触发字体大小调整 begin/end。
- 颜色 swatch 组（`AnnotationColorPreset.allCases` 8 色，`AnnotationColorSwatchButton`，`CaptureToolGroupView` 包裹）。
- `Text Background` 按钮（`character.textbox`，tool）与 `Mosaic Strength` 按钮（`square.grid.3x3.fill`，tool）。
- 历史按钮：Undo `arrow.uturn.backward`（`⌘Z`，keyEquivalent `z` + `.command`）、Redo `arrow.uturn.forward`（`⇧⌘Z`，`z` + `[.command,.shift]`），初始禁用；Clear `trash`（secondary，初始禁用）。
- 标题 `Kiri Editor`（font 11 semibold，颜色 `accentSoft alpha 0.72`，居中）。
- `Cancel (Esc)`（`xmark.circle`，secondary，keyEquivalent `"\u{1b}"`）、`Save As…`（`square.and.arrow.down`，secondary）、`Copy`（`doc.on.doc`，**primary**，`showsTitle: true`，keyEquivalent `"\r"`）。

### 4.4 工具上下文 / 尺寸滑块范围

`configureSizeControl(for:)` 按工具切换标题/滑块范围/当前值/单位：

| 工具 | 标题 | min | max | 当前值来源 | 单位 |
|---|---|---|---|---|---|
| `.select` | `Select` | 0 | 1 | 0 | —（空） |
| `.pen` | `Brush` | 1 | 24 | `penWidth` | `px` |
| `.rectangle/.line/.arrow` | `Line` | 1 | 16 | `shapeWidth` | `px` |
| `.text` | `Font` | 12 | 64 | `textFontSize` | `pt` |
| `.mosaic` | `Brush` | 12 | 120 | `mosaicBrushDiameter` | `px` |

值标签显示 `"\(Int(round(value))) \(unit)"`（如 `"3 px"`、`"18 pt"`）。

### 4.5 文本背景 / 马赛克强度菜单

- `Text Background` 菜单（`NSMenu`，`autoenablesItems = false`，弹出于 `sender.bounds.minX, maxY+4`）：
  - `Transparent`（`circle.slash`）→ `.transparent`
  - `Dark`（`moon.fill`）→ `.dark`
  - `Light`（`sun.max.fill`）→ `.light`
  - 当前项 `state = .on`；选中后 `updateTextBackgroundControl()` + `useText()`。
- `Mosaic Strength` 菜单：`Soft`/`Standard`/`Strong`（均 `square.grid.3x3.fill`）→ `selectMosaicIntensity(.soft/.standard/.strong)`；选中后 `useMosaic()`。

### 4.6 关闭 / 发布（Copy / Save）流程

- `cancel()` → `close()`（Esc 或取消按钮）。
- `copyImage()`：`canvas.renderedImage()` 为空返回；`completion(image, true, nil)` 然后 `close()`。
- `save()`：`NSSavePanel`（`allowedContentTypes = [.png]`、`nameFieldStringValue = "kiri-<timestamp>.png"`），`runModal() == .OK` 且取到 `url` 且 `renderedImage()` 非空 → `completion(image, false, url)` 然后 `close()`。
- `completion` 回调进 `AppModel.updateStoredCapture`：
  1. 把渲染后 `CGImage` 编码 PNG；
  2. `library.replaceData(data, for: stored.asset.id)`（**覆盖原库内 PNG**）；
  3. 若 `saveURL != nil` → `data.write(to: saveURL, options: [.atomic])`；
  4. 若 `copyToClipboard` → 写剪贴板（失败置 `CaptureExportError.clipboardWriteFailed`，成功 `showNotice("Copied to Clipboard", …)`）；
  5. `await refresh()`。

---

## 5. PinnedImageController（置顶图片）

### 5.1 面板属性

- `NSPanel(contentRect: size, styleMask: [.borderless, .resizable, .nonactivatingPanel], backing: .buffered, defer: false)`。
- `level = .floating`；`appearance = .darkAqua`；`collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]`（跨所有 Space + 全屏辅助窗口）。
- `backgroundColor = .clear`、`isOpaque = false`、`hasShadow = true`、`isReleasedWhenClosed = false`、`hidesOnDeactivate = false`（**失活不隐藏**）、`isMovableByWindowBackground = true`。
- `contentMinSize = 140×90`；`contentView = PinnedImageView`；`setFrameOrigin(Self.origin(for: size))`；`orderFrontRegardless()`。

### 5.2 初始尺寸（`initialSize`）

- 图像宽高 ≤ 0 → `360×240`。
- 否则 `maximum = 520×420`；`scale = min(520/w, 420/h, 1)`；返回 `width = max(180, w*scale)`、`height = max(120, h*scale)`。即**最长边不超过 520/420，短边下限 180/120，绝不放大于原图（scale ≤ 1）**。

### 5.3 初始位置（`origin`）

- `mouse = NSEvent.mouseLocation`；`screen = 第一个 NSMouseInRect(mouse, frame, false) ?? NSScreen.main`；`frame = screen.visibleFrame ?? (0,0,900,700)`。
- 居中于鼠标：`x = clamp(mouse.x - size.width/2, frame.minX+12, frame.maxX - size.width - 12)`；`y` 同理。即贴图中心对齐鼠标，距屏幕可见区边 **12pt** 内夹取。

### 5.4 外观与交互

- `PinnedImageView`（NSView）：`layer.cornerRadius = 16`、`cornerCurve = .continuous`、`masksToBounds = true`、背景 `rgb(0.06,0.055,0.09) alpha 0.98`、`borderWidth = 1`、`borderColor = white alpha 0.16`。
- 内层 `PinnedContentImageView`：`imageScaling = .scaleProportionallyUpOrDown`，四周 inset **7pt**。
- 关闭按钮：`xmark.circle.fill`、`bezelStyle = .inline`、`imagePosition = .imageOnly`、`isBordered = false`、`contentTintColor = .white`、`cornerRadius = 10`、背景 `black alpha 0.58`、尺寸 **24×24**、位于 `top 10` / `trailing -10`。
- 拖拽：`mouseDown` → `window?.performDrag(with: event)`（内容视图与图片视图都可拖）。
- **透明度**：代码中**没有可调透明度/不透明度控件**。面板整体 `backgroundColor = .clear`、内容容器不透明度固定 `alpha 0.98`、`appearance .darkAqua`；用户只能缩放（`resizable`）、拖动、关闭。没有快捷键或菜单调整透明度。

---

## 6. 设计系统（KiriDesignSystem + CaptureUIStyle + ADR 0004）

### 6.1 kawaii-professional 视觉体系规则（ADR 0004）

- 浅色模式：白色画布 + 白色抬升表面；深色模式：李紫炭色（plum-charcoal）表面。
- 主色薰衣草紫（lavender，`accent`）用于主操作；天蓝（sky blue，`cyan`）用于清新点缀；桃粉（peach pink，`blossom`/`coral`）仅用于暖色强调或破坏性状态。
- 圆角几何、细边框、柔和阴影，同时保留原生 macOS 材质与控件。
- 应用图标：紫蓝色头发、取景框元素的 chibi 少女；应用内品牌标记使用同一 chibi 插画（而非通用取景框 glyph）。
- 截图/OCR 覆盖层使用紧凑深色材质（HUD），保证在任意屏幕内容上可读；OCR 结果面板为浅色 + 深色可编辑文本以保证对比度。
- 永久删除使用自定义应用内确认 sheet（而非系统 action sheet）。
- 可爱细节集中在图标、品牌标记、渐变与空状态；密集工作区优先可读性，**不用装饰性字符画**。

### 6.2 设计 token（KiriUI）

- **间距 `KiriUI.Spacing`**（pt）：`tight 6`、`compact 10`、`standard 14`、`roomy 20`、`page 24`。
- **圆角 `KiriUI.Radius`**（pt）：`control 11`、`badge 9`、`preview 14`、`card 18`、`surface 24`。
- **头部 `KiriUI.Header`**（pt）：`searchWidth 228`、`sectionPickerWidth 176`、`controlHeight 36`。
- **卡片 `KiriUI.Card`**（pt）：`thumbnailHeight 184`、`padding 12`、`actionSpacing 8`、`metadataSpacing 7`。
- **动画时长 `KiriUI.Motion`**（秒）：`hover 0.14`、`feedback 0.20`。
- **渐变**：
  - `brandGradient` = `LinearGradient([accentStrong, accent, cyan], topLeading → bottomTrailing)`。
  - `warmGradient` = `LinearGradient([coral, accent], topLeading → bottomTrailing)`。
- **品牌标记 `KiriBrandMark`**：默认 `size = 38`；图像插值 `.high`、`scaledToFill`；兜底 SF Symbol `viewfinder`（`system(size: size*0.42, weight: .bold)`，白，`brandGradient` 背景）；`clipShape(RoundedRectangle(cornerRadius: size*0.3))`；描边 `border.opacity(0.9)` 1pt；阴影 `accent.opacity(0.24)` radius 10 y 4；`accessibilityHidden(true)`。
- **符号标记 `KiriSymbolMark`**：同品牌标记，但 SF Symbol 固定传入；描边 `white.opacity(0.24)`。
- **主按钮 `KiriPrimaryButtonStyle`**：font `system(size: 12.5, weight: .semibold)`；白字；`padding(.horizontal, 14)`；`minHeight 36`；`brandGradient` + `control` 圆角；描边 `white.opacity(0.2)` 1pt；阴影 `accent.opacity(pressed ? 0.12 : 0.24)`、radius `pressed ? 4 : 10`、y `pressed ? 1 : 4`；`scaleEffect(pressed ? 0.97 : 1)`；`saturation(isEnabled ? 1 : 0.18)`；`opacity(isEnabled ? (pressed ? 0.92 : 1) : 0.48)`；动画 `easeOut(duration: hover)` 绑 `isPressed`/`isEnabled`。
- **表面 `KiriSurfaceModifier`**：背景 `elevated ? elevated : card`；圆角可传（默认 `card`）；描边 `border` 1pt；阴影 `black.opacity(elevated ? 0.10 : 0.045)`、radius `elevated ? 18 : 8`、y `elevated ? 8 : 3`。

### 6.3 CaptureUIColors 完整色板

> 数值以 Swift 源里的 `calibratedRed/Green/Blue` 浮点为准（精确到 2 位小数）；`#HEX` 为按 255 四舍五入的近似，供 Tauri 复刻使用。

| 语义名 | 源值 (R,G,B) | 近似 HEX | 说明 |
|---|---|---|---|
| `accent` | (0.49, 0.41, 0.96) | `#7D69F5` | 主色薰衣草紫 |
| `accentStrong` | (0.39, 0.31, 0.86) | `#634FDB` | 主色深紫（主按钮/选中段底色） |
| `blossom` | (1.00, 0.50, 0.66) | `#FF80A8` | 桃粉（`coral` 别名） |
| `cyan` | (0.31, 0.75, 0.94) | `#4FBFF0` | 天蓝 |
| `accentSoft` | (0.67, 0.58, 1.00) | `#AB94FF` | 淡紫（编辑器标题等） |
| `label` | `NSColor.labelColor` | 系统 label | 一级文字 |
| `secondaryLabel` | `NSColor.secondaryLabelColor` | 系统 secondary | 二级文字 |
| `disabledLabel` | `NSColor.tertiaryLabelColor` | 系统 tertiary | 禁用文字 |
| `hoverFill` | `accent` alpha **0.10** | — | hover 底色 |
| `selectedFill` | `accent` alpha **0.18** | — | 选中底色 |
| `divider` | `separatorColor` alpha **0.55** | — | 分隔线 |
| `canvas` | 动态 浅 `0xFFFFFF` / 深 `0x15131D` | `#FFFFFF` / `#15131D` | 画布 |
| `card` | 动态 浅 `0xFFFFFF` / 深 `0x1E1B28` | `#FFFFFF` / `#1E1B28` | 卡片 |
| `elevated` | 动态 浅 `0xFFFFFF` / 深 `0x282334` | `#FFFFFF` / `#282334` | 抬升表面 |
| `surfaceBorder` | 动态 浅 `0xE5DFF0` / 深 `0x40394E` | `#E5DFF0` / `#40394E` | 边框 |
| `groupFill` | 动态 浅 `0xF3EFF9` / 深 `0x302A3D` | `#F3EFF9` / `#302A3D` | 分组填充 |
| `surfaceShadow` | `black` alpha **0.20** | — | 表面阴影色 |

动态色规则 `dynamic(light:dark:)`：`NSColor(name:nil) { appearance in appearance.bestMatch([.darkAqua,.aqua]) == .darkAqua ? dark : light }`——深色外观取深色值，否则浅色值。

### 6.4 标注颜色预设（AnnotationColorPreset）

| 预设 | 名称 key | 源值 (R,G,B) | 近似 HEX |
|---|---|---|---|
| `.violet` | `Violet` | = `accent` (0.49,0.41,0.96) | `#7D69F5` |
| `.cherry` | `Cherry` | (0.98, 0.28, 0.43) | `#FA476E` |
| `.orange` | `Orange` | (1.00, 0.49, 0.18) | `#FF7D2E` |
| `.yellow` | `Yellow` | (1.00, 0.82, 0.16) | `#FFD129` |
| `.mint` | `Mint` | (0.16, 0.78, 0.56) | `#29C78F` |
| `.blue` | `Blue` | (0.16, 0.58, 1.00) | `#2994FF` |
| `.white` | `White` | `.white` | `#FFFFFF` |
| `.black` | `Black` | `calibratedWhite 0.08` | `#141414` |

文本背景 `AnnotationTextBackgroundStyle`：`.transparent` → 无填充；`.dark` → `black alpha 0.72`；`.light` → `white alpha 0.90`。

马赛克强度 `MosaicIntensityPreset`（`viewBlockSize`，块像素）：`.soft 7`、`.standard 12`、`.strong 20`。

### 6.5 字体与字号

- 主按钮/segment 标题：`system 12.5 semibold`（SwiftUI）/ `system 12 semibold`（AppKit attributedTitle）。
- `CaptureActionButton`：SF Symbol `pointSize 13 semibold`；标题 `system 12 medium`（有标题时 attributed `12 semibold`）；默认尺寸 **32×32**（`showsTitle` 时 **78×32**）；圆角 **10**。
- `CaptureModeSegmentButton`：symbol `pointSize 12 semibold`；标题 `system 12 semibold`；圆角 **10**；高 **32**、最小宽 **92**。
- `CaptureHintLabel`：`system 11 medium` 白字。
- 覆盖层滑块（`makeSizeSlider`）：`controlSize = .mini`、宽 **76**；值标签 `monospacedDigitSystemFont 9 medium`、宽 **28**。
- 覆盖层 context 图标：`symbolConfiguration pointSize 11 semibold`、16×16。
- 覆盖层 `NSSegmentedControl`：`segmentStyle = .capsule`、`controlSize = .mini`、`font system 9 medium`；文本背景三段宽 `[26,26,26]`，马赛克强度三段宽 `[24,24,24]`。
- 覆盖层提示/徽章：`system 11 medium`（尺寸徽章 `monospacedSystemFont 11 medium`）；初始提示 `system 12 medium` 白。
- 编辑器：工具栏按钮 symbol `13 semibold`；标题 `11 semibold`；尺寸控件标题 `10 semibold`；值标签 `monospacedDigit 9 medium`。
- 缩略图 fixture 标题栏文字 `system 14 semibold`。

### 6.6 圆角 / 边框 / 阴影 / 动画（组件级）

- 覆盖层模式控件 & 标注工具条（`NSVisualEffectView`，material `.hudWindow`、blending `.withinWindow`、state `.active`、appearance `.darkAqua`）：圆角 **13**（continuous）；边框 1pt `white alpha 0.14`。模式控件阴影 `black opacity 0.2` radius **8** offset `(0,3)`；工具条阴影 `black opacity 0.24` radius **12** offset `(0,5)`。
- `CaptureToolGroupView`：圆角 **11**（continuous），内容四周 inset **2**，背景 `groupFill`，边框 1pt `surfaceBorder alpha 0.55`。
- `CaptureHintLabel`：胶囊圆角 **9**；填充 `black alpha 0.76`；描边 1pt `white alpha 0.16`；内边距按文本计算（宽 `textW+20`、高 `textH+9`）。
- `CaptureActionButton`：圆角 **10**；primary 边框 1pt `white alpha 0.22`、阴影 `accentStrong` opacity **0.25** radius **7** offset `(0,3)`；tool 选中边框 1pt `accent alpha 0.32`；按下 `CATransform3DMakeScale(0.94, 0.94, 1)`。
- `AnnotationColorSwatchButton`：frame **22×28**、圆角 **8**；选中时底色 `swatch alpha 0.2` + 边框 **1.5**pt `swatch` + 圆形直径 **12**；未选中 hover 底色 `hoverFill`、圆形直径 **10**、描边 `black alpha 0.18` 0.75pt。
- `CaptureDividerView`：默认 **1×24**（宽 1pt）。
- 选区/窗口外框（覆盖层 `draw`）：无选区 hover 窗口 → 黑色遮罩 `alpha 0.34`、窗口边框 `accent alpha 0.92` 2pt；有选区 → 遮罩 `alpha 0.48`、白框 `alpha 0.92` 3pt（标注阶段 4pt）+ 内层 `accent` 1.5pt（标注阶段 2pt）。
- 8 个缩放手柄：外圆 **10×10** 白底、内圆 inset 2pt（直径 6）`accent`。
- 尺寸徽章（`drawBadge`）：胶囊（圆角 = 高/2）、`black alpha 0.76` 填充、`white alpha 0.16` 1pt 描边；`drawDimensions` 文字 `"W × H"`（`Int(selection.width) × Int(selection.height)`），位置在选区左上，越界夹到 `minX+6`，上方放不下放内部 `+6`。
- 通知 `AppNotice` 自动消失 **2 秒**；hover 动画 0.14s、feedback 0.20s。

### 6.7 图标风格

- 全部使用 **SF Symbols**，`weight` 一般为 `.semibold` 或 `.medium`。主要符号：`viewfinder`（菜单栏/品牌兜底）、`camera.viewfinder`（Screenshot）、`record.circle`（Record）、`text.viewfinder`（OCR）、`pencil.tip`/`rectangle.dashed`/`line.diagonal`/`arrow.up.right`/`textformat`/`square.grid.3x3.fill`（标注工具）、`cursorarrow`（Select）、`arrow.uturn.backward/forward`（undo/redo）、`trash`/`trash.fill`/`trash.slash`（删除/清空）、`doc.on.doc`/`doc.on.clipboard.fill`（复制）、`square.and.arrow.down`（另存）、`pin`（置顶）、`slider.horizontal.3`（编辑）、`xmark`/`xmark.circle`（取消）、`checkmark`（完成）、`checkmark.circle.fill`（复制成功）、`pause.circle.fill`/`play.circle.fill`/`stop.circle.fill`（录制控制）、`record.circle.fill`（录制开始）、`video.fill`（录制保存）、`photo.on.rectangle`（打开库）、`power`（退出）、`hourglass`（准备中）、`sparkles.rectangle.stack`（GIF）、`crop`（重选区域）、`character.textbox`（文本背景）、`moon.fill`/`sun.max.fill`/`circle.slash`（深/浅/透明背景）、`lineweight`（线宽）、`ellipsis.circle`（更多）。
- 品牌标记优先加载应用内 `kiri-icon.png`（或环境变量 `KIRI_BRAND_ICON_PATH` 指定路径），兜底 `NSImage.applicationIconName`。

---

## 7. L10n.swift 与本地化

### 7.1 机制

- `L10n.text(key, fallback: nil)` = `Bundle.main.localizedString(forKey: key, value: fallback ?? key, table: nil)`。
  - 关键行为：**key 找不到翻译时返回 key 本身**（即英文 key 即回退值）。这就是为何 `Localizable.strings` 里英文条目 `"X" = "X"` 与 key 相同。
- `L10n.format(key, _ args...)` = `String(format: text(key), locale: Locale.current, arguments: args)`（带参数模板）。
- 语言选择：依赖 macOS 系统首选语言 + 标准 `.lproj` 解析（`en.lproj` / `zh-Hans.lproj`）。**无自定义语言检测、无应用内切换**；跟随 `Bundle.main` 的本地化解析（AppleLanguages 优先顺序）。
- 资源文件：`Sources/KiriApp/Resources/en.lproj/Localizable.strings`、`zh-Hans.lproj/Localizable.strings`；`InfoPlist.strings`（en/zh-Hans 各一份）。

### 7.2 全部 Localizable 键（en / zh-Hans 对照，共 210 条）

> 以下逐条列出 key 与两种语言的值。key 本身即英文回退值（en 值等于 key）。`%@` 为 `String(format:)` 占位符（`Locale.current`）。

| key（= en 值） | zh-Hans 值 |
|---|---|
| `Capture` | 截图 / 录屏 |
| `Resume Recording` | 继续录制 |
| `Pause Recording` | 暂停录制 |
| `Stop and Save  %@` | 停止并保存  %@ |
| `Find in Library` | 在素材库中查找 |
| `Finalizing Recording…` | 正在处理录屏… |
| `Preparing Capture…` | 正在准备… |
| `Capture  %@` | 截图 / 录屏  %@ |
| `Open Library` | 打开素材库 |
| `Quit Kiri` | 退出 Kiri |
| `Library` | 素材库 |
| `Trash` | 回收站 |
| `Section` | 分类 |
| `Library section` | 素材库分类 |
| `Preparing…` | 正在准备… |
| `Capture or record a region, with optional annotation tools` | 截取或录制区域，并可直接添加标注 |
| `Dismiss` | 关闭 |
| `Search captures` | 搜索截图与录屏 |
| `Clear Search` | 清除搜索 |
| `Loading Library…` | 正在加载素材库… |
| `No matching captures` | 没有匹配的内容 |
| `Try a different search, or clear the current one.` | 换个关键词试试，或清除当前搜索。 |
| `Trash is empty` | 回收站是空的 |
| `Captures you delete stay recoverable here.` | 删除的截图与录屏会暂存在这里，仍可恢复。 |
| `Ready for your first capture` | 开始第一次截图或录屏 |
| `Choose Screenshot or Record, then select the region you need.` | 选择截图或录屏，再框选你需要的区域。 |
| `or press  %@` | 或按  %@ |
| `Mode` | 模式 |
| `Screenshot or Record` | 截图或录屏 |
| `Select` | 框选 |
| `Choose a region` | 选择区域 |
| `Finish` | 完成 |
| `Copy or save` | 复制或保存 |
| `%d capture` | %d 项内容 |
| `%d captures` | %d 项内容 |
| `Double-click to open` | 双击打开 |
| `Remove Favorite` | 取消收藏 |
| `Favorite` | 收藏 |
| `Move to Trash` | 移到回收站 |
| `Restore` | 恢复 |
| `Delete Permanently` | 永久删除 |
| `Empty Trash` | 清空回收站 |
| `Close` | 关闭 |
| `Empty Trash?` | 清空回收站？ |
| `All captures in Trash will be permanently deleted. This cannot be undone.` | 回收站中的所有内容将被永久删除，此操作无法撤销。 |
| `Trash Emptied` | 回收站已清空 |
| `Permanently delete all captures in Trash` | 永久删除回收站中的所有内容 |
| `Copy` | 复制 |
| `Open` | 打开 |
| `Show in Finder` | 在访达中显示 |
| `Convert to GIF` | 转换为 GIF |
| `Converting to GIF…` | 正在转换为 GIF… |
| `Delete this capture permanently?` | 要永久删除这项内容吗？ |
| `Cancel` | 取消 |
| `This cannot be undone.` | 此操作无法撤销。 |
| `More Actions` | 更多操作 |
| `Record Region` | 区域录屏 |
| `MP4 · 30 fps · Saved locally` | MP4 · 30 帧/秒 · 保存在本地 |
| `3-second countdown` | 3 秒倒计时 |
| `System audio` | 系统声音 |
| `Microphone` | 麦克风 |
| `Requires macOS 15` | 需要 macOS 15 |
| `Show pointer` | 显示鼠标指针 |
| `Highlight clicks` | 显示点击轨迹 |
| `Start Recording` | 开始录制 |
| `Recording Controls` | 录屏控制 |
| `Paused` | 已暂停 |
| `Preparing recording` | 正在准备录制 |
| `Stop and Save Recording` | 停止并保存录屏 |
| `Stop Recording` | 停止录制 |
| `Esc to cancel` | 按 Esc 取消 |
| `Screenshot` | 截图 |
| `Record` | 录屏 |
| `OCR` | 文字 |
| `Recognize Text` | 识别文字 |
| `Capture mode` | 截图或录屏模式 |
| `Drag to choose text to recognize   ·   Esc to cancel` | 拖动框选要识别的文字区域   ·   Esc 取消 |
| `Release to recognize text` | 松开以识别文字 |
| `Recognizing Text…` | 正在识别文字… |
| `Recognized Text` | 识别结果 |
| `Text Copied` | 文字已复制 |
| `No Text Found` | 未找到文字 |
| `Text Recognition Failed` | 文字识别失败 |
| `Try a larger region or clearer text` | 试试框选更大的区域，或选择更清晰的文字 |
| `Adjust the region and try again` | 调整识别区域后再试一次 |
| `%d lines · %d chars` | %d 行 · %d 字 |
| `Cancel (Esc)` | 取消 (Esc) |
| `Cancel capture · Esc` | 取消截图 · Esc |
| `Select (V)` | 选择 (V) |
| `Select and edit annotations (V)` | 选择并编辑标注 (V) |
| `Pen (P)` | 画笔 (P) |
| `Pen (P) — Draw freehand` | 画笔 (P) — 自由绘制 |
| `Rectangle (R)` | 矩形 (R) |
| `Rectangle (R) — Draw a box` | 矩形 (R) — 绘制方框 |
| `Line (L)` | 直线 (L) |
| `Line (L) — Connect two points` | 直线 (L) — 连接两点 |
| `Arrow (A)` | 箭头 (A) |
| `Arrow (A) — Point something out` | 箭头 (A) — 指向重点 |
| `Text (T)` | 文字 (T) |
| `Text (T) — Click the image, type, then press Return` | 文字 (T) — 点击画面输入，按 Return 完成 |
| `Mosaic (M)` | 马赛克 (M) |
| `Mosaic (M) — Hide sensitive content` | 马赛克 (M) — 遮挡敏感内容 |
| `Undo (⌘Z)` | 撤销 (⌘Z) |
| `Undo the last annotation · ⌘Z` | 撤销上一步标注 · ⌘Z |
| `Redo (⇧⌘Z)` | 重做 (⇧⌘Z) |
| `Redo the last annotation · ⇧⌘Z` | 重做上一步标注 · ⇧⌘Z |
| `Done (Return)` | 完成 (Return) |
| `Done — Copy to clipboard · Return` | 完成并复制到剪贴板 · Return |
| `More — Save, pin, edit, or clear` | 更多 — 保存、贴图、编辑或清除 |
| `Annotation line width` | 标注线条粗细 |
| `Stroke size` | 线条粗细 |
| `Text font size` | 文字大小 |
| `Transparent background` | 透明背景 |
| `Dark background` | 深色背景 |
| `Light background` | 浅色背景 |
| `Text background` | 文字背景 |
| `Transparent` | 透明 |
| `Dark` | 深色 |
| `Light` | 浅色 |
| `Text options` | 文字设置 |
| `Mosaic brush size` | 马赛克笔刷大小 |
| `Mosaic strength` | 马赛克强度 |
| `Mosaic brush` | 马赛克笔刷 |
| `Reselect Region` | 重新框选区域 |
| `Save As…` | 另存为… |
| `Pin on Screen` | 贴在屏幕上 |
| `Open in Editor` | 在编辑器中打开 |
| `Clear Annotations` | 清除标注 |
| `Release for recording settings` | 松开以设置录屏 |
| `Release to show tools` | 松开以显示工具 |
| `Adjust the region · Recording settings below` | 调整区域 · 下方可设置录屏 |
| `Drag handles to resize · Drag inside to move` | 拖动控制点调整大小 · 拖动区域内部移动 |
| `Drag to choose a capture area   ·   Click a window   ·   Esc to cancel` | 拖动选择截图区域   ·   单击选择窗体   ·   Esc 取消 |
| `Drag to choose a recording area   ·   Click a window   ·   Esc to cancel` | 拖动选择录屏区域   ·   单击选择窗体   ·   Esc 取消 |
| `Text Background` | 文字背景 |
| `Mosaic Strength` | 马赛克强度 |
| `Line` | 线条 |
| `Tool size` | 工具大小 |
| `Text background: %@` | 文字背景：%@ |
| `Mosaic strength: %@` | 马赛克强度：%@ |
| `Brush` | 笔刷 |
| `Font` | 字号 |
| `Violet` | 紫色 |
| `Cherry` | 樱桃红 |
| `Orange` | 橙色 |
| `Yellow` | 黄色 |
| `Mint` | 薄荷绿 |
| `Blue` | 蓝色 |
| `White` | 白色 |
| `Black` | 黑色 |
| `Soft` | 轻度 |
| `Standard` | 标准 |
| `Strong` | 强力 |
| `Type something…` | 输入文字… |
| `Annotation text` | 标注文字 |
| `Annotation color: %@` | 标注颜色：%@ |
| `Close` | 关闭 |
| `Open Settings` | 打开系统设置 |
| `Open Accessibility Settings` | 打开辅助功能设置 |
| `Open Input Monitoring Settings` | 打开输入监控设置 |
| `Open Microphone Settings` | 打开麦克风设置 |
| `Recording Started` | 录制已开始 |
| `Recording Paused` | 录制已暂停 |
| `Recording Resumed` | 已继续录制 |
| `Recording Saved` | 录屏已保存 |
| `Moved to Trash` | 已移到回收站 |
| `Restored to Library` | 已恢复到素材库 |
| `Deleted Permanently` | 已永久删除 |
| `The capture file is unavailable.` | 这项内容的文件不可用。 |
| `Could not copy the capture.` | 无法复制这项内容。 |
| `Copied to Clipboard` | 已复制到剪贴板 |
| `GIF Created` | GIF 已生成 |
| `Could not encode the capture as PNG.` | 无法将截图编码为 PNG。 |
| `Saved` | 已保存 |
| `Using a temporary library: %@` | 正在使用临时素材库：%@ |
| `Microphone access is off. Enable it in System Settings to record your voice.` | 麦克风权限未开启。请在系统设置中允许 Kiri 使用麦克风。 |
| `Could not copy the capture to the clipboard.` | 无法将截图复制到剪贴板。 |
| `Enable Kiri in Accessibility settings, then quit and reopen it to reserve ⇧⌘A exclusively.` | 请在辅助功能设置中启用 Kiri，然后退出并重新打开，以独占 ⇧⌘A 快捷键。 |
| `Enable Kiri in Input Monitoring settings, then quit and reopen it to reserve ⇧⌘A exclusively.` | 请在输入监控设置中启用 Kiri，然后退出并重新打开，以独占 ⇧⌘A 快捷键。 |
| `Kiri could not create the exclusive ⇧⌘A keyboard filter. Check Input Monitoring and Accessibility, then quit and reopen Kiri.` | Kiri 无法独占 ⇧⌘A。请检查输入监控和辅助功能权限，然后退出并重新打开 Kiri。 |
| `Screen Recording access was granted. Quit and reopen Kiri once to finish enabling capture.` | 屏幕录制权限已开启。请退出并重新打开 Kiri，以完成启用。 |
| `Screen Recording is off. Enable Kiri in System Settings, then quit and reopen it once.` | 屏幕录制权限未开启。请在系统设置中启用 Kiri，然后退出并重新打开。 |
| `The active display could not be captured.` | 无法截取当前显示器。 |
| `The selected display is no longer available.` | 所选显示器已不可用。 |
| `The recording region is too small.` | 录屏区域太小。 |
| `Kiri could not prepare the MP4 encoder.` | Kiri 无法准备 MP4 编码器。 |
| `The recording ended before a complete frame arrived.` | 录制在收到完整画面前已结束。 |
| `The MP4 could not be finalized: %@` | 无法完成 MP4 文件：%@ |
| `No recording segments are available.` | 没有可用的录屏片段。 |
| `Kiri could not prepare the paused recording for export.` | Kiri 无法准备导出暂停后的录屏。 |
| `Kiri could not prepare the final MP4 export.` | Kiri 无法准备最终的 MP4 导出。 |
| `The paused recording could not be merged: %@` | 无法合并暂停后的录屏：%@ |
| `The video track could not be read.` | 无法读取视频轨道。 |
| `The video duration is unavailable.` | 无法获取视频时长。 |
| `GIF conversion currently supports videos up to 15 seconds.` | GIF 转换目前仅支持不超过 15 秒的视频。 |
| `Kiri could not create the GIF file.` | Kiri 无法创建 GIF 文件。 |
| `Kiri could not extract a video frame.` | Kiri 无法提取视频画面。 |
| `The GIF could not be finalized.` | 无法完成 GIF 文件。 |
| `The capture could not be found.` | 找不到这项内容。 |
| `The capture filename is invalid.` | 这项内容的文件名无效。 |
| `Unknown export error` | 未知导出错误 |
| `Unknown encoder error` | 未知编码错误 |
| `Frame append failed` | 写入画面失败 |
| `Audio append failed` | 写入音频失败 |
| `Choose a capture mode, then select the region you need.` | 选择一种截取方式，再框出你需要的区域。 |
| `Screenshot · Record · OCR` | 截图 · 录屏 · 识字 |
| `Image` | 图片 |
| `Video` | 视频 |
| `Saved locally · Never uploaded` | 仅保存在本机 · 不会上传 |
| `Kiri Editor` | Kiri 编辑器 |

> 注意：key 中有两条 `Close`（一条 UI 通用、一条权限恢复区），值相同，翻译后均为 `关闭`；源 `.strings` 文件里确实出现两次。

### 7.3 InfoPlist.strings（4 条）

| key | en | zh-Hans |
|---|---|---|
| `CFBundleDisplayName` | `Kiri` | `Kiri` |
| `NSScreenCaptureUsageDescription` | `Kiri needs screen access to capture or record the region you choose.` | `Kiri 需要访问屏幕，才能截取或录制你选择的区域。` |
| `NSInputMonitoringUsageDescription` | `Kiri uses keyboard access only to reserve ⇧⌘A as its exclusive capture shortcut.` | `Kiri 仅使用键盘访问权限，将 ⇧⌘A 设为专属截图与录屏快捷键。` |
| `NSMicrophoneUsageDescription` | `Kiri records microphone audio only when you enable the Microphone switch before recording.` | `仅当你在录屏前打开“麦克风”开关时，Kiri 才会录制麦克风声音。` |

---

## 8. 隐式行为（通知 / 偏好 / 路径 / 错误处理）

### 8.1 全局快捷键（`GlobalShortcutMonitor`）

- 独占快捷键：`⇧⌘A`（`keycode == kVK_ANSI_A`，即 `0`）。
- 判定 `isKiriCaptureEvent`：keycode 为 A，且 `event.flags.intersection([.maskCommand,.maskShift,.maskControl,.maskAlternate]) == [.maskCommand,.maskShift]`（**恰好 ⇧⌘，不允许 ⌃/⌥ 同时按下**）。
- Event tap：`CGEvent.tapCreate(tap: .cgSessionEventTap, place: .headInsertEventTap, options: .defaultTap, eventsOfInterest: keyDown|keyUp)`；加入主 RunLoop `.commonModes`；`tapEnable(true)`。
- 触发条件：`type == .keyDown` 且 `keyboardEventAutorepeat == 0`（忽略长按自动重复）→ MainActor 执行 `startCapture()`；匹配的事件回调**返回 `nil`（吞掉事件）**，其它事件原样透传。
- `tapDisabledByTimeout` / `tapDisabledByUserInput` → 自动 `reenableEventTap()`。
- 启动失败错误（`GlobalShortcutError`）：`inputMonitoringPermissionRequired`（`CGPreflightListenEventAccess() || CGRequestListenEventAccess()` 为假）、`accessibilityPermissionRequired`（tap 创建失败且 `AXIsProcessTrustedWithOptions` 为假）、`eventTapCreationFailed`。

### 8.2 UserDefaults / 偏好

- 唯一持久化偏好：`RecordingPreferences`，键 **`recording.options.v1`**，值 = `JSONEncoder` 编码的 `RecordingOptions.normalized`（`Data`）。读取失败/缺失 → 返回 `RecordingOptions()`（默认值）。
- `RecordingOptions` 默认值：`usesCountdown = true`、`capturesSystemAudio = false`、`capturesMicrophone = false`、`showsCursor = true`、`highlightsClicks = false`。
- `normalized`：`!showsCursor → highlightsClicks = false`（不显示指针则强制关闭点击高亮）。
- 无其它 `UserDefaults`/`@AppStorage` 持久化（搜索词、回收站开关、窗口尺寸等均不持久化）。

### 8.3 通知（`AppNotice`）

- `AppNotice { let id = UUID(); let title: String; let symbol: String }`，`Identifiable & Equatable`。
- `showNotice(title:symbol:)`：置 `notice = AppNotice(...)`；`Task { sleep 2s; if notice?.id == 该 id { notice = nil } }`（**2 秒后若仍是同一条则消失**，新通知会替换旧通知并中止旧计时）。
- `dismissNotice()`：立即 `notice = nil`。
- 触发点（title → symbol）：`Recording Started`→`record.circle.fill`；`Recording Paused`→`pause.circle.fill`；`Recording Resumed`→`play.circle.fill`；`Recording Saved`→`video.fill`；`Copied to Clipboard`→`checkmark.circle.fill`；`Moved to Trash`→`trash`；`Restored to Library`→`arrow.uturn.backward`；`Deleted Permanently`→`trash.fill`；`Trash Emptied`→`trash.slash`；`GIF Created`→`sparkles.rectangle.stack`；`Saved`→`checkmark.circle.fill`；`Text Copied`→`doc.on.clipboard.fill`。

### 8.4 文件路径约定

- 默认素材库根目录：`FileManager.default.url(for: .applicationSupportDirectory, in: .userDomainMask, …)` + `/kiri` = **`~/Library/Application Support/kiri/`**。
- 结构：`Assets/`（原始文件）、`Thumbnails/`（缩略图）、`library.json`（索引）。
- 素材文件名：`yyyyMMdd-HHmmss-<UUID小写>.<ext>`（`DateFormatter` `locale = en_US_POSIX`、`dateFormat = "yyyyMMdd-HHmmss"`）；`createdAt` 归一化到毫秒（`(t*1000).rounded()/1000`）。
- 缩略图名：`<UUID小写>.jpg`。
- 截图 PNG：`kind = .image`、扩展名 `png`；录制 MP4：`kind = .video`、扩展名 `mp4`；GIF：`kind = .gif`、扩展名 `gif`。
- 另存默认名：`kiri-yyyyMMdd-HHmmss.png`（`CaptureFilename.timestamp()`）。
- 库初始化失败时回退到 `FileManager.default.temporaryDirectory/kiri-library/`，并设 warning（`Using a temporary library: %@`）；若回退也失败 → `preconditionFailure("kiri could not create its local capture library")`。
- `CaptureKind` 解码兼容旧值 `"longImage"` → 映射为 `.image`。

### 8.5 环境变量 / 调试开关（仅 DEBUG）

- `KIRI_LIBRARY_ROOT`：覆盖素材库根目录（仅 `#if DEBUG`）。
- `KIRI_CAPTURE_FIXTURE == "1"` 或参数 `--capture-fixture`：使用合成截图 fixture。
- `KIRI_BRAND_ICON_PATH`：覆盖品牌图标图片路径（非 DEBUG 也生效，见 `KiriBrandArtwork`）。

### 8.6 错误处理汇总

- 所有用户可见错误通过 `errorMessage`（`@Published`）呈现；其 `didSet` 在值变化时清空 `capturePermissionRecoveryAction`。
- `errorMessage` 展示位置：菜单栏 `MenuBarView`（`.foregroundStyle(.secondary)`、`lineLimit(3)`），若 `capturePermissionRecoveryLabel` 非空则附加恢复按钮。
- 权限类错误设置 `capturePermissionRecoveryAction`（见 2.8）；普通错误不设。
- 录制与 GIF 转换的临时文件在 `defer`/失败分支中 `try? FileManager.default.removeItem` 清理。
- `copyRecognizedText`：`trimmingCharacters(in: .whitespacesAndNewlines)` 为空则**静默返回（不提示、不复制）**。

### 8.7 覆盖层窗口细节（补充行为）

- `CaptureOverlayWindow`：`styleMask .borderless`、`level .screenSaver`、`backgroundColor .clear`、`isOpaque false`、`hasShadow false`、`isReleasedWhenClosed false`、`acceptsMouseMovedEvents true`、`collectionBehavior [.canJoinAllSpaces, .fullScreenAuxiliary]`。
- Escape（keyCode `53`）在 `sendEvent` 与 `cancelOperation` 中都触发 `onCancel`。
- 覆盖层 `present`：`NSApplication.shared.activate(ignoringOtherApps: true)` + `makeKeyAndOrderFront` + `makeFirstResponder(sessionView)` + `NSCursor.crosshair.set()`；`close()` 时 `NSCursor.arrow.set()`。
- Return（keyCode `36` 或 `76`）在 `selecting` 阶段且有有效选区：screenshot → `complete(.copy)`；recording → `presentRecordingOptions()`；ocr → 完成/弹出 OCR 面板。`annotating` 阶段 Return → `complete(.copy)`；`⌘C/⌘S/⌘Z/⇧⌘Z` 对应复制/保存/撤销/重做；`keyCode 51`（Delete）或 `117`（Forward Delete）→ 删除所选标注。
- 右键：`selecting` 阶段取消，`annotating` 阶段 `returnToSelection()`。
- `phase` 两态：`selecting` / `annotating`；截图选区选定后 `prepareSelectionToolbar()` 进入标注准备，点工具才 `phase = .annotating`。

### 8.8 录制策略常量（KiriCore `RecordingPolicy`）

- `framesPerSecond = 30`、`countdownSeconds = 3`、`maximumGIFDuration = 15`（秒）、`gifFramesPerSecond = 12`、`maximumGIFLongEdge = 720`。
- `highQualityBitRate(width:height:)` = `min(40_000_000, max(4_000_000, width*height*8))` bps（界于 4–40 Mbps）。
- `pixelDimension(points:backingScale:)` = 偶化（`evenDimension`，`max(2, …)` 后向下取偶数）的 `points*scale` 四舍五入。
- `gifFrameCount(duration:)` = `max(1, ceil(duration*12))`。

---

## 附：跨文件关键数值速查

| 项 | 值 |
|---|---|
| 库窗口最小 / 默认尺寸 | 820×540 / 960×640 |
| 编辑器窗口 / 最小尺寸 | 880×620 / 860×520 |
| 编辑器工具栏高度 | 58pt |
| 覆盖层模式控件 | 距顶 88pt，最小 220×44 |
| 覆盖层 hover 窗口边框 | `accent` alpha 0.92，2pt（无手柄/尺寸/堆叠边框/跟随 tooltip） |
| 选区遮罩透明度 | hover 0.34 / 已选 0.48 / 无 0.25 |
| 手柄 | 8 个，外径 10、内径 6，命中半径 10 |
| 拖拽判定阈值 | 3pt |
| 选区最小有效边长 | 3pt（创建）；resize 最小边 16pt（拖拽内） |
| 窗口命中最小边长 | 8pt |
| 放大镜 | 采样 11×11px，边长 88pt，圆角 6 |
| 置顶图尺寸 | 初始最长 520×420、下限 180×120、最小 140×90、内容 inset 7 |
| 单实例扫描间隔 / 强杀宽限 | 1000ms / 350ms |
| 隐藏库窗口后的冻结延迟 | 120ms |
| Kiri 前台归还焦点探测 | 100ms |
| 通知自动消失 | 2s |
| 录制时钟节拍 | 250ms |
| 录制 FPS / 倒计时 / GIF 上限 | 30 / 3s / 15s（GIF 12fps、长边 720） |
| 全局快捷键 | `⇧⌘A`（A keycode 0，恰好 ⇧⌘） |
| 动画 | hover 0.14s、feedback 0.20s |
