<div align="center">
  <img src="src-tauri/icons/128x128.png" width="112" alt="Kiri 应用图标">
  <h1>Kiri</h1>
  <p>截图、文字识别和录屏，素材保存在本机。</p>
  <p>
    <a href="https://kiri.yuxino.cn">官网</a>
    · <strong>简体中文</strong>
    · <a href="README_EN.md">English</a>
    · <a href="README_ZH_TW.md">繁體中文</a>
    · <a href="README_JA.md">日本語</a>
    · <a href="README_DE.md">Deutsch</a>
    · <a href="README_KO.md">한국어</a>
    · <a href="README_FR.md">Français</a>
  </p>
  <p>
    <a href="https://github.com/yuxino/kiri/releases/latest"><img src="https://img.shields.io/github/v/release/yuxino/kiri?style=flat&amp;logo=github&amp;logoColor=white" alt="最新版本"></a>
    <a href="https://github.com/yuxino/kiri/actions/workflows/build.yml?query=branch%3Amain"><img src="https://img.shields.io/github/actions/workflow/status/yuxino/kiri/build.yml?style=flat&amp;logo=githubactions&amp;logoColor=white&amp;branch=main&amp;event=push&amp;label=CI" alt="main 分支 CI 状态"></a>
    <a href="LICENSE"><img src="https://img.shields.io/github/license/yuxino/kiri?style=flat&amp;logo=opensourceinitiative&amp;logoColor=white" alt="MIT 许可证"></a>
  </p>
  <p>
    <a href="https://github.com/yuxino/kiri/releases/latest"><img src="https://img.shields.io/badge/macOS-14%2B-555?style=flat&amp;logo=apple&amp;logoColor=white" alt="macOS 14+"></a>
    <a href="https://github.com/yuxino/kiri/releases/latest"><img src="https://img.shields.io/badge/Windows-x64-0078D4?style=flat&amp;logo=data%3Aimage%2Fsvg%2Bxml%3Bbase64%2CPHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHZpZXdCb3g9IjAgMCAyNCAyNCI%2BPHBhdGggZmlsbD0id2hpdGUiIGQ9Ik0wIDBoMTF2MTFIMHptMTMgMGgxMXYxMUgxM3pNMCAxM2gxMXYxMUgwem0xMyAwaDExdjExSDEzeiIvPjwvc3ZnPg%3D%3D&amp;logoColor=white" alt="Windows x64"></a>
    <a href="docs/linux.md"><img src="https://img.shields.io/badge/Linux-Ubuntu%2024.04-FCC624?style=flat&amp;logo=linux&amp;logoColor=white" alt="Linux"></a>
  </p>
</div>

<p align="center">Kiri 是截图与录屏工具，支持 macOS、Windows 和 Linux。截图后可以画箭头、打马赛克、提取文字，图片和视频都保存在本机。</p>

![Kiri 标注界面预览，使用新绘制的插画素材](docs/assets/readme-preview.png)

## 功能

- 截取窗口或区域，裁剪、画图、加文字或马赛克，也能把截图置顶作参考。
- 在文字工具旁的小箭头中切换到序号标注，点击放置后直接在画布中输入说明，回车换行；直接拖动序号或透明说明框即可分别移动，编辑时可拖动说明框的边框；大小、颜色和实心／空心样式都可调整。
- 用本地 OCR 复制图片里的文字，识别二维码。
- 录制 MP4，可选系统声音和麦克风；也可保存为无声 GIF。素材库中的视频转 GIF 会先检查视频，检查和转换中可取消；macOS 显示逐帧进度，失败后保留具体原因。
- 裁切、重排同一视频的片段，导出新的 MP4。
- 搜索、打标签、收藏与导出本地素材，误删可从回收站恢复。

## 开始使用

1. 按 `⇧⌘A`（macOS）或 `Shift+Ctrl+A`（Windows / Linux X11），点击窗口或拖出一个区域。
2. 选择截图、录屏或 OCR。
3. 按 `Enter` 或双击选区内的空白处完成截图，`Esc` 取消。标注时请先切到选择工具；双击文字仍用于编辑。截图会复制到剪贴板，也会保存在素材库；点击截图工具栏的图钉可一步保存并置顶；完成卡上的「置顶」也能打开悬浮参考图。置顶图无边框，拖动图片可移动，拖动右下角可等比例缩放，移入后可取消置顶或关闭。

截图模式下，悬停可放大查看原始像素、坐标和 HEX 色值；按 `⌘C`（macOS）或 `Ctrl+C` 复制色值，选区会保留。开始标注后放大镜隐藏。

文字工具旁的小箭头可切换到「标签气泡」（B）。点击图片输入说明，点击气泡的小圆点，气泡围绕同一个点切换到左侧或右侧；字号和圆点颜色可调整，双击文字可再次编辑。

开始标注或录屏前，点击滑杆按钮，可在选区边上直接输入宽高，单位为像素。

在「设置 → 通用 → 语言」中选择七种界面语言。选择会应用到每个 Kiri 窗口，重启后仍会保留；首次启动跟随系统语言。

选中已有标注即可修改样式。马赛克支持自由画笔、矩形和椭圆，可选像素或模糊效果并调整强度。点击水印（W）后直接在图片中输入平铺水印，调整不透明度、角度和间距；再次点击水印工具即可继续编辑已有内容，保存后仍可修改。

素材库中重命名会同时更新实际文件名，并保留文件类型；右键菜单的「复制文件」可直接粘贴到文件夹。原有「复制」仍可将图片粘贴到聊天或编辑器。

素材保存在本机。远程 OCR 可选，每次上传前都会询问。Linux 配置、MP4 声音录制、Wayland 快捷键和平台限制见 [Linux 指南](docs/linux.md)。

[使用与常见问题](docs/usage.zh-CN.md) · [视频剪辑](docs/video-editing.zh-CN.md) · [反馈问题](https://github.com/yuxino/kiri/issues) · [贡献指南](CONTRIBUTING.md)

## 贡献者

感谢每一位写代码、提问题、试用和分享的朋友 (๑•̀ㅂ•́)و✧

特别感谢 [@kerwin2046](https://github.com/kerwin2046) 提供 [Linux 初始支持](https://github.com/yuxino/kiri/pull/20)，以及 [@LLLin000](https://github.com/LLLin000) 修复 [Windows 文字缩放下的截图对齐](https://github.com/yuxino/kiri/pull/61)。

[查看所有贡献者](https://github.com/yuxino/kiri/graphs/contributors)

## 社区致谢

也感谢 [V2EX](https://www.v2ex.com/)、[LINUX DO](https://linux.do/)、[小众软件](https://www.appinn.com/)和 [NodeLoc](https://www.nodeloc.com/)社区朋友的试用、反馈与分享。

[MIT](LICENSE) © 2026 yuxino
