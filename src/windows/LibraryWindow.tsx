// LibraryWindow — the main window: asset grid, search, sections, trash,
// notices, and error recovery. Port of LibraryView.swift + AppModel.

import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import {
  api,
  mediaUrl,
  onAssetContentChanged,
  onError,
  onGifConversionState,
  onLibraryChanged,
  onNotice,
  type AssetDto,
  type AssetAvailability,
  type ErrorDto,
  type LibraryStatusDto,
  type NoticeDto,
  type ShortcutStatusDto,
} from "../lib/ipc";
import { t, fmt } from "../i18n";
import brandIcon from "../../src-tauri/icons/128x128.png";
import { KiriIcon, type IconName } from "../components/KiriIcons";
import { kiriResourceUrl } from "../lib/kiri-resource-url.js";
import {
  getAvailableShortcutLabel,
  getLibraryBandRect,
  getLibraryCardInteraction,
  getLibraryCardPrimaryAction,
  getLibraryContentPoint,
  getMenuFocusIndex,
} from "./library-card-interaction.js";

const SettingsView = React.lazy(() =>
  import("../settings/SettingsView").then((module) => ({ default: module.SettingsView })),
);

type Section = "library" | "trash";
type Destination = "captures" | "settings";

function thumbnailUrl(id: string, revision: number): string {
  return kiriResourceUrl("thumbnail", [id], { v: revision });
}

/** Groups assets by calendar day, newest group first. */
function groupByDay(assets: AssetDto[]): { key: string; label: string; assets: AssetDto[] }[] {
  const groups = new Map<string, AssetDto[]>();
  const now = new Date();
  const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  const dayMs = 86_400_000;
  for (const asset of assets) {
    const created = new Date(asset.createdAt); // ms epoch
    const start = new Date(
      created.getFullYear(),
      created.getMonth(),
      created.getDate(),
    ).getTime();
    const key = String(start);
    if (!groups.has(key)) groups.set(key, []);
    groups.get(key)!.push(asset);
  }
  const sorted = [...groups.entries()].sort((a, b) => Number(b[0]) - Number(a[0]));
  return sorted.map(([start, items]) => {
    const startMs = Number(start);
    const diffDays = Math.round((startOfToday - startMs) / dayMs);
    let label: string;
    if (diffDays <= 0) label = t("Today");
    else if (diffDays === 1) label = t("Yesterday");
    else label = new Date(startMs).toLocaleDateString(undefined, {
      month: "short",
      day: "numeric",
    });
    return { key: start, label, assets: items };
  });
}

