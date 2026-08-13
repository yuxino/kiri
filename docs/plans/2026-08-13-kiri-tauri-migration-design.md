# Kiri Tauri 迁移设计(1:1 复刻, macOS + Windows)

> 状态:已定稿,正在实施
> 蓝本:本仓库 Swift/macOS 原版(约 1.07 万行,行为规格见 `docs/spec/swift/`)
> 目标:用 Tauri 2 + React 全量复刻,支持 macOS 与 Windows,原版行为逐条对齐。

## 1. 目标与范围

把 Swift 原版 Kiri 完整迁移为 Tauri 桌面应用:

- **截图**:全局快捷键 ⇧⌘A(macOS)/Shift+Ctrl+A(Windows)→ 冻结屏 → 窗口悬停单描边选中 / 手动区域拖拽(8 手柄)→ 标注 → 剪贴板优先 + 入库,焦点归还原应用。
- **标注**:画笔/矩形/直线/箭头/文字/马赛克,撤销重做,内联文字编辑(IME),实时字号。
- **OCR**:第三捕获模式,本地识别,结果复制到剪贴板。
- **录屏**:区域录制 + 可选系统音频/麦克风/光标/点击涟漪,3-2-1 倒计时,暂停恢复(分段合并),控件不入视频。
- **GIF**:≤15s 录屏转 GIF(12fps、长边 720)。
- **库**:本地文件 + `library.json` 索引,搜索/收藏/复制/打开/在文件管理器中显示/回收站(可恢复)。
- **双语言**:en + zh-Hans,跟随系统首选语言。
- **本地优先**:无网络、无分析、无账号。

不可妥协清单与跨平台等价映射详见 `docs/spec/swift/product-contract.md`。

## 2. 技术选型

| 层 | 选择 | 说明 |
| --- | --- | --- |
| 壳 | Tauri 2.11 + Rust 2021 | 多窗口、透明窗口、系统集成 |
| 前端 | React 19 + TypeScript + Vite 7 | 覆盖层/标注画布/库/编辑器 |
| 截图(macOS) | `objc2-screen-capture-kit` `SCScreenshotManager` | 与原版 SCK 路径 1:1,Retina 倍率 |
| 截图(Windows) | `xcap`(WGC 后端) | Graphics Capture,像素级准确 |
| 窗口枚举 | `xcap::Window::all()` 双平台 | 几何/标题/应用名;按原版规则过滤 |
| 录屏(macOS) | `objc2-screen-capture-kit` `SCStream`(视频+系统音频+麦克风) | 与原版 Legacy 后端等价,同一时钟域 |
| 录屏(Windows) | `windows-capture`(WGC)+ `cpal`(WASAPI 回环 + 麦克风) | 系统音频内置支持 |
| 编码/复用 | ffmpeg-sidecar(H.264/HEVC 硬件优先,AAC) | 双平台同一条编码链路 |
| 全局快捷键 | macOS:`CGEventTap`(吞事件,独占);Windows:`RegisterHotKey` | 与原版独占语义一致 |
| 剪贴板 | `arboard` | 图片/文本双平台 |
| OCR | macOS:`objc2-vision` VNRecognizeTextRequest;Windows:`Windows.Media.Ocr` | 均本地 |
| 存储 | `~/Library/Application Support/kiri` / `%APPDATA%\kiri` | 与 Swift 版同一 schema,用户库无缝延续 |
| i18n | 自定义轻量字典(210 key 全表见 spec) | 跟随系统语言,key 即英文回退 |

## 3. 窗口架构

| 窗口 label | 角色 | 属性 | 是否被录制 |
| --- | --- | --- | --- |
| `library` | 主窗口(库+设置) | 常规 960×640,min 820×540 | — |
| `overlay-<n>` | 每显示器一个覆盖层 | 全屏、无边框、透明、最高层、screenSaver 级 | 截图时不录制;录屏时已销毁 |
| `countdown` | 3-2-1 倒计时 | 透明小窗覆盖选区,居中圆形徽章 | 排除(macOS `excludingApplications`,Windows `WDA_EXCLUDEFROMCAPTURE`) |
| `control-panel` | 录制控制条 | 296×64,屏幕水平居中、距顶 18pt | 排除(同上) |
| `ripple` | 点击涟漪面板 | 透明、点击穿透、跟随全局鼠标 | **必须被录制**(macOS `exceptingWindows`,Windows 不设 affinity) |
| `editor` | 截图编辑器 | 880×620,深色 | — |
| `pin-<uuid>` | 置顶图 | floating 面板 | — |

- 覆盖层一次只覆盖**活动显示器**(与原版单显示器行为一致)。
- 前端按 `?window=` 查询参数渲染对应组件;所有窗口共享同一 Vite bundle。
- 冻结截图经自定义 `kiri://` 协议从 Rust 内存直接供给 WebView,不落盘。

## 4. 录制管线(双平台统一)

```
平台采集(视频 BGRA + 音频 PCM)
   │  stdin 管道(两个 pipe:video / audio)
   ▼
ffmpeg -f rawvideo -pix_fmt bgra -s WxH -r 30 -i pipe:v
       -f f32le -ar 48000 -ac 2 -i pipe:a
       -use_wallclock_as_timestamps 1
       -c:v <hw 编码器优先, 否则 libx264> -b:v clamp(w·h·8, 4M, 40M)
       -g 60 -c:a aac -b:a 192k -movflags +faststart out.mp4
```

