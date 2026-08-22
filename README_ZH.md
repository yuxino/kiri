<div align="center">
  <img src="src-tauri/icons/128x128.png" width="112" alt="kiri 应用图标">
  <h1>kiri</h1>
  <p>快速、本地优先的截屏工作台,支持 macOS 与 Windows。</p>
  <p>
    <a href="README.md">English</a>
    · <a href="README_JA.md">日本語</a>
  </p>
</div>

`kiri` 源自日语「切り取り」——剪取、裁切之意。

截屏、标注、识别文字、区域录屏,全部保存在本地素材库。无需云端账号;可选的远程 OCR 始终由你明确控制。

## 界面预览

![Kiri 素材库](docs/screenshots/library.png)

## 功能

- **截屏** — 点选窗口或框选区域,精准选区。
- **标注** — 画笔、图形、箭头、文字、马赛克,支持撤销/重做;已添加的标注可再次选中编辑。
- **OCR** — 默认使用本地文字识别(macOS Vision / Windows.Media.Ocr),也可同时保存阿里云、OpenAI,或支持图片输入的 OpenAI Chat Completions 兼容服务。每次远程识别前都会显示目标地址、模型与图片信息;只有明确点击「发送」或「重试」才会上传当前选区。
- **录屏** — 区域录屏,可选系统声音、麦克风、指针与点击轨迹;3-2-1 倒计时、可拖动的控制条(Esc 停止)、Retina 高清 MP4。
- **GIF** — 把短视频转成循环 GIF。
- **素材库** — 按日期分组的素材,支持收藏、标签、重命名、搜索、复制、在文件管理器中显示与可恢复的回收站;侧边栏与筛选条可按类型、收藏与标签浏览。

## 下载

从 GitHub Releases 下载最新构建。

- **macOS**:解压后把 `Kiri.app` 拖入「应用程序」。全局快捷键需要「输入监控」权限,截屏与录屏需要「屏幕与系统音频录制」权限;只有开启麦克风录制时才需要「麦克风」权限。除非你主动导出,或明确把当前 OCR 选区发送到已配置的服务,所有内容都只留在本机。

> **macOS 权限说明**:GitHub Releases 的构建包为 ad-hoc 签名(暂无 Apple Developer ID 证书),macOS 会把每次新构建视为不同应用,升级后可能再次弹出「屏幕录制」授权 — 在 系统设置 → 隐私与安全性 → 屏幕录制 中授予一次后重新打开 Kiri 即可。本地构建(`./scripts/install-app.sh`)使用稳定证书签名,授权在重装后仍然有效。
- **Windows**:运行安装包即可,截屏无需系统授权;开启麦克风录制时,访问权限由 Windows 隐私设置控制。

远程 OCR 完全可选。API Key 只在 Kiri 内输入,并存入 macOS 钥匙串或 Windows 凭据管理器,不会写进配置 JSON。本地 OCR 始终是初始选项;远程失败不会自动重试、切换服务商或上传到其他地址。

## 从源码构建

需要 Rust 1.88+、Node.js 20.19+(或 22.12+)与 pnpm。

```bash
git clone https://github.com/yuxino/kiri.git
cd kiri
pnpm install
cargo test --manifest-path src-tauri/Cargo.toml
pnpm tauri dev                # 开发模式,支持前端热更新
pnpm tauri build --no-bundle   # 或 ./scripts/package-app.sh 生成安装包
```

macOS 下,`pnpm tauri dev` 会用稳定证书和独立开发 identifier
`io.yuxino.kiri.dev` 签名每次重新编译的调试程序。因此「屏幕录制」和
「输入监控」只需为开发版授权一次,Rust 重编译后不会失效。命令会自动选用
已安装的 Apple Development / Developer ID 或现有本地开发证书;也可以通过
`KIRI_DEV_SIGNING_IDENTITY` 明确指定。找不到稳定证书时会直接给出错误,不会
悄悄使用导致反复弹窗的 ad-hoc 临时签名。

如果没有签名证书、只做界面开发,可以明确使用生成的截屏夹具。夹具模式会使用仅属于当前进程的临时素材库,进程正常退出后自动删除,不会读写用户的真实素材库。

```bash
KIRI_CAPTURE_FIXTURE=1 KIRI_ALLOW_ADHOC_SIGNING=1 pnpm tauri dev
```

> 注意:直接 `cargo build` 出的二进制打开会是空白窗口——`pnpm tauri
> build` 会嵌入前端资源,`pnpm tauri dev` 则由 Vite 在开发时提供资源。

macOS 打包还需要 Xcode 命令行工具。

## 快捷键

- **⇧⌘A**(macOS)/ **Shift+Ctrl+A**(Windows)— 打开 Kiri
- **Esc** — 取消截屏
- **Return** — 复制截屏
- **V** — 选择 / 移动标注
- **P / R / L / A / T / M** — 画笔 / 矩形 / 直线 / 箭头 / 文字 / 马赛克
- **Delete** — 删除选中的标注
- **Esc**(录制中)— 停止
- **⌘F**(macOS)/ **Ctrl+F**(Windows)— 搜索素材库
- **⌘Z / ⇧⌘Z**(macOS)/ **Ctrl+Z / Shift+Ctrl+Z**(Windows)— 撤销 / 重做

另见 [PRIVACY_ZH.md](PRIVACY_ZH.md)、[ROADMAP.md](ROADMAP.md)、[CONTRIBUTING.md](CONTRIBUTING.md) 与 [SECURITY.md](SECURITY.md)。

[MIT](LICENSE) © 2026 yuxino
