<div align="center">
  <img src="Resources/Assets/kiri-icon.png" width="112" alt="kiri app icon">
  <h1>kiri</h1>
  <p>轻快、原生的 macOS 截图工作台。</p>
  <p>
    <a href="README.md">English</a>
    · <a href="README_JA.md">日本語</a>
  </p>
</div>

`kiri` 来自日语「切り取り」，意思是截取、裁切。

截图、标注、文字识别、区域录屏，再把结果自动留在本地素材库里。不依赖云端。

<p align="center">
  <img src="Resources/Assets/kiri-library-preview.png" width="820" alt="Kiri 素材库">
</p>

## 功能

- **截图** — 窗口或区域截图，支持精确选区。
- **标注** — 画笔、图形、箭头、文字、马赛克与撤销/重做。
- **文字识别** — 基于 macOS Vision，在本机完成 OCR。
- **录屏** — 区域录制，可选声音、鼠标指针和点击高亮。
- **GIF** — 将短录屏转换为循环 GIF。
- **本地素材库** — 搜索、收藏、复制、Finder 定位和可恢复删除。

## 下载

从 [GitHub Releases](https://github.com/yuxino/kiri/releases/latest) 下载最新版本，解压后把 `Kiri.app` 放进“应用程序”。

Kiri 的全局快捷键需要 **输入监控** 权限，截图与录屏需要 **屏幕与系统音频录制** 权限。内容默认只保存在你的 Mac 上。

## 从源码运行

需要 macOS 14+ 和 Swift 6。

```bash
git clone https://github.com/yuxino/kiri.git
cd kiri
swift run kiri-core-tests
./scripts/install-app.sh
open /Applications/Kiri.app
```

## 快捷键

- **⇧⌘A** — 唤起 Kiri
- **Esc** — 取消截图
- **Return** — 复制截图
- **V** — 选择 / 移动标注
- **P / R / L / A / T / M** — 画笔 / 矩形 / 直线 / 箭头 / 文字 / 马赛克
- **Delete** — 删除选中标注
- **⌘F** — 搜索素材库
- **⌘Z / ⇧⌘Z** — 撤销 / 重做

更多内容见 [ROADMAP.md](ROADMAP.md)、[CONTRIBUTING.md](CONTRIBUTING.md) 和 [SECURITY.md](SECURITY.md)。

[MIT](LICENSE) © 2026 yuxino
