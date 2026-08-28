export function createViewerLoadingState() {
  return { kind: "loading" };
}

export function createViewerReadyState(asset) {
  return { kind: "ready", asset };
}

export function viewerStateAfterFailure(asset, availability) {
  if (availability == null) {
    return asset
      ? { kind: "playbackFailed", asset }
      : {
          kind: "unreadable",
          asset: null,
          libraryUnavailable: false,
          availabilityUnknown: true,
        };
  }
  if (availability === "missing") return { kind: "missing", asset };
  if (availability === "unreadable" || availability === "libraryUnavailable") {
    return {
      kind: "unreadable",
      asset,
      libraryUnavailable: availability === "libraryUnavailable",
      availabilityUnknown: false,
    };
  }
  return asset
    ? { kind: "playbackFailed", asset }
    : {
        kind: "unreadable",
        asset: null,
        libraryUnavailable: false,
        availabilityUnknown: false,
      };
}

export function viewerMediaKind(state) {
  if (state.kind !== "ready") return null;
  return state.asset.kind === "video" ? "video" : "image";
}
