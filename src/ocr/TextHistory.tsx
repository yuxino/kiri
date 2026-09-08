import { useEffect, useRef, useState } from "react";
import { api, mediaUrl, onLibraryChanged, type AssetDto, type OcrRecognitionDto } from "../lib/ipc";
import { KiriIcon } from "../components/KiriIcons";
import { t } from "../i18n";
import "./text-history.css";

export function TextReader({ text, imageId, saved = true }: {
  text: string; imageId?: string; saved?: boolean;
}) {
  const [copied, setCopied] = useState(false);
  const [copyError, setCopyError] = useState(false);
  const [imageFailed, setImageFailed] = useState(false);
  const copyGeneration = useRef(0);
  useEffect(() => {
    setCopied(false); setCopyError(false); setImageFailed(false);
    copyGeneration.current += 1;
    return () => { copyGeneration.current += 1; };
  }, [text, imageId]);
  const copy = async () => {
    const generation = ++copyGeneration.current;
    setCopyError(false);
    try {
      await api.copyHistoryText(text);
      if (generation === copyGeneration.current) setCopied(true);
    } catch {
      if (generation === copyGeneration.current) setCopyError(true);
    }
  };
  return <div className="text-reader">
    <div className="text-reader__actions">
      <span className="text-reader__caption">{t("Recognized Text")}</span>
      <button type="button" className="kiri-button kiri-button--primary" disabled={!text.trim()} onClick={() => void copy()}>
        <KiriIcon name={copied ? "checkmark" : "doc.on.doc"} size={14} />
        {t(copied ? "Text Copied" : "Copy Text")}
      </button>
    </div>
    {copyError && <p className="text-history__error" role="alert">{t("Couldn't copy text.")}</p>}
    {!saved && text.trim() && <p className="text-history__error" role="status">{t("History wasn't saved. Copy the text before closing.")}</p>}
    <div className="text-reader__body" tabIndex={0} role="region" aria-label={t("Recognized Text")}>
      {text.trim() ? text : t("No Text Found")}
    </div>
    {imageId && <details className="text-reader__source" key={imageId}>
      <summary><KiriIcon name="photo.on.rectangle" size={14} />{t("View Source Image")}</summary>
      {imageFailed ? <p role="status">{t("Can't read this file")}</p> :
        <a href="#" onClick={(event) => { event.preventDefault(); void api.openAsset(imageId).catch(() => setImageFailed(true)); }} aria-label={t("Open Source Image")}>
          <img src={mediaUrl(imageId)} alt={t("OCR Source Image")} loading="lazy" onError={() => setImageFailed(true)} />
        </a>}
    </details>}
  </div>;
}

export function OcrDialog({ asset, onClose }: { asset: AssetDto; onClose(): void }) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [result, setResult] = useState<OcrRecognitionDto | null>(asset.ocrText != null
    ? { text: asset.ocrText, saved: true, asset } : null);
  const [error, setError] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);
  // A single promise survives React's effect replay. Closing never starts a retry.
  const request = useRef<Promise<OcrRecognitionDto> | null>(null);
  useEffect(() => {
    const element = dialog.current;
    element?.showModal();
    return () => element?.close();
  }, []);
  useEffect(() => {
    if (asset.ocrText != null) return;
    let current = true;
    setError(null);
    request.current ??= api.recognizeAssetLocal(asset.id);
    void request.current.then((value) => { if (current) setResult(value); }, (reason: unknown) => {
      if (current) setError(typeof reason === "string" ? reason : "Local OCR failed.");
    });
    return () => { current = false; };
  }, [asset.id, asset.ocrText, attempt]);
  return <dialog ref={dialog} className="text-dialog" onCancel={(event) => { event.preventDefault(); event.stopPropagation(); onClose(); }}
    onKeyDown={(event) => event.stopPropagation()} aria-labelledby="text-dialog-title">
    <header className="text-dialog__header">
      <div><h2 id="text-dialog-title">{t("Recognized Text")}</h2><p>{t(result?.saved ? "Saved to Text History" : "Local OCR")}</p></div>
      <button type="button" className="kiri-icon-button" aria-label={t("Close")} onClick={onClose}><KiriIcon name="xmark" size={16} /></button>
    </header>
    {result ? <TextReader text={result.text} saved={result.saved} imageId={result.asset?.id ?? asset.id} /> :
      <div className="text-history__empty" role={error ? "alert" : "status"}>
        <KiriIcon name="text.viewfinder" size={28} />
        <p>{t(error ?? "Recognizing Text…")}</p>
        {!error && <small>{t("You can close this window. Successful results are saved to Text History.")}</small>}
        {error && <button type="button" className="kiri-button kiri-button--secondary" onClick={() => { request.current = null; setAttempt((value) => value + 1); }}>{t("Retry")}</button>}
      </div>}
  </dialog>;
}

