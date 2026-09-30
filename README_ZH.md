<div align="center">
  <img src="src-tauri/icons/128x128.png" width="112" alt="Kiri 应用图标">
  <h1>Kiri</h1>
  <p>截图、文字识别和录屏，素材保存在本机。</p>
  <p>
    <a href="https://kiri.yuxino.cn">官网</a>
    · <strong>简体中文</strong>
    · <a href="README.md">English</a>
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

Kiri 是一款支持 **macOS、Windows 和 Linux** 的本地截图工具。

<!-- project-demo-v1 -->
## 演示

https://github.com/user-attachments/assets/13742f07-1845-4201-9295-39f83515547f

<!-- /project-demo-v1 -->

## 可以做什么

- **截图与标注**：选取窗口或区域，裁剪、绘图、加文字、打马赛克，保存后仍可重新编辑。画完直线或箭头后，可直接拖动端点调整，或拖动线段移动。已保存的截图可置顶并调整大小，作为参考。
- **文字识别**：通过本地 OCR，复制屏幕或已有图片中的文字；可在文字历史中修正结果，并恢复原始识别文字。
- **录屏**：将区域保存为 MP4 或 GIF；macOS 和 Windows 支持系统声音、麦克风和醒目的红色点击高亮。
- **视频剪辑**：在 macOS 和 Windows 上裁剪、重排、变速，添加标注或隐私遮挡。
- **本地素材库**：导入素材，搜索、打标签、收藏，并从回收站恢复误删内容。可粘贴剪贴板图片后标注或识别文字，也可将选中的截图和录屏复制到指定文件夹，不移动原件。

## 下载

**[下载最新版本 →](https://github.com/yuxino/kiri/releases/latest)**

| 平台 | 安装方式 |
| --- | --- |
| macOS 14+ · Apple 芯片与 Intel | 打开 Universal `.dmg`，将 Kiri 拖入“应用程序”。 |
| Windows 11 · x64 | 运行 `.exe` 安装包，或解压 Portable ZIP 后运行 `kiri.exe`。 |
| Ubuntu 24.04 · x64 · GNOME / X11 | 下载 `.deb`，执行 `sudo apt install ./kiri_VERSION_amd64.deb`，文件名替换为实际下载的版本。 |

macOS 需要“**屏幕与系统音频录制**”权限；点击高亮还需要“**输入监控**”，麦克风录制需要 macOS 15+。应用未经过 Apple 公证，若被拦截，请在“**系统设置 → 隐私与安全性 → 仍要打开**”中放行。Windows 安装包未经过 Authenticode 签名，SmartScreen 可能提示警告。

**Linux：**Wayland 捕获支持连接一台显示器；录屏无声，不支持点击高亮和视频剪辑。配置与录屏控制方式见 [Linux 指南](docs/linux.md)。

**更新：**macOS 与 Windows 安装版使用“**设置 → 关于 → 检查更新**”；Linux 与 Windows 绿色版从 Releases 下载新版安装包。绿色版的设置和素材仍保存在 Windows 用户目录中。

## 开始使用

按 **⇧⌘A**（macOS）或 **Shift+Ctrl+A**（Windows / Linux X11），点击窗口或拖出一个区域。Wayland 使用“截图”按钮，或在桌面设置中绑定 `kiri --capture`。

选择截图、录屏或 OCR。**Enter** 确认截图，**Esc** 取消捕获。截图会复制到剪贴板并保存在本地素材库。 截图工具、取消和完成常驻紧凑主条；“更多操作”展开颜色和精确尺寸，选择绘图工具会展开样式参数。两组控件均保持在当前屏幕内。macOS、Windows 和 X11 可在设置中修改截图快捷键。

在 GNOME Wayland 上，如果首次截图没有出现授权窗口，请打开 Kiri 素材库，在错误提示中点击**请求授权**。在 GNOME 弹窗中允许截图，然后重新发起截图；授权时产生的图片不会进入 Kiri。详见 [Linux 指南](docs/linux.md)。

## 隐私

素材与媒体处理都留在本机。远程 OCR 可选，每次上传前都会询问。可编辑截图会在本地保留源图，其中仍可能包含被标注遮住的内容。详见[隐私说明](PRIVACY_ZH.md)。

## 开发与文档

从源码运行见[贡献指南](CONTRIBUTING.md)，操作说明见[视频剪辑指南](docs/video-editing.zh-CN.md)与[文档索引](docs/README.md)。[反馈问题](https://github.com/yuxino/kiri/issues) · [路线图](ROADMAP.md) · [安全策略](SECURITY.md)。

## 致谢

感谢 [@kerwin2046](https://github.com/kerwin2046) 在 [PR #20](https://github.com/yuxino/kiri/pull/20) 中提供 Linux 初始支持，以及为此投入的时间与心力。

[MIT](LICENSE) © 2026 yuxino