export function LibraryWindow() {
  const [assets, setAssets] = useState<AssetDto[]>([]);
  const [section, setSection] = useState<Section>("library");
  const [destination, setDestination] = useState<Destination>("captures");
  const [query, setQuery] = useState("");
  const [loaded, setLoaded] = useState(false);
  const [libraryStatus, setLibraryStatus] = useState<LibraryStatusDto | null>(null);
  const [libraryStatusError, setLibraryStatusError] = useState(false);
  const [libraryRecoveryBusy, setLibraryRecoveryBusy] = useState(false);
  const [pendingRecordingCount, setPendingRecordingCount] = useState(0);
  const [pendingRetryBusy, setPendingRetryBusy] = useState(false);
  const [assetAvailability, setAssetAvailability] = useState<Record<string, AssetAvailability>>({});
  const [thumbnailRevisions, setThumbnailRevisions] = useState<Record<string, number>>({});
  const [notice, setNotice] = useState<NoticeDto | null>(null);
  const [error, setError] = useState<ErrorDto | null>(null);
  const [menuFor, setMenuFor] = useState<string | null>(null);
  const [gifConversionIds, setGifConversionIds] = useState<Set<string>>(new Set());
  // Menu anchor in viewport coordinates (mouse position on right-click, or
  // the ⋯ button's corner), so the menu appears where the user looked.
  const [menuPos, setMenuPos] = useState<{ x: number; y: number } | null>(null);
  const [shortcutStatus, setShortcutStatus] = useState<ShortcutStatusDto | null>(null);
  const [kindFilter, setKindFilter] = useState<"all" | "image" | "video" | "gif">("all");
  const [favoritesOnly, setFavoritesOnly] = useState(false);
  const [tagFilter, setTagFilter] = useState<string | null>(null);
  // Batch selection starts only from a rubber-band drag. Ordinary card clicks
  // open the asset and never introduce selection chrome.
  const [selection, setSelection] = useState<Set<string>>(new Set());
  // Drag-to-select (rubber band): pointer origin + current corner in the
  // scroll container's coordinates; null when not band-selecting.
  const [band, setBand] = useState<{ x0: number; y0: number; x1: number; y1: number } | null>(null);
  const gridScrollRef = useRef<HTMLDivElement>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);
  const menuTriggerRef = useRef<HTMLButtonElement | null>(null);
  const refreshGenerationRef = useRef(0);
  const queryRef = useRef(query);
  queryRef.current = query;
  // Card id → DOM element, registered while rendering, used for hit tests.
  const cardElsRef = useRef<Map<string, HTMLDivElement>>(new Map());

  // View-layer filters shared by Library and Trash, applied on top of the
  // backend text search.
  const filteredAssets = useMemo(() => {
    return assets.filter((asset) => {
      if (favoritesOnly && !asset.isFavorite) return false;
      if (tagFilter && !asset.tags.some((tag) => tag.toLowerCase() === tagFilter.toLowerCase())) {
        return false;
      }
      if (kindFilter === "all") return true;
      return asset.kind === kindFilter;
    });
  }, [assets, kindFilter, favoritesOnly, tagFilter]);

  const visibleAssetIds = useMemo(
    () => new Set(filteredAssets.map((asset) => asset.id)),
    [filteredAssets],
  );

  const clearSelection = () => setSelection(new Set());

  // Only visible assets are actionable. A library refresh can remove a card
  // before React has committed the effect that prunes its stale selection.
  // Deriving this intersection keeps the batch bar from flashing for an
  // asset that was just moved, restored, or permanently deleted.
  const selectionIds = useMemo(
    () => [...selection].filter((id) => visibleAssetIds.has(id)),
    [selection, visibleAssetIds],
  );

  const localNoticeSeq = useRef(0);
  const showLocalNotice = (title: string) => {
    const id = `local-${++localNoticeSeq.current}`;
    setNotice({ id, title, symbol: "checkmark" });
    setTimeout(() => setNotice((current) => (current?.id === id ? null : current)), 2000);
  };

  // --- Rubber-band (drag) selection -------------------------------------
  const bandStart = useRef<{ x: number; y: number } | null>(null);

  // Absolute children and card offsets both use the scroll container's
  // padding edge as their origin. clientX/Y already include the visible
  // padding, so adding the CSS padding again would shift the band right and
  // make it enlarge the horizontal scroll area.
  const contentPoint = (e: { clientX: number; clientY: number }, container: HTMLDivElement) => {
    const rect = container.getBoundingClientRect();
    return getLibraryContentPoint({
      clientX: e.clientX,
      clientY: e.clientY,
      rectLeft: rect.left,
      rectTop: rect.top,
      clientLeft: container.clientLeft,
      clientTop: container.clientTop,
      scrollLeft: container.scrollLeft,
      scrollTop: container.scrollTop,
    });
  };

  const releaseBandPointer = (e: React.PointerEvent<HTMLDivElement>) => {
    if (e.currentTarget.hasPointerCapture(e.pointerId)) {
      e.currentTarget.releasePointerCapture(e.pointerId);
    }
  };

  const bandPointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    // Only start a band from the empty grid area (not on a card/button).
    const target = e.target as HTMLElement | null;
    if (target && target.closest("[data-card],button,input,a,[data-menu]")) return;
    if (e.button !== 0) return;
    const container = gridScrollRef.current;
    if (!container) return;
    const p = contentPoint(e, container);
    bandStart.current = p;
    setBand({ x0: p.x, y0: p.y, x1: p.x, y1: p.y });
    e.currentTarget.setPointerCapture(e.pointerId);
    e.preventDefault();
  };

  const bandPointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!bandStart.current || !gridScrollRef.current) return;
    const p = contentPoint(e, gridScrollRef.current);
    setBand((prev) => (prev ? { ...prev, x1: p.x, y1: p.y } : prev));
  };

  const bandPointerUp = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!bandStart.current) return;
    const container = gridScrollRef.current;
    const start = bandStart.current;
    bandStart.current = null;
    if (!container) {
      setBand(null);
      releaseBandPointer(e);
      return;
    }
    const p = contentPoint(e, container);
    const x1 = p.x;
    const y1 = p.y;
    const minX = Math.min(start.x, x1);
    const maxX = Math.max(start.x, x1);
    const minY = Math.min(start.y, y1);
    const maxY = Math.max(start.y, y1);
    // Tiny drags (e.g. an accidental click) don't select anything.
    const dragged = Math.hypot(x1 - start.x, y1 - start.y) >= 5;
    if (dragged) {
      const next = new Set<string>();
      // Card positions in content space (offsetLeft/Top are relative to the
      // container's padding box — same coordinate space as the band).
      cardElsRef.current.forEach((el, id) => {
        const cardLeft = el.offsetLeft;
        const cardTop = el.offsetTop;
        const cardRight = cardLeft + el.offsetWidth;
        const cardBottom = cardTop + el.offsetHeight;
        const intersects =
          cardLeft <= maxX && cardRight >= minX && cardTop <= maxY && cardBottom >= minY;
        if (intersects) next.add(id);
      });
      setSelection((prev) => {
        const merged = new Set(prev);
        next.forEach((id) => merged.add(id));
        return merged;
      });
    }
    setBand(null);
    releaseBandPointer(e);
  };

  const bandPointerCancel = (e: React.PointerEvent<HTMLDivElement>) => {
    bandStart.current = null;
    setBand(null);
    releaseBandPointer(e);
  };

  const bandRect = useMemo(() => {
    if (!band) return null;
    return getLibraryBandRect(band);
  }, [band]);

  const closeMenu = useCallback((restoreFocus = false) => {
    const trigger = menuTriggerRef.current;
    setMenuFor(null);
    setMenuPos(null);
    menuTriggerRef.current = null;
    if (restoreFocus && trigger?.isConnected) {
      requestAnimationFrame(() => trigger.focus());
    }
  }, []);

  // Esc closes the active menu first, otherwise it clears batch selection.
  useEffect(() => {
    if (selection.size === 0 && menuFor === null) return;
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        if (menuFor !== null) {
          closeMenu(true);
        } else {
          clearSelection();
        }
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [closeMenu, menuFor, selection.size]);

  useEffect(() => {
    if (menuFor === null) return;
    const frame = requestAnimationFrame(() => {
      document
        .querySelector<HTMLButtonElement>(".kiri-card-menu button:not(:disabled)")
        ?.focus();
    });
    return () => cancelAnimationFrame(frame);
  }, [menuFor]);

  // Changing section/query clears the selection so the bar never points at
  // assets that are no longer visible.
  useEffect(() => {
    clearSelection();
  }, [section, destination, query, kindFilter, favoritesOnly, tagFilter]);

  // Library mutations arrive through `onLibraryChanged`. Reconcile the
  // stored selection with the refreshed view so removed cards cannot leave
  // an invisible selection behind.
  useEffect(() => {
    setSelection((current) => {
      const next = new Set([...current].filter((id) => visibleAssetIds.has(id)));
      return next.size === current.size ? current : next;
    });
  }, [visibleAssetIds]);

  useEffect(() => {
    if (destination !== "captures") return;
    api.getShortcutStatus().then(setShortcutStatus).catch(() => {});
  }, [destination]);

  const showingTrash = section === "trash";
  const showingTrashRef = useRef(showingTrash);
  showingTrashRef.current = showingTrash;

  const refresh = useCallback(async () => {
    const generation = ++refreshGenerationRef.current;
    try {
      const status = await api.getLibraryStatus();
      if (generation !== refreshGenerationRef.current) return;
      setLibraryStatus(status);
      setLibraryStatusError(false);

      if (status.availability !== "ready") {
        setAssets([]);
        setPendingRecordingCount(0);
        return;
      }

      const [list, pending] = await Promise.all([
        api.listAssets(queryRef.current.trim(), showingTrashRef.current),
        api.listPendingRecordings().catch(() => []),
      ]);
      if (generation !== refreshGenerationRef.current) return;
      setAssets(list);
      setPendingRecordingCount(pending.length);
      setAssetAvailability({});
    } catch {
      if (generation !== refreshGenerationRef.current) return;
      setLibraryStatusError(true);
      setAssets([]);
      setPendingRecordingCount(0);
    } finally {
      if (generation === refreshGenerationRef.current) setLoaded(true);
    }
  }, []);

  useEffect(() => {
    void refresh().catch(() => {});
  }, [query, refresh, showingTrash]);

  useEffect(() => {
    const subscriptions = [
      onLibraryChanged(() => {
        void refresh().catch(() => {});
      }),
      onAssetContentChanged((assetId) => {
        setThumbnailRevisions((revisions) => ({
          ...revisions,
          [assetId]: (revisions[assetId] ?? 0) + 1,
        }));
        setAssetAvailability((current) => ({ ...current, [assetId]: "ready" }));
      }),
      onGifConversionState(({ id, isConverting }) => {
        setGifConversionIds((current) => {
          const next = new Set(current);
          if (isConverting) next.add(id);
          else next.delete(id);
          return next;
        });
      }),
      onNotice((n) => {
        setNotice(n);
        setTimeout(() => {
          setNotice((current) => (current && current.id === n.id ? null : current));
        }, 2000);
      }),
      onError((e) => setError(e)),
    ];
    return () => {
      subscriptions.forEach((subscription) => {
        void subscription.then((dispose) => dispose()).catch(() => {});
      });
    };
  }, [refresh]);

  // ⌘/Ctrl+F focuses search. Batch selection is intentionally pointer-only
  // so selection chrome appears only after a visible rubber-band gesture.
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      const mod = e.metaKey || e.ctrlKey;
      if (
        destination === "captures" &&
        mod &&
        !e.altKey &&
        e.key.toLowerCase() === "f"
      ) {
        e.preventDefault();
        searchInputRef.current?.focus();
        searchInputRef.current?.select();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [assets, destination, favoritesOnly, kindFilter, query, tagFilter]);

  const gridStyle: React.CSSProperties = {
    display: "grid",
    gridTemplateColumns: "repeat(auto-fill, minmax(206px, 1fr))",
    gap: 16,
    alignContent: "start",
  };

  // All tags in the current view, for the tag filter bar.
  const allTags = useMemo(() => {
    const set = new Set<string>();
    for (const asset of assets) {
      for (const tag of asset.tags) set.add(tag);
    }
    return [...set].sort((a, b) => a.localeCompare(b));
  }, [assets]);

  const hasActiveFilter =
    query.trim().length > 0 || kindFilter !== "all" || favoritesOnly || tagFilter !== null;
  const isEmpty = assets.length === 0 && loaded && !hasActiveFilter;
  const isFilterEmpty = filteredAssets.length === 0 && loaded && hasActiveFilter;
  const libraryUnavailable =
    loaded && (libraryStatusError || libraryStatus?.availability === "unavailable");
  const libraryMigrating = loaded && libraryStatus?.availability === "migrating";

  const openMenu = (id: string, x: number, y: number, trigger?: HTMLButtonElement) => {
    if (menuFor === id) {
      closeMenu();
      return;
    }
    menuTriggerRef.current = trigger ?? null;
    setMenuFor(id);
    setMenuPos({ x, y });
  };

  // Clicking anywhere outside a card menu closes it (matches native menus).
  useEffect(() => {
    const close = (e: MouseEvent) => {
      const target = e.target as HTMLElement | null;
      if (target && target.closest(".kiri-card-menu")) return;
      closeMenu();
    };
    window.addEventListener("mousedown", close);
    return () => window.removeEventListener("mousedown", close);
  }, [closeMenu]);

  // Keep the open menu inside the window (flip when near the right/bottom
  // edge) — mirrors how native context menus avoid the screen edges.
  // Menu heights vary (image cards get "Copy"; trash gets "Delete
  // Permanently"), so estimate generously and clamp to the viewport.
  const menuStyle = useMemo(() => {
    if (!menuPos) return undefined;
    const MENU_W = 196;
    const MENU_H = 280;
    const pad = 10;
    const right = menuPos.x + MENU_W;
    const bottom = menuPos.y + 8 + MENU_H;
    const flipX = right > window.innerWidth - pad;
    const flipY = bottom > window.innerHeight - pad;
    // Flip upward keeps the menu's bottom edge at the trigger point (the
    // window is short, ~640px, so bottom-anchored triggers usually flip).
    const left = flipX ? Math.max(pad, menuPos.x - MENU_W) : menuPos.x;
    const top = flipY
      ? Math.max(pad, menuPos.y - MENU_H)
      : Math.min(menuPos.y + 8, window.innerHeight - MENU_H - pad);
    return { left, top };
  }, [menuPos]);

  // Run a menu action then close the menu (native menus dismiss on click).
  const run = useCallback(
    (fn: () => void, restoreFocus = true) => () => {
      fn();
      closeMenu(restoreFocus);
    },
    [closeMenu],
  );
  const startGifConversion = useCallback((id: string) => {
    // Give immediate feedback before the backend thread publishes its state.
    setGifConversionIds((current) => new Set(current).add(id));
    void api.convertToGif(id).catch(() => {
      setGifConversionIds((current) => {
        const next = new Set(current);
        next.delete(id);
        return next;
      });
    });
  }, []);

  const restoreMissing = useCallback(async (id: string) => {
    setError(null);
    try {
      const restored = await api.restoreMissingAsset(id);
      if (!restored) return;
      setAssetAvailability((current) => ({ ...current, [id]: "ready" }));
      setThumbnailRevisions((revisions) => ({
        ...revisions,
        [id]: (revisions[id] ?? 0) + 1,
      }));
      await refresh();
    } catch {
      setError({ message: "Couldn't restore this file", recovery: null });
    }
  }, [refresh]);

  const runLibraryRecovery = useCallback(async (action: () => Promise<unknown>) => {
    if (libraryRecoveryBusy) return;
    setLibraryRecoveryBusy(true);
    setError(null);
    try {
      await action();
    } catch {
      setError({ message: "Couldn't update location", recovery: null });
    } finally {
      await refresh().catch(() => {});
      setLibraryRecoveryBusy(false);
    }
  }, [libraryRecoveryBusy, refresh]);

  const retryPendingRecordings = useCallback(async () => {
    if (pendingRetryBusy) return;
    setPendingRetryBusy(true);
    setError(null);
    try {
      await api.retryPendingRecordings();
    } catch {
      setError({ message: "Could not save the recording.", recovery: null });
    } finally {
      await refresh().catch(() => {});
      setPendingRetryBusy(false);
    }
  }, [pendingRetryBusy, refresh]);

  const itemMenu = useCallback(
    (asset: AssetDto) => (
      <div
        id={`kiri-card-menu-${asset.id}`}
        className="kiri-card-menu"
        role="menu"
        aria-label={t("More Actions")}
        onKeyDown={(event) => {
          if (event.key === "Tab") {
            event.preventDefault();
            closeMenu(true);
            return;
          }
          if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
          const items = [
            ...event.currentTarget.querySelectorAll<HTMLButtonElement>(
              '[role="menuitem"]:not(:disabled)',
            ),
          ];
          if (items.length === 0) return;
          event.preventDefault();
          const current = items.indexOf(document.activeElement as HTMLButtonElement);
          const next = getMenuFocusIndex(
            event.key as "ArrowDown" | "ArrowUp" | "Home" | "End",
            current,
            items.length,
          );
          items[next]?.focus();
        }}
        onClick={(event) => event.stopPropagation()}
        onDoubleClick={(event) => event.stopPropagation()}
        style={{
          position: "fixed",
          ...(menuStyle ?? { left: 0, top: 0 }),
          background: "color-mix(in srgb, var(--kiri-elevated) 92%, transparent)",
          backdropFilter: "blur(18px)",
          WebkitBackdropFilter: "blur(18px)",
          border: "1px solid var(--kiri-surface-border)",
          borderRadius: 14,
          padding: 6,
          minWidth: 196,
          boxShadow: "0 8px 24px rgba(0,0,0,0.14)",
          zIndex: 100,
          display: "flex",
          flexDirection: "column",
          animation: "kiri-menu-in 0.12s ease-out",
        }}
      >
        {asset.kind === "image" && (
          <MenuRow
            icon="doc.on.doc"
            label={t("Copy")}
            disabled={assetAvailability[asset.id] !== undefined && assetAvailability[asset.id] !== "ready"}
            onClick={run(() => void api.copyAsset(asset.id).catch(() => {}))}
          />
        )}
        <MenuRow
          icon="character.textbox"
          label={t("Rename")}
          onClick={run(() => {
            window.dispatchEvent(new CustomEvent(`kiri-rename:${asset.id}`));
          }, false)}
        />
        <MenuRow
          icon="tag"
          label={t("Add Tag…")}
          onClick={run(() => {
            window.dispatchEvent(new CustomEvent(`kiri-addtag:${asset.id}`));
          }, false)}
        />
        <MenuRow
          icon={asset.kind === "image" ? "pencil.tip" : "photo.on.rectangle"}
          label={t(asset.kind === "image" ? "Edit" : "Open")}
          disabled={assetAvailability[asset.id] !== undefined && assetAvailability[asset.id] !== "ready"}
          onClick={run(() =>
            void (asset.kind === "image"
              ? api.openEditor(asset.id)
              : api.openAsset(asset.id)
            ).catch(() => {}),
          )}
        />
        <MenuRow
          icon="folder"
          label={t("Show in Folder")}
          disabled={assetAvailability[asset.id] === "missing"}
          onClick={run(() => void api.revealAsset(asset.id).catch(() => {}))}
        />
        {asset.gifEligible && (
          <MenuRow
            icon="sparkles.rectangle.stack"
            label={gifConversionIds.has(asset.id) ? t("Creating GIF…") : t("Convert to GIF")}
            disabled={
              gifConversionIds.has(asset.id) ||
              (assetAvailability[asset.id] !== undefined && assetAvailability[asset.id] !== "ready")
            }
            onClick={run(() => startGifConversion(asset.id))}
          />
        )}
        {assetAvailability[asset.id] === "missing" && !showingTrash && (
          <>
            <MenuRow
              icon="folder"
              label={t("Restore File…")}
              onClick={run(() => void restoreMissing(asset.id).catch(() => {}))}
            />
            <MenuRow
              icon="trash.fill"
              label={t("Remove Record")}
              destructive
              onClick={run(() =>
                void api.showConfirmDialog(
                  `removeMissing:${asset.id}`,
                  t("Remove this record?"),
                  "",
                  t("Remove Record"),
                ),
              )}
            />
          </>
        )}
        <div style={{ height: 1, background: "var(--kiri-surface-border)", margin: "5px 8px", opacity: 0.8 }} />
        {showingTrash ? (
          <>
            <MenuRow
              icon="arrow.uturn.backward"
              label={t("Restore")}
              onClick={run(() => void api.restoreAsset(asset.id).catch(() => {}))}
            />
            <MenuRow
              icon="trash.fill"
              label={t("Delete Permanently")}
              destructive
              onClick={run(() =>
                void api.showConfirmDialog(
                  `delete:${asset.id}`,
                  t("Delete this capture permanently?"),
                  t("This cannot be undone."),
                  t("Delete Permanently"),
                ),
              )}
            />
          </>
        ) : (
          <MenuRow
            icon="trash.fill"
            label={t("Move to Trash")}
            onClick={run(() => void api.moveToTrash(asset.id).catch(() => {}))}
          />
        )}
      </div>
    ),
    [
      assetAvailability,
      closeMenu,
      gifConversionIds,
      menuStyle,
      restoreMissing,
      run,
      showingTrash,
      startGifConversion,
    ],
  );

  return (
    <div
      className="library-root kiri-canvas-surface"
      style={{ display: "flex", flexDirection: "column", height: "100%", position: "relative" }}
    >
      {/* The top controls form one workspace: identity and global navigation
          above, contextual asset filters in an inset rail below. */}
      <header className="library-control-panel">
        <div className="library-control-panel__primary">
          <div className="library-control-panel__identity">
            <div className="library-control-panel__brand">
              <img src={brandIcon} alt="" />
            </div>
            <div className="library-control-panel__titles">
              <span className="library-control-panel__title">
                {destination === "settings"
                  ? t("Settings")
                  : showingTrash
                    ? t("Trash")
                    : t("Library")}
              </span>
              <span className="library-control-panel__subtitle">
                {destination === "settings"
                  ? t("Language, storage, and text recognition")
                  : libraryUnavailable
                    ? t("Unavailable")
                    : libraryMigrating
                      ? t("Moving…")
                      : fmt(assets.length === 1 ? "%d capture" : "%d captures", assets.length)}
              </span>
            </div>
          </div>

          <div className="library-control-panel__actions">
            {destination === "captures" &&
              !libraryStatusError &&
              libraryStatus?.availability === "ready" && (
              <div className="kiri-library-search">
                <KiriIcon name="magnifyingglass" size={14} />
                <input
                  ref={searchInputRef}
                  type="search"
                  value={query}
                  aria-label={t("Search captures")}
                  placeholder={t("Search captures")}
                  onChange={(event) => setQuery(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Escape" && query) {
                      event.preventDefault();
                      event.stopPropagation();
                      setQuery("");
                    }
                  }}
                />
                {query && (
                  <button
                    type="button"
                    className="kiri-search-clear"
                    aria-label={t("Clear Search")}
                    title={t("Clear Search")}
                    onClick={() => {
                      setQuery("");
                      searchInputRef.current?.focus();
                    }}
                  >
                    <KiriIcon name="xmark" size={10} />
                  </button>
                )}
              </div>
            )}
            <SegmentedPicker
              options={[
                { label: t("Library"), icon: "square.grid.3x3.fill" },
                { label: t("Trash"), icon: "trash" },
                { label: t("Settings"), icon: "slider.horizontal.3" },
              ]}
              value={destination === "settings" ? 2 : section === "library" ? 0 : 1}
              onChange={(index) => {
                clearSelection();
                setMenuFor(null);
                if (index === 2) {
                  setDestination("settings");
                  return;
                }
                setDestination("captures");
                setSection(index === 0 ? "library" : "trash");
                setKindFilter("all");
                setFavoritesOnly(false);
                setTagFilter(null);
              }}
            />
          </div>
        </div>

        {destination === "captures" &&
          !libraryStatusError &&
          libraryStatus?.availability === "ready" && (
          <div className="library-control-panel__filter-rail">
            <FilterBar
              kind={kindFilter}
              favoritesOnly={favoritesOnly}
              tagFilter={tagFilter}
              allTags={allTags}
              onChangeKind={setKindFilter}
              onToggleFavorites={() => setFavoritesOnly((value) => !value)}
              onToggleTag={(tag) => setTagFilter((current) => (current === tag ? null : tag))}
            />
            {showingTrash && assets.length > 0 && (
              <button
                type="button"
                className="kiri-button kiri-button--destructive"
                title={t("Empty Trash")}
                onClick={() =>
                  void api.showConfirmDialog(
                    "emptyTrash",
                    t("Empty Trash?"),
                    t("All captures in Trash will be permanently deleted. This cannot be undone."),
                    t("Empty Trash"),
                  )
                }
                style={{ minHeight: 30, padding: "0 10px", fontSize: 11.5, flexShrink: 0 }}
              >
                <KiriIcon name="trash.fill" size={12} />
                {t("Empty Trash")}
              </button>
            )}
          </div>
        )}
      </header>

      {destination === "captures" ? (
        <>
      {libraryUnavailable ? (
        <LibraryUnavailableState
          status={libraryStatus}
          busy={libraryRecoveryBusy}
          onRetry={() => void runLibraryRecovery(api.retryLibrary)}
          onLocate={() => void runLibraryRecovery(api.locateLibrary)}
        />
      ) : libraryMigrating ? (
        <LibraryStatusState title={t("Moving Library…")} />
      ) : (
        <>
      {!showingTrash && pendingRecordingCount > 0 && (
        <div className="library-recovery-banner" role="status">
          <span>
            {fmt(
              pendingRecordingCount === 1
                ? "%d recording is waiting to save"
                : "%d recordings are waiting to save",
              pendingRecordingCount,
            )}
          </span>
          <button
            type="button"
            className="kiri-button kiri-button--secondary"
            disabled={pendingRetryBusy}
            onClick={() => void retryPendingRecordings().catch(() => {})}
          >
            {t("Retry")}
          </button>
        </div>
      )}
      {/* Grid */}
      {selectionIds.length > 0 && (
        <BatchActionBar
          count={selectionIds.length}
          showingTrash={showingTrash}
          allFavorites={selectionIds.length > 0 && selectionIds.every((id) => assets.find((a) => a.id === id)?.isFavorite)}
          onRestore={() => {
            void api
              .batchRestore(selectionIds)
              .then(() => showLocalNotice(t("Restored to Library")))
              .catch(() => {});
            clearSelection();
          }}
          onDelete={() =>
            void api.showConfirmDialog(
              "batchDelete",
              t("Delete these captures permanently?"),
              t("This cannot be undone."),
              t("Delete Permanently (N)").replace("{n}", String(selectionIds.length)),
              selectionIds,
            )
          }
          onMoveToTrash={() => {
            void api
              .batchMoveToTrash(selectionIds)
              .then(() => showLocalNotice(t("Moved to Trash")))
              .catch(() => {});
            clearSelection();
          }}
          onToggleFavorite={() => {
            const allFav = selectionIds.every((id) => assets.find((a) => a.id === id)?.isFavorite);
            void api
              .batchSetFavorite(selectionIds, !allFav)
              .then(() => showLocalNotice(allFav ? t("Remove from Favorites") : t("Add to Favorites")))
              .catch(() => {});
            clearSelection();
          }}
          onClear={clearSelection}
        />
      )}
      {!loaded ? (
        <div style={{ flex: 1, display: "flex", alignItems: "center", justifyContent: "center", color: "var(--kiri-secondary-label)" }}>
          {t("Loading Library…")}
        </div>
      ) : isEmpty ? (
        <EmptyState
          isTrashEmpty={isEmpty && showingTrash}
          shortcutStatus={shortcutStatus}
        />
      ) : isFilterEmpty ? (
        <div
          style={{
            flex: 1,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            flexDirection: "column",
            gap: 6,
            color: "var(--kiri-secondary-label)",
            fontSize: 12.5,
          }}
        >
          {t("No captures match this filter")}
        </div>
      ) : (
        <div
          ref={gridScrollRef}
          style={{
            flex: 1,
            overflowY: "auto",
            overflowX: "hidden",
            padding: "0 22px 24px",
            position: "relative",
            touchAction: "none",
            cursor: band ? "crosshair" : undefined,
          }}
          onPointerDown={bandPointerDown}
          onPointerMove={bandPointerMove}
          onPointerUp={bandPointerUp}
          onPointerCancel={bandPointerCancel}
          onLostPointerCapture={bandPointerCancel}
        >
          {/* Rubber-band selection rectangle */}
          {bandRect && (
            <div
              style={{
                position: "absolute",
                left: bandRect.x,
                top: bandRect.y,
                width: bandRect.w,
                height: bandRect.h,
                boxSizing: "border-box",
                background: "rgba(0,0,0,0.08)",
                border: "1px solid rgba(0,0,0,0.62)",
                borderRadius: 6,
                pointerEvents: "none",
                zIndex: 5,
              }}
            />
          )}
          {groupByDay(filteredAssets).map((group) => (
            <div key={group.key} style={{ marginBottom: 22 }}>
              <div
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 8,
                  marginBottom: 10,
                  paddingTop: 12,
                }}
              >
                <div
                  style={{
                    width: 5,
                    height: 5,
                    borderRadius: "50%",
                    background: "var(--kiri-accent)",
                  }}
                />
                <span
                  style={{
                    fontSize: 12.5,
                    fontWeight: 700,
                    color: "var(--kiri-label)",
                  }}
                >
                  {group.label}
                </span>
                <span
                  style={{
                    fontSize: 10.5,
                    fontWeight: 600,
                    color: "var(--kiri-secondary-label)",
                    background: "var(--kiri-group-fill)",
                    borderRadius: 8,
                    padding: "1px 7px",
                  }}
                >
                  {group.assets.length}
                </span>
              </div>
              <div style={gridStyle}>
                {group.assets.map((asset) => (
                  <AssetCard
                    key={asset.id}
                    asset={asset}
                    availability={assetAvailability[asset.id]}
                    thumbnailRevision={thumbnailRevisions[asset.id] ?? 0}
                    menuOpen={menuFor === asset.id}
                    onMenu={(x, y, trigger) => openMenu(asset.id, x, y, trigger)}
                    menu={menuFor === asset.id ? itemMenu(asset) : null}
                    selected={selection.has(asset.id)}
                    selectionActive={selectionIds.length > 0}
                    registerRef={(el) => {
                      if (el) cardElsRef.current.set(asset.id, el);
                      else cardElsRef.current.delete(asset.id);
                    }}
                    onAvailability={(availability) => {
                      setAssetAvailability((current) => ({
                        ...current,
                        [asset.id]: availability,
                      }));
                      if (availability === "libraryUnavailable") {
                        void refresh().catch(() => {});
                      }
                    }}
                    onRestoreMissing={() => restoreMissing(asset.id)}
                    onOpen={() =>
                      assetAvailability[asset.id] === undefined ||
                      assetAvailability[asset.id] === "ready"
                        ? void (asset.kind === "image"
                            ? api.openEditor(asset.id)
                            : api.openAsset(asset.id)
                          ).catch(() => {})
                        : undefined
                    }
                  />
                ))}
              </div>
            </div>
          ))}
        </div>
      )}
        </>
      )}
        </>
      ) : (
        <React.Suspense
          fallback={
            <div
              role="status"
              style={{
                flex: 1,
                display: "grid",
                placeItems: "center",
                color: "var(--kiri-secondary-label)",
                fontSize: 12.5,
              }}
            >
              {t("Loading Settings…")}
            </div>
          }
        >
          <SettingsView />
        </React.Suspense>
      )}

      {/* Window-level progress and local notices stay in one predictable
          place below the header. Global capture/recording completions use the
          separate always-on-top toast on the originating display. */}
      {(gifConversionIds.size > 0 || notice) && (
        <div
          aria-live="polite"
          style={{
            position: "fixed",
            left: "50%",
            top: destination === "captures" ? 116 : 76,
            transform: "translateX(-50%)",
            background: "var(--kiri-elevated)",
            border: "1px solid var(--kiri-surface-border)",
            borderRadius: 13,
            padding: "8px 14px",
            display: "flex",
            gap: 8,
            alignItems: "center",
            boxShadow: "none",
            color: "var(--kiri-label)",
            fontSize: 12.5,
            fontWeight: 500,
            zIndex: 30,
            whiteSpace: "nowrap",
          }}
        >
          {gifConversionIds.size > 0 ? (
            <>
              <span
                aria-hidden="true"
                style={{
                  width: 12,
                  height: 12,
                  borderRadius: "50%",
                  border: "1.5px solid var(--kiri-surface-border)",
                  borderTopColor: "var(--kiri-label)",
                  animation: "kiri-library-spin 0.75s linear infinite",
                }}
              />
              {t("Creating GIF…")}
            </>
          ) : (
            <>
              {notice?.symbol && <KiriIcon name={notice.symbol as never} size={13} />}
              {notice && t(notice.title)}
            </>
          )}
          <style>{`@keyframes kiri-library-spin { to { transform: rotate(360deg); } }`}</style>
        </div>
      )}

      {/* Error banner */}
      {error && (
        <div
          style={{
            position: "absolute",
            left: "50%",
            top: destination === "captures" ? 116 : 76,
            transform: "translateX(-50%)",
            background: "var(--kiri-elevated)",
            border: "1px solid var(--kiri-surface-border)",
            borderRadius: 13,
            padding: "10px 16px",
            display: "flex",
            gap: 12,
            alignItems: "center",
            boxShadow: "none",
            maxWidth: 560,
            zIndex: 20,
          }}
        >
          {/* Error message: pure i18n keys translate; dynamically-composed
              errors (key + suffix) fall back to the raw text via t(). */}
          <span
            style={{
              color: "var(--kiri-label)",
              fontSize: 12.5,
              flex: "1 1 auto",
              minWidth: 0,
              lineHeight: 1.35,
            }}
          >
            {t(error.message)}
          </span>
          {error.recovery && (
            <button
              type="button"
              className="kiri-primary-button"
              style={{ minHeight: 30, flexShrink: 0, whiteSpace: "nowrap" }}
              onClick={() => {
                if (error.recovery === "quitKiri") void api.quitApp().catch(() => {});
                else void api.openSettings(error.recovery!).catch(() => {});
                setError(null);
              }}
            >
              {recoveryLabel(error.recovery)}
            </button>
          )}
          <button
            type="button"
            className="kiri-icon-button kiri-inline-dismiss"
            aria-label={t("Close")}
            title={t("Close")}
            style={{
              flexShrink: 0,
            }}
            onClick={() => setError(null)}
          >
            <KiriIcon name="xmark" size={11} />
          </button>
        </div>
      )}

    </div>
  );
}

