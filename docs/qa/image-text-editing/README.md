# #54 actual component comparison / 实际组件对比

These are untouched PNG screenshots of the actual built frontend in a single isolated headless Chromium on macOS arm64. IPC, source image and native window-close events use a documentation fixture. This is not native Mac/Windows/Linux acceptance or real IME coverage. All visible source content is the public repository sample; no user data.

相同 1280×720 logical viewport、scale 1、英文 UI、公开 sample.html 背景；截图选区 (180,140)→(1050,570)，矩形 (230,180)→(500,250)，文字 test123。按 Cmd+Z 后拍 undo 图；恢复文字并重新进入输入，再按 Esc 拍 Escape 图。图片编辑器公开 1000×600 灰色图、文字 unsaved text、点 Cancel 后拍关闭图。图片为原始字节，未编辑。

Before: PR #66 initial CI merge source a8bdb4ea423a13416bb1dd8b8c8eeb3ee701b866, includes initial #66 head 18259a1. This build retained the pre-fix annotation behavior. After: fix/image-text-editing source commit 0bf8303cdf44b593241a541749878e3e84aa7f7e. Component regression covers text undo/redo, multiline/Enter, composition event routing only, two-level Escape, pending text close warning, Keep editing, Save As cancel, failed save/retry, save/discard, undo to saved baseline and successful Save As.

#54 云端 native 官方 v1.6.6 before 在 issue comment-5911953475 单独记录；这组图片不把该 native 结果冒充浏览器图来源。最终集成包 native 回归待验。
