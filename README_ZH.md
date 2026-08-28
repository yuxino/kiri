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

按下快捷键，选择窗口或区域，就能截图、标注、识别文字或录制屏幕。截图会自动复制到剪贴板；截图、MP4 和 GIF 都会保存在本地素材库。

Kiri 目前主要在 macOS 上开发和测试。Windows 构建尚未完成真机验收，安装、权限或部分功能可能存在问题。

## 功能

- **截图与可重编辑标注**：支持窗口和区域截图，以及画笔、图形、箭头、文字、马赛克、撤销与重做。新版 Kiri 创建的标注会随截图保存在本地；点击完成卡缩略图或双击素材库图片，可以继续选择和修改。旧截图里已经压平的标注无法还原，但仍可基于当前图片重新标注。
- **OCR**：默认在本机识别文字；也可以手动配置远程服务，每次发送前都会明确确认。
- **录屏与 GIF**：录制指定区域，可选系统声音、麦克风、指针和点击高亮；支持 MP4 与 GIF。
- **本地素材库**：支持搜索、收藏、标签、重命名和可恢复的回收站。

> 可重编辑截图工程已进入 `main`；当前 v1.4.4 下载包尚不包含这项功能。

## 下载与安装

从 [GitHub Releases](https://github.com/yuxino/kiri/releases/latest) 下载最新版本。

- **macOS 14+**：按设备下载 Apple Silicon（`arm64`）或 Intel（`x64`）版 `.dmg`，打开后把 `Kiri.app` 拖入“应用程序”。截图与录屏需要“屏幕与系统音频录制”权限；只有启用点击高亮时才需要“输入监控”，只有启用麦克风录制时才需要“麦克风”权限。
- **Windows**：运行安装包即可，截图不需要额外的系统授权；麦克风权限由 Windows 隐私设置控制。当前安装包尚未进行 Authenticode 签名，Windows SmartScreen 可能显示警告；Windows 版本也尚未经过真机测试，可能无法正常安装或使用部分功能。

> 当前可下载的 v1.4.4 macOS GitHub 发布包（`arm64` 与 `x64`）使用项目持续维护的本地自签名身份，而不是 ad-hoc 签名。两个包的版本、`io.yuxino.kiri` 标识符、架构、签名结构和 Designated Requirement 均已核验；但它不是 Developer ID 签名，也未经过 Apple 公证。第一次启动仍可能被 Gatekeeper 拦截，请按住 Control 点按 `Kiri.app` 并选择“打开”，或前往“系统设置 → 隐私与安全性 → 仍要打开”。不需要关闭 Gatekeeper。
>
> Kiri 当前采用可直接从 GitHub 下载、按系统提示手动放行后安装启动的分发标准，不要求 App Store、Developer ID 或 Apple 公证。macOS DMG 由维护者在受信任的 Mac 上使用项目长期稳定的本地自签名身份打包并验证，再手工附加到 Release；GitHub Actions 只校验版本并生成 Windows draft，不负责 macOS 打包，这不妨碍 macOS 发布。若未来更换签名身份，macOS 可能要求重新授予屏幕录制、输入监控或麦克风权限。

远程 OCR 完全可选。API Key 保存在 macOS 钥匙串或 Windows 凭据管理器中，不会写入配置文件。创建或选择远程配置不会自动发送图片，每次识别都需要点击“发送”或“重试”。

为了让新版标注可以再次编辑，Kiri 会在本地保存压平截图、未加标注的源图和标注文档。源图可能仍包含被马赛克或图形遮住的像素；它不会由编辑功能上传，移入回收站后仍可恢复，永久删除截图时才会一并移除。

录屏和 GIF 转换使用 FFmpeg。如果系统中没有可用版本，Kiri 会在第一次录屏或手动转换 GIF 时下载一次并保存到系统缓存。浏览素材库不会触发下载，编码仍在本机完成。

## 从源码运行

需要 Rust 1.88+、Node.js 20.19+（或 22.12+）和 pnpm。macOS 打包还需要 Xcode Command Line Tools。

```bash
git clone https://github.com/yuxino/kiri.git
cd kiri
pnpm install
pnpm tauri dev
pnpm tauri build --no-bundle
```

macOS 开发版必须使用稳定签名。`pnpm tauri dev` 会使用独立的开发标识符，并在找不到稳定签名身份时直接报错。不要直接运行 `cargo build` 生成的二进制；它不包含前端资源，会显示空白窗口。

## 快捷键

- **⇧⌘A**（macOS）/ **Shift+Ctrl+A**（Windows）：打开 Kiri
- **Esc**：取消截图；录屏时停止录制
- **Return**：确认截图
- **⌘F**（macOS）/ **Ctrl+F**（Windows）：搜索素材库
- **⌘Z / ⇧⌘Z**（macOS）/ **Ctrl+Z / Shift+Ctrl+Z**（Windows）：撤销 / 重做

另见 [隐私说明](PRIVACY_ZH.md)、[路线图](ROADMAP.md)、[贡献指南](CONTRIBUTING.md)、[安全策略](SECURITY.md) 与[文档索引](docs/README.md)。

[MIT](LICENSE) © 2026 yuxino