function LibraryStatusState(props: { title: string; children?: React.ReactNode }) {
  return (
    <div className="library-status-state" role="status">
      <strong>{props.title}</strong>
      {props.children}
    </div>
  );
}

function LibraryUnavailableState(props: {
  status: LibraryStatusDto | null;
  busy: boolean;
  onRetry(): void;
  onLocate(): void;
}) {
  const locationLabel = props.status?.isDefault
    ? t("Default")
    : props.status?.locationLabel;
  return (
    <LibraryStatusState title={t("Library unavailable")}>
      {locationLabel && <span>{locationLabel}</span>}
      <div className="library-status-state__actions">
        <button
          type="button"
          className="kiri-button kiri-button--secondary"
          disabled={props.busy}
          onClick={props.onRetry}
        >
          {t("Retry")}
        </button>
        <button
          type="button"
          className="kiri-button kiri-button--secondary"
          disabled={props.busy}
          onClick={props.onLocate}
        >
          {t("Locate…")}
        </button>
      </div>
    </LibraryStatusState>
  );
}

function recoveryLabel(recovery: string): string {
  switch (recovery) {
    case "openSettings":
      return t("Open Settings");
    case "quitKiri":
      return t("Quit Kiri");
    case "openInputMonitoringSettings":
      return t("Open Input Monitoring Settings");
    case "openMicrophoneSettings":
      return t("Open Microphone Settings");
    default:
      return recovery;
  }
}

