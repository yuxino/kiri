# GIF #67 native before / 原生 before

Debian 13 / Xfce / X11 x86_64，1364×1024 云端虚拟显示器 scale 1，隔离 QA 素材库。不是实体 mixed DPI，不代表正式 Ubuntu/GNOME 或 Mac/Windows。PR #66 initial head 18259a1，CI merge a8bdb4ea423a13416bb1dd8b8c8eeb3ee701b866，deb SHA256 9ca27cdd8a917385b6f73d8e6586a7b699b49b5d4b932c2127832b1a46c745ca。

Library → More Actions → Convert to GIF，两次输出字节相同，SHA256 bfc2ea91624e8c2936f9daef7c5da964a25d24fb3156fc0e29567f5fa77ee6e0。源 MP4 154.156667 s；实际解码 1850 帧，每帧 80 ms，共 148 s。原始 JPEG 图未编辑（交付 .png 名仅更正扩展名）；公开测试文字、灰色截图，不含用户内容。图中 Preview unavailable 是隔离 Debian WebKit 环境限制，未确证产品 bug，不混入 GIF 修复结论。

This is historical native before evidence. The final integration package native after is pending, including replaying the same input MP4 and checking actual GIF frame delays, full decode, dimensions and Library metadata. Rust encoder and platform-native fixture results are recorded separately, never presented as GUI after.
