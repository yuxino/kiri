// LibraryWindow — the main window: asset grid, search, sections, trash,
// notices, and error recovery. Port of LibraryView.swift + AppModel.

import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { api, onError, onLibraryChanged, onNotice, type AssetDto, type ErrorDto, type NoticeDto } from "../lib/ipc";
import { t, fmt } from "../i18n";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import brandIcon from "../assets/kiri-icon.png";
import { KiriIcon, type IconName } from "../components/KiriIcons";

type Section = "library" | "trash";

function assetUrl(id: string): string {
  return `kiri://asset/${id}`;
}

/** Groups assets by calendar day, newest group first. */
function groupByDay(assets: AssetDto[]): { label: string; assets: AssetDto[] }[] {
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
    return { label, assets: items };
  });
}

export function LibraryWindow() {
  const [assets, setAssets] = useState<AssetDto[]>([]);
  const [query, setQuery] = useState("");
  const [section, setSection] = useState<Section>("library");
  const [loaded, setLoaded] = useState(false);
  const [notice, setNotice] = useState<NoticeDto | null>(null);
  const [error, setError] = useState<ErrorDto | null>(null);
  const [menuFor, setMenuFor] = useState<string | null>(null);
  // Menu anchor in viewport coordinates (mouse position on right-click, or
  // the ⋯ button's corner), so the menu appears where the user looked.
  const [menuPos, setMenuPos] = useState<{ x: number; y: number } | null>(null);
  const [shortcutLabel, setShortcutLabel] = useState("");
  const [kindFilter, setKindFilter] = useState<"all" | "image" | "video" | "gif">("all");
  const [favoritesOnly, setFavoritesOnly] = useState(false);
  const [tagFilter, setTagFilter] = useState<string | null>(null);
  // Batch selection (ids currently selected). A non-empty selection turns
  // the grid into selection mode with a batch action bar.
  const [selection, setSelection] = useState<Set<string>>(new Set());
  const searchRef = useRef<HTMLInputElement>(null);

  const toggleSelect = (id: string) => {
    setSelection((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  };

  const clearSelection = () => setSelection(new Set());

  // Stable ordered list of selected ids (Set preserves insertion order).
  const selectionIds = [...selection];

  const localNoticeSeq = useRef(0);
  const showLocalNotice = (title: string) => {
    const id = `local-${++localNoticeSeq.current}`;
    setNotice({ id, title, symbol: "checkmark" });
    setTimeout(() => setNotice((current) => (current?.id === id ? null : current)), 2000);
  };

  // Esc exits selection mode.
  useEffect(() => {
    if (selection.size === 0) return;
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        clearSelection();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [selection.size]);

  // Changing section/query clears the selection so the bar never points at
  // assets that are no longer visible.
  useEffect(() => {
    clearSelection();
  }, [section, query, kindFilter, favoritesOnly, tagFilter]);

  useEffect(() => {
    api.getShortcutLabel().then(setShortcutLabel).catch(() => {});
  }, []);

  const showingTrash = section === "trash";

  const refresh = useCallback(async () => {
    const list = await api.listAssets(query, showingTrash);
    setAssets(list);
    setLoaded(true);
  }, [query, showingTrash]);

  useEffect(() => {
    void refresh().catch(() => {});
  }, [refresh]);

  useEffect(() => {
    const unsubs: Promise<() => void>[] = [];
    unsubs.push(
      onLibraryChanged(() => void refresh().catch(() => {})).then((fn) => () => fn()),
      onNotice((n) => {
        setNotice(n);
        setTimeout(() => {
          setNotice((current) => (current && current.id === n.id ? null : current));
        }, 2000);
      }),
      onError((e) => setError(e)),
    );
    return () => {
      unsubs.forEach((p) => p.then((fn) => fn()));
    };
  }, [refresh]);

  // ⌘F focuses search; ⌘⌥I toggles the developer tools.
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "f") {
        e.preventDefault();
        searchRef.current?.focus();
        searchRef.current?.select();
      } else if ((e.metaKey || e.ctrlKey) && e.altKey && e.key.toLowerCase() === "i") {
        e.preventDefault();
        void import("@tauri-apps/api/core").then(({ invoke }) => {
          invoke("open_devtools").catch(() => {});
        });
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  const gridStyle: React.CSSProperties = {
    display: "grid",
    gridTemplateColumns: "repeat(auto-fill, minmax(210px, 1fr))",
    gap: 20,
    alignContent: "start",
  };

  // Front-end filters (kind + favorites) applied on top of the backend
  // search — view-layer conveniences shared by Library and Trash so both
  // sections behave identically.
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

  // All tags in the current view, for the tag filter bar.
  const allTags = useMemo(() => {
    const set = new Set<string>();
    for (const asset of assets) {
      for (const tag of asset.tags) set.add(tag);
    }
    return [...set].sort((a, b) => a.localeCompare(b));
  }, [assets]);

  const isSearchEmpty = query.trim() !== "" && assets.length === 0 && loaded;
  const isEmpty = query.trim() === "" && assets.length === 0 && loaded;
  const isFilterEmpty = !isSearchEmpty && !isEmpty && filteredAssets.length === 0 && loaded;

  const openMenu = (id: string, x: number, y: number) => {
    if (menuFor === id) {
      setMenuFor(null);
      setMenuPos(null);
      return;
    }
    setMenuFor(id);
    setMenuPos({ x, y });
  };

  // Clicking anywhere outside a card menu closes it (matches native menus).
  useEffect(() => {
    const close = (e: MouseEvent) => {
      const target = e.target as HTMLElement | null;
      if (target && target.closest(".kiri-card-menu")) return;
      setMenuFor(null);
      setMenuPos(null);
    };
    window.addEventListener("mousedown", close);
    return () => window.removeEventListener("mousedown", close);
  }, []);

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
  const closeMenu = useCallback(() => {
    setMenuFor(null);
    setMenuPos(null);
  }, []);
  const run = useCallback(
    (fn: () => void) => () => {
      fn();
      closeMenu();
    },
    [closeMenu],
  );

  const itemMenu = useCallback(
    (asset: AssetDto) => (
      <div
        className="kiri-card-menu"
        style={{
          position: "fixed",
          ...(menuStyle ?? { left: 0, top: 0 }),
          background: "color-mix(in srgb, var(--kiri-elevated) 92%, transparent)",
          backdropFilter: "blur(24px) saturate(1.5)",
          WebkitBackdropFilter: "blur(24px) saturate(1.5)",
          border: "1px solid var(--kiri-surface-border)",
          borderRadius: 14,
          padding: 6,
          minWidth: 196,
          boxShadow: "0 10px 24px rgba(0,0,0,0.22), 0 2px 6px rgba(0,0,0,0.10)",
          zIndex: 100,
          display: "flex",
          flexDirection: "column",
          animation: "kiri-menu-in 0.12s ease-out",
        }}
      >
        {asset.kind === "image" && (
          <MenuRow icon="doc.on.doc" label={t("Copy")} onClick={run(() => void api.copyAsset(asset.id).catch(() => {}))} />
        )}
        <MenuRow
          icon="character.textbox"
          label={t("Rename")}
          onClick={run(() => {
            window.dispatchEvent(new CustomEvent(`kiri-rename:${asset.id}`));
          })}
        />
        <MenuRow
          icon="tag"
          label={t("Add Tag…")}
          onClick={run(() => {
            window.dispatchEvent(new CustomEvent(`kiri-addtag:${asset.id}`));
          })}
        />
        <MenuRow icon="photo.on.rectangle" label={t("Open")} onClick={run(() => void api.openAsset(asset.id).catch(() => {}))} />
        <MenuRow icon="folder" label={t("Show in Finder")} onClick={run(() => void api.revealAsset(asset.id).catch(() => {}))} />
        {asset.gifEligible && (
          <MenuRow icon="sparkles.rectangle.stack" label={t("Convert to GIF")} onClick={run(() => void api.convertToGif(asset.id).catch(() => {}))} />
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
    [showingTrash, menuStyle, run],
  );

  return (
    <div
      className="library-root kiri-canvas-surface"
      style={{ display: "flex", flexDirection: "column", height: "100%", position: "relative" }}
    >
      {/* Header — single compact row (mirrors Swift wideHeader): title,
          search, section picker, then a slim filter bar under the content
          area instead of crowding the header. */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 12,
          padding: "12px 24px",
          background: "color-mix(in srgb, var(--kiri-canvas) 80%, transparent)",
          backdropFilter: "blur(20px) saturate(1.3)",
          WebkitBackdropFilter: "blur(20px) saturate(1.3)",
          borderBottom: "1px solid var(--kiri-surface-border)",
          zIndex: 2,
        }}
      >
        <div
          style={{
            width: 32,
            height: 32,
            borderRadius: 9,
            overflow: "hidden",
            boxShadow: "0 2px 8px rgba(0,0,0,0.08)",
            flexShrink: 0,
          }}
        >
          <img src={brandIcon} alt="" style={{ width: "100%", height: "100%", objectFit: "cover", display: "block" }} />
        </div>
        <div style={{ display: "flex", flexDirection: "column", gap: 1, flexShrink: 0 }}>
          <span style={{ fontSize: 15, fontWeight: 700, color: "var(--kiri-label)", lineHeight: 1.2 }}>
            {showingTrash ? t("Trash") : t("Library")}
          </span>
          <span style={{ fontSize: 10.5, color: "var(--kiri-secondary-label)" }}>
            {fmt("%d captures", assets.length)}
          </span>
        </div>
        <div style={{ flex: 1 }} />
        <div style={{ position: "relative", flexShrink: 0 }}>
          <span
            style={{
              position: "absolute",
              left: 10,
              top: "50%",
              transform: "translateY(-50%)",
              color: "var(--kiri-disabled-label)",
              display: "flex",
              alignItems: "center",
              pointerEvents: "none",
            }}
          >
            <KiriIcon name="magnifyingglass" size={13} />
          </span>
          <input
            ref={searchRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t("Search captures")}
            style={{
              width: 200,
              height: 32,
              borderRadius: 10,
              border: "1px solid var(--kiri-surface-border)",
              background: "var(--kiri-group-fill)",
              color: "var(--kiri-label)",
              padding: "0 30px 0 30px",
              fontSize: 12,
              transition: "border-color 0.14s ease-out, box-shadow 0.14s ease-out",
            }}
            onFocus={(e) => {
              e.currentTarget.style.borderColor = "var(--kiri-accent)";
              e.currentTarget.style.boxShadow = "0 0 0 3px var(--kiri-accent-soft-alpha-10)";
            }}
            onBlur={(e) => {
              e.currentTarget.style.borderColor = "var(--kiri-surface-border)";
              e.currentTarget.style.boxShadow = "none";
            }}
          />
          {query !== "" && (
            <button
              onClick={() => setQuery("")}
              title={t("Clear Search")}
              style={{
                position: "absolute",
                right: 5,
                top: 4,
                width: 24,
                height: 24,
                borderRadius: 6,
                border: "none",
                background: "transparent",
                color: "var(--kiri-secondary-label)",
                cursor: "default",
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
              }}
            >
              <KiriIcon name="xmark" size={10} />
            </button>
          )}
        </div>
        <SegmentedPicker
          options={[t("Library"), t("Trash")]}
          value={section === "library" ? 0 : 1}
          onChange={(index) => {
            setSection(index === 0 ? "library" : "trash");
            setQuery("");
            setKindFilter("all");
            setFavoritesOnly(false);
            setTagFilter(null);
          }}
        />
      </div>

      {/* Slim filter bar above the content — identical in Library and
          Trash so both sections share the same interaction language.
          Section-level actions (Empty Trash) sit at the right end. */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 10,
          padding: "8px 24px",
          borderBottom: "1px solid var(--kiri-surface-border)",
          background: "color-mix(in srgb, var(--kiri-canvas) 92%, transparent)",
        }}
      >
        {selection.size > 0 ? (
          <>
            <span style={{ fontSize: 12, fontWeight: 600, color: "var(--kiri-label)", whiteSpace: "nowrap" }}>
              {t("Selected {n}").replace("{n}", String(selection.size))}
            </span>
            <div style={{ width: 1, height: 20, background: "var(--kiri-surface-border)" }} />
            {showingTrash ? (
              <>
                <button
                  className="kiri-button"
                  onClick={() => {
                    void api
                      .batchRestore([...selection])
                      .then(() => showLocalNotice(t("Restored to Library")))
                      .catch(() => {});
                    clearSelection();
                  }}
                  style={{ minHeight: 28, padding: "0 10px", fontSize: 11.5, flexShrink: 0 }}
                >
                  <KiriIcon name="arrow.uturn.backward" size={12} />
                  {t("Restore (N)").replace("(N)", `(${selection.size})`)}
                </button>
                <button
                  className="kiri-button kiri-button--destructive"
                  onClick={() =>
                    void api.showConfirmDialog(
                      "batchDelete",
                      t("Delete these captures permanently?"),
                      t("This cannot be undone."),
                      t("Delete Permanently (N)").replace("(N)", `(${selection.size})`),
                      [...selection],
                    )
                  }
                  style={{ minHeight: 28, padding: "0 10px", fontSize: 11.5, flexShrink: 0 }}
                >
                  <KiriIcon name="trash.fill" size={12} />
                  {t("Delete Permanently (N)").replace("(N)", `(${selection.size})`)}
                </button>
              </>
            ) : (
              <>
                <button
                  className="kiri-button kiri-button--destructive"
                  onClick={() => {
                    void api
                      .batchMoveToTrash([...selection])
                      .then(() => showLocalNotice(t("Moved to Trash")))
                      .catch(() => {});
                    clearSelection();
                  }}
                  style={{ minHeight: 28, padding: "0 10px", fontSize: 11.5, flexShrink: 0 }}
                >
                  <KiriIcon name="trash" size={12} />
                  {t("Delete (N)").replace("(N)", `(${selection.size})`)}
                </button>
                <button
                  className="kiri-button"
                  onClick={() => {
                    const allFav = selectionIds.every((id) => assets.find((a) => a.id === id)?.isFavorite);
                    void api
                      .batchSetFavorite([...selection], !allFav)
                      .then(() => showLocalNotice(allFav ? t("Remove from Favorites") : t("Add to Favorites")))
                      .catch(() => {});
                    clearSelection();
                  }}
                  style={{ minHeight: 28, padding: "0 10px", fontSize: 11.5, flexShrink: 0 }}
                >
                  <KiriIcon name="star" size={12} />
                  {selectionIds.every((id) => assets.find((a) => a.id === id)?.isFavorite)
                    ? t("Remove from Favorites")
                    : t("Add to Favorites")}
                </button>
              </>
            )}
            <div style={{ flex: 1 }} />
            <button
              className="kiri-button"
              onClick={clearSelection}
              style={{ minHeight: 28, padding: "0 10px", fontSize: 11.5, flexShrink: 0 }}
            >
              {t("Cancel")}
            </button>
          </>
        ) : (
          <>
            <FilterBar
              kind={kindFilter}
              favoritesOnly={favoritesOnly}
              tagFilter={tagFilter}
              allTags={allTags}
              onChangeKind={setKindFilter}
              onToggleFavorites={() => setFavoritesOnly((v) => !v)}
              onToggleTag={(tag) => setTagFilter((current) => (current === tag ? null : tag))}
            />
            <div style={{ flex: 1 }} />
            {showingTrash && assets.length > 0 && (
              <button
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
                style={{ minHeight: 28, padding: "0 10px", fontSize: 11.5, flexShrink: 0 }}
              >
                <KiriIcon name="trash.fill" size={12} />
                {t("Empty Trash")}
              </button>
            )}
          </>
        )}
      </div>

      {/* Grid */}
      {!loaded ? (
        <div style={{ flex: 1, display: "flex", alignItems: "center", justifyContent: "center", color: "var(--kiri-secondary-label)" }}>
          {t("Loading Library…")}
        </div>
      ) : isEmpty || isSearchEmpty ? (
        <EmptyState
          isSearchEmpty={isSearchEmpty}
          isTrashEmpty={isEmpty && showingTrash}
          shortcutLabel={shortcutLabel}
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
        <div style={{ flex: 1, overflowY: "auto", padding: "0 24px 24px" }}>
          {groupByDay(filteredAssets).map((group) => (
            <div key={group.label} style={{ marginBottom: 24 }}>
              <div
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 8,
                  marginBottom: 12,
                  paddingTop: 10,
                }}
              >
                <div
                  style={{
                    width: 4,
                    height: 14,
                    borderRadius: 2,
                    background: "var(--kiri-accent)",
                    opacity: 0.7,
                  }}
                />
                <span
                  style={{
                    fontSize: 12,
                    fontWeight: 600,
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
                    menuOpen={menuFor === asset.id}
                    onMenu={(x, y) => openMenu(asset.id, x, y)}
                    menu={itemMenu(asset)}
                    selected={selection.has(asset.id)}
                    onToggleSelect={() => toggleSelect(asset.id)}
                    onDoubleClick={() => void api.openAsset(asset.id).catch(() => {})}
                    onDragStart={(e) => {
                      // Only start a file drag from the thumbnail — dragging
                      // from the action buttons (Copy / favorite / ⋯) would
                      // swallow their click (HTML5 drag cancels click).
                      const target = e.target as HTMLElement | null;
                      if (target && target.closest("button")) {
                        e.preventDefault();
                        return;
                      }
                      // Drag-out exports the file (HTML5 drag handled by Tauri).
                      void (
                        getCurrentWebview() as unknown as {
                          startDragging(args: unknown): Promise<void>;
                        }
                      )
                        .startDragging({
                          type: "files",
                          files: [asset.filePath],
                        })
                        .catch(() => {});
                      e.preventDefault();
                    }}
                  />
                ))}
              </div>
            </div>
          ))}
        </div>
      )}

      {/* Notice toast */}
      {notice && (
        <div
          style={{
            position: "absolute",
            left: "50%",
            bottom: 24,
            transform: "translateX(-50%)",
            background: "var(--kiri-elevated)",
            border: "1px solid var(--kiri-surface-border)",
            borderRadius: 13,
            padding: "8px 14px",
            display: "flex",
            gap: 8,
            alignItems: "center",
            boxShadow: "0 8px 18px rgba(0,0,0,0.18)",
            color: "var(--kiri-label)",
            fontSize: 12.5,
            fontWeight: 500,
          }}
        >
          {notice.symbol && <KiriIcon name={notice.symbol as never} size={13} />}
          {t(notice.title)}
        </div>
      )}

      {/* Error banner */}
      {error && (
        <div
          style={{
            position: "absolute",
            left: "50%",
            top: 72,
            transform: "translateX(-50%)",
            background: "var(--kiri-elevated)",
            border: "1px solid var(--kiri-surface-border)",
            borderRadius: 13,
            padding: "10px 16px",
            display: "flex",
            gap: 12,
            alignItems: "center",
            boxShadow: "0 8px 18px rgba(0,0,0,0.18)",
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
            style={{
              border: "none",
              background: "transparent",
              color: "var(--kiri-secondary-label)",
              cursor: "default",
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
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

function recoveryLabel(recovery: string): string {
  switch (recovery) {
    case "openSettings":
      return t("Open Settings");
    case "quitKiri":
      return t("Quit Kiri");
    case "openAccessibilitySettings":
      return t("Open Accessibility Settings");
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
  menuOpen: boolean;
  onMenu(x: number, y: number): void;
  menu: React.ReactNode;
  onDoubleClick(): void;
  onDragStart(e: React.DragEvent): void;
  selected: boolean;
  onToggleSelect(): void;
}) {
  const { asset, menuOpen, onMenu, menu, onDoubleClick, onDragStart, selected, onToggleSelect } =
    props;
  const [hovered, setHovered] = useState(false);
  const [editingTitle, setEditingTitle] = useState(false);
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
  return (
    <div
      draggable
      onDragStart={onDragStart}
      onDoubleClick={onDoubleClick}
      onContextMenu={(e) => {
        // Right-click shows the localized action menu at the cursor
        // (same as ⋯), never the webview's system context menu.
        e.preventDefault();
        onMenu(e.clientX, e.clientY);
      }}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      style={{
        position: "relative",
        background: "var(--kiri-card)",
        border: `1px solid ${selected ? "#7D69F5" : hovered ? "rgba(125,105,245,0.55)" : "var(--kiri-surface-border)"}`,
        borderRadius: 18,
        padding: 12,
        transform: hovered && !selected ? "translateY(-1px)" : "none",
        boxShadow: selected
          ? "0 0 0 2px rgba(125,105,245,0.35), 0 8px 18px rgba(0,0,0,0.08)"
          : hovered
            ? "0 8px 18px rgba(0,0,0,0.08)"
            : "0 3px 8px rgba(0,0,0,0.045)",
        transition: "transform 0.14s ease-out, box-shadow 0.14s ease-out, border-color 0.14s ease-out",
        cursor: "default",
      }}
    >
      {/* Selection check — visible on hover, pinned when selected. */}
      <button
        onMouseDown={(e) => e.preventDefault()}
        onClick={(e) => {
          e.stopPropagation();
          onToggleSelect();
        }}
        title={selected ? t("Cancel") : t("Select")}
        style={{
          position: "absolute",
          top: 8,
          left: 8,
          width: 24,
          height: 24,
          borderRadius: "50%",
          border: `1.5px solid ${selected ? "#7D69F5" : "rgba(255,255,255,0.9)"}`,
          background: selected
            ? "#7D69F5"
            : "rgba(0,0,0,0.35)",
          color: "#fff",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          cursor: "default",
          opacity: selected || hovered ? 1 : 0,
          transition: "opacity 0.14s ease-out, background 0.14s ease-out",
          zIndex: 2,
          boxShadow: "0 2px 6px rgba(0,0,0,0.3)",
        }}
      >
        {selected ? <KiriIcon name="checkmark" size={13} /> : null}
      </button>
      <div
        style={{
          height: 184,
          borderRadius: 14,
          overflow: "hidden",
          // Spec: thumbnail sits on an accent→cyan gradient (Swift
          // CaptureThumbnail), not a flat black background.
          background:
            "linear-gradient(135deg, rgba(125,105,245,0.075), rgba(79,191,240,0.04))",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        {asset.kind === "image" ? (
          <img
            src={assetUrl(asset.id)}
            alt=""
            draggable={false}
            // Spec: scaledToFit + 5pt padding — the whole image is visible,
            // never cropped.
            style={{ width: "100%", height: "100%", objectFit: "contain", padding: 5, boxSizing: "border-box" }}
          />
        ) : asset.kind === "video" ? (
          <div style={{ position: "relative", width: "100%", height: "100%" }}>
            <img
              src={assetUrl(asset.id)}
              alt=""
              draggable={false}
              style={{ width: "100%", height: "100%", objectFit: "contain", padding: 5, boxSizing: "border-box" }}
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
              src={assetUrl(asset.id)}
              alt=""
              draggable={false}
              style={{ width: "100%", height: "100%", objectFit: "contain", padding: 5, boxSizing: "border-box" }}
            />
            <div style={{ position: "absolute", left: 8, bottom: 8, background: "rgba(0,0,0,0.6)", color: "#fff", borderRadius: 6, padding: "2px 6px", fontSize: 10, fontWeight: 600 }}>
              GIF
            </div>
          </div>
        )}
      </div>
      <div style={{ display: "flex", alignItems: "center", gap: 8, marginTop: 8, position: "relative" }}>
        {editingTitle ? (
          <input
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
              fontSize: 12,
              fontWeight: 500,
              color: "var(--kiri-label)",
              border: "1px solid var(--kiri-accent)",
              borderRadius: 7,
              background: "var(--kiri-group-fill)",
              padding: "3px 6px",
              outline: "none",
            }}
          />
        ) : (
          <span
            title={asset.filename}
            onDoubleClick={(e) => {
              e.stopPropagation();
              setTitleDraft(asset.title ?? "");
              setEditingTitle(true);
            }}
            style={{
              flex: 1,
              fontSize: 11.5,
              fontWeight: 600,
              color: asset.title ? "var(--kiri-label)" : "var(--kiri-secondary-label)",
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
              cursor: "text",
            }}
          >
            {asset.title ??
              new Date(asset.createdAt).toLocaleTimeString(undefined, {
                hour: "2-digit",
                minute: "2-digit",
              })}
          </span>
        )}
        <button
          className="kiri-icon-button"
          title={t("View")}
          draggable={false}
          onDragStart={(e) => e.preventDefault()}
          onMouseDown={(e) => e.preventDefault()}
          onClick={(e) => {
            e.stopPropagation();
            void api.openAsset(asset.id).catch(() => {});
          }}
          onDoubleClick={(e) => {
            e.stopPropagation();
            void api.openAsset(asset.id).catch(() => {});
          }}
          style={{
            width: 26,
            height: 26,
            borderRadius: 8,
            fontSize: 13,
            cursor: "default",
          }}
        >
          <KiriIcon name="eye" size={14} />
        </button>
        <button
          className="kiri-icon-button"
          title={t("Copy")}
          draggable={false}
          onDragStart={(e) => e.preventDefault()}
          onMouseDown={(e) => {
            // Prevent the draggable card from starting an HTML5 drag when
            // pressing the button — a drag would swallow the click.
            e.preventDefault();
          }}
          onClick={(e) => {
            e.stopPropagation();
            void api.copyAsset(asset.id).catch(() => {});
          }}
          onDoubleClick={(e) => {
            // A rapid double-click on Copy must copy, not open the asset.
            e.stopPropagation();
            void api.copyAsset(asset.id).catch(() => {});
          }}
          style={{
            width: 26,
            height: 26,
            borderRadius: 8,
            fontSize: 13,
            cursor: "default",
          }}
        >
          <KiriIcon name="doc.on.doc" size={14} />
        </button>
        <button
          className="kiri-icon-button"
          title={asset.isFavorite ? t("Remove Favorite") : t("Favorite")}
          draggable={false}
          onDragStart={(e) => e.preventDefault()}
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
            color: asset.isFavorite ? "#FFD129" : "var(--kiri-disabled-label)",
            fontSize: 13,
            cursor: "default",
          }}
          onMouseEnter={(e) => {
            if (!asset.isFavorite) e.currentTarget.style.color = "#FFD129";
          }}
          onMouseLeave={(e) => {
            if (!asset.isFavorite) e.currentTarget.style.color = "var(--kiri-disabled-label)";
          }}
        >
          <KiriIcon name={asset.isFavorite ? "star.fill" : "star"} size={15} />
        </button>
        <button
          className="kiri-icon-button"
          title={t("More Actions")}
          draggable={false}
          onDragStart={(e) => e.preventDefault()}
          onMouseDown={(e) => e.preventDefault()}
          onClick={(e) => {
            e.stopPropagation();
            const rect = e.currentTarget.getBoundingClientRect();
            // Anchor the menu below the ⋯ button; edge-flip handled in
            // menuStyle (left-aligned so a near-right flip stays sane).
            onMenu(rect.left, rect.bottom + 4);
          }}
          onDoubleClick={(e) => {
            e.stopPropagation();
          }}
          style={{
            width: 26,
            height: 26,
            borderRadius: 8,
            fontSize: 13,
            cursor: "default",
          }}
        >
          <KiriIcon name="ellipsis.circle" size={14} />
        </button>
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
                onClick={(e) => {
                  e.stopPropagation();
                  void api
                    .setTags(
                      asset.id,
                      asset.tags.filter((existing) => existing !== tag),
                    )
                    .catch(() => {});
                }}
                style={{
                  border: "none",
                  background: "transparent",
                  color: "inherit",
                  cursor: "default",
                  padding: 0,
                  display: "flex",
                  fontSize: 9,
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
                outline: "none",
              }}
            />
          )}
        </div>
      )}
    </div>
  );
}

function MenuRow(props: { label: string; icon?: IconName; onClick(): void; destructive?: boolean }) {
  return (
    <button
      onClick={props.onClick}
      style={{
        background: "transparent",
        border: "none",
        textAlign: "left",
        padding: "7px 10px",
        borderRadius: 9,
        color: props.destructive ? "#FA476E" : "var(--kiri-label)",
        font: "400 12.5px var(--kiri-font-ui)",
        cursor: "default",
        display: "flex",
        alignItems: "center",
        gap: 10,
        transition: "background 0.12s ease-out, transform 0.06s ease-out",
      }}
      onMouseEnter={(e) => {
        e.currentTarget.style.background =
          "color-mix(in srgb, var(--kiri-accent) 18%, transparent)";
      }}
      onMouseLeave={(e) => {
        e.currentTarget.style.background = "transparent";
      }}
      onMouseDown={(e) => {
        e.currentTarget.style.transform = "translateY(0.5px)";
      }}
      onMouseUp={(e) => {
        e.currentTarget.style.transform = "none";
      }}
    >
      {props.icon && (
        <span
          style={{
            width: 15,
            display: "flex",
            justifyContent: "center",
            color: props.destructive ? "#FA476E" : "var(--kiri-accent)",
            opacity: 0.9,
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
    { value: "all", label: t("All"), icon: "photo.on.rectangle" },
    { value: "image", label: t("Images"), icon: "photo.on.rectangle" },
    { value: "video", label: t("Videos"), icon: "play.rectangle" },
    { value: "gif", label: t("GIFs"), icon: "sparkles.rectangle.stack" },
  ];
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
      <div
        style={{
          display: "flex",
          background: "var(--kiri-group-fill)",
          borderRadius: 11,
          padding: 3,
          gap: 2,
        }}
      >
        {kinds.map((entry) => (
          <button
            key={entry.value}
            onClick={() => onChangeKind(entry.value)}
            title={entry.label}
            style={{
              height: 30,
              minWidth: 34,
              padding: "0 9px",
              borderRadius: 9,
              border: "none",
              background: kind === entry.value ? "#634FDB" : "transparent",
              color: kind === entry.value ? "#fff" : "var(--kiri-secondary-label)",
              cursor: "default",
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              transition: "background 0.14s ease-out, color 0.14s ease-out",
            }}
            onMouseEnter={(e) => {
              if (kind !== entry.value) {
                e.currentTarget.style.background = "rgba(125,105,245,0.10)";
              }
            }}
            onMouseLeave={(e) => {
              if (kind !== entry.value) e.currentTarget.style.background = "transparent";
            }}
          >
            {entry.value === "all" ? (
              <KiriIcon name={entry.icon} size={13} />
            ) : (
              <span style={{ fontSize: 11, fontWeight: 600, whiteSpace: "nowrap" }}>
                {entry.label}
              </span>
            )}
          </button>
        ))}
      </div>
      <button
        onClick={onToggleFavorites}
        title={t("Favorites")}
        style={{
          height: 30,
          minWidth: 30,
          padding: "0 9px",
          borderRadius: 9,
          border: "1px solid",
          borderColor: favoritesOnly
            ? "rgba(255,209,41,0.5)"
            : "var(--kiri-surface-border)",
          background: favoritesOnly ? "rgba(255,209,41,0.12)" : "transparent",
          color: favoritesOnly ? "#FFD129" : "var(--kiri-secondary-label)",
          cursor: "default",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          transition: "background 0.14s ease-out, color 0.14s ease-out",
        }}
        onMouseEnter={(e) => {
          if (!favoritesOnly) e.currentTarget.style.color = "#FFD129";
        }}
        onMouseLeave={(e) => {
          if (!favoritesOnly) e.currentTarget.style.color = "var(--kiri-secondary-label)";
        }}
      >
        <KiriIcon name="star" size={13} />
      </button>
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 4,
          maxWidth: 340,
          overflowX: "auto",
          paddingBottom: 1,
        }}
      >
        {allTags.length === 0 ? (
          <span
            style={{
              fontSize: 10.5,
              color: "var(--kiri-disabled-label)",
              whiteSpace: "nowrap",
              display: "inline-flex",
              alignItems: "center",
              gap: 4,
              padding: "0 4px",
            }}
          >
            <KiriIcon name="tag" size={10} />
            {t("Tag captures via the ⋯ menu to filter by category")}
          </span>
        ) : (
          allTags.map((tag) => {
            const active = tagFilter?.toLowerCase() === tag.toLowerCase();
            return (
              <button
                key={tag}
                onClick={() => onToggleTag(tag)}
                title={tag}
                style={{
                  height: 26,
                  padding: "0 9px",
                  borderRadius: 8,
                  border: "1px solid",
                  borderColor: active
                    ? "var(--kiri-accent)"
                    : "var(--kiri-surface-border)",
                  background: active
                    ? "var(--kiri-accent-soft-alpha-10)"
                    : "transparent",
                  color: active
                    ? "var(--kiri-accent)"
                    : "var(--kiri-secondary-label)",
                  fontSize: 10.5,
                  fontWeight: 600,
                  cursor: "default",
                  whiteSpace: "nowrap",
                  display: "inline-flex",
                  alignItems: "center",
                  gap: 4,
                  transition: "background 0.14s ease-out, color 0.14s ease-out",
                }}
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

function SegmentedPicker(props: {
  options: string[];
  value: number;
  onChange(index: number): void;
}) {
  return (
    <div
      style={{
        display: "flex",
        background: "var(--kiri-group-fill)",
        borderRadius: 11,
        padding: 3,
        gap: 2,
      }}
    >
      {props.options.map((option, index) => (
        <button
          key={option}
          onClick={() => props.onChange(index)}
          style={{
            height: 30,
            padding: "0 16px",
            borderRadius: 9,
            border: "none",
            background: props.value === index ? "#634FDB" : "transparent",
            color: props.value === index ? "#fff" : "var(--kiri-secondary-label)",
            font: "600 12px var(--kiri-font-ui)",
            cursor: "default",
            transition: "background 0.14s ease-out, color 0.14s ease-out",
            boxShadow: props.value === index ? "0 1px 4px rgba(99,79,219,0.3)" : "none",
          }}
          onMouseEnter={(e) => {
            if (props.value !== index) {
              e.currentTarget.style.background = "rgba(125,105,245,0.10)";
              e.currentTarget.style.color = "var(--kiri-label)";
            }
          }}
          onMouseLeave={(e) => {
            if (props.value !== index) {
              e.currentTarget.style.background = "transparent";
              e.currentTarget.style.color = "var(--kiri-secondary-label)";
            }
          }}
        >
          {option}
        </button>
      ))}
    </div>
  );
}

function EmptyState(props: { isSearchEmpty: boolean; isTrashEmpty: boolean; shortcutLabel: string }) {
  const { isSearchEmpty, isTrashEmpty, shortcutLabel } = props;
  const title = isSearchEmpty
    ? t("No matching captures")
    : isTrashEmpty
      ? t("Trash is empty")
      : t("Ready for your first capture");
  const message = isSearchEmpty
    ? t("Try a different search, or clear the current one.")
    : isTrashEmpty
      ? t("Captures you delete stay recoverable here.")
      : null;
  const guidance = !isSearchEmpty && !isTrashEmpty
    ? t("Choose Screenshot or Record, then select the region you need.")
    : null;
  const hint = !isSearchEmpty && !isTrashEmpty && shortcutLabel
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
        <img src={brandIcon} alt="" style={{ width: "100%", height: "100%", objectFit: "cover", display: "block" }} />
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
