# kiri expanded interface demonstration

This replaces the earlier three-to-four-scene, 2x demo with **10 recorded scenes** from the actual production frontend at `d3ebccbe86f1df4990651ce1e2569a60d88646fb`.

**Pacing:** every source action interval is played at **10x**, followed by a **0.8-second result hold**. The final clip lasts 21.93 seconds; it is not a uniformly accelerated full video. The GIF and MP4 share the same timing. The fast-forward and sample-data labels remain visible.

**Scope:** Annotation UI · original sample image · no OS screen capture. The browser harness substitutes native API boundaries with original local examples; this is not native macOS/Windows end-to-end validation. No user credentials, personal files, live provider output or upstream comic content are included. Satori question composition is shown without submitting an AI request; no answer is fabricated.

## Scenes

1. 01 / Rectangle annotation / 矩形标注：先圈出重点
2. 02 / Adjust the annotation style / 调整接下来绘制的线宽
3. 03 / Draw an arrow / 箭头：把视线引向关键细节
4. 04 / Add an editable text annotation / 文字标注：补上一句说明
5. 05 / Freehand pen / 画笔：随手划一道重点
6. 06 / Pixel mosaic with adjustable strength / 马赛克：遮住不想展示的区域
7. 07 / Switch to the blur brush / 也可以换成柔和的模糊
8. 08 / Undo an edit / 一步撤销，继续修改
9. 09 / Redo and review / 重做后，回到完整标注
10. 10 / Inspect crop controls without exporting / 裁剪工具也在同一处

## Files

`preview.gif` is the inline README preview; `demo.mp4` is the complete silent H.264 video. `poster.png` is an actual recorded result frame. `provenance.json` records source, pacing, scene boundaries and media hashes.

The reproducible documentation-only recorder is `yuxino/kiri/docs/demos/capture/expanded.py`. It is not loaded by the applications. The shipped application code, versions, signing and update workflows remain unchanged.
