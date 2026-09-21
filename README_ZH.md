<div align="center">
  <img src="src-tauri/icons/128x128.png" width="112" alt="Kiri 应用图标">
  <h1>Kiri</h1>
  <p>本地优先的截图、标注、OCR 与区域录屏工具。</p>
  <p>
    <a href="https://kiri.yuxino.cn">官网</a>
    · <strong>简体中文</strong>
    · <a href="README.md">English</a>
  </p>
</div>

Kiri 支持 macOS、Windows，以及实验性的 Linux。按 `⇧⌘A`（macOS）或 `Shift+Ctrl+A`（Windows / Linux），选择窗口或区域，即可截图、标注、识别文字或录屏。截图会复制到剪贴板；截图、MP4 和 GIF 保存在本地素材库。

<!-- project-demo-v1 -->
<h2 align="center">演示</h2>

https://github.com/user-attachments/assets/13742f07-1845-4201-9295-39f83515547f

<p align="center">4K、60 帧完整功能演示，配有中文女声旁白和字幕。可直接在本页播放。</p>
<p align="center"><a href="https://kiri.yuxino.cn/#demo">Watch in English</a> · <a href="https://kiri.yuxino.cn/zh/#demo">观看中文版</a> · <a href="docs/demos/full-tour-4k.json">录制说明</a></p>
<!-- /project-demo-v1 -->

## 功能

- **截图与标注**：点击窗口或拖选区域，使用裁剪、画笔、图形、箭头、文字、马赛克、撤销和重做。新版创建的标注可从完成卡或素材库继续编辑。
- **OCR**：默认使用 macOS Vision 或 Windows.Media.Ocr 在本机识别；可选远程 OCR 每次发送前都会确认。 识别成功后会自动保存到「文字」，支持搜索、回看、再次复制和展开原图；历史截图也可以进行本地文字识别。
- **录屏与 GIF**：录制指定区域，可选系统声音、麦克风、指针和点击高亮，开始前显示小巧的圆环倒计时，点击圆环或按 Esc 可取消；输出 MP4 或 GIF。
- **导入外部素材**：点击“导入素材”或将本地 PNG、JPEG、WebP、MP4、MOV 文件拖入素材库。图片校正方向并转为 PNG，视频复制到素材库，均不修改原文件。导入后使用现有图片或视频编辑器。每次最多 32 个文件；图片最多 32 MB、每边 8192 像素并受解码内存上限约束，视频最多 8 GB。
- **直接标注与贴图**：进入视频剪辑即显示截图标注工具，共享已选样式，首次默认红色。画完自动选中新标注，属性区可修改其颜色、粗细和马赛克样式，画面上的手柄可调整形状与大小。视频马赛克支持自由涂抹、矩形和椭圆。文字提供明确的「编辑文字」按钮，颜色、字号和背景实时生效，轨道显示文字内容；Esc 只取消本次文字编辑。标注、贴图和遮挡可直接在画面中选中并拖动。同一工具栏可添加本地 PNG/JPEG/WebP 静态贴图，保留透明背景，支持拖动、等比例缩放、独立显示时段和撤销。遮挡样式通过文字选项切换，效果直接在中央画面查看；工具栏和属性区域保持固定布局，新增轨道不会挤动画面。小播放窗口进入剪辑时会在当前屏幕可用范围内扩大，为画面、属性和时间轴留出空间。
- **本地素材库**：支持搜索、收藏、标签、重命名和可恢复的回收站。可在设置中使用其他本机目录或外接盘。

录屏完成后，打开 MP4，在视频上方常驻工具栏选择「剪辑与导出」。时间轴连续显示成片：点击定位、拖动片段排序、拖动两端裁剪，或在播放头处分割并删除片段。选中片段可单独设置 0.25～4 倍速度，预览与导出均生效。时间轴高度可调整，多条效果轨道在内部滚动，保留预览空间。裁剪、排序、变速、效果、标注及时间调整均支持撤销和重做。支持按时段设置 1.5～4 倍局部放大，可调整平滑进入和退出的时长；隐私遮挡提供模糊、像素和自定义颜色的纯色样式，可调强度。中央画面始终显示编辑结果，调整标注和遮挡也即时可见。可添加聚光强调、保持比例的裁切与自定义背景留白，以及可调时长和颜色的淡入淡出。放大后直接拖动画面调整取景，整段剪辑只保留一处播放／暂停。截图中的画笔、矩形、直线、箭头、可编辑文字和像素／模糊马赛克笔刷也可用于视频，每个标注都有独立时间轨道。效果选项先说明用途，再显示参数；较少用的选项收在「画面修饰」中。拖轨道左侧上下调整顺序，拖中间移动时间，拖两端调整时长。上方叠加内容会盖住下方内容，预览和导出一致；整幅画面的调整在单独一组中排序。轨道排序支持撤销，拖到列表边缘时只滚动轨道区域。保存会在素材库中创建新副本，保留原片。导出固定在右上角，画质菜单会说明各档用途并显示实际输出尺寸。高画质保留原始尺寸；日常分享和小文件分别将最长边限制在 1080 和 720 像素，不放大小视频。导出保留同步声音；macOS 变速保持音调，Windows 片段变速会同时改变音调。文件大小取决于素材和编辑后时长。Windows 的画面效果目前要求原视频不含旋转标记（Kiri 录屏符合此条件）。播放控件位于画面外，进度条统一定制样式，支持悬停时间提示和键盘调整；拖动时暂停画面，松手恢复此前的播放状态。倍速支持预设和 0.1～8 倍自定义输入，并记住播放偏好，不改变导出速度。

