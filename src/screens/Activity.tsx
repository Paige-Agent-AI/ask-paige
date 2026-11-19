import { useEffect } from "react";
import { Button } from "../components/Button";
import { Card } from "../components/Card";
import { Feedback } from "../components/Feedback";
import { ActivityIcon } from "../components/Icons";
import { useBroker } from "../hooks/useBroker";
import "./Activity.css";

export function Activity() {
  const broker = useBroker();

  useEffect(() => {
    broker.loadOperations();
  }, []);

  return (
    <div className="activity">
      <div className="activity-toolbar">
        <h2 className="activity-title">
          <ActivityIcon /> Activity & Receipts
        </h2>
        <Button variant="secondary" size="sm" onClick={broker.loadOperations} disabled={broker.busy}>
          Refresh
        </Button>
      </div>

      <Feedback error={broker.lastError} notice={broker.lastNotice} />

      {broker.operations.length === 0 ? (
        <Card className="activity-empty">
          <p>No recorded activity yet.</p>
          <p className="activity-empty-hint">
            File operations from the Files and Approvals screens will appear here as
            durable receipts once activity persistence is implemented.
          </p>
        </Card>
      ) : (
        <div className="activity-list">
          {broker.operations.map((op, idx) => (
            <Card key={idx} className="activity-item">
              <div className="activity-item-header">
                <span className={`activity-item-outcome activity-item-outcome-${op.outcome === "committed_verified" ? "success" : "neutral"}`}>
                  {op.outcome}
                </span>
                <span className="activity-item-time">
                  {new Date(op.timestamp * 1000).toLocaleString()}
                </span>
              </div>
              <p className="activity-item-path">{op.path}</p>
              <p className="activity-item-session">Session {op.session_id}</p>
              <div className="activity-item-hashes">
                <span>before {op.before_sha256.slice(0, 16)}…</span>
                <span>after {op.after_sha256.slice(0, 16)}…</span>
              </div>
            </Card>
          ))}
        </div>
      )}
    </div>
  );
}
