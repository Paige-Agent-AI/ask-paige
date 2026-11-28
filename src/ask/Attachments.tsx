import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { invoke, isTauri } from "../adapter/core";
import { getCurrentWebview } from "../adapter/web";
import { FileTextIcon, FolderIcon } from "../components/Icons";

const MAX_ITEMS = 16;
const MAX_BYTES = 8 * 1024 * 1024;
type Inspection = {
  state: "ready" | "unsupported" | "failed";
  kind: "image" | "text" | "folder" | "file";
  bytes: number;
  sha256?: string;
  thumbnail?: string;
  detail?: string;
};
type Entry = Omit<Inspection, "state"> & { path: string; state: Inspection["state"] | "processing" };
const nameOf = (path: string) => path.split(/[\\/]/).filter(Boolean).pop() ?? path;

export function useAttachments({ chatId, project, generation, enabled }: {
  chatId: string | null;
  project?: string | null;
  generation: number;
  enabled: boolean;
}) {
  const key = JSON.stringify([chatId ?? generation, project ?? null]);
  const [drafts, setDrafts] = useState<Record<string, Entry[]>>({});
  const [notice, setNotice] = useState<{ key: string; text: string }>({ key, text: "" });
  const [dragging, setDragging] = useState(false);
  const [pending, setPending] = useState(false);
  const epoch = useRef(0);
  const working = useRef<number | null>(null);
  const entries = drafts[key] ?? [];
  useLayoutEffect(() => {
    epoch.current++;
    setDragging(false);
    return () => { epoch.current++; };
  }, [key, enabled]);

  const setEntries = (update: (previous: Entry[]) => Entry[]) => {
    setDrafts((previous) => ({ ...previous, [key]: update(previous[key] ?? []) }));
  };
  const show = (text: string) => setNotice({ key, text });
  const stage = async (choose: () => Promise<string[]>) => {
    if (!enabled || working.current !== null) return;
    const started = epoch.current;
    working.current = started;
    setPending(true);
    show("");
    let candidates: string[] = [];
    try {
      const paths = await choose();
      if (epoch.current !== started) return;
      const seen = new Set(entries.filter((entry) => entry.state !== "failed").map((entry) => entry.path.toLowerCase()));
      const fresh = paths.filter((path) => {
        const normalized = path.toLowerCase();
        if (seen.has(normalized)) return false;
        seen.add(normalized);
        return true;
      });
      const replacements = new Set(entries.filter((entry) => entry.state === "failed").map((entry) => entry.path));
      let capacity = MAX_ITEMS - entries.length;
      candidates = fresh.filter((path) => replacements.has(path) || capacity-- > 0);
      if (fresh.length > candidates.length) show(`Up to ${MAX_ITEMS} attachments per draft. Remove one to add more.`);
      if (!candidates.length) return;
      const selected = new Set(candidates);
      setEntries((previous) => [...previous.filter((entry) => !selected.has(entry.path)),
        ...candidates.map((path): Entry => ({ path, state: "processing", kind: "file", bytes: 0 }))]);
      const result = await invoke<Inspection[]>("attachment_inspect", {
        paths: candidates, chatId, pendingProject: project ?? null,
        availableBytes: MAX_BYTES - entries.reduce((sum, entry) => sum + entry.bytes, 0),
      });
      if (epoch.current !== started) return;
      const byPath = new Map(candidates.map((path, index) => [path, result[index]]));
      setEntries((previous) => previous.map((entry) => {
        const verified = byPath.get(entry.path);
        return verified ? { path: entry.path, ...verified } : entry;
      }));
    } catch (error) {
      if (epoch.current === started) {
        const detail = typeof error === "string" ? error : "Unable to check the attachments.";
        if (!candidates.length) show(detail);
        setEntries((previous) => previous.map((entry) => candidates.includes(entry.path) && entry.state === "processing"
          ? { ...entry, state: "failed", detail } : entry));
      }
    } finally {
      if (epoch.current !== started) {
        setEntries((previous) => previous.filter((entry) => entry.state !== "processing"));
      }
      working.current = null;
      setPending(false);
    }
  };
  const latest = useRef({ stage, enabled });
  useLayoutEffect(() => { latest.current = { stage, enabled }; });
  useEffect(() => {
    if (!enabled || !isTauri()) return;
    let cancelled = false;
    const stop = getCurrentWebview().onDragDropEvent(({ payload }: { payload: any }) => {
      if (cancelled || !latest.current.enabled) return;
      if (payload.type === "leave") { setDragging(false); return; }
      const { x, y } = payload.position;
      const inside = !!document.elementFromPoint(x / devicePixelRatio, y / devicePixelRatio)?.closest(".ap-composer");
      setDragging(inside && payload.type !== "drop");
      if (inside && payload.type === "drop") void latest.current.stage(async () => payload.paths);
    });
    void stop.catch(() => {});
    return () => { cancelled = true; void stop.then((unlisten) => unlisten(), () => {}); };
  }, [enabled]);

  return { entries, pending, dragging, notice: notice.key === key ? notice.text : "",
    pick: (folder: boolean) => stage(() => invoke<string[]>("attachment_pick", { folder })),
    retry: (path: string) => stage(async () => [path]),
    remove: (path: string) => setEntries((previous) => previous.filter((entry) => entry.path !== path)),
    clear: () => setEntries(() => []),
  };
}

export function AttachmentTray({ items, hint, pending, onRetry, onRemove }: {
  items: Entry[]; hint: string; pending: boolean;
  onRetry: (path: string) => void; onRemove: (path: string) => void;
}) {
  if (!items.length && !hint) return null;
  return <section className="ap-attachments" aria-label="Draft attachments">
    {items.length > 0 && <p className="ap-attachment-note">{items.length} {items.length === 1 ? "attachment" : "attachments"} · Local preview only</p>}
    {hint && <p className="ap-attachment-note" role="status">{hint}</p>}
    <ul className="ap-attachment-list" aria-label="Staged files">
      {items.map((item) => {
        const name = nameOf(item.path);
        const type = item.kind === "text" ? "Text" : item.kind === "image" ? "PNG" : item.kind === "folder" ? "Folder" : "File";
        const state = item.state === "processing" ? "Checking…" : item.state === "ready" ? "Ready on this device" : item.state === "unsupported" ? "Unsupported" : "Could not read";
        return <li className="ap-attachment" key={item.path} data-state={item.state}>
          {item.thumbnail ? <img src={item.thumbnail} alt="" /> : item.kind === "folder"
            ? <FolderIcon width={20} height={20} aria-hidden /> : <FileTextIcon width={20} height={20} aria-hidden />}
          <div className="ap-attachment-info">
            <strong title={name}>{name}</strong>
            <span aria-live="polite">{type} · {state}</span>
            {item.detail && <small>{item.detail}</small>}
          </div>
          {item.state === "failed" && <button type="button" className="ap-btn" disabled={pending} onClick={() => onRetry(item.path)} aria-label={`Retry ${name}`}>Retry</button>}
          <button type="button" className="ap-btn icon" onClick={() => onRemove(item.path)} aria-label={`Remove ${name}`} title="Remove from draft">×</button>
        </li>;
      })}
    </ul>
  </section>;
}
