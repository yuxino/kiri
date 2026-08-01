<div align="center">
  <img src="Resources/Assets/kiri-icon.png" width="112" alt="kiri">
  <h1>kiri</h1>
  <p>轻快、可找回的 macOS 截图工具</p>
  <p>
    <strong>早期预览版</strong>
    · <a href="README_EN.md">English</a>
    · <a href="README_JA.md">日本語</a>
  </p>
</div>

`kiri` 来自日语「切り取り」，意思是截取、裁切。

它是一款原生 macOS 视觉捕获工具：快速截取屏幕区域、添加标注并复制，
同时自动把结果放进本地素材库。即使剪贴板内容被覆盖，刚刚的截图仍然可以找回。

## 现在可以做什么

- 使用独占的 **⌥⌘2**（默认）或 **⌃⇧2** 开始区域截图
- 冻结当前屏幕，自动吸附最前方窗口，也可自由拖出选区
- 用八个手柄调整选区，双击或 Return 直接进入就地标注
- 使用带颜色区分的画笔、矩形、箭头、文字和马赛克工具
- 按 Return 立即复制，也可保存、贴图或进入完整编辑器
- 通过右下角 Quick Access 预览、拖拽分享或继续处理
- 每次完成只保存一份原图，后台自动进入历史记录
- 在本地素材库中搜索、收藏和再次复制
- 将截图移入废纸篓，再恢复或永久删除
- 记录来源应用、尺寸、类型和创建时间

> kiri 目前处于早期源码预览阶段。录屏、GIF 和滚动长截图已经进入数据模型和路线图，
> 但还没有在此版本开放。

## 从源码运行

需要 macOS 14+ 和 Swift 6。

```bash
git clone https://github.com/yuxino/kiri.git
cd kiri
swift run kiri-core-tests
./scripts/package-app.sh
open dist/kiri.app
```

打包脚本会优先选择 Apple Development、Developer ID 或 kiri 已有的稳定本地证书，
并拒绝静默使用会破坏录屏授权的临时签名。也可用
`KIRI_CODESIGN_IDENTITY="证书名称" ./scripts/package-app.sh` 明确指定证书；只有不需要
持久录屏权限的临时构建才应设置 `KIRI_ALLOW_ADHOC_SIGNING=1`。

第一次截图时，macOS 会要求授予“屏幕与系统音频录制”权限。授权后如果截图仍不可用，
请退出并重新打开 kiri。kiri 每次运行最多调用一次系统授权请求；如果权限尚未生效，
提示条只会提供“打开设置”或“退出 kiri”，不会在每次截图时重复弹出系统窗口。

## 素材保存在哪里

kiri 默认把素材保存在：

```text
~/Library/Application Support/kiri/
```

所有内容默认只留在本机，不会自动上传。删除的素材会先进入 kiri 自己的废纸篓。

## 接下来

- **v0.1**：实体多显示器验收、正式签名与 Apple 公证
- **v0.2**：区域录屏、MP4 导出和短视频转 GIF
- **v0.3**：滚动长截图、自动拼接和接缝修正

完整计划见 [ROADMAP.md](ROADMAP.md)。

## 参与 kiri

Issue 和 Pull Request 都欢迎。开始之前请阅读
[CONTRIBUTING.md](CONTRIBUTING.md)；安全问题请按
[SECURITY.md](SECURITY.md) 私下报告。

[MIT](LICENSE) © 2026 yuxino