开启麦克风录制后，可点击「检测麦克风」查看系统默认设备和实时音量，检测持续五秒，不保存测试音频。

## 下载与安装

公开版本和 macOS Universal / Windows x64 安装包见 [GitHub Releases](https://github.com/yuxino/kiri/releases)。

从 v1.4.9 起，设置页支持手动检查、下载并安装经过签名验证的更新；每一步都需要你明确点击，Kiri 不会后台检查或静默安装。v1.4.8 及更早版本需要先从 Releases 手动安装一次 v1.4.9 或更新版本，之后才能使用应用内更新。

日常更新请用**设置 → 关于 → 检查更新**。下载时显示已下载大小和可用的百分比，随后验证签名。Windows 点击「安装并重启」后，Kiri 会短暂关闭，更新完成自动重新打开，无需手动卸载现有 NSIS 版本；macOS 安装后再点击重启。手动运行下载的安装包则可能出现卸载／重装选项。

- **macOS 14+**：下载 Universal `.dmg`（Apple 芯片与 Intel），把 `Kiri.app` 拖入“应用程序”。截图与录屏需要“屏幕与系统音频录制”权限；点击高亮才需要“输入监控”。麦克风录制需要 macOS 15+。
- **Windows 11（x64）**：提供 x64 安装包，完整捕获流程的真机验收进度见[路线图](ROADMAP.md)。运行 `.exe` 安装程序；屏幕捕获不需要额外系统授权，麦克风权限由 Windows 隐私设置控制。安装程序未经过 Authenticode 签名，SmartScreen 可能提示警告。
- **Linux（实验性）**：目前以源码构建为主（启用 Linux 打包目标时可生成 AppImage）。Wayland 静态截图优先使用系统自带的 `grim`（Hyprland / Sway 推荐安装），否则回退到 xdg-desktop-portal Screenshot；录屏走 ScreenCast → PipeWire，并用系统 GStreamer 编码，不会下载 FFmpeg。在 Hyprland 上，`Shift+Ctrl+A` 通过合成器注册。窗口悬停轮廓、本地 OCR、系统声音、麦克风和点击高亮可能受合成器限制或暂不可用。

Windows 安装器跟随系统语言，支持简体中文、英文和日文，安装、更新与卸载提示均已翻译；应用内语言可在设置中单独选择。Kiri 的半身立绘与公共安装样式统一在 [desktop-installer](https://github.com/yuxino/desktop-installer) 维护。

macOS 发布包使用项目维护的本地自签名身份，未使用 Developer ID 签名或 Apple 公证。首次启动若被拦截，请按住 Control 点按 `Kiri.app` 并选择“打开”，或在“系统设置 → 隐私与安全性”中选择“仍要打开”。

## 隐私

素材、OCR 和编码默认都在本机处理。远程 OCR 完全可选，API Key 保存在 macOS 钥匙串、Windows 凭据管理器或 Linux Secret Service 中，每次请求都需要明确点击“发送”或“重试”。

可重编辑截图会在本地保存未加标注的源图；其中可能仍有被马赛克或图形遮住的像素。保存裁剪会同时移除框外像素。macOS 的 MP4 录屏、合并、缩略图和 GIF 生成使用 AVFoundation 与 ImageIO；Windows 使用 Media Foundation 与系统图像组件。两个平台都不下载 FFmpeg，媒体处理始终在本机完成。

## 从源码运行

需要 Rust 1.88+、Node.js 20.19+（或 22.12+）和 pnpm。macOS 需要 Xcode Command Line Tools；Windows 需要 MSVC C++ 构建工具。

```bash
git clone https://github.com/yuxino/kiri.git
cd kiri
pnpm install
pnpm tauri dev
pnpm tauri build --no-bundle
```

macOS 开发版还需要稳定的签名身份。请通过 Tauri 命令运行或构建；普通 `cargo build` 生成的二进制不包含前端资源。

## 快捷键

- **⇧⌘A**（macOS）/ **Shift+Ctrl+A**（Windows / Linux）：打开 Kiri
- **Esc**：取消截图；录屏时停止录制
- **Return**：确认截图
- **C**：在截图编辑器中裁剪
- **⌘F**（macOS）/ **Ctrl+F**（Windows）：搜索素材库
- **⌘Z / ⇧⌘Z**（macOS）/ **Ctrl+Z / Shift+Ctrl+Z**（Windows）：撤销 / 重做

另见 [隐私说明](PRIVACY_ZH.md)、[路线图](ROADMAP.md)、[贡献指南](CONTRIBUTING.md)、[安全策略](SECURITY.md) 与[文档索引](docs/README.md)。

[MIT](LICENSE) © 2026 yuxino