function AssetCard(props: {
  asset: AssetDto;
  availability?: AssetAvailability;
  thumbnailRevision: number;
  menuOpen: boolean;
  onMenu(x: number, y: number, trigger?: HTMLButtonElement): void;
  menu: React.ReactNode;
  onOpen(): void;
  selected: boolean;
  selectionActive: boolean;
  registerRef(el: HTMLDivElement | null): void;
  onAvailability(availability: AssetAvailability): void;
  onRestoreMissing(): Promise<void>;
}) {
  const {
    asset,
    availability,
    thumbnailRevision,
    menuOpen,
    onMenu,
    menu,
    onOpen,
    selected,
    selectionActive,
    registerRef,
    onAvailability,
    onRestoreMissing,
  } = props;
  const [hovered, setHovered] = useState(false);
  const [focused, setFocused] = useState(false);
  const [editingTitle, setEditingTitle] = useState(false);
  const previewRef = useRef<HTMLDivElement>(null);
  const [previewVisible, setPreviewVisible] = useState(false);
  const [previewRetry, setPreviewRetry] = useState(0);
  const [previewFailed, setPreviewFailed] = useState(false);
  const [previewBusy, setPreviewBusy] = useState(false);
  const [restoreBusy, setRestoreBusy] = useState(false);

  const checkAvailability = async (mediaFailed = false) => {
    try {
      const next = (await api.getAssetAvailability(asset.id)).status;
      onAvailability(next);
      setPreviewFailed(mediaFailed && next === "ready");
      return next;
    } catch {
      setPreviewFailed(true);
      return null;
    }
  };

  const retryPreview = async () => {
    if (previewBusy) return;
    setPreviewBusy(true);
    try {
      const next = await checkAvailability();
      if (next === "ready") {
        setPreviewFailed(false);
        setPreviewRetry((revision) => revision + 1);
      }
    } finally {
      setPreviewBusy(false);
    }
  };

  const restoreAsset = async () => {
    if (restoreBusy) return;
    setRestoreBusy(true);
    try {
      await onRestoreMissing();
    } finally {
      setRestoreBusy(false);
    }
  };

  // Keep only previews near the viewport mounted. Native lazy-loading delays
  // the first decode, but does not release images after the user scrolls past
  // them; unmounting does, while the fixed-height card preserves layout.
  useEffect(() => {
    const preview = previewRef.current;
    if (!preview || typeof IntersectionObserver === "undefined") {
      setPreviewVisible(true);
      return;
    }
    const observer = new IntersectionObserver(
      ([entry]) => setPreviewVisible(entry.isIntersecting),
      { rootMargin: "400px 0px" },
    );
    observer.observe(preview);
    return () => observer.disconnect();
  }, []);
  const lastOpenAtRef = useRef(0);
  const highlighted = hovered || focused;
  const interaction = getLibraryCardInteraction({
    selectionActive,
    selected,
    menuOpen,
    editingTitle,
    highlighted,
  });
  const primaryAction = getLibraryCardPrimaryAction(asset.kind);
  const openCard = () => {
    if (!interaction.opensOnClick) return;
    const now = Date.now();
    if (now - lastOpenAtRef.current < 500) return;
    lastOpenAtRef.current = now;
    onOpen();
  };
  const handleClick = (event: React.MouseEvent) => {
    if (event.defaultPrevented) return;
    openCard();
  };
  const [titleDraft, setTitleDraft] = useState(asset.title ?? "");
  const [addingTag, setAddingTag] = useState(false);

  // "Rename" menu item dispatches a custom event to enter edit mode.
  useEffect(() => {
    const handler = () => {
      setTitleDraft(asset.title ?? "");
      setEditingTitle(true);
    };
    window.addEventListener(`kiri-rename:${asset.id}`, handler);
    return () => window.removeEventListener(`kiri-rename:${asset.id}`, handler);
  }, [asset.id, asset.title]);

  // "Add Tag" menu item opens the inline tag input.
  useEffect(() => {
    const handler = () => setAddingTag(true);
    window.addEventListener(`kiri-addtag:${asset.id}`, handler);
    return () => window.removeEventListener(`kiri-addtag:${asset.id}`, handler);
  }, [asset.id]);
  const previewImageStyle: React.CSSProperties = {
    width: "100%",
    height: "100%",
    objectFit: "contain",
    padding: 8,
    boxSizing: "border-box",
    transform: highlighted ? "scale(1.018)" : "scale(1)",
    transition: "transform 0.22s cubic-bezier(0.2, 0.8, 0.2, 1)",
  };
  const contentAvailable = availability === undefined || availability === "ready";
  return (
    <div
      ref={registerRef}
      className="library-asset-card"
      data-card={asset.id}
      role="article"
      aria-label={asset.title ?? asset.filename}
      onClick={handleClick}
      onContextMenu={(e) => {
        // Right-click shows the localized action menu at the cursor
        // (same as ⋯), never the webview's system context menu.
        e.preventDefault();
        onMenu(e.clientX, e.clientY);
      }}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      onFocus={() => setFocused(true)}
      onBlur={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setFocused(false);
      }}
      style={{
        position: "relative",
        background: "var(--kiri-card)",
        border: `1px solid ${selected ? "var(--kiri-accent-strong)" : highlighted ? "color-mix(in srgb, var(--kiri-label) 34%, var(--kiri-surface-border))" : "var(--kiri-surface-border)"}`,
        borderRadius: 16,
        padding: 9,
        transform: highlighted && !selected ? "translateY(-2px)" : "none",
        boxShadow: selected
          ? "0 0 0 2px var(--kiri-accent-alpha-18), 0 10px 24px rgba(0,0,0,0.08)"
          : highlighted
            ? "0 10px 24px rgba(0,0,0,0.075)"
            : "0 1px 0 rgba(0,0,0,0.025)",
        transition: "transform 0.18s ease-out, box-shadow 0.18s ease-out, border-color 0.18s ease-out",
        cursor: "default",
      }}
    >
      {/* Selected corner badge — small, non-interactive status mark. */}
      {selected && (
        <div
          style={{
            position: "absolute",
            top: 7,
            right: 7,
            width: 20,
            height: 20,
            borderRadius: "50%",
            background: "var(--kiri-accent)",
            color: "var(--kiri-on-accent)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            boxShadow: "none",
            zIndex: 2,
            pointerEvents: "none",
          }}
        >
          <KiriIcon name="checkmark" size={11} />
        </div>
      )}
      <div
        ref={previewRef}
        style={{
          height: 154,
          borderRadius: 11,
          overflow: "hidden",
          background: "var(--kiri-group-fill)",
          border: "1px solid color-mix(in srgb, var(--kiri-surface-border) 72%, transparent)",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        {previewVisible && previewFailed ? (
          <div className="library-asset-unavailable">
            <span>{t("Preview unavailable")}</span>
            <button
              type="button"
              className="kiri-button kiri-button--secondary"
              disabled={previewBusy}
              onClick={(event) => {
                event.stopPropagation();
                void retryPreview();
              }}
              onDoubleClick={(event) => event.stopPropagation()}
            >
              {t("Retry")}
            </button>
          </div>
        ) : previewVisible && availability === "missing" ? (
          <div className="library-asset-unavailable">
            <span>{t("File missing")}</span>
            <button
              type="button"
              className="kiri-button kiri-button--secondary"
              disabled={restoreBusy}
              onClick={(event) => {
                event.stopPropagation();
                void restoreAsset();
              }}
              onDoubleClick={(event) => event.stopPropagation()}
            >
              {t("Restore File…")}
            </button>
          </div>
        ) : previewVisible && availability && availability !== "ready" ? (
          <div className="library-asset-unavailable">
            <span>
              {t(availability === "libraryUnavailable" ? "Library unavailable" : "Can't read this file")}
            </span>
            {availability !== "libraryUnavailable" && (
              <button
                type="button"
                className="kiri-button kiri-button--secondary"
                disabled={previewBusy}
                onClick={(event) => {
                  event.stopPropagation();
                  void retryPreview();
                }}
                onDoubleClick={(event) => event.stopPropagation()}
              >
                {t("Retry")}
              </button>
            )}
          </div>
        ) : previewVisible && (asset.kind === "image" ? (
          <img
            key={`${asset.id}:${thumbnailRevision}:${previewRetry}`}
            src={thumbnailUrl(asset.id, thumbnailRevision)}
            alt=""
            draggable={false}
            loading="lazy"
            decoding="async"
            onError={() => void checkAvailability(true)}
            // Scaled-to-fit keeps the whole capture visible; the neutral
            // stage and tighter height stop wide captures floating in a void.
            // never cropped.
            style={previewImageStyle}
          />
        ) : asset.kind === "video" ? (
          <div style={{ position: "relative", width: "100%", height: "100%" }}>
            <video
              key={`${asset.id}:${thumbnailRevision}:${previewRetry}`}
              src={mediaUrl(asset.id)}
              muted
              playsInline
              preload="metadata"
              onLoadedMetadata={(event) => {
                // Seeking a fraction past zero asks WebView2 to decode the
                // first frame without playing or downloading FFmpeg.
                event.currentTarget.currentTime = 0.001;
              }}
              onError={() => void checkAvailability(true)}
              style={previewImageStyle}
            />
            <div style={{ position: "absolute", inset: 0, display: "flex", alignItems: "center", justifyContent: "center" }}>
              <svg width="34" height="34" viewBox="0 0 24 24">
                <circle cx="12" cy="12" r="11" fill="rgba(0,0,0,0.45)" />
                <path d="M10 8.5v7l6-3.5z" fill="#fff" />
              </svg>
            </div>
          </div>
        ) : (
          <div style={{ position: "relative", width: "100%", height: "100%" }}>
            <img
              key={`${asset.id}:${thumbnailRevision}:${previewRetry}`}
              src={thumbnailUrl(asset.id, thumbnailRevision)}
              alt=""
              draggable={false}
              loading="lazy"
              decoding="async"
              onError={() => void checkAvailability(true)}
              style={previewImageStyle}
            />
            <div style={{ position: "absolute", left: 8, bottom: 8, background: "rgba(0,0,0,0.6)", color: "#fff", borderRadius: 6, padding: "2px 6px", fontSize: 10, fontWeight: 600 }}>
              GIF
            </div>
          </div>
        ))}
      </div>
      <div style={{ display: "flex", alignItems: "center", gap: 7, marginTop: 8, padding: "0 1px 1px", position: "relative" }}>
        {editingTitle ? (
          <input
            className="kiri-inline-rename-input"
            autoFocus
            value={titleDraft}
            onChange={(e) => setTitleDraft(e.target.value)}
            placeholder={t("Name")}
            onBlur={() => {
              void api.renameAsset(asset.id, titleDraft.trim()).catch(() => {});
              setEditingTitle(false);
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                void api.renameAsset(asset.id, titleDraft.trim()).catch(() => {});
                setEditingTitle(false);
              } else if (e.key === "Escape") {
                setTitleDraft(asset.title ?? "");
                setEditingTitle(false);
              }
              e.stopPropagation();
            }}
            style={{
              flex: 1,
              minWidth: 0,
              height: 28,
              fontSize: 11.5,
              fontWeight: 600,
              color: "var(--kiri-label)",
              border: "1px solid var(--kiri-accent)",
              borderRadius: 9,
              background: "var(--kiri-group-fill)",
              padding: "0 10px",
              boxShadow: "0 0 0 3px var(--kiri-accent-alpha-18)",
              caretColor: "var(--kiri-accent)",
            }}
          />
        ) : (
          <div
            title={asset.filename}
            onClick={(e) => {
              // Keep title gestures separate from opening the asset because
              // double-click edits the title inline.
              e.stopPropagation();
            }}
            onDoubleClick={(e) => {
              e.stopPropagation();
              setTitleDraft(asset.title ?? "");
              setEditingTitle(true);
            }}
            style={{
              flex: 1,
              minWidth: 0,
              display: "flex",
              flexDirection: "column",
              gap: 2,
              overflow: "hidden",
              cursor: "text",
            }}
          >
            <span
              style={{
                fontSize: 11.5,
                fontWeight: 700,
                color: asset.title ? "var(--kiri-label)" : "var(--kiri-secondary-label)",
                overflow: "hidden",
                textOverflow: "ellipsis",
                whiteSpace: "nowrap",
              }}
            >
              {asset.title ??
                new Date(asset.createdAt).toLocaleTimeString(undefined, {
                  hour: "2-digit",
                  minute: "2-digit",
                })}
            </span>
            <span
              title={`${asset.pixelWidth} × ${asset.pixelHeight} · ${t(asset.kind === "image" ? "Image" : asset.kind === "video" ? "Video" : "GIF")}`}
              style={{
                fontSize: 9.5,
                fontWeight: 500,
                color: "var(--kiri-disabled-label)",
                letterSpacing: "0.01em",
                whiteSpace: "nowrap",
              }}
            >
              {asset.pixelWidth}×{asset.pixelHeight}
            </span>
          </div>
        )}
        <div
          className="kiri-card-actions"
          data-visible={interaction.showsActions || undefined}
        >
          <button
            type="button"
            className="kiri-icon-button"
            title={t(primaryAction.title)}
            aria-label={t(primaryAction.title)}
            disabled={!contentAvailable}
            onMouseDown={(e) => e.preventDefault()}
            onClick={(e) => {
              e.stopPropagation();
              if (contentAvailable) {
                void (primaryAction.opensEditor
                  ? api.openEditor(asset.id)
                  : api.openAsset(asset.id)
                ).catch(() => {});
              }
            }}
            onDoubleClick={(e) => {
              e.stopPropagation();
              if (contentAvailable) {
                void (primaryAction.opensEditor
                  ? api.openEditor(asset.id)
                  : api.openAsset(asset.id)
                ).catch(() => {});
              }
            }}
            style={{
              width: 26,
              height: 26,
              borderRadius: 8,
              fontSize: 13,
              cursor: contentAvailable ? "pointer" : "default",
            }}
          >
            <KiriIcon name={primaryAction.icon} size={14} />
          </button>
          <button
            type="button"
            className="kiri-icon-button"
            title={t("Copy")}
            aria-label={t("Copy")}
            disabled={!contentAvailable}
            onMouseDown={(e) => e.preventDefault()}
            onClick={(e) => {
              e.stopPropagation();
              if (contentAvailable) void api.copyAsset(asset.id).catch(() => {});
            }}
            onDoubleClick={(e) => {
              // A rapid double-click on Copy must copy, not open the asset.
              e.stopPropagation();
              if (contentAvailable) void api.copyAsset(asset.id).catch(() => {});
            }}
            style={{
              width: 26,
              height: 26,
              borderRadius: 8,
              fontSize: 13,
              cursor: contentAvailable ? "pointer" : "default",
            }}
          >
            <KiriIcon name="doc.on.doc" size={14} />
          </button>
          <button
            type="button"
            className="kiri-icon-button kiri-card-favorite"
            title={asset.isFavorite ? t("Remove Favorite") : t("Favorite")}
            aria-label={asset.isFavorite ? t("Remove Favorite") : t("Favorite")}
            aria-pressed={asset.isFavorite}
            data-favorite={asset.isFavorite || undefined}
            onMouseDown={(e) => e.preventDefault()}
            onClick={(e) => {
              e.stopPropagation();
              void api.setFavorite(asset.id, !asset.isFavorite).catch(() => {});
            }}
            onDoubleClick={(e) => {
              // Rapid double-click must not open the asset.
              e.stopPropagation();
              void api.setFavorite(asset.id, !asset.isFavorite).catch(() => {});
            }}
            style={{
              width: 26,
              height: 26,
              borderRadius: 8,
              fontSize: 13,
              cursor: "pointer",
            }}
          >
            <KiriIcon name={asset.isFavorite ? "star.fill" : "star"} size={15} />
          </button>
          <button
            type="button"
            className="kiri-icon-button"
            title={t("More Actions")}
            aria-label={t("More Actions")}
            aria-haspopup="menu"
            aria-expanded={menuOpen}
            aria-controls={`kiri-card-menu-${asset.id}`}
            onMouseDown={(e) => e.preventDefault()}
            onClick={(e) => {
              e.stopPropagation();
              const rect = e.currentTarget.getBoundingClientRect();
              // Anchor the menu below the ⋯ button; edge-flip handled in
              // menuStyle (left-aligned so a near-right flip stays sane).
              onMenu(rect.left, rect.bottom + 4, e.currentTarget);
            }}
            onDoubleClick={(e) => {
              e.stopPropagation();
            }}
            style={{
              width: 26,
              height: 26,
              borderRadius: 8,
              fontSize: 13,
              cursor: "pointer",
            }}
          >
            <KiriIcon name="ellipsis.circle" size={14} />
          </button>
        </div>
        {menuOpen &&
          createPortal(
            // Render the context menu outside the card subtree: the card's
            // hover transform (translateY) would otherwise become the fixed
            // positioning containing block, breaking both the menu's screen
            // position and its stacking above neighboring cards.
            menu,
            document.body,
          )}
      </div>
      {/* Tag chips */}
      {(asset.tags.length > 0 || addingTag) && (
        <div style={{ display: "flex", flexWrap: "wrap", gap: 4, marginTop: 8 }}>
          {asset.tags.map((tag) => (
            <span
              key={tag}
              title={tag}
              style={{
                display: "inline-flex",
                alignItems: "center",
                gap: 4,
                fontSize: 10,
                fontWeight: 600,
                color: "var(--kiri-accent)",
                background: "var(--kiri-accent-soft-alpha-10)",
                borderRadius: 7,
                padding: "2px 7px",
                maxWidth: 90,
                overflow: "hidden",
                textOverflow: "ellipsis",
                whiteSpace: "nowrap",
              }}
            >
              {tag}
              <button
                type="button"
                className="kiri-tag-remove-button"
                aria-label={`${t("Remove tag")}: ${tag}`}
                title={t("Remove tag")}
                onClick={(e) => {
                  e.stopPropagation();
                  void api
                    .setTags(
                      asset.id,
                      asset.tags.filter((existing) => existing !== tag),
                    )
                    .catch(() => {});
                }}
              >
                <KiriIcon name="xmark" size={8} />
              </button>
            </span>
          ))}
          {addingTag && (
            <input
              autoFocus
              placeholder={t("Add tag…")}
              onBlur={() => setAddingTag(false)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  const value = e.currentTarget.value.trim();
                  if (value) {
                    void api
                      .setTags(asset.id, [...asset.tags, value])
                      .catch(() => {});
                  }
                  setAddingTag(false);
                } else if (e.key === "Escape") {
                  setAddingTag(false);
                }
                e.stopPropagation();
              }}
              style={{
                width: 70,
                fontSize: 10,
                fontWeight: 500,
                color: "var(--kiri-label)",
                border: "1px solid var(--kiri-accent)",
                borderRadius: 7,
                background: "var(--kiri-group-fill)",
                padding: "2px 6px",
              }}
            />
          )}
        </div>
      )}
    </div>
  );
}

