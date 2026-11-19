import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "../components/Button";
import { Card } from "../components/Card";
import { PaigeCommandMark } from "../components/brand/PaigeCommandMark";
import { useBroker } from "../hooks/useBroker";
import { useAsk, type AskMode, type ComposerState } from "../hooks/useAsk";
import type { FileEntry } from "../types/broker";
import "./AskPaige.css";

const LAB_ROOT = "C:\\Users\\tonig\\paige-desktop-lab";

interface ChatMessage {
  role: "user" | "assistant";
  text: string;
  toolEvents?: { tool: string; ok: boolean; summary: string }[];
  selectionRationale?: string;
  modelLabel?: string;
}

const SLASH_COMMANDS: { cmd: string; label: string; prompt: string }[] = [
  { cmd: "/inspect", label: "Inspect the project", prompt: "Inspect the authorized project: list its files, read the main source files, and summarize the structure and purpose of each file." },
  { cmd: "/test", label: "Run the project tests", prompt: "Run the project's tests and report the exact output and pass/fail result." },
  { cmd: "/build", label: "Build the project", prompt: "Build the project and report the exact build output and result." },
  { cmd: "/review", label: "Review the code", prompt: "Review the project source for correctness issues and improvement opportunities. Cite file names and lines." },
  { cmd: "/explain", label: "Explain the code", prompt: "Explain what the project source does, file by file, in clear language." },
  { cmd: "/research", label: "Research a question", prompt: "Research this question using the project sources and give a sourced answer: " },
];

