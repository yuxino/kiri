// KiriIcons — inline SVG port of the SF Symbols used by the Swift original
// (docs/spec/swift/selection-overlay.md §7.4, §1.2, §8.4). Drawn in a 24×24
// viewBox with 1.5–2 stroke, round caps, matching SF Symbols proportions.

import React from "react";

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
  | "ellipsis.circle" // More
  | "xmark" // Cancel
  | "camera.viewfinder" // Screenshot mode
  | "record.circle" // Record mode
  | "text.viewfinder" // OCR mode
  | "square.dashed" // text background: transparent
  | "moon.fill" // text background: dark
  | "sun.max.fill" // text background: light
  | "lineweight" // stroke context icon
  | "character.textbox" // text context icon
  | "timer" // countdown toggle
  | "checkmark.circle.fill" // notice
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
  | "arrow.uturn.backward" // library: restore (reuse)
  | "sparkles.rectangle.stack" // library: convert to GIF
  | "star" // library: favorite
  | "star.fill" // library: favorite (filled)
  | "magnifyingglass" // library: search
  | "power" // quit
  | "folder" // show in finder
  | "photo.on.rectangle" // open in library
  | "play.rectangle" // open video
  | "tag"; // library: tag

const PATHS: Record<IconName, React.ReactNode> = {
  // --- annotation tools (spec §7.4) ---
  cursorarrow: (
    <>
      <path
        d="M4 3.5 14.5 12.5 10.5 13 8 18 5 11 4 3.5Z"
        fill="currentColor"
        stroke="none"
        opacity="0.9"
      />
    </>
  ),
  "pencil.tip": (
    <>
      <path
        d="M14.5 4.5 19.5 9.5"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
      />
      <path
        d="M12.8 6.2 3.5 20.5h2.6l1.2-2.4 5.4-1.2 2.1-2.2"
        fill="currentColor"
        stroke="currentColor"
        strokeWidth="1.2"
        strokeLinejoin="round"
        opacity="0.95"
      />
    </>
  ),
  "rectangle.dashed": (
    <>
      <rect
        x="4"
        y="6"
        width="16"
        height="12"
        rx="1.5"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.7"
        strokeDasharray="3.5 2.5"
      />
    </>
  ),
  "line.diagonal": (
    <>
      <path
        d="M5 19 19 5"
        stroke="currentColor"
        strokeWidth="2.2"
        strokeLinecap="round"
      />
    </>
  ),
  "arrow.up.right": (
    <>
      <path
        d="M6.5 17.5 17.5 6.5"
        stroke="currentColor"
        strokeWidth="2.2"
        strokeLinecap="round"
      />
      <path
        d="M10 6.5h7.5V14"
        fill="none"
        stroke="currentColor"
        strokeWidth="2.2"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </>
  ),
  textformat: (
    <>
      <path
        d="M12 5v14M8 5h8M6 19h12"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
      />
    </>
  ),
  "square.grid.3x3.fill": (
    <>
      <path
        d="M4 4h4.5v4.5H4zM9.75 4h4.5v4.5h-4.5zM15.5 4H20v4.5h-4.5zM4 9.75h4.5v4.5H4zM9.75 9.75h4.5v4.5h-4.5zM15.5 9.75H20v4.5h-4.5zM4 15.5h4.5V20H4zM9.75 15.5h4.5V20h-4.5zM15.5 15.5H20V20h-4.5z"
        fill="currentColor"
      />
    </>
  ),
  // --- history / confirm (spec §7.4) ---
  "arrow.uturn.backward": (
    <>
      <path
        d="M4.5 8h11a5.5 5.5 0 0 1 0 11H9"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path
        d="M8 4.5 4.5 8 8 11.5"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </>
  ),
  "arrow.uturn.forward": (
    <>
      <path
        d="M19.5 8h-11a5.5 5.5 0 0 0 0 11H15"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path
        d="M16 4.5 19.5 8 16 11.5"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </>
  ),
  checkmark: (
    <path
      d="M5 12.5 10 17.5 19 7"
      fill="none"
      stroke="currentColor"
      strokeWidth="2.4"
      strokeLinecap="round"
      strokeLinejoin="round"
    />
  ),
  "ellipsis.circle": (
    <>
      <circle
        cx="12"
        cy="12"
        r="8.5"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.7"
      />
      <circle cx="8" cy="12" r="1.3" fill="currentColor" />
      <circle cx="12" cy="12" r="1.3" fill="currentColor" />
      <circle cx="16" cy="12" r="1.3" fill="currentColor" />
    </>
  ),
  xmark: (
    <path
      d="M6 6 18 18M18 6 6 18"
      stroke="currentColor"
      strokeWidth="2.2"
      strokeLinecap="round"
    />
  ),
  // --- capture modes (spec §1.2) ---
  "camera.viewfinder": (
    <>
      <rect
        x="3.5"
        y="5"
        width="17"
        height="14"
        rx="3"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.7"
      />
      <circle
        cx="12"
        cy="12"
        r="3.6"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.7"
      />
    </>
  ),
  "record.circle": (
    <>
      <circle
        cx="12"
        cy="12"
        r="8.5"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.7"
      />
      <circle cx="12" cy="12" r="3.8" fill="currentColor" />
    </>
  ),
  "text.viewfinder": (
    <>
      <rect
        x="3.5"
        y="5"
        width="17"
        height="14"
        rx="3"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.7"
      />
      <path
        d="M8 9h8M8 12.5h6M8 16h4"
        stroke="currentColor"
        strokeWidth="1.7"
        strokeLinecap="round"
      />
    </>
  ),
  // --- text background segments (spec §8.4) ---
  "square.dashed": (
    <rect
      x="5"
      y="6"
      width="14"
      height="12"
      rx="1.5"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.7"
      strokeDasharray="3.5 2.5"
    />
  ),
  "moon.fill": (
    <path
      d="M14.5 3.5A8.5 8.5 0 1 0 20.5 15 7 7 0 0 1 14.5 3.5Z"
      fill="currentColor"
    />
  ),
  "sun.max.fill": (
    <>
      <circle cx="12" cy="12" r="4" fill="currentColor" />
      <path
        d="M12 2.5v2.2M12 19.3v2.2M2.5 12h2.2M19.3 12h2.2M5 5l1.6 1.6M17.4 17.4 19 19M19 5l-1.6 1.6M6.6 17.4 5 19"
        stroke="currentColor"
        strokeWidth="1.8"
        strokeLinecap="round"
      />
    </>
  ),
  // --- context icons (spec §7.5) ---
  lineweight: (
    <>
      <path d="M4 6h16" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
      <path d="M4 11h16" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" />
      <path d="M4 16.5h16" stroke="currentColor" strokeWidth="3" strokeLinecap="round" />
    </>
  ),
  "character.textbox": (
    <>
      <rect
        x="3.5"
        y="5.5"
        width="17"
        height="13"
        rx="2"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.6"
      />
      <path
        d="M8 9.5h8M12 9.5V15"
        stroke="currentColor"
        strokeWidth="1.6"
        strokeLinecap="round"
      />
    </>
  ),
  timer: (
    <>
      <circle
        cx="12"
        cy="13"
        r="7"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.7"
      />
      <path
        d="M12 13V8.5M9.5 3.5h5"
        stroke="currentColor"
        strokeWidth="1.7"
        strokeLinecap="round"
      />
    </>
  ),
  "checkmark.circle.fill": (
    <>
      <circle cx="12" cy="12" r="8.5" fill="currentColor" opacity="0.95" />
      <path
        d="M8 12.5 11 15.5 16.5 9.5"
        fill="none"
        stroke="#fff"
        strokeWidth="1.9"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </>
  ),
  "play.fill": (
    <path
      d="M7.5 4.5 18.5 12 7.5 19.5Z"
      fill="currentColor"
    />
  ),
  "pause.fill": (
    <>
      <rect x="6.5" y="4.5" width="4" height="15" rx="1.2" fill="currentColor" />
      <rect x="13.5" y="4.5" width="4" height="15" rx="1.2" fill="currentColor" />
    </>
  ),
  "stop.fill": (
    <rect
      x="6.5"
      y="6.5"
      width="11"
      height="11"
      rx="1.5"
      fill="currentColor"
    />
  ),
  // --- more menu (spec §9.2) ---
  crop: (
    <>
      <path
        d="M7 3v11a3 3 0 0 0 3 3h11"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
      />
      <path
        d="M3 7h4M7 7v4"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path
        d="M17 17v4M21 21h-4"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </>
  ),
  "square.and.arrow.down": (
    <>
      <rect
        x="5"
        y="4"
        width="14"
        height="13"
        rx="2"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.7"
      />
      <path
        d="M12 10v8M8.5 14.5 12 18l3.5-3.5"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.7"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </>
  ),
  pin: (
    <>
      <path
        d="M14 3.5 20.5 10 14.5 12.5 11.5 20.5 9.5 14.5 3.5 11.5Z"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.8"
        strokeLinejoin="round"
      />
      <path
        d="M10.5 13.5 15 9"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.8"
        strokeLinecap="round"
      />
    </>
  ),
  "slider.horizontal.3": (
    <>
      <path d="M4 6.5h16" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
      <circle cx="9" cy="6.5" r="2.4" fill="currentColor" />
      <path d="M4 17.5h16" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
      <circle cx="15" cy="17.5" r="2.4" fill="currentColor" />
    </>
  ),
  trash: (
    <>
      <path
        d="M5 7h14M9 7V4.5h6V7M7 7l1 12.5h8L17 7"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.8"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path d="M10 11v5M14 11v5" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
    </>
  ),
  "trash.fill": (
    <>
      <path
        d="M5 7h14M9 7V4.5h6V7M7 7l1 12.5h8L17 7Z"
        fill="currentColor"
      />
    </>
  ),
  "doc.on.doc": (
    <>
      <rect
        x="8"
        y="5"
        width="11"
        height="13"
        rx="2"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.7"
      />
      <rect
        x="4"
        y="9"
        width="11"
        height="13"
        rx="2"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.7"
        opacity="0.6"
      />
    </>
  ),
  "sparkles.rectangle.stack": (
    <>
      <rect
        x="4"
        y="6"
        width="16"
        height="12"
        rx="2.5"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.6"
      />
      <path
        d="M12 8.5l.9 2.1 2.1.9-2.1.9-.9 2.1-.9-2.1-2.1-.9 2.1-.9z"
        fill="currentColor"
        opacity="0.9"
      />
    </>
  ),
  star: (
    <path
      d="M12 4l2.1 4.6 5 .6-3.7 3.5 1 5L12 15.4 7.6 17.7l1-5L4.9 9.2l5-.6z"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.7"
      strokeLinejoin="round"
    />
  ),
  "star.fill": (
    <path
      d="M12 4l2.1 4.6 5 .6-3.7 3.5 1 5L12 15.4 7.6 17.7l1-5L4.9 9.2l5-.6z"
      fill="currentColor"
    />
  ),
  magnifyingglass: (
    <>
      <circle
        cx="11"
        cy="11"
        r="6.5"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.8"
      />
      <path
        d="M15.5 15.5 20 20"
        stroke="currentColor"
        strokeWidth="1.8"
        strokeLinecap="round"
      />
    </>
  ),
  power: (
    <>
      <path
        d="M12 4v7"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.9"
        strokeLinecap="round"
      />
      <path
        d="M7.5 6.5a7.5 7.5 0 1 0 9 0"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.7"
        strokeLinecap="round"
      />
    </>
  ),
  folder: (
    <path
      d="M3.5 6.5h6l2 2.5h9V18a1.5 1.5 0 0 1-1.5 1.5H5A1.5 1.5 0 0 1 3.5 18z"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.7"
      strokeLinejoin="round"
    />
  ),
  "photo.on.rectangle": (
    <>
      <rect
        x="3.5"
        y="7"
        width="15"
        height="11"
        rx="2"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.6"
      />
      <circle cx="8" cy="10.5" r="1.3" fill="currentColor" />
      <path
        d="M6.5 15l2.5-2.5 2 2 3-3 2.5 2.5"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </>
  ),
  "play.rectangle": (
    <>
      <rect
        x="3.5"
        y="5.5"
        width="17"
        height="13"
        rx="2.5"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.6"
      />
      <path d="M10 9.5v5l4-2.5z" fill="currentColor" />
    </>
  ),
  tag: (
    <>
      <path
        d="M4 4h6l10 10-6 6L4 10z"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.7"
        strokeLinejoin="round"
      />
      <circle cx="8.5" cy="8.5" r="1.3" fill="currentColor" />
    </>
  ),
};

export function KiriIcon(props: {
  name: IconName;
  size?: number;
  className?: string;
  style?: React.CSSProperties;
}) {
  const { name, size = 13, className, style } = props;
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      className={className}
      style={{ display: "block", ...style }}
      aria-hidden="true"
    >
      {PATHS[name]}
    </svg>
  );
}