function MenuRow(props: {
  label: string;
  icon?: IconName;
  onClick(): void;
  destructive?: boolean;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      className="kiri-menu-row"
      data-destructive={props.destructive || undefined}
      role="menuitem"
      onClick={(event) => {
        // Portaled menu events still follow the React component tree. Stop at
        // the row before its action closes/unmounts the menu; otherwise the
        // click reaches AssetCard and enters batch selection 220ms later.
        event.stopPropagation();
        props.onClick();
      }}
      onDoubleClick={(event) => event.stopPropagation()}
      disabled={props.disabled}
    >
      {props.icon && (
        <span
          style={{
            width: 15,
            display: "flex",
            justifyContent: "center",
            color: props.destructive ? "var(--kiri-coral)" : "var(--kiri-accent)",
            opacity: props.disabled ? 0.45 : 0.9,
            flexShrink: 0,
          }}
        >
          <KiriIcon name={props.icon} size={14} />
        </span>
      )}
      {props.label}
    </button>
  );
}

function FilterBar(props: {
  kind: "all" | "image" | "video" | "gif";
  favoritesOnly: boolean;
  tagFilter: string | null;
  allTags: string[];
  onChangeKind(kind: "all" | "image" | "video" | "gif"): void;
  onToggleFavorites(): void;
  onToggleTag(tag: string): void;
}) {
  const {
    kind,
    favoritesOnly,
    tagFilter,
    allTags,
    onChangeKind,
    onToggleFavorites,
    onToggleTag,
  } = props;
  const kinds: { value: "all" | "image" | "video" | "gif"; label: string; icon: IconName }[] = [
    { value: "all", label: t("All"), icon: "square.grid.3x3.fill" },
    { value: "image", label: t("Images"), icon: "photo.on.rectangle" },
    { value: "video", label: t("Videos"), icon: "play.rectangle" },
    { value: "gif", label: t("GIFs"), icon: "sparkles.rectangle.stack" },
  ];
  return (
    <div className="library-filter-bar">
      <div className="library-kind-picker">
        {kinds.map((entry) => (
          <button
            type="button"
            key={entry.value}
            className="library-kind-picker__button"
            data-active={kind === entry.value || undefined}
            aria-pressed={kind === entry.value}
            onClick={() => onChangeKind(entry.value)}
            title={entry.label}
          >
            <KiriIcon name={entry.icon} size={12} />
            <span>{entry.label}</span>
          </button>
        ))}
      </div>
      <button
        type="button"
        className="library-favorite-filter"
        data-active={favoritesOnly || undefined}
        aria-pressed={favoritesOnly}
        onClick={onToggleFavorites}
        title={t("Favorites")}
      >
        <KiriIcon name={favoritesOnly ? "star.fill" : "star"} size={12} />
        <span>{t("Favorites")}</span>
      </button>
      <div className="library-tag-filter">
        {allTags.length === 0 ? (
          <span className="library-tag-filter__hint">
            <KiriIcon name="tag" size={10} />
            <span>{t("Tag captures via the ⋯ menu to filter by category")}</span>
          </span>
        ) : (
          allTags.map((tag) => {
            const active = tagFilter?.toLowerCase() === tag.toLowerCase();
            return (
              <button
                type="button"
                key={tag}
                className="library-tag-filter__chip"
                data-active={active || undefined}
                aria-pressed={active}
                onClick={() => onToggleTag(tag)}
                title={tag}
              >
                <KiriIcon name="tag" size={10} />
                {tag}
              </button>
            );
          })
        )}
      </div>
    </div>
  );
}
/** Floating batch-action bar — a frosted pill anchored at the bottom-center
 * of the grid. Appears while items are selected; actions adapt to the
 * current section (Trash → restore / delete; Library → trash / favorite). */
