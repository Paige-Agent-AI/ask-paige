import { useEffect, useState } from "react";
import { Button } from "../components/Button";
import { Card } from "../components/Card";
import { Feedback } from "../components/Feedback";
import { LockIcon, ShieldCheckIcon } from "../components/Icons";
import { useBroker } from "../hooks/useBroker";
import "./Approvals.css";

export function Approvals() {
  const broker = useBroker();
  const [token, setToken] = useState("");
  const [approved, setApproved] = useState<Record<string, boolean>>({});

  useEffect(() => {
    broker.loadOperations();
  }, []);

  async function handleApprove(proposalId: string) {
    const ok = await broker.approve(proposalId, token);
    if (ok) {
      setApproved((prev) => ({ ...prev, [proposalId]: true }));
      setToken("");
    }
  }

  async function handleApply(proposalId: string) {
    await broker.applyWrite(proposalId);
    await broker.loadOperations();
  }

  async function handleDeny(proposalId: string) {
    await broker.deny(proposalId);
  }

  const proposal = broker.proposals[0];

  return (
    <div className="approvals">
      <div className="approvals-toolbar">
        <div className="approvals-status">
          {broker.status?.granted ? (
            <span className="approvals-status-granted">
              <ShieldCheckIcon /> Authorization active
            </span>
          ) : (
            <span className="approvals-status-denied">
              <LockIcon /> No active authorization
            </span>
          )}
        </div>
        <Button variant="secondary" onClick={broker.grant} disabled={broker.busy}>
          Grant Access
        </Button>
      </div>

      <Feedback error={broker.lastError} notice={broker.lastNotice} />

      <h2 className="approvals-section-title">Pending Approval</h2>
      {proposal ? (
        <Card className="approval-card approval-card-pending">
          <div className="approval-card-header">
            <span className="approval-card-id">{proposal.proposal_id}</span>
            <span className="approval-card-badge approval-card-badge-pending">
              Pending
            </span>
          </div>
          <p className="approval-card-path">{proposal.path}</p>
          <p className="approval-card-meta">
            Before sha256: {proposal.before_sha256.slice(0, 16)}… · New size:{" "}
            {proposal.new_size} bytes
          </p>
          <div className="approval-card-code">
            Approval code: <code>{proposal.approval_token}</code>
          </div>
          <input
            className="approval-card-input"
            placeholder="Enter approval code"
            value={token}
            onChange={(e) => setToken(e.target.value)}
          />
          <div className="approval-card-actions">
            <Button
              variant="secondary"
              size="sm"
              onClick={() => handleApprove(proposal.proposal_id)}
              disabled={broker.busy || !token}
            >
              Approve
            </Button>
            <Button
              variant="primary"
              size="sm"
              onClick={() => handleApply(proposal.proposal_id)}
              disabled={broker.busy || !approved[proposal.proposal_id]}
            >
              Apply
            </Button>
            <Button variant="ghost" size="sm" onClick={() => handleDeny(proposal.proposal_id)} disabled={broker.busy}>
              Deny
            </Button>
          </div>
        </Card>
      ) : (
        <Card className="approval-card approval-card-empty">
          <p>No pending proposals. Create one from the Files screen.</p>
        </Card>
      )}

      <h2 className="approvals-section-title">Activity Log</h2>
      {broker.operations.length === 0 ? (
        <Card className="approval-card approval-card-empty">
          <p>No operations recorded yet.</p>
        </Card>
      ) : (
        <div className="approvals-list">
          {broker.operations.map((op) => (
            <Card key={op.timestamp + op.proposal_id} className="approval-card approval-card-history">
              <div className="approval-card-header">
                <span className="approval-card-id">{op.proposal_id}</span>
                <span className={`approval-card-badge approval-card-badge-${op.outcome === "committed_verified" ? "success" : "neutral"}`}>
                  {op.outcome}
                </span>
              </div>
              <p className="approval-card-path">{op.path}</p>
              <p className="approval-card-meta">
                Session {op.session_id} · {new Date(op.timestamp * 1000).toLocaleString()}
              </p>
              <p className="approval-card-meta">
                before {op.before_sha256.slice(0, 12)}… → after {op.after_sha256.slice(0, 12)}…
              </p>
            </Card>
          ))}
        </div>
      )}
    </div>
  );
}
