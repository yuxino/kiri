import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { AnnotationDocumentV1, AppearanceSettings } from "../annotation/model";
import type { CropPixels } from "../annotation/crop.js";

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
  gifEligible: boolean;
}

export type LibraryAvailability = "ready" | "unavailable" | "migrating";

export interface LibraryStatusDto {
  availability: LibraryAvailability;
  locationLabel: string;
  isDefault: boolean;
}

export type AssetAvailability = "ready" | "missing" | "unreadable" | "libraryUnavailable";

export interface AssetAvailabilityDto {
  status: AssetAvailability;
}

export interface PendingRecordingDto {
  id: string;
  createdAt: number;
}

export type RecordingOutputFormat = "mp4" | "gif";

export interface RecordingOptions {
  outputFormat: RecordingOutputFormat;
  usesCountdown: boolean;
  capturesSystemAudio: boolean;
  capturesMicrophone: boolean;
  showsCursor: boolean;
  highlightsClicks: boolean;
}

export const DEFAULT_RECORDING_OPTIONS: RecordingOptions = {
  outputFormat: "mp4",
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

export interface GifConversionStateDto {
  id: string;
  isConverting: boolean;
}

export interface ErrorDto {
  message: string;
  recovery: string | null;
}

export interface ShortcutStatusDto {
  label: string;
  status: "enabled" | "occupied";
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

export interface UpdateCheckDto {
  currentVersion: string;
  latestVersion: string;
  updateAvailable: boolean;
}

export interface AnnotationProjectDto {
  revisionSha256: string;
  state: "none" | "valid" | "invalid";
  documentJson: string | null;
}

export interface EditorUpdateDto {
  revisionSha256: string;
  actionSucceeded: boolean;
}

const EDITOR_REVISION_MISMATCH_ERROR = "The screenshot changed after the editor opened.";

export function isEditorRevisionMismatch(error: unknown): boolean {
  return String(error) === EDITOR_REVISION_MISMATCH_ERROR;
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

export const api = {
  listAssets: (query: string, showingTrash: boolean) =>
    invoke<AssetDto[]>("list_assets", { query, showingTrash }),
  getAsset: (id: string) => invoke<AssetDto>("get_asset", { id }),
  getLibraryStatus: () => invoke<LibraryStatusDto>("get_library_status"),
  chooseLibraryLocation: () => invoke<LibraryStatusDto>("choose_library_location"),
  locateLibrary: () => invoke<LibraryStatusDto>("locate_library"),
  restoreDefaultLibrary: () => invoke<LibraryStatusDto>("restore_default_library"),
  retryLibrary: () => invoke<LibraryStatusDto>("retry_library"),
  revealLibrary: () => invoke<void>("reveal_library"),
  getAssetAvailability: (id: string) =>
    invoke<AssetAvailabilityDto>("get_asset_availability", { id }),
  restoreMissingAsset: (id: string) =>
    invoke<boolean>("restore_missing_asset", { id }),
  removeMissingAsset: (id: string) =>
    invoke<void>("remove_missing_asset", { id }),
  listPendingRecordings: () =>
    invoke<PendingRecordingDto[]>("list_pending_recordings"),
  retryPendingRecordings: () => invoke<number>("retry_pending_recordings"),
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
  openEditor: (id: string) => invoke<void>("open_editor", { id }),
  revealAsset: (id: string) => invoke<void>("reveal_asset", { id }),
  convertToGif: (id: string) => invoke<void>("convert_to_gif", { id }),

  startCapture: () => invoke<CaptureContextDto>("start_capture"),
  cancelCapture: () => invoke<void>("cancel_capture"),
  confirmCapture: async (
    png: Uint8Array,
    annotation: {
      selection: { x: number; y: number; width: number; height: number };
      document: AnnotationDocumentV1;
    },
  ) => {
    const token = await invoke<string>("prepare_capture_annotation", {
      request: {
        selection: annotation.selection,
        documentJson: JSON.stringify(annotation.document),
      },
    });
    return invoke<void>("confirm_capture", png, {
      headers: { "x-kiri-annotation-token": token },
    });
  },
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
  getShortcutStatus: () => invoke<ShortcutStatusDto>("get_shortcut_status"),
  retryShortcut: () => invoke<ShortcutStatusDto>("retry_shortcut"),
  checkForUpdates: () => invoke<UpdateCheckDto>("check_for_updates"),
  openReleasePage: () => invoke<void>("open_release_page"),
  openSettings: (action: string) => invoke<void>("open_settings", { action }),
  quitApp: () => invoke<void>("quit_app"),
  getRecordingOptions: () => invoke<RecordingOptions>("get_recording_options"),
  setRecordingOptions: (options: RecordingOptions) =>
    invoke<void>("set_recording_options", { options }),
  getAnnotationAppearance: () =>
    invoke<AppearanceSettings>("get_annotation_appearance"),
  setAnnotationAppearance: (appearance: AppearanceSettings) =>
    invoke<void>("set_annotation_appearance", { appearance }),

  saveFileDialog: (defaultName: string) =>
    invoke<string | null>("save_file_dialog", { defaultName }),
  getAssetAnnotationProject: (id: string) =>
    invoke<AnnotationProjectDto>("get_asset_annotation_project", { id }),
  updateAsset: (
    id: string,
    png: Uint8Array,
    document: AnnotationDocumentV1,
    options: {
      action: "save" | "saveAs";
      cropPixels: CropPixels | null;
      saveToken: string | null;
      revisionSha256: string;
    },
  ) => {
    return invoke<string>("prepare_asset_annotation", {
      id,
      documentJson: JSON.stringify(document),
      cropPixels: options.cropPixels,
      revisionSha256: options.revisionSha256,
    }).then((annotationToken) => invoke<EditorUpdateDto>("update_asset", png, {
      headers: {
        "x-kiri-asset-id": id,
        "x-kiri-annotation-token": annotationToken,
        "x-kiri-editor-action": options.action === "saveAs" ? "save-as" : "save",
        ...(options.saveToken
          ? { "x-kiri-save-token": options.saveToken }
          : {}),
      },
    }));
  },
};

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

export function onNotice(handler: (notice: NoticeDto) => void): Promise<UnlistenFn> {
  return listen<NoticeDto>("notice", (event) => handler(event.payload));
}

export function onGifConversionState(
  handler: (state: GifConversionStateDto) => void,
): Promise<UnlistenFn> {
  return listen<GifConversionStateDto>("gif-conversion-state", (event) =>
    handler(event.payload),
  );
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

export function mediaUrl(id: string): string {
  return `kiri://media/${id}`;
}