export function AskPaige() {
  const ask = useAsk();
  const broker = useBroker();
  const [input, setInput] = useState("");
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [mode, setMode] = useState<AskMode>("build");
  const [attachments, setAttachments] = useState<string[]>([]);
  const [contextRefs, setContextRefs] = useState<string[]>([]);
  const [modelChoice, setModelChoice] = useState<"auto" | string>("auto");
  const [workspaceKey, setWorkspaceKey] = useState("");
  const [workspaceConnecting, setWorkspaceConnecting] = useState(false);
  const [workspaceMsg, setWorkspaceMsg] = useState<{ text: string; ok: boolean } | null>(null);
  const [slashOpen, setSlashOpen] = useState(false);
  const [atOpen, setAtOpen] = useState(false);
  const [atQuery, setAtQuery] = useState("");
  const [attachOpen, setAttachOpen] = useState(false);
  const [attachQuery, setAttachQuery] = useState("");
  const [labFiles, setLabFiles] = useState<FileEntry[]>([]);
  const transcriptRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);

  // Load the broker status + file list when a picker opens (needs the lab
  // grant; AskPaige can mount before the grant, so re-check on open).
  useEffect(() => {
    if (!(slashOpen || atOpen || attachOpen)) return;
    let mounted = true;
    (async () => {
      try {
        const files = (await invoke("broker_list_files")) as FileEntry[];
        if (mounted) setLabFiles(files);
      } catch {
        /* not granted — pickers show the grant hint */
      }
    })();
    broker.refreshStatus();
    return () => {
      mounted = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [slashOpen, atOpen, attachOpen]);

  useEffect(() => {
    if (transcriptRef.current) {
      transcriptRef.current.scrollTop = transcriptRef.current.scrollHeight;
    }
  }, [messages, ask.streamingText, ask.toolEvents]);

  const filteredFiles = useMemo(() => {
    const q = atQuery.toLowerCase();
    return labFiles.filter((f) => f.rel_path.toLowerCase().includes(q)).slice(0, 12);
  }, [labFiles, atQuery]);

  const filteredAttach = useMemo(() => {
    const q = attachQuery.toLowerCase();
    return labFiles.filter((f) => f.rel_path.toLowerCase().includes(q)).slice(0, 12);
  }, [labFiles, attachQuery]);

  const brokerGranted = broker.status?.granted ?? false;

  const send = async () => {
    const text = input.trim();
    if (!text || ask.busy) return;
    const composer: ComposerState = {
      mode,
      attachments,
      contextRefs,
      modelChoice,
    };
    setMessages((prev) => [...prev, { role: "user", text }]);
    setInput("");
    setSlashOpen(false);
    setAtOpen(false);
    setAttachOpen(false);
    await ask.send(text, composer, []);
    // The assistant reply arrives via streaming; the done event closes it.
  };

  // Close the streaming turn into a message when it completes.
  useEffect(() => {
    if (!ask.busy && ask.streamingText && messages.length > 0 && messages[messages.length - 1].role === "user") {
      const sel = ask.selection;
      setMessages((prev) => [
        ...prev,
        {
          role: "assistant",
          text: ask.streamingText,
          toolEvents: ask.toolEvents.map((t) => ({ tool: t.tool, ok: t.ok, summary: t.summary })),
          selectionRationale: sel?.rationale,
          modelLabel: sel?.selected ? `${sel.selected.id} (${sel.selected.gateway})` : undefined,
        },
      ]);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ask.busy]);

  const handleInputChange = (value: string) => {
    setInput(value);
    // Slash command trigger: "/" at the start of an empty or fresh input.
    setSlashOpen(value.startsWith("/") && !value.includes(" "));
    // @ reference trigger: trailing "@word"
    const atMatch = value.match(/@([\w./-]*)$/);
    if (atMatch) {
      setAtOpen(true);
      setAtQuery(atMatch[1]);
    } else {
      setAtOpen(false);
    }
  };

  const applySlash = (cmd: { cmd: string; prompt: string }) => {
    const prompt = cmd.cmd === "/research" ? cmd.prompt : cmd.prompt;
    setInput(prompt);
    setSlashOpen(false);
    inputRef.current?.focus();
  };

  const addContextRef = (path: string) => {
    if (!contextRefs.includes(path)) setContextRefs((prev) => [...prev, path]);
    setInput((prev) => prev.replace(/@([\w./-]*)$/, ""));
    setAtOpen(false);
    inputRef.current?.focus();
  };

  const addAttachment = (path: string) => {
    if (!attachments.includes(path)) setAttachments((prev) => [...prev, path]);
    setAttachOpen(false);
  };

  const activeModel =
    modelChoice === "auto"
      ? "Auto"
      : (ask.status?.catalog.find((c) => c.id === modelChoice)?.id ?? modelChoice);

  const gatewaySummary = ask.status
    ? ask.status.gateways.map((g) => `${g.label}: ${g.reachable ? (g.authenticated ? "ready" : "auth needed") : "offline"}`).join(" · ")
    : "probing…";

  const omnirouteHealth = ask.status?.gateways.find((g) => g.gateway === "omniroute");
  const openrouterHealth = ask.status?.gateways.find((g) => g.gateway === "openrouter-direct");

  const productionReady = !!(
    omnirouteHealth?.authenticated || openrouterHealth?.authenticated
  );

  const connectFromWorkspace = async () => {
    if (!workspaceKey.trim()) return;
    setWorkspaceConnecting(true);
    setWorkspaceMsg(null);
    const ok = await ask.connectOpenrouter(workspaceKey.trim());
    setWorkspaceConnecting(false);
    if (ok) {
      setWorkspaceMsg({ text: "Connected to OpenRouter. PAIGE can now use live models.", ok: true });
      setWorkspaceKey("");
    } else {
      setWorkspaceMsg({ text: ask.error || "Connection failed. Check the key and try again.", ok: false });
    }
  };

  return (
    <div className="ask">
      {!productionReady && (
        <div className="ask-connect-banner">
          <div className="ask-connect-banner-text">
            <strong>Connect an inference gateway to use Ask PAIGE.</strong>
            <span>
              OmniRoute is currently {omnirouteHealth?.authenticated ? "ready" : "not ready"}. You can connect your
              OpenRouter API key directly in the workspace to start using live models.
            </span>
          </div>
          <div className="ask-connect-banner-row">
            <input
              type="password"
              className="ask-connect-keyinput"
              placeholder="sk-or-v1-…"
              value={workspaceKey}
              onChange={(e) => setWorkspaceKey(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && !workspaceConnecting) connectFromWorkspace();
              }}
            />
            <Button
              variant="primary"
              size="sm"
              onClick={connectFromWorkspace}
              disabled={workspaceConnecting || !workspaceKey.trim()}
            >
              {workspaceConnecting ? "Verifying…" : "Connect OpenRouter"}
            </Button>
          </div>
          {workspaceMsg && (
            <div className={`ask-connect-banner-msg ${workspaceMsg.ok ? "ok" : "err"}`}>
              {workspaceMsg.text}
            </div>
          )}
          <div className="ask-connect-banner-note">
            Verified live against OpenRouter’s authenticated endpoint; stored DPAPI-encrypted on this machine only.
          </div>
        </div>
      )}

      <div className="ask-transcript" ref={transcriptRef}>
        {messages.length === 0 && !ask.streamingText && (
          <div className="ask-empty">
            <PaigeCommandMark className="ask-empty-mark" label="PAIGE" />
            <h2>Ask PAIGE</h2>
            <p>
              Your engineering workstation. PAIGE reads and edits files inside the
              authorized project, runs the real test suite, and reports verified
              results with receipts.
            </p>
            <div className="ask-empty-hints">
              {SLASH_COMMANDS.slice(0, 4).map((c) => (
                <button key={c.cmd} className="ask-hint-chip" onClick={() => handleInputChange(c.prompt)}>
                  <code>{c.cmd}</code> {c.label}
                </button>
              ))}
            </div>
          </div>
        )}
        {messages.map((m, i) => (
          <div key={i} className={`ask-msg ask-msg-${m.role}`}>
            <div className="ask-msg-role">{m.role === "user" ? "You" : "PAIGE"}</div>
            <div className="ask-msg-text">{m.text}</div>
            {m.role === "assistant" && m.modelLabel && (
              <div className="ask-msg-model">{m.modelLabel}</div>
            )}
            {m.role === "assistant" && m.selectionRationale && (
              <div className="ask-msg-rationale">{m.selectionRationale}</div>
            )}
            {m.toolEvents && m.toolEvents.length > 0 && (
              <div className="ask-msg-tools">
                {m.toolEvents.map((t, j) => (
                  <div key={j} className={`ask-tool ${t.ok ? "ok" : "err"}`}>
                    <span className="ask-tool-name">{t.tool}</span>
                    <span className="ask-tool-summary">{t.summary}</span>
                  </div>
                ))}
              </div>
            )}
          </div>
        ))}
        {ask.streamingText && (
          <div className="ask-msg ask-msg-assistant">
            <div className="ask-msg-role">PAIGE</div>
            <div className="ask-msg-text">
              {ask.streamingText}
              <span className="ask-caret" />
            </div>
            {ask.toolEvents.length > 0 && (
              <div className="ask-msg-tools">
                {ask.toolEvents.map((t, j) => (
                  <div key={j} className={`ask-tool ${t.ok ? "ok" : "err"}`}>
                    <span className="ask-tool-name">{t.tool}</span>
                    <span className="ask-tool-summary">{t.summary}</span>
                  </div>
                ))}
              </div>
            )}
          </div>
        )}
        {ask.busy && !ask.streamingText && (
          <div className="ask-msg ask-msg-assistant">
            <div className="ask-msg-role">PAIGE</div>
            <div className="ask-msg-text ask-thinking">
              {ask.selection?.selected
                ? `Selecting route via ${ask.selection.selected.id}…`
                : "Selecting a model…"}
            </div>
          </div>
        )}
        {ask.error && <div className="ask-error-banner">{ask.error}</div>}
      </div>

      <div className="ask-composer">
        {/* Row 1: chips */}
        <div className="ask-chips">
          <span
            className={`ask-chip ask-chip-project ${brokerGranted ? "granted" : "locked"}`}
            title={brokerGranted ? `Authorized root: ${LAB_ROOT}` : "Grant access in the Files screen"}
            onClick={() => broker.grant()}
          >
            {brokerGranted ? "◉ lab project · authorized" : "◌ lab project · click to grant"}
          </span>
          {attachments.map((a) => (
            <span key={a} className="ask-chip ask-chip-attach" title={`Attached: ${a}`}>
              📎 {a}
              <button className="ask-chip-x" onClick={() => setAttachments((p) => p.filter((x) => x !== a))}>×</button>
            </span>
          ))}
          {contextRefs.map((c) => (
            <span key={c} className="ask-chip ask-chip-ctx" title={`Context: ${c}`}>
              @ {c}
              <button className="ask-chip-x" onClick={() => setContextRefs((p) => p.filter((x) => x !== c))}>×</button>
            </span>
          ))}
        </div>

        {/* Slash palette */}
        {slashOpen && (
          <Card className="ask-palette">
            {SLASH_COMMANDS.map((c) => (
              <button key={c.cmd} className="ask-palette-item" onClick={() => applySlash(c)}>
                <code>{c.cmd}</code>
                <span>{c.label}</span>
              </button>
            ))}
          </Card>
        )}

        {/* @ reference popover */}
        {atOpen && (
          <Card className="ask-palette">
            <div className="ask-palette-title">Reference a file (@)</div>
            {!brokerGranted && (
              <div className="ask-palette-hint">Grant lab access to reference files (click the project chip above).</div>
            )}
            {brokerGranted &&
              filteredFiles.map((f) => (
                <button key={f.rel_path} className="ask-palette-item" onClick={() => addContextRef(f.rel_path)}>
                  <code>@ {f.rel_path}</code>
                </button>
              ))}
            {brokerGranted && filteredFiles.length === 0 && (
              <div className="ask-palette-hint">No files match “{atQuery}”.</div>
            )}
          </Card>
        )}

        {/* Attach popover */}
        {attachOpen && (
          <Card className="ask-palette">
            <div className="ask-palette-title">Attach a file</div>
            <input
              className="ask-palette-search"
              placeholder="Filter files…"
              value={attachQuery}
              onChange={(e) => setAttachQuery(e.target.value)}
              autoFocus
            />
            {!brokerGranted && (
              <div className="ask-palette-hint">Grant lab access to attach files (click the project chip above).</div>
            )}
            {brokerGranted &&
              filteredAttach.map((f) => (
                <button key={f.rel_path} className="ask-palette-item" onClick={() => addAttachment(f.rel_path)}>
                  <code>📎 {f.rel_path}</code>
                  <span>{Math.ceil(f.size / 1024)} KB</span>
                </button>
              ))}
          </Card>
        )}

        {/* Row 2: input */}
        <div className="ask-inputrow">
          <textarea
            ref={inputRef}
            className="ask-input"
            placeholder={ask.busy ? "PAIGE is working… (Cancel to interrupt)" : "Give PAIGE an engineering instruction… use / for commands, @ for files"}
            value={input}
            onChange={(e) => handleInputChange(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                send();
              }
            }}
            disabled={ask.busy}
            rows={2}
          />
        </div>

        {/* Row 3: controls */}
        <div className="ask-controls">
          <div className="ask-segmented" role="tablist" aria-label="Operating mode">
            <button
              className={`ask-segment ${mode === "plan" ? "active" : ""}`}
              onClick={() => setMode("plan")}
              title="Research/inspect only — no file writes, read-only commands"
            >
              PLAN
            </button>
            <button
              className={`ask-segment ${mode === "build" ? "active" : ""}`}
              onClick={() => setMode("build")}
              title="Implement — file edits via broker approval, tests and builds"
            >
              BUILD
            </button>
          </div>

          <Button
            variant="secondary"
            size="sm"
            onClick={() => setAttachOpen((v) => !v)}
            disabled={ask.busy}
          >
            📎 Attach
          </Button>

          <label className="ask-model">
            <span className="ask-model-label">Model</span>
            <select
              value={modelChoice}
              onChange={(e) => {
                const v = e.target.value;
                setModelChoice(v);
                ask.setModelSelection(v === "auto" ? "auto" : "manual", v === "auto" ? null : v);
              }}
              disabled={ask.busy}
            >
              <option value="auto">Auto (recommended)</option>
              {(ask.status?.catalog ?? []).map((c) => (
                <option key={c.id} value={c.id}>
                  {c.id} · {c.gateway}
                </option>
              ))}
            </select>
          </label>

          <div className="ask-status-chip" title={gatewaySummary}>
            <span
              className={`ask-dot ${omnirouteHealth?.reachable && omnirouteHealth?.authenticated ? "on" : omnirouteHealth?.reachable ? "warn" : "off"}`}
            />
            OmniRoute
            <span
              className={`ask-dot ${openrouterHealth?.authenticated ? "on" : "off"}`}
              style={{ marginLeft: 8 }}
            />
            OpenRouter
          </div>

          <div className="ask-send">
            {ask.busy ? (
              <Button variant="secondary" size="sm" onClick={ask.cancel}>
                Cancel
              </Button>
            ) : (
              <Button variant="primary" size="sm" onClick={send} disabled={!input.trim()}>
                Send
              </Button>
            )}
          </div>
        </div>
        <div className="ask-selected-line">
          {ask.selection?.selected
            ? `Route: ${ask.selection.selected.id} · ${ask.selection.selected.gateway}`
            : `Mode: ${mode.toUpperCase()} · Model: ${activeModel}`}
        </div>
      </div>
    </div>
  );
}
