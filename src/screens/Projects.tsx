import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "../components/Button";
import { Card } from "../components/Card";
import { Feedback } from "../components/Feedback";
import { FolderIcon } from "../components/Icons";
import { useBroker } from "../hooks/useBroker";
import type { BrokerStatus } from "../types/broker";
import { folderName, LAB_ROOT, rememberFolder, savedFolders } from "../projectFolders";
import "./Projects.css";

export function Projects() {
  const broker = useBroker();
  const [folders, setFolders] = useState(savedFolders);
  const [adding, setAdding] = useState(false);
  const [path, setPath] = useState("");
  const [opening, setOpening] = useState(false);
  const [browsing, setBrowsing] = useState(false);
  const busy = opening || browsing;
  const [error, setError] = useState<{ message: string } | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const browse = async () => {
    if (busy) return;
    setBrowsing(true);
    setError(null);
    try {
      const selected = await invoke<string | null>("project_pick_folder");
      if (selected !== null) setPath(selected);
    } catch (cause) {
      setError({ message: String(cause) });
    } finally {
      setBrowsing(false);
    }
  };

  const open = async (folder: string, saveInList: boolean, rememberAccess = false) => {
    if (busy) return;
    setOpening(true);
    setError(null);
    setNotice(null);
    try {
      const status = await invoke<BrokerStatus>("project_open_folder", { root: folder, rememberAccess });
      if (!status.granted || !status.root) throw Error("PAIGE couldn't authorize that folder.");
      const root = status.root;
      if (saveInList) {
        setFolders(rememberFolder(root));
        setPath("");
        setAdding(false);
      }
      await broker.refreshStatus();
      setNotice(`${folderName(root)} is ready to select in a chat.`);
    } catch (cause) {
      setError({ message: String(cause) });
    } finally {
      setOpening(false);
    }
  };

  return (
    <div className="projects">
      <div className="projects-toolbar">
        <Button variant="primary" disabled={busy} aria-expanded={adding} aria-controls="add-project"
          onClick={() => { setAdding((value) => !value); setError(null); setNotice(null); }}>
          Add local project
        </Button>
      </div>
      {adding && <Card className="project-form">
        <form id="add-project" onSubmit={(event) => {
          event.preventDefault();
          const folder = path.trim().replace(/^"(.*)"$/, "$1");
          if (folder) void open(folder, true);
        }}>
          <label htmlFor="project-folder">Project folder</label>
          <div className="project-folder-field">
            <input id="project-folder" spellCheck={false} value={path} onChange={(event) => setPath(event.target.value)}
              placeholder="Choose or enter a folder path" disabled={busy} />
            <Button type="button" autoFocus disabled={busy} onClick={() => void browse()}>Browse…</Button>
          </div>
          <div className="project-form-actions">
            <Button type="submit" disabled={!path.trim() || busy}>{opening ? "Waiting for approval…" : "Add project"}</Button>
            <Button type="button" variant="ghost" disabled={busy} onClick={() => { setAdding(false); setPath(""); }}>Cancel</Button>
          </div>
        </form>
      </Card>}
      <div role="status"><Feedback error={error ?? broker.lastError} notice={notice ?? broker.lastNotice} /></div>

      <div className="projects-grid">
        {[...new Set([LAB_ROOT, ...folders, ...(broker.status?.remembered_roots ?? [])])].map((folder) => {
          const active = broker.status?.root?.toLowerCase() === folder.toLowerCase() && broker.status?.read;
          const remembered = broker.status?.remembered_roots.includes(folder) ?? false;
          return (
            <Card key={folder} className="project-card">
              <div className="project-card-header">
                <FolderIcon aria-hidden />
                <div>
                  <h3 className="project-card-name">{folder === LAB_ROOT ? "Ask PAIGE Lab" : folderName(folder)}</h3>
                  <p className="project-card-path">{folder}</p>
                </div>
              </div>
              <div className="project-card-footer">
                <span className={`project-card-status project-card-status-${active ? "active" : "pending"}`}>
                  {remembered ? active ? "Active · Remembered access" : "Remembered access" : active ? "Active workspace" : "Saved"}
                </span>
                <div className="project-form-actions">
                  {!active && (
                    <Button variant="secondary" size="sm" disabled={broker.busy || busy} onClick={() => void open(folder, false)}>
                      Use project
                    </Button>
                  )}
                  {active && !remembered && (
                    <Button variant="secondary" size="sm" disabled={broker.busy || busy} onClick={() => void open(folder, false, true)}>
                      Remember access
                    </Button>
                  )}
                  {(active || remembered) && (
                    <Button variant="secondary" size="sm" disabled={broker.busy || busy}
                      onClick={() => { setNotice(null); setError(null); void broker.revoke(active ? broker.status?.root ?? folder : folder); }}>
                      Revoke access
                    </Button>
                  )}
                </div>
              </div>
            </Card>
          );
        })}
      </div>
    </div>
  );
}
