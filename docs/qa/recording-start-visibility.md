# Linux recording startup visibility / Linux 录制启动退屏

Cloud QA reproduced on integration head `b8c35876f6b0f8167f0da19761e56f3201139cc3`,
actual CI source `f72b2e6701a9f2cea951fa03047fe5546547c8a2`, deb SHA256
`a95119b6a5ee1230f3e9a8f97b6daf019ef6794c229314d7779d6dcea18ebb3a`.
Debian13/Xfce/X11, virtual display, same public 560×300 region, countdown off.
Two recordings fully decoded: 648 frames / 21.582 s and 716 frames / 23.830667 s.
Only frame zero contained the selection border, handles and size label; each
had 22,108 changed pixels versus the clean final frame. Original media/images
remain private Library material pending separate permission to publish.

Tauri runtime-wry 2.11.4 queues Close events even when called on the main
thread. Waiting for the callback to return does not acknowledge native unmap.
Without a countdown the Linux ximagesrc worker can start before that queue is
processed; its first sample is correctly retained by the recording timeline.

The Linux startup now directly hides each GTK capture/countdown/feedback
widget on the GTK main thread, synchronizes the display connection, verifies
hidden state, then requests ordinary close where appropriate. A failure stops
startup rather than recording visible capture UI. No first frame is discarded,
no fixed startup delay is introduced, and recording fps/timestamps are unchanged.
This is a display-protocol synchronization boundary, not a universal compositor
presentation guarantee; exact-package X11/Wayland replay remains required.

The X11 native regression disables the countdown and fully decodes every frame,
including zero. It checks source pixel errors and all selection edges so a thin
border cannot hide in a whole-frame average. Its pause/resume sentinel scenario
remains in CI; cloud pause/resume acceptance is still incomplete and distinct.
No local Mac native recording or Rust build is used for this follow-up.

新包验收：同一公开文字、同一560×300选区、关闭倒计时，Start→Stop两次；
全帧解码、检查第0帧与所有后续帧均无Kiri选区控件。另覆盖开启倒计时启动，
以及取消后再启动的session隔离。截图、关闭、裁剪等既有通过仍按旧包版本标注。
