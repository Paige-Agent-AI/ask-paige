import { useEffect, useRef, useState } from "react";
import { Button } from "../components/Button";
import { Card } from "../components/Card";
import { TerminalIcon } from "../components/Icons";
import { useTerminal } from "../hooks/useTerminal";
import "./Terminal.css";

function parseCommandLine(line: string): { command: string; args: string[] } {
  const trimmed = line.trim();
  if (!trimmed) return { command: "", args: [] };
  // Simple whitespace split. Quotes are not handled for Increment 1C.
  const parts = trimmed.split(/\s+/);
  return { command: parts[0], args: parts.slice(1) };
}

export function Terminal() {
  const { lines, session, busy, pendingApproval, execute, approve, kill } = useTerminal();
  const [input, setInput] = useState("");
  const [history, setHistory] = useState<string[]>([]);
  const [historyIndex, setHistoryIndex] = useState(-1);
  const outputRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (outputRef.current) {
      outputRef.current.scrollTop = outputRef.current.scrollHeight;
    }
  }, [lines]);

  const runCommand = async (raw: string) => {
    if (!raw.trim()) return;
    setHistory((prev) => [raw, ...prev].slice(0, 50));
    setHistoryIndex(-1);
    setInput("");
    const { command, args } = parseCommandLine(raw);
    await execute(command, args);
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") {
      e.preventDefault();
      if (pendingApproval) {
        approve(pendingApproval.token);
      } else {
        runCommand(input);
      }
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      if (history.length === 0) return;
      const next = Math.min(historyIndex + 1, history.length - 1);
      setHistoryIndex(next);
      setInput(history[next]);
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      if (historyIndex <= 0) {
        setHistoryIndex(-1);
        setInput("");
      } else {
        const next = historyIndex - 1;
        setHistoryIndex(next);
        setInput(history[next]);
      }
    } else if (e.key === "c" && e.ctrlKey && busy && session) {
      e.preventDefault();
      kill();
    }
  };

  return (
    <div className="terminal">
      <div className="terminal-toolbar">
        <h2 className="terminal-title">
          <TerminalIcon /> Terminal
        </h2>
        <div className="terminal-actions">
          {busy && (
            <Button variant="danger" size="sm" onClick={kill}>
              Kill
            </Button>
          )}
        </div>
      </div>

      <Card className="terminal-body">
        <div className="terminal-output" ref={outputRef}>
          {lines.length === 0 && (
            <div className="terminal-line terminal-line-welcome">
              PAIGE Desktop Terminal — scoped execution. Allowed dev commands run inside
              authorized project roots. Destructive commands require explicit approval.
            </div>
          )}
          {lines.map((line, i) => (
            <div key={i} className={`terminal-line terminal-line-${line.stream}`}>
              {line.text}
            </div>
          ))}
        </div>
        <div className="terminal-input-bar">
          <span className="terminal-prompt">
            {pendingApproval ? "[confirm]" : busy ? "~" : "$"}
          </span>
          <input
            ref={inputRef}
            type="text"
            className="terminal-input"
            placeholder={
              pendingApproval
                ? "Type 'yes' and press Enter to approve, or clear to cancel..."
                : busy
                ? "Process running..."
                : "Enter an allowed command (e.g. cargo --version)..."
            }
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={handleKeyDown}
            disabled={busy && !pendingApproval}
            autoFocus
          />
          {pendingApproval && (
            <Button
              variant="primary"
              size="sm"
              onClick={() => approve(pendingApproval.token)}
            >
              Approve
            </Button>
          )}
        </div>
      </Card>

      <Card className="terminal-info">
        <div className="terminal-info-row">
          <span>Status</span>
          <span className="terminal-info-value">
            {busy
              ? session
                ? `running ${session.command_line}`
                : "starting..."
              : "ready"}
          </span>
        </div>
        {session && (
          <>
            <div className="terminal-info-row">
              <span>Session</span>
              <span className="terminal-info-value">{session.session_id}</span>
            </div>
            <div className="terminal-info-row">
              <span>PID</span>
              <span className="terminal-info-value">{session.pid}</span>
            </div>
            <div className="terminal-info-row">
              <span>CWD</span>
              <span className="terminal-info-value">{session.cwd}</span>
            </div>
          </>
        )}
      </Card>
    </div>
  );
}
