<div align="center">
  <img src="src-tauri/icons/128x128.png" width="112" alt="Kiri 应用图标">
  <h1>Kiri</h1>
  <p>本地优先的截图、标注、OCR 与区域录屏工具。</p>
  <p>
    <strong>简体中文</strong>
    · <a href="README_EN.md">English</a>
    · <a href="README_JA.md">日本語</a>
  </p>
</div>

按 `⇧⌘A`（macOS）或 `Shift+Ctrl+A`（Windows），选择窗口或区域，即可截图、标注、识别文字或录屏。截图会复制到剪贴板；截图、MP4 和 GIF 保存在本地素材库。

Kiri 主要在 macOS 上开发和测试。Windows 尚未完成真机验收，安装或部分功能可能有问题。

## 功能

- **截图、裁剪与可重编辑标注**：点击窗口或拖选区域，使用裁剪、画笔、图形、箭头、文字、马赛克、撤销和重做。颜色、线宽、文字和马赛克样式会沿用上次选择。当前 `main` 创建的标注可从完成卡或素材库继续编辑，旧截图中已压平的标注无法还原。
- **OCR**：默认使用 macOS Vision 或 Windows.Media.Ocr 在本机识别；可选远程 OCR 每次发送前都会确认。
- **录屏与 GIF**：录制指定区域，可选系统声音、麦克风、指针和点击高亮，输出 MP4 或 GIF。
- **本地素材库**：支持搜索、收藏、标签、重命名和可恢复的回收站。可在设置中使用其他本机目录或外接盘。

> 可重编辑截图工程、裁剪、标注样式记忆与素材库位置管理/恢复已进入 `main`；当前 v1.4.4 下载包尚不包含这些功能。

素材库离线时可重试或重新定位；缺失文件可选择文件补回。未导入的录屏会保留并可重试。

## 下载与安装

从 [GitHub Releases](https://github.com/yuxino/kiri/releases/latest) 下载最新版本。

- **macOS 14+**：下载 Apple Silicon（`arm64`）或 Intel（`x64`）版 `.dmg`，打开后把 `Kiri.app` 拖入“应用程序”。截图与录屏需要“屏幕与系统音频录制”权限；点击高亮才需要“输入监控”。麦克风录制需要 macOS 15+，并只在启用时请求“麦克风”权限。
- **Windows**：运行安装程序即可；屏幕捕获不需要额外系统授权，麦克风权限由 Windows 隐私设置控制。安装程序未经过 Authenticode 签名，SmartScreen 可能警告；Windows 也尚未完成真机测试。

> 当前 v1.4.4 macOS 发布包使用项目维护的本地自签名身份，不是 ad-hoc、Developer ID 签名或 Apple 公证。首次启动可能需要按住 Control 点按 `Kiri.app` 并选择“打开”，或在“系统设置 → 隐私与安全性”中选择“仍要打开”；不需要关闭 Gatekeeper。
>
> macOS DMG 由维护者在可信 Mac 上打包并附加到 Release，GitHub Actions 生成 Windows draft。若未来更换签名身份，macOS 可能要求重新授予相关权限。

远程 OCR 完全可选。API Key 保存在 macOS 钥匙串或 Windows 凭据管理器中；新建或选择配置不会发送图片，每次请求都需要明确点击“发送”或“重试”。

当前 `main` 会在本地保存压平截图、未加标注的源图和标注文档。源图仍可能包含被马赛克或图形遮住的像素；保存裁剪后，框外像素也会从源图移除。编辑功能不会上传这些内容。

录屏和 GIF 转换使用 FFmpeg。本机没有可用版本时，Kiri 会在第一次录屏或手动转换 GIF 时下载并缓存一次；浏览素材库不会触发下载，编码仍在本机完成。

## 从源码运行

需要 Rust 1.88+、Node.js 20.19+（或 22.12+）和 pnpm。macOS 打包还需要 Xcode Command Line Tools。

```bash
git clone https://github.com/yuxino/kiri.git
cd kiri
pnpm install
pnpm tauri dev
pnpm tauri build --no-bundle
```

macOS 开发版需要稳定的签名身份，找不到可用身份时 `pnpm tauri dev` 会直接报错。不要直接运行 `cargo build` 生成的二进制；它不包含前端资源。

## 快捷键

- **⇧⌘A**（macOS）/ **Shift+Ctrl+A**（Windows）：打开 Kiri
- **Esc**：取消截图；录屏时停止录制
- **Return**：确认截图
- **C**：在截图编辑器中裁剪
- **⌘F**（macOS）/ **Ctrl+F**（Windows）：搜索素材库
- **⌘Z / ⇧⌘Z**（macOS）/ **Ctrl+Z / Shift+Ctrl+Z**（Windows）：撤销 / 重做

另见 [隐私说明](PRIVACY_ZH.md)、[路线图](ROADMAP.md)、[贡献指南](CONTRIBUTING.md)、[安全策略](SECURITY.md) 与[文档索引](docs/README.md)。

[MIT](LICENSE) © 2026 yuxino
