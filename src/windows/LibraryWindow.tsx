// LibraryWindow — the main window: asset grid, search, sections, trash,
// notices, and error recovery. Port of LibraryView.swift + AppModel.

import React, { useCallback, useEffect, useRef, useState } from "react";
import { api, onError, onLibraryChanged, onNotice, type AssetDto, type ErrorDto, type NoticeDto } from "../lib/ipc";
import { t, fmt } from "../i18n";
import { getCurrentWebview } from "@tauri-apps/api/webview";

type Section = "library" | "trash";

interface ConfirmState {
  title: string;
  message: string;
  confirmLabel: string;
  onConfirm(): void;
}

function assetUrl(id: string): string {
  return `kiri://asset/${id}`;
}

export function LibraryWindow() {
  const [assets, setAssets] = useState<AssetDto[]>([]);
  const [query, setQuery] = useState("");
  const [section, setSection] = useState<Section>("library");
  const [loaded, setLoaded] = useState(false);
  const [notice, setNotice] = useState<NoticeDto | null>(null);
  const [error, setError] = useState<ErrorDto | null>(null);
  const [confirm, setConfirm] = useState<ConfirmState | null>(null);
  const [menuFor, setMenuFor] = useState<string | null>(null);
  const searchRef = useRef<HTMLInputElement>(null);

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

  // ⌘F focuses search.
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "f") {
        e.preventDefault();
        searchRef.current?.focus();
        searchRef.current?.select();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  const gridStyle: React.CSSProperties = {
    display: "grid",
    gridTemplateColumns: "repeat(auto-fill, minmax(210px, 1fr))",
    gap: 20,
    padding: "0 24px 24px",
    overflowY: "auto",
    flex: 1,
    alignContent: "start",
  };

  const isSearchEmpty = query.trim() !== "" && assets.length === 0 && loaded;
  const isEmpty = query.trim() === "" && assets.length === 0 && loaded;

  const openMenu = (id: string) => setMenuFor((current) => (current === id ? null : id));

  const itemMenu = useCallback(
    (asset: AssetDto) => (
      <div
        className="kiri-card-menu"
        style={{
          position: "absolute",
          right: 0,
          bottom: 44,
          background: "var(--kiri-elevated)",
          border: "1px solid var(--kiri-surface-border)",
          borderRadius: 12,
          padding: 6,
          minWidth: 180,
          boxShadow: "0 8px 18px rgba(0,0,0,0.10)",
          zIndex: 5,
          display: "flex",
          flexDirection: "column",
        }}
      >
        {asset.kind === "image" && (
          <MenuRow label={t("Copy")} onClick={() => void api.copyAsset(asset.id).catch(() => {})} />
        )}
        <MenuRow label={t("Open")} onClick={() => void api.openAsset(asset.id).catch(() => {})} />
        <MenuRow label={t("Show in Finder")} onClick={() => void api.revealAsset(asset.id).catch(() => {})} />
        {asset.gifEligible && (
          <MenuRow label={t("Convert to GIF")} onClick={() => void api.convertToGif(asset.id).catch(() => {})} />
        )}
        {showingTrash ? (
          <>
            <MenuRow
              label={t("Restore")}
              onClick={() => void api.restoreAsset(asset.id).catch(() => {})}
            />
            <MenuRow
              label={t("Delete Permanently")}
              destructive
              onClick={() =>
                setConfirm({
                  title: t("Delete this capture permanently?"),
                  message: t("This cannot be undone."),
                  confirmLabel: t("Delete Permanently"),
                  onConfirm: () => void api.permanentlyDelete(asset.id).catch(() => {}),
                })
              }
            />
          </>
        ) : (
          <MenuRow
            label={t("Move to Trash")}
            onClick={() => void api.moveToTrash(asset.id).catch(() => {})}
          />
        )}
      </div>
    ),
    [showingTrash],
  );

  return (
    <div
      className="library-root kiri-canvas-surface"
      style={{ display: "flex", flexDirection: "column", height: "100%", position: "relative" }}
    >
      {/* Header */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 14,
          padding: "16px 24px 12px",
        }}
      >
        <div
          style={{
            width: 38,
            height: 38,
            borderRadius: 11,
            background: "linear-gradient(135deg, #634FDB, #7D69F5, #4FBFF0)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            color: "#fff",
            fontWeight: 700,
            fontSize: 15,
          }}
        >
          ✦
        </div>
        <div style={{ position: "relative" }}>
          <input
            ref={searchRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t("Search captures")}
            style={{
              width: 228,
              height: 36,
              borderRadius: 11,
              border: "1px solid var(--kiri-surface-border)",
              background: "var(--kiri-group-fill)",
              color: "var(--kiri-label)",
              padding: "0 32px 0 12px",
              fontSize: 12.5,
            }}
          />
          {query !== "" && (
            <button
              onClick={() => setQuery("")}
              title={t("Clear Search")}
              style={{
                position: "absolute",
                right: 6,
                top: 6,
                width: 24,
                height: 24,
                borderRadius: 6,
                border: "none",
                background: "transparent",
                color: "var(--kiri-secondary-label)",
                cursor: "default",
              }}
            >
              ✕
            </button>
          )}
        </div>
        <SegmentedPicker
          options={[t("Library"), t("Trash")]}
          value={section === "library" ? 0 : 1}
          onChange={(index) => {
            setSection(index === 0 ? "library" : "trash");
            setQuery("");
          }}
        />
        <div style={{ flex: 1 }} />
        {showingTrash && assets.length > 0 && (
          <button
            className="kiri-secondary-button"
            onClick={() =>
              setConfirm({
                title: t("Empty Trash?"),
                message: t("All captures in Trash will be permanently deleted. This cannot be undone."),
                confirmLabel: t("Empty Trash"),
                onConfirm: () => void api.emptyTrash().catch(() => {}),
              })
            }
          >
            {t("Empty Trash")}
          </button>
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
          shortcutLabel={""}
        />
      ) : (
        <div style={gridStyle}>
          {assets.map((asset) => (
            <AssetCard
              key={asset.id}
              asset={asset}
              menuOpen={menuFor === asset.id}
              onMenu={() => openMenu(asset.id)}
              menu={itemMenu(asset)}
              onDoubleClick={() => void api.openAsset(asset.id).catch(() => {})}
              onDragStart={(e) => {
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
          {notice.title}
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
          <span style={{ color: "var(--kiri-label)", fontSize: 12.5 }}>{error.message}</span>
          {error.recovery && (
            <button
              className="kiri-primary-button"
              style={{ minHeight: 30 }}
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
            style={{ border: "none", background: "transparent", color: "var(--kiri-secondary-label)", cursor: "default" }}
            onClick={() => setError(null)}
          >
            ✕
          </button>
        </div>
      )}

      {/* Confirm sheet (custom in-app, per ADR) */}
      {confirm && (
        <div
          style={{
            position: "absolute",
            inset: 0,
            background: "rgba(0,0,0,0.35)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            zIndex: 30,
          }}
        >
          <div
            style={{
              background: "var(--kiri-elevated)",
              borderRadius: 18,
              padding: 20,
              width: 340,
              border: "1px solid var(--kiri-surface-border)",
            }}
          >
            <div style={{ fontSize: 13.5, fontWeight: 600, marginBottom: 6 }}>{confirm.title}</div>
            <div style={{ fontSize: 12.5, color: "var(--kiri-secondary-label)", marginBottom: 16 }}>
              {confirm.message}
            </div>
            <div style={{ display: "flex", justifyContent: "flex-end", gap: 8 }}>
              <button className="kiri-secondary-button" onClick={() => setConfirm(null)}>
                {t("Cancel")}
              </button>
              <button
                className="kiri-primary-button"
                style={{ background: "#FA476E" }}
                onClick={() => {
                  confirm.onConfirm();
                  setConfirm(null);
                }}
              >
                {confirm.confirmLabel}
              </button>
            </div>
          </div>
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
  onMenu(): void;
  menu: React.ReactNode;
  onDoubleClick(): void;
  onDragStart(e: React.DragEvent): void;
}) {
  const { asset, menuOpen, onMenu, menu, onDoubleClick, onDragStart } = props;
  const [hovered, setHovered] = useState(false);
  return (
    <div
      draggable
      onDragStart={onDragStart}
      onDoubleClick={onDoubleClick}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      style={{
        position: "relative",
        background: "var(--kiri-card)",
        border: `1px solid ${hovered ? "#7D69F5" : "var(--kiri-surface-border)"}`,
        borderRadius: 18,
        padding: 12,
        transform: hovered ? "translateY(-1px)" : "none",
        boxShadow: hovered ? "0 8px 18px rgba(0,0,0,0.08)" : "0 3px 8px rgba(0,0,0,0.045)",
        transition: "transform 0.14s ease-out, box-shadow 0.14s ease-out, border-color 0.14s ease-out",
        cursor: "default",
      }}
    >
      <div
        style={{
          height: 184,
          borderRadius: 14,
          overflow: "hidden",
          background: "#141414",
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
            style={{ width: "100%", height: "100%", objectFit: "cover" }}
          />
        ) : asset.kind === "video" ? (
          <div style={{ position: "relative", width: "100%", height: "100%" }}>
            <img
              src={assetUrl(asset.id)}
              alt=""
              draggable={false}
              style={{ width: "100%", height: "100%", objectFit: "cover" }}
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
              style={{ width: "100%", height: "100%", objectFit: "cover" }}
            />
            <div style={{ position: "absolute", left: 8, bottom: 8, background: "rgba(0,0,0,0.6)", color: "#fff", borderRadius: 6, padding: "2px 6px", fontSize: 10, fontWeight: 600 }}>
              GIF
            </div>
          </div>
        )}
      </div>
      <div style={{ display: "flex", alignItems: "center", gap: 8, marginTop: 8, position: "relative" }}>
        <span
          style={{
            flex: 1,
            fontSize: 11,
            color: "var(--kiri-secondary-label)",
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
          }}
        >
          {asset.filename}
        </span>
        <button
          title={asset.isFavorite ? t("Remove Favorite") : t("Favorite")}
          onClick={() => void api.setFavorite(asset.id, !asset.isFavorite).catch(() => {})}
          style={{
            width: 26,
            height: 26,
            borderRadius: 8,
            border: "none",
            background: "transparent",
            color: asset.isFavorite ? "#FFD129" : "var(--kiri-disabled-label)",
            fontSize: 13,
            cursor: "default",
          }}
        >
          ★
        </button>
        <button
          title={t("More Actions")}
          onClick={onMenu}
          style={{
            width: 26,
            height: 26,
            borderRadius: 8,
            border: "none",
            background: "transparent",
            color: "var(--kiri-secondary-label)",
            fontSize: 13,
            cursor: "default",
          }}
        >
          ⋯
        </button>
        {menuOpen && menu}
      </div>
    </div>
  );
}

function MenuRow(props: { label: string; onClick(): void; destructive?: boolean }) {
  return (
    <button
      onClick={props.onClick}
      style={{
        background: "transparent",
        border: "none",
        textAlign: "left",
        padding: "6px 10px",
        borderRadius: 8,
        color: props.destructive ? "#FA476E" : "var(--kiri-label)",
        font: "400 12.5px var(--kiri-font-ui)",
        cursor: "default",
      }}
    >
      {props.label}
    </button>
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
      : fmt("or press  %@", shortcutLabel);
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
          background: "linear-gradient(135deg, #634FDB, #7D69F5, #4FBFF0)",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          color: "#fff",
          fontSize: 24,
          fontWeight: 700,
        }}
      >
        ✦
      </div>
      <div style={{ fontSize: 15, fontWeight: 600 }}>{title}</div>
      <div style={{ fontSize: 12.5, color: "var(--kiri-secondary-label)", maxWidth: 320 }}>
        {message}
      </div>
    </div>
  );
}
