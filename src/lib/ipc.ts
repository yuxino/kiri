import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// ---------------------------------------------------------------------------
// Shared DTO types (mirror src-tauri/src/commands.rs)
// ---------------------------------------------------------------------------

interface RectDto {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface CaptureContextDto {
  displayWidth: number;
  displayHeight: number;
  scale: number;
  pixelWidth: number;
  pixelHeight: number;
  windowRects: RectDto[];
  sourceApplication: string | null;
}

export interface AssetDto {
  id: string;
  kind: "image" | "video" | "gif";
  createdAt: number;
  filename: string;
  title: string | null;
  tags: string[];
  pixelWidth: number;
  pixelHeight: number;
  duration: number | null;
  sourceApplication: string | null;
  isFavorite: boolean;
  trashedAt: number | null;
  filePath: string;
  gifEligible: boolean;
}

export interface RecordingOptions {
  usesCountdown: boolean;
  capturesSystemAudio: boolean;
  capturesMicrophone: boolean;
  showsCursor: boolean;
  highlightsClicks: boolean;
}

export const DEFAULT_RECORDING_OPTIONS: RecordingOptions = {
  usesCountdown: true,
  capturesSystemAudio: false,
  capturesMicrophone: false,
  showsCursor: true,
  highlightsClicks: false,
};

export interface RecordingState {
  isStarting: boolean;
  isRecording: boolean;
  isPaused: boolean;
  isTransitioning: boolean;
  isFinalizing: boolean;
  elapsed: number;
  elapsedLabel: string;
}

export interface NoticeDto {
  id: string;
  title: string;
  symbol: string;
}

export interface ErrorDto {
  message: string;
  recovery: string | null;
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

export const api = {
  listAssets: (query: string, showingTrash: boolean) =>
    invoke<AssetDto[]>("list_assets", { query, showingTrash }),
  setFavorite: (id: string, favorite: boolean) =>
    invoke<void>("set_favorite", { id, favorite }),
  renameAsset: (id: string, title: string) =>
    invoke<void>("rename_asset", { id, title }),
  setTags: (id: string, tags: string[]) =>
    invoke<void>("set_tags", { id, tags }),
  moveToTrash: (id: string) => invoke<void>("move_to_trash", { id }),
  restoreAsset: (id: string) => invoke<void>("restore_asset", { id }),
  permanentlyDelete: (id: string) => invoke<void>("permanently_delete", { id }),
  emptyTrash: () => invoke<void>("empty_trash"),
  copyAsset: (id: string) => invoke<void>("copy_asset", { id }),
  openAsset: (id: string) => invoke<void>("open_asset", { id }),
  revealAsset: (id: string) => invoke<void>("reveal_asset", { id }),
  convertToGif: (id: string) => invoke<void>("convert_to_gif", { id }),

  startCapture: () => invoke<CaptureContextDto>("start_capture"),
  cancelCapture: () => invoke<void>("cancel_capture"),
  confirmCapture: (png: number[], action: string) =>
    invoke<void>("confirm_capture", { request: { png, action } }),
  recognizeText: (png: number[]) => invoke<string>("recognize_text", { png }),
  copyText: (text: string) => invoke<void>("copy_text", { text }),

  startRecordingFlow: (region: RectDto, options: RecordingOptions) =>
    invoke<void>("start_recording_flow", { request: { region, options } }),
  cancelRecordingFlow: () => invoke<void>("cancel_recording_flow"),
  beginRecording: () => invoke<void>("begin_recording"),
  pauseRecording: () => invoke<void>("pause_recording"),
  resumeRecording: () => invoke<void>("resume_recording"),
  stopRecording: () => invoke<void>("stop_recording"),

  micSupported: () => invoke<boolean>("mic_supported"),
  getShortcutLabel: () => invoke<string>("get_shortcut_label"),
  openSettings: (action: string) => invoke<void>("open_settings", { action }),
  quitApp: () => invoke<void>("quit_app"),
  getRecordingOptions: () => invoke<RecordingOptions>("get_recording_options"),
  setRecordingOptions: (options: RecordingOptions) =>
    invoke<void>("set_recording_options", { options }),

  saveFileDialog: (defaultName: string) =>
    invoke<string | null>("save_file_dialog", { defaultName }),
  updateAsset: (id: string, request: { png: number[]; copyToClipboard: boolean; savePath: string | null }) =>
    invoke<void>("update_asset", { id, request }),
};

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

export function dbg(message: string): void {
  void invoke("frontend_log", { message }).catch(() => {});
}

export function onNotice(handler: (notice: NoticeDto) => void): Promise<UnlistenFn> {
  return listen<NoticeDto>("notice", (event) => handler(event.payload));
}

export function onError(handler: (error: ErrorDto) => void): Promise<UnlistenFn> {
  return listen<ErrorDto>("error", (event) => handler(event.payload));
}

export function onLibraryChanged(handler: () => void): Promise<UnlistenFn> {
  return listen("library-changed", handler);
}

export function onRecordingState(
  handler: (state: RecordingState) => void,
): Promise<UnlistenFn> {
  return listen<RecordingState>("recording-state", (event) => handler(event.payload));
}

export function frozenImageUrl(): string {
  return "kiri://capture/frozen.png";
}

export function pinImageUrl(id: string): string {
  return `kiri://pin/${id}.png`;
}