function BatchActionBar(props: {
  count: number;
  showingTrash: boolean;
  allFavorites: boolean;
  onRestore(): void;
  onDelete(): void;
  onMoveToTrash(): void;
  onToggleFavorite(): void;
  onClear(): void;
}) {
  const { count, showingTrash, allFavorites, onRestore, onDelete, onMoveToTrash, onToggleFavorite, onClear } = props;
  const countLabel = t("Selected {n}").replace("{n}", String(count));
  return (
    <div
      style={{
        position: "absolute",
        left: "50%",
        bottom: 28,
        transform: "translateX(-50%)",
        zIndex: 20,
        display: "flex",
        alignItems: "center",
        gap: 4,
        padding: "6px 8px",
        borderRadius: 16,
        background: "color-mix(in srgb, var(--kiri-elevated) 94%, transparent)",
        backdropFilter: "blur(18px)",
        WebkitBackdropFilter: "blur(18px)",
        border: "1px solid var(--kiri-surface-border)",
        boxShadow: "0 8px 24px rgba(0,0,0,0.14)",
      }}
    >
      <span
        style={{
          fontSize: 12,
          fontWeight: 700,
          color: "var(--kiri-label)",
          padding: "0 10px",
          whiteSpace: "nowrap",
        }}
      >
        {countLabel}
      </span>
      <div style={{ width: 1, height: 22, background: "var(--kiri-surface-border)" }} />
      {showingTrash ? (
        <>
          <BatchBarButton icon="arrow.uturn.backward" label={t("Restore (N)").replace("{n}", String(count))} onClick={onRestore} />
          <BatchBarButton icon="trash.fill" label={t("Delete Permanently (N)").replace("{n}", String(count))} destructive onClick={onDelete} />
        </>
      ) : (
        <>
          <BatchBarButton icon="trash" label={t("Delete (N)").replace("{n}", String(count))} destructive onClick={onMoveToTrash} />
          <BatchBarButton
            icon={allFavorites ? "star.fill" : "star"}
            label={allFavorites ? t("Remove from Favorites") : t("Add to Favorites")}
            accent
            onClick={onToggleFavorite}
          />
        </>
      )}
      <div style={{ width: 1, height: 22, background: "var(--kiri-surface-border)" }} />
      <BatchBarButton icon="xmark" label={t("Cancel")} onClick={onClear} />
    </div>
  );
}

