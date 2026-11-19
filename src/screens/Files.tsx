import { useEffect, useState } from "react";
import { Button } from "../components/Button";
import { Card } from "../components/Card";
import { Feedback } from "../components/Feedback";
import { FileTextIcon, FolderIcon, LockIcon } from "../components/Icons";
import { useBroker } from "../hooks/useBroker";
import "./Files.css";

export function Files() {
  const broker = useBroker();
  const [draft, setDraft] = useState("");
  const [hasProposal, setHasProposal] = useState(false);
  const [token, setToken] = useState("");
  const [approved, setApproved] = useState(false);

  useEffect(() => {
    if (broker.status?.granted && broker.files.length === 0) {
      broker.listFiles();
    }
  }, [broker.status?.granted]);

  useEffect(() => {
    if (broker.selectedFile) {
      setDraft(broker.selectedFile.content);
      setHasProposal(false);
      setApproved(false);
      setToken("");
    }
  }, [broker.selectedFile?.path]);

  const canWrite = broker.status?.write === true;
  const selected = broker.selectedFile;

  async function handlePropose() {
    if (!selected || !canWrite) return;
    const p = await broker.proposeWrite(selected.path.split("\\").pop() ?? "", draft, selected.sha256);
    if (p) {
      setHasProposal(true);
      setApproved(false);
      setToken("");
    }
  }

  async function handleApprove() {
    const proposal = broker.proposals[0];
    if (!proposal) return;
    const ok = await broker.approve(proposal.proposal_id, token);
    if (ok) setApproved(true);
  }

  async function handleApply() {
    const proposal = broker.proposals[0];
    if (!proposal || !approved) return;
    const r = await broker.applyWrite(proposal.proposal_id);
    if (r) {
      setHasProposal(false);
      setApproved(false);
      setToken("");
      broker.listFiles();
    }
  }

  async function handleDeny() {
    const proposal = broker.proposals[0];
    if (!proposal) return;
    await broker.deny(proposal.proposal_id);
    setHasProposal(false);
    setApproved(false);
    setToken("");
  }

  const proposal = broker.proposals[0];

  return (
    <div className="files">
      <div className="files-toolbar">
        <div className="files-status">
          {broker.status?.granted ? (
            <span className="files-status-granted">
              Authorized: {broker.status.root}
            </span>
          ) : (
            <span className="files-status-denied">
              <LockIcon /> No file access granted
            </span>
          )}
        </div>
        <div className="files-actions">
          <Button variant="secondary" onClick={broker.grant} disabled={broker.busy}>
            Grant Access
          </Button>
          <Button variant="ghost" onClick={broker.revoke} disabled={broker.busy || !broker.status?.granted}>
            Revoke
          </Button>
          <Button variant="secondary" onClick={broker.listFiles} disabled={broker.busy || !broker.status?.granted}>
            Refresh
          </Button>
        </div>
      </div>

      <Feedback error={broker.lastError} notice={broker.lastNotice} />

      <div className="files-layout">
        <Card className="files-tree">
          <h3 className="files-panel-title">
            <FolderIcon /> Files
          </h3>
          {broker.files.length === 0 ? (
            <p className="files-empty">Grant access and refresh to list files.</p>
          ) : (
            <ul className="files-list">
              {broker.files.map((f) => (
                <li key={f.rel_path}>
                  <button
                    className={`files-list-item ${selected?.path.endsWith(f.rel_path) ? "active" : ""}`}
                    onClick={() => broker.readFile(f.rel_path)}
                    disabled={broker.busy}
                  >
                    <FileTextIcon />
                    <span className="files-list-name">{f.rel_path}</span>
                    <span className="files-list-size">{f.size} B</span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </Card>

        <Card className="files-editor">
          {!selected ? (
            <div className="files-empty-state">
              <FileTextIcon />
              <p>Select a file from the tree to preview or edit.</p>
            </div>
          ) : (
            <>
              <div className="files-editor-header">
                <div>
                  <h3 className="files-editor-title">{selected.path}</h3>
                  <p className="files-editor-meta">
                    {selected.size} bytes · sha256 {selected.sha256.slice(0, 16)}…
                  </p>
                </div>
                <Button
                  variant="primary"
                  size="sm"
                  onClick={handlePropose}
                  disabled={broker.busy || !canWrite || draft === selected.content}
                >
                  Propose Change
                </Button>
              </div>

              <textarea
                className="files-editor-textarea"
                value={draft}
                onChange={(e) => setDraft(e.target.value)}
                disabled={broker.busy || !canWrite}
                spellCheck={false}
              />

              {hasProposal && proposal && (
                <div className="files-proposal">
                  <p className="files-proposal-label">Pending proposal</p>
                  <p className="files-proposal-id">{proposal.proposal_id}</p>
                  <p className="files-proposal-code">
                    Approval code: <code>{proposal.approval_token}</code>
                  </p>
                  <input
                    className="files-proposal-input"
                    placeholder="Enter approval code"
                    value={token}
                    onChange={(e) => setToken(e.target.value)}
                  />
                  <div className="files-proposal-actions">
                    <Button variant="secondary" size="sm" onClick={handleApprove} disabled={broker.busy || !token}>
                      Approve
                    </Button>
                    <Button variant="primary" size="sm" onClick={handleApply} disabled={broker.busy || !approved}>
                      Apply
                    </Button>
                    <Button variant="ghost" size="sm" onClick={handleDeny} disabled={broker.busy}>
                      Deny
                    </Button>
                  </div>
                </div>
              )}
            </>
          )}
        </Card>
      </div>
    </div>
  );
}
