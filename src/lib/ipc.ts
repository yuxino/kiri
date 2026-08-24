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

export type OcrProviderPreset = "aliyunBailian" | "openAi" | "customOpenAi";

type OcrProtocol = "openAiChatCompletions";

export type OcrEngineRef =
  | { kind: "local" }
  | { kind: "profile"; profileId: string };

export interface OcrProviderProfileDto {
  id: string;
  revision: number;
  name: string;
  provider: OcrProviderPreset;
  protocol: OcrProtocol;
  baseUrl: string;
  model: string;
  hasApiKey: boolean;
}

export interface OcrProviderSettingsDto {
  schemaVersion: number;
  activeEngine: OcrEngineRef;
  profiles: OcrProviderProfileDto[];
  warning?: string | null;
}

export interface SaveOcrProviderProfileRequest {
  id?: string;
  revision?: number;
  name: string;
  provider: OcrProviderPreset;
  protocol: OcrProtocol;
  baseUrl: string;
  model: string;
  apiKey?: string;
}

interface PreparedOcrProfileDto {
  id: string;
  revision: number;
  name: string;
  provider: OcrProviderPreset;
  origin: string;
  model: string;
  hasApiKey: boolean;
}

export interface PreparedOcrRequestDto {
  requestId: string;
  engine: OcrEngineRef;
  imageWidth: number;
  imageHeight: number;
  byteLength: number;
  profile?: PreparedOcrProfileDto | null;
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

export const api = {
  listAssets: (query: string, showingTrash: boolean) =>
    invoke<AssetDto[]>("list_assets", { query, showingTrash }),
  getAsset: (id: string) => invoke<AssetDto>("get_asset", { id }),
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
  batchMoveToTrash: (ids: string[]) => invoke<void>("batch_move_to_trash", { ids }),
  batchRestore: (ids: string[]) => invoke<void>("batch_restore", { ids }),
  batchPermanentlyDelete: (ids: string[]) => invoke<void>("batch_permanently_delete", { ids }),
  batchSetFavorite: (ids: string[], favorite: boolean) =>
    invoke<void>("batch_set_favorite", { ids, favorite }),
  showConfirmDialog: (
    kind: string,
    title: string,
    message: string,
    confirmLabel: string,
    ids?: string[],
  ) => invoke<void>("show_confirm_dialog", { kind, title, message, confirmLabel, ids }),
  setLanguage: (language: string) => invoke<void>("set_language", { language }),
  copyAsset: (id: string) => invoke<void>("copy_asset", { id }),
  openAsset: (id: string) => invoke<void>("open_asset", { id }),
  revealAsset: (id: string) => invoke<void>("reveal_asset", { id }),
  convertToGif: (id: string) => invoke<void>("convert_to_gif", { id }),

  startCapture: () => invoke<CaptureContextDto>("start_capture"),
  cancelCapture: () => invoke<void>("cancel_capture"),
  confirmCapture: (png: Uint8Array, action: string) =>
    invoke<void>("confirm_capture", png, {
      headers: { "x-kiri-capture-action": action },
    }),
  copyText: (text: string) => invoke<void>("copy_text", { text }),
  getOcrProviderSettings: () =>
    invoke<OcrProviderSettingsDto>("get_ocr_provider_settings"),
  saveOcrProviderProfile: (request: SaveOcrProviderProfileRequest) =>
    invoke<OcrProviderSettingsDto>("save_ocr_provider_profile", { request }),
  deleteOcrProviderProfile: (profileId: string, profileRevision: number) =>
    invoke<OcrProviderSettingsDto>("delete_ocr_provider_profile", {
      profileId,
      profileRevision,
    }),
  setActiveOcrEngine: (engine: OcrEngineRef) =>
    invoke<OcrProviderSettingsDto>("set_active_ocr_engine", { engine }),
  prepareOcrRequest: (selection: RectDto) =>
    invoke<PreparedOcrRequestDto>("prepare_ocr_request", { selection }),
  recognizePreparedOcrLocal: (requestId: string) =>
    invoke<string>("recognize_prepared_ocr_local", { requestId }),
  recognizePreparedOcrRemote: (
    requestId: string,
    profileId: string,
    profileRevision: number,
  ) =>
    invoke<string>("recognize_prepared_ocr_remote", {
      requestId,
      profileId,
      profileRevision,
    }),
  cancelPreparedOcr: (requestId: string) =>
    invoke<void>("cancel_prepared_ocr", { requestId }),

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
  updateAsset: (
    id: string,
    png: Uint8Array,
    options: { copyToClipboard: boolean; savePath: string | null },
  ) =>
    invoke<void>("update_asset", png, {
      headers: {
        "x-kiri-asset-id": id,
        "x-kiri-copy-to-clipboard": options.copyToClipboard ? "1" : "0",
        ...(options.savePath
          ? { "x-kiri-save-path": encodeURIComponent(options.savePath) }
          : {}),
      },
    }),
};

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

export function onNotice(handler: (notice: NoticeDto) => void): Promise<UnlistenFn> {
  return listen<NoticeDto>("notice", (event) => handler(event.payload));
}

export function onError(handler: (error: ErrorDto) => void): Promise<UnlistenFn> {
  return listen<ErrorDto>("error", (event) => handler(event.payload));
}

export function onLibraryChanged(handler: () => void): Promise<UnlistenFn> {
  return listen("library-changed", handler);
}

export function onAssetContentChanged(handler: (assetId: string) => void): Promise<UnlistenFn> {
  return listen<string>("asset-content-changed", (event) => handler(event.payload));
}

export function onRecordingState(
  handler: (state: RecordingState) => void,
): Promise<UnlistenFn> {
  return listen<RecordingState>("recording-state", (event) => handler(event.payload));
}

export function pinImageUrl(id: string): string {
  return `kiri://pin/${id}.png`;
}

export function mediaUrl(id: string): string {
  return `kiri://media/${id}`;
}