function BatchBarButton(props: {
  icon: IconName;
  label: string;
  onClick(): void;
  destructive?: boolean;
  accent?: boolean;
}) {
  const { icon, label, onClick, destructive, accent } = props;
  return (
    <button
      type="button"
      className="kiri-batch-button"
      data-variant={destructive ? "destructive" : accent ? "accent" : undefined}
      onClick={onClick}
    >
      <KiriIcon name={icon} size={13} />
      {label}
    </button>
  );
}


function SegmentedPicker(props: {
  options: { label: string; icon: IconName }[];
  value: number;
  onChange(index: number): void;
}) {
  return (
    <nav
      key={props.value}
      className="library-section-picker"
      aria-label={t("Library")}
    >
      {props.options.map((option, index) => (
        <button
          type="button"
          key={option.label}
          className="library-section-picker__button"
          data-active={props.value === index || undefined}
          aria-pressed={props.value === index}
          onClick={() => props.onChange(index)}
        >
          <KiriIcon name={option.icon} size={12} />
          <span>{option.label}</span>
        </button>
      ))}
    </nav>
  );
}

function EmptyState(props: {
  isTrashEmpty: boolean;
  shortcutStatus: ShortcutStatusDto | null;
}) {
  const { isTrashEmpty, shortcutStatus } = props;
  const title = isTrashEmpty
    ? t("Trash is empty")
    : t("Ready for your first capture");
  const message = isTrashEmpty
    ? t("Captures you delete stay recoverable here.")
    : null;
  const guidance = !isTrashEmpty
    ? t("Choose Screenshot or Record, then select the region you need.")
    : null;
  const shortcutLabel = getAvailableShortcutLabel(shortcutStatus);
  const hint = !isTrashEmpty && shortcutLabel
    ? fmt("or press  %@", shortcutLabel)
    : null;
  return (
    <div
      style={{
        flex: 1,
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        gap: 8,
        padding: 40,
        textAlign: "center",
      }}
    >
      <div
        style={{
          width: 64,
          height: 64,
          borderRadius: 20,
          overflow: "hidden",
          // Spec (KiriBrandMark): the chibi artwork fills the container.
        }}
      >
        <img
          src={brandIcon}
          alt=""
          style={{ width: "100%", height: "100%", objectFit: "cover", display: "block" }}
        />
      </div>
      <div style={{ fontSize: 15, fontWeight: 600 }}>{title}</div>
      {message && (
        <div style={{ fontSize: 12.5, color: "var(--kiri-secondary-label)", maxWidth: 320 }}>
          {message}
        </div>
      )}
      {guidance && (
        <div style={{ fontSize: 12.5, color: "var(--kiri-secondary-label)", maxWidth: 320 }}>
          {guidance}
        </div>
      )}
      {hint && (
        <div
          style={{
            fontSize: 12.5,
            color: "var(--kiri-accent)",
            fontWeight: 600,
            marginTop: 2,
          }}
        >
          {hint}
        </div>
      )}
    </div>
  );
}
