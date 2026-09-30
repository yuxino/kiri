# Image text editing and unsaved close / 图片文字编辑与关闭保护

Status: Accepted — 2026-09-30. Refs #54.

While an annotation textarea is focused, Ctrl/Cmd+Z and redo belong to its
native text history. Canvas history applies after leaving the text editor.
Escape cancels only the current text edit, preserving committed marks; a
second Escape cancels the capture. IME composition owns its Enter/Escape.
Shift+Enter adds a line, Enter commits (and still completes a capture).
Show a localized hint next to the textarea.

Closing the saved-image editor via Cancel, Escape, Cmd/Ctrl+W or the native
window close warns only when marks, a crop or pending text differ from the
saved baseline. Offer Save & close, Discard, and Keep editing. Failed saves
retain the window and show the error. Undoing to the baseline removes the
warning; existing saved marks and empty new text are not dirty. A successful
Save As updates the baseline too. Export keeps its immutable interaction lock.
Video already owns autosave and close protection; preserve that contract.

文字输入框获得焦点时，撤销与重做使用原生文字历史；退出输入后再操作画布历史。
Esc 先取消本次文字编辑，保留已提交标注；再按 Esc 取消捕获。输入法组合态自行处理
Enter/Esc。Shift+Enter 换行，Enter 提交，截图仍按现有约定完成捕获；输入框附近显示三语提示。

已保存图片的编辑器关闭时，仅在标注、裁剪或尚未提交文字相对保存基线有变化时询问。
提供保存并关闭、放弃及继续编辑；保存失败保持窗口并显示错误。撤销回基线无需提示，
原有标注与新空输入框不算修改；成功另存也更新基线。视频现有自动保存与关闭保护保持不变。
