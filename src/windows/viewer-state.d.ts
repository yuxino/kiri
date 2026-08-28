import type { AssetAvailability, AssetDto } from "../lib/ipc";

export type ViewerState =
  | { kind: "loading" }
  | { kind: "ready"; asset: AssetDto }
  | { kind: "missing"; asset: AssetDto | null }
  | {
      kind: "unreadable";
      asset: AssetDto | null;
      libraryUnavailable: boolean;
      availabilityUnknown: boolean;
    }
  | { kind: "playbackFailed"; asset: AssetDto };

export type ViewerMediaKind = "image" | "video" | null;

export function createViewerLoadingState(): ViewerState;
export function createViewerReadyState(asset: AssetDto): ViewerState;
export function viewerStateAfterFailure(
  asset: AssetDto | null,
  availability: AssetAvailability | null,
): ViewerState;
export function viewerMediaKind(state: ViewerState): ViewerMediaKind;
