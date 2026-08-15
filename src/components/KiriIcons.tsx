// KiriIcons — icon set backed by lucide-react. The IconName union keeps the
// SF-Symbol-style names used across the app (data-driven toolbars, menus);
// each maps to a crisp, consistent Lucide glyph.

import React from "react";
import {
  ArrowUpRight,
  Eye,
  Camera,
  Check,
  CircleCheck,
  CircleDot,
  CirclePause,
  CirclePlay,
  Copy,
  Crop,
  Download,
  Film,
  Folder,
  Grid3x3,
  Image,
  Moon,
  MoreHorizontal,
  MousePointer2,
  Pause,
  Pen,
  Pin,
  Play,
  PlaySquare,
  Redo2,
  ScanText,
  Search,
  Slash,
  SlidersHorizontal,
  Square,
  SquareDashed,
  Star,
  Tag,
  TextCursorInput,
  Trash2,
  Type,
  Undo2,
  Video,
  X,
} from "lucide-react";

export type IconName =
  | "cursorarrow" // Select (V)
  | "pencil.tip" // Pen (P)
  | "rectangle.dashed" // Rectangle (R)
  | "line.diagonal" // Line (L)
  | "arrow.up.right" // Arrow (A)
  | "textformat" // Text (T)
  | "square.grid.3x3.fill" // Mosaic (M)
  | "arrow.uturn.backward" // Undo
  | "arrow.uturn.forward" // Redo
  | "checkmark" // Done
  | "checkmark.circle.fill" // notice: copied/saved
  | "record.circle.fill" // notice: recording started
  | "video.fill" // notice: recording saved
  | "trash.slash" // notice: trash emptied
  | "pause.circle.fill" // notice: recording paused
  | "play.circle.fill" // notice: recording resumed
  | "ellipsis.circle" // More
  | "xmark" // Cancel
  | "camera.viewfinder" // Screenshot mode
  | "record.circle" // Record mode
  | "text.viewfinder" // OCR mode
  | "square.dashed" // text background: transparent
  | "moon.fill" // text background: dark
  | "character.textbox" // text context icon
  | "play.fill" // resume recording
  | "pause.fill" // pause recording
  | "stop.fill" // stop recording
  | "crop" // more menu: reselect region
  | "square.and.arrow.down" // more menu: save as
  | "pin" // more menu: pin on screen
  | "slider.horizontal.3" // more menu: open in editor
  | "trash" // more menu: clear annotations
  | "trash.fill" // library: move to trash
  | "doc.on.doc" // library: copy
  | "sparkles.rectangle.stack" // library: convert to GIF
  | "star" // library: favorite
  | "star.fill" // library: favorite (filled)
  | "magnifyingglass" // library: search
  | "folder" // show in finder
  | "photo.on.rectangle" // open in library
  | "play.rectangle" // open video
  | "tag" // library: tag
  | "eye"; // library: view

const ICONS: Record<IconName, React.ComponentType<Record<string, unknown>>> = {
  cursorarrow: MousePointer2,
  "pencil.tip": Pen,
  "rectangle.dashed": SquareDashed,
  "line.diagonal": Slash,
  "arrow.up.right": ArrowUpRight,
  textformat: Type,
  "square.grid.3x3.fill": Grid3x3,
  "arrow.uturn.backward": Undo2,
  "arrow.uturn.forward": Redo2,
  checkmark: Check,
  "checkmark.circle.fill": CircleCheck,
  "record.circle.fill": CircleDot,
  "video.fill": Film,
  "trash.slash": Trash2,
  "pause.circle.fill": CirclePause,
  "play.circle.fill": CirclePlay,
  "ellipsis.circle": MoreHorizontal,
  xmark: X,
  "camera.viewfinder": Camera,
  "record.circle": Video,
  "text.viewfinder": ScanText,
  "square.dashed": SquareDashed,
  "moon.fill": Moon,
  "character.textbox": TextCursorInput,
  "play.fill": Play,
  "pause.fill": Pause,
  "stop.fill": Square,
  crop: Crop,
  "square.and.arrow.down": Download,
  pin: Pin,
  "slider.horizontal.3": SlidersHorizontal,
  trash: Trash2,
  "trash.fill": Trash2,
  "doc.on.doc": Copy,
  "sparkles.rectangle.stack": Film,
  star: Star,
  "star.fill": Star,
  magnifyingglass: Search,
  folder: Folder,
  "photo.on.rectangle": Image,
  "play.rectangle": PlaySquare,
  tag: Tag,
  eye: Eye,
};

export function KiriIcon(props: {
  name: IconName;
  size?: number;
  className?: string;
  strokeWidth?: number;
  style?: React.CSSProperties;
}) {
  const { name, size = 16, className, strokeWidth, style } = props;
  const Glyph = ICONS[name];
  return (
    <Glyph
      size={size}
      className={className}
      strokeWidth={strokeWidth ?? 2}
      style={style}
      aria-hidden="true"
    />
  );
}
