<div align="center">
  <img src="src-tauri/icons/128x128.png" width="112" alt="kiri 应用图标">
  <h1>kiri</h1>
  <p>快速、完全本地的截屏工作台,支持 macOS 与 Windows。</p>
  <p>
    <a href="README.md">English</a>
    · <a href="README_JA.md">日本語</a>
  </p>
</div>

`kiri` 源自日语「切り取り」——剪取、裁切之意。

截屏、标注、识别文字、区域录屏,全部保存在本地素材库。无需云端。

## 界面预览

![Kiri 素材库](docs/screenshots/library.png)

## 功能

- **截屏** — 点选窗口或框选区域,精准选区。
- **标注** — 画笔、图形、箭头、文字、马赛克,支持撤销/重做;已添加的标注可再次选中编辑。
- **OCR** — 本地文字识别(macOS Vision / Windows.Media.Ocr)。
- **录屏** — 区域录屏,可选系统声音、麦克风、指针与点击轨迹;3-2-1 倒计时、可拖动的控制条(Esc 停止)、Retina 高清 MP4。
- **GIF** — 把短视频转成循环 GIF。
- **素材库** — 按日期分组的素材,支持收藏、标签、重命名、搜索、复制、在文件管理器中显示与可恢复的回收站;侧边栏与筛选条可按类型、收藏与标签浏览。

## 下载

从 GitHub Releases 下载最新构建。

- **macOS**:解压后把 `Kiri.app` 拖入「应用程序」。全局快捷键需要「输入监控」权限,截屏与录屏需要「屏幕与系统音频录制」权限。除非你主动导出,所有内容都只留在本机。
- **Windows**:运行安装包即可,无需额外授权。

## 从源码构建

需要 Rust 1.85+、Node.js 20+ 与 pnpm。

```bash
git clone https://github.com/yuxino/kiri.git
cd kiri
pnpm install
cargo test --manifest-path src-tauri/Cargo.toml
pnpm tauri build --no-bundle   # 或 ./scripts/package-app.sh 生成安装包
```

> 注意:直接 `cargo build` 出的二进制打开会是空白窗口——前端资源只在
> `pnpm tauri build`(或开发用的 `pnpm tauri dev`)时才会嵌入。

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

另见 [ROADMAP.md](ROADMAP.md)、[CONTRIBUTING.md](CONTRIBUTING.md) 与 [SECURITY.md](SECURITY.md)。

[MIT](LICENSE) © 2026 yuxino
