import { useState } from "react";
import { Button } from "../components/Button";
import { Card } from "../components/Card";
import { LockIcon, TerminalIcon } from "../components/Icons";
import "./Terminal.css";

export function Terminal() {
  const [input, setInput] = useState("");

  return (
    <div className="terminal">
      <div className="terminal-toolbar">
        <h2 className="terminal-title">
          <TerminalIcon /> Terminal
        </h2>
        <Button variant="secondary" size="sm" disabled>
          New tab
        </Button>
      </div>

      <Card className="terminal-body">
        <div className="terminal-output">
          <div className="terminal-line terminal-line-welcome">
            PAIGE Desktop Terminal — prepared interface, not yet activated.
          </div>
          <div className="terminal-line">
            Scoped terminal execution with allowlisted commands, process isolation,
            output capture and kill switch will be implemented in Increment 1C.
          </div>
        </div>
        <div className="terminal-input-bar">
          <span className="terminal-prompt">$</span>
          <input
            type="text"
            className="terminal-input"
            placeholder="Commands are disabled until Increment 1C is authorized..."
            value={input}
            onChange={(e) => setInput(e.target.value)}
            disabled
          />
        </div>
      </Card>

      <Card className="terminal-locked">
        <div className="terminal-locked-icon">
          <LockIcon />
        </div>
        <h3>Terminal execution is disabled</h3>
        <p>
          Running commands on this device requires separate owner authorization,
          an approved command allowlist and native per-command approval. This
          screen shows where the terminal will live in the workstation.
        </p>
      </Card>
    </div>
  );
}
