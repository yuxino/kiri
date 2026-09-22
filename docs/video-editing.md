# Edit a video in Kiri

**English** · [简体中文](video-editing.zh-CN.md)

Open a video from the library and choose **Trim & Export** above the picture. This guide describes the current source; check the release notes for the controls available in your installed version.

A project edits one source video. You can cut that video into clips and reorder them, but importing several videos does not combine them into one timeline.

## Import a recording or local file

Kiri recordings appear in the library. For other files, choose **Import media** or drag PNG, JPEG, WebP, MP4, or MOV files into the library. Imported images open in the image editor; imported videos use the video editor.

Imports keep the original names and appear at the top of the library after filters are cleared. Images are converted to PNG with their orientation applied; videos are copied. The original files remain unchanged. Each import accepts up to 32 files. Images must fit the decoder memory limit, be no larger than 32 MB, and have no edge above 8192 pixels; videos are limited to 8 GB.

## Cut and adjust clips

- Click the timeline to seek. Drag clip edges to trim, split at the playhead, delete an unwanted clip, or drag clips into a new order.
- Select a clip to set its speed from 0.25–4×. This changes both the edit preview and exported video.
- Adjust the timeline height as needed. Additional tracks scroll within it to leave room for the picture.
- Undo and redo cover cuts, clip order, speed, effects, annotations, and timing. Delete acts on the selected clip, annotation, sticker, or effect.

Playback controls sit outside the picture. The seek bar supports keyboard input and shows times on hover. Scrubbing pauses the video and restores its previous play state on release. Playback speed presets and a custom 0.1–8× setting are remembered for viewing only; they do not change export speed.

## Add annotations, stickers, and effects

The editor shows the screenshot tools: pen, rectangle, line, arrow, editable text, and pixel or blur mosaic brushes. They reuse your saved appearance settings; the initial color is red. Video mosaics can follow a freehand stroke or fill a rectangle or ellipse.

A new mark is selected after drawing. Use the inspector to change its color, stroke width, or mosaic settings, and drag its handles to reshape it. Choose **Edit text** to change wording; font, color, and background changes appear immediately. Text tracks show their content. Escape cancels the current text edit.

Add static PNG, JPEG, or WebP stickers from the toolbar. Stickers preserve transparency and resize without stretching. You can select and drag annotations, stickers, and masks directly on the picture.

Effects include a timed 1.5–4× zoom with adjustable entry and exit, blur or pixel masks, and solid masks with a custom color. Drag a zoomed image to reframe it. You can also add a spotlight, crop with a chosen background and padding while keeping the aspect ratio, or set fade-in and fade-out duration and color. Less frequently used options are under **Finishing touches**. The picture shows changes as you make them.

Each annotation, sticker, and effect has its own timing. Drag a track's middle to move it in time, or its ends to change duration. Drag its left grip vertically to change layer order; the list scrolls as you approach an edge. Upper overlays cover lower ones in preview and export. Whole-picture adjustments have a separate ordered group. Timing summaries follow the finished-video timeline after cuts and speed changes.

## Save and continue later

Edits, timed layers, stickers, and your current position save automatically to the local library. Reopen the source video to continue, or press Cmd/Ctrl+S to save immediately. Closing the window finishes the current text edit and waits for saving. If saving fails, you can retry or continue editing; explicitly closing without saving keeps the last successfully saved project.

The editable project and exported MP4 are separate. Exporting does not replace the source video.

## Export an MP4

Choose **Export** in the upper-right toolbar. The quality menu shows the actual output dimensions:

| Setting | Output size |
| --- | --- |
| High quality | Keeps source dimensions |
| Everyday sharing | Longest edge up to 1080 pixels |
| Compact file | Longest edge up to 720 pixels |

Small sources are never enlarged. File size depends on the source and edited duration. Export creates a new library copy, shows encoding progress, and can be cancelled before the final library save begins. Cancelling keeps your project and adds no partial video to the library.

Exports retain synchronized audio. macOS preserves audio pitch when clip speed changes; Windows changes the pitch along with the speed. On Windows, video effects currently require a source without rotation metadata. Kiri recordings meet that requirement.

[Back to Kiri](../README.md)