export function TextHistory() {
  const [query, setQuery] = useState("");
  const [records, setRecords] = useState<AssetDto[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState(false);
  const [mutationError, setMutationError] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [revision, setRevision] = useState(0);
  const search = useRef<HTMLInputElement>(null);
  useEffect(() => {
    const subscription = onLibraryChanged(() => setRevision((value) => value + 1));
    return () => { void subscription.then((dispose) => dispose()).catch(() => {}); };
  }, []);
  useEffect(() => {
    let current = true;
    setLoading(true); setLoadError(false);
    const timer = setTimeout(() => {
      void api.listOcrRecords(query).then((items) => {
        if (!current) return;
        setRecords(items);
        setSelectedId((id) => items.some((item) => item.id === id) ? id : items[0]?.id ?? null);
        setLoading(false);
      }, () => { if (current) { setLoadError(true); setLoading(false); setRecords([]); } });
    }, query ? 150 : 0);
    return () => { current = false; clearTimeout(timer); };
  }, [query, revision]);
  useEffect(() => {
    const keydown = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "f") {
        event.preventDefault(); search.current?.focus();
      }
    };
    window.addEventListener("keydown", keydown);
    return () => window.removeEventListener("keydown", keydown);
  }, []);
  const selected = records.find((item) => item.id === selectedId);
  const remove = async () => {
    if (!selected || deleting) return;
    setDeleting(true); setMutationError(false);
    try { await api.moveToTrash(selected.id); setRevision((value) => value + 1); }
    catch { setMutationError(true); }
    finally { setDeleting(false); }
  };
  return <section className="text-history" aria-label={t("Text History")}>
    <aside className="text-history__sidebar">
      <label className="text-history__search"><KiriIcon name="magnifyingglass" size={15} />
        <input ref={search} type="search" value={query} onChange={(event) => setQuery(event.target.value)}
          placeholder={t("Search recognized text")} aria-label={t("Search recognized text")} />
      </label>
      <div className="text-history__list" aria-label={t("Text History")} aria-busy={loading}>
        {records.map((record) => <button type="button" key={record.id} className="ocr-history__item"
          aria-current={selectedId === record.id ? "true" : undefined}
          onClick={() => { setSelectedId(record.id); setMutationError(false); }}>
          <span className="text-history__excerpt">{record.ocrText}</span>
          <time dateTime={new Date(record.createdAt).toISOString()}>{new Date(record.createdAt).toLocaleString(undefined, { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" })}</time>
        </button>)}
      </div>
      <p className="text-history__footnote">{t("Saved on this device")}</p>
    </aside>
    <main className="text-history__detail">
      {loadError ? <div className="text-history__empty" role="alert"><p>{t("Couldn't load Text History.")}</p><button type="button" className="kiri-button kiri-button--secondary" onClick={() => setRevision((v) => v + 1)}>{t("Retry")}</button></div> :
        selected ? <>
          <header className="text-history__record-header">
            <time>{new Date(selected.createdAt).toLocaleString()}</time>
            <button type="button" className="kiri-icon-button" disabled={deleting || loading} onClick={() => void remove()} title={t("Move to Trash")} aria-label={t("Move to Trash")}><KiriIcon name="trash" size={16} /></button>
          </header>
          {mutationError && <p role="alert" className="text-history__error">{t("Couldn't move this text to Trash.")}</p>}
          <TextReader key={selected.id} text={selected.ocrText ?? ""} imageId={selected.id} />
        </> : <div className="text-history__empty" role="status"><KiriIcon name="text.viewfinder" size={32} />
          <h2>{t(loading ? "Loading…" : query ? "No matching text" : "Your text, ready to revisit")}</h2>
          {!loading && <p>{t(query ? "Try another word or clear the search." : "Recognize text from your screen or a saved screenshot. It will appear here.")}</p>}
        </div>}
    </main>
  </section>;
}