- 参数与原版 `RecordingPolicy` 一致:30fps、Retina 像素(`points × backingScale` 取偶)、AAC 48kHz 双声道 192kbps。
- 音频编码统一由 ffmpeg AAC 完成(`-c:a aac -b:a 192k`),Rust 侧不做音频转码,只把采集端 PCM(SCK LPCM / WASAPI f32)经管道直送 ffmpeg。
- macOS 编码器:`h264_videotoolbox` / `hevc_videotoolbox`;Windows:`h264_nvenc` → `h264_qsv` → `h264_amf` → `libx264` 降级链。
- **暂停/恢复**:暂停即优雅终止当前 ffmpeg 段;恢复以相同配置重启,不倒计时;停止时 ffmpeg concat demuxer `-c copy` 合并,失败则重编码合并(等价原版 `RecordingSegmentMerger`)。
- 音频:macOS 走 SCK `capturesAudio + captureMicrophone + excludesCurrentProcessAudio`(原版同款);Windows 走 WASAPI 回环(系统)+ 麦克风,ffmpeg `amix` 混音。
- 光标:macOS 由 SCK `showsCursor`;Windows 由 WGC 合成。点击涟漪由 `ripple` 窗口绘制(非系统 API),故双平台视觉完全一致。
- 涟漪监听:macOS `NSEvent.addGlobalMonitorForEvents`(原版同款);Windows `WH_MOUSE_LL` 钩子。

## 5. 数据兼容

- 库目录:macOS `~/Library/Application Support/kiri`,Windows `%APPDATA%\kiri`。
- `library.json`:`CaptureAsset` JSON schema 与 Swift 版逐字段兼容(UUID 大写串、`createdAt` 毫秒数值、`longImage`→`image` 兼容解码、key 字母序)。
- 文件名 `yyyyMMdd-HHmmss-<uuid小写>.<ext>`(本地时区、POSIX 格式)。
- 回收站 = `trashedAt` 软删除,文件不移动;清空/彻底删除先持久化再删文件,无自动清理。
- 缩略图:原版预留 `Thumbnails/` 从不写入;Tauri 版同样即时生成,不落盘。

## 6. 平台差异决策

| 行为 | macOS | Windows |
| --- | --- | --- |
| 快捷键 | ⇧⌘A,CGEventTap 吞事件 | Shift+Ctrl+A,RegisterHotKey 独占 |
| 权限 | 屏幕录制(TCC)、输入监控、麦克风;重启后生效场景给 "Quit Kiri" | 无需捕获权限;麦克风为系统级授权 |
| 焦点归还 | `NSRunningApplication.activate` | `SetForegroundWindow` 目标 HWND |
| 在访达/资源管理器显示 | `NSWorkspace.activateFileViewerSelecting` | `explorer /select,` |
| 打开文件 | `NSWorkspace.open` | `ShellExecute`(tauri-plugin-opener) |
| 排除自身窗口 | SCK `excludingApplications + exceptingWindows` | `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)` |
| 麦克风(旧系统) | macOS<15 禁用并显示 "Requires macOS 15"(原版行为) | 全版本支持 |
| 语言跟随 | 系统首选语言 | 系统 UI 语言 |
| 应用入口 | Dock 图标 + 菜单栏 | 任务栏 + 托盘(启动后常驻) |

## 7. 里程碑

1. **M1 工程骨架**:仓库整体替换为 Tauri 工程,双平台 CI 绿。
2. **M2 核心库**:policy/geometry/shortcut/asset/library(带单测)+ 剪贴板 + 权限门。
3. **M3 截图闭环**:冻结捕获 → 覆盖层(模式选择/悬停/拖拽/八手柄)→ 工具栏/尺寸滑块 → 标注画布 → 复制+入库+焦点归还。
4. **M4 本地库**:网格 UI、搜索、收藏、复制/打开/显示、回收站、空态。
5. **M5 录屏**:选项弹窗、倒计时、控制条、涟漪、暂停分段、音频、ffmpeg 管线。
6. **M6 OCR + GIF + 编辑器 + 置顶图**。
7. **M7 打磨**:i18n 全量、设计 token 核对、打包脚本、README、macOS 实测验收 + Windows CI。

## 8. 验证

- Rust 单测:`cargo test`(policy/geometry/library 与原版测试集逐条等价)。
- 前端:手工验收走原版 UI 验收清单(见 `docs/spec/swift/product-contract.md` §3)。
- 构建:`pnpm tauri build`(macOS 本机);Windows 经 GitHub Actions `windows-latest` 编译验证。
- 录制质量:抽帧检查起止/暂停/涟漪,确认 Kiri 控件不出现在视频中。
- 库兼容:用 Swift 版生成的 `library.json` + 资产目录做读取回归。

## 9. 风险与对策

| 风险 | 对策 |
| --- | --- |
| Windows 侧行为无法本机实测 | 全部 Windows 特定代码集中在 `cfg(windows)` 模块 + CI 编译验证 + 接口镜像 macOS 路径 |
| ffmpeg 体积/下载 | ffmpeg-sidecar 按平台打包进 bundle;打包脚本固化下载 |
| 音视频同步 | `-use_wallclock_as_timestamps` 同一时钟;SCK 单流天然同步 |
| 透明窗口点击穿透(Windows) | WS_EX_TRANSPARENT|LAYERED 样式 + `set_ignore_cursor_events` 等价实现 |
| macOS 权限重启生效 | 沿用原版文案与恢复动作(Quit Kiri / Open Settings) |
| xcap 的 CGWindowList 截图在 macOS 26 废弃 | macOS 截图不走 xcap,直接 SCK `SCScreenshotManager` |
