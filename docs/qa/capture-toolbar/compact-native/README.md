# 紧凑工具栏实际验收 / Compact toolbar native acceptance

Debian 13 / Xfce / X11，云端虚拟 DUMMY0 1364×1024@(0,0) + DUMMY1 800×600@(1364,0)，scale 1、英文。选区副屏局部坐标 (650,420)→(790,580)。不是实体 mixed DPI；Debian/Xfce 在正式 Ubuntu/GNOME 支持范围外。

PR #66 head `d6cf7e46efa2527911eeb17989de12b3f09b6107`；CI merge source `8b4106d61eb13839bf9a0833d71d08a484dfe3cd`；deb SHA256 `62e19e40a4176348a18cd69489c72b1b1e97badd594f71663cef160f999f02ae`。全部 10 项 CI 通过。

主条约 464 px，More/Text/Mosaic 面板及 Done/Cancel 都在选区上方可达；真实键盘聚焦 More 后 Enter/Space 仅切换面板，尺寸框 Enter 仅应用尺寸；鼠标点击 Done 导出 140×160 PNG。原图未经编辑，交付 .png 实际是 JPEG，公开仅更正扩展名；导出文件是 PNG。四张图及导出 SHA256 已逐一核对报告。

初次 AX invoke More 并未建立键盘焦点，该次 Enter 保存；随后通过 Tab/ShiftTab 确认焦点重试通过，不把 AX invoke 当作键盘聚焦。桌面已恢复 1364×1024，DUMMY1 inactive。Mac/Windows native、正式 Ubuntu 双屏、实体 mixed DPI 仍待验。

Actual native application capture on a cloud virtual X11 desktop, not a browser fixture or physical mixed-DPI validation. Original bytes and report are preserved. The independently saved mouse-Done output is 140×160 PNG.
