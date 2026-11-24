import { useEffect, useRef, useState, type ReactNode, type RefObject } from "react";
import { ArrowUpIcon } from "../components/Icons";
import "./composer.css";

/**
 * The Stage 2A unified composer — one consistent surface for new and existing
 * conversations. Environment and project context sit above the input with the
 * single access truth; compact controls live below it; advanced preferences
 * stay in contextual menus.
 *
 * Honest capability mapping (owner rule: every label reflects real behavior):
 * - Working style = the existing plan/build execution modes, renamed for
 *   humans. No "automatic" option is presented because none is implemented.
 * - Conversation type, dictation and bulk input are Stage 2C/2D/2B contracts;
 *   the architecture leaves room for their controls without rendering them.
 *
 * Security invariants preserved verbatim: environment selection, project
 * selection and working style never expand authority; attachments remain
 * local references until the owner sends.
 */
export function Composer({
  topNode,
  paletteNode,
  chipsNode,
  menuNode,
  modelNode,
  menuTriggerNode,
  healthNode,
  routeLine,
  input,
  inputRef,
  inputPlaceholder,
  inputAriaDescribedBy,
  inputDisabled,
  onInputChange,
  onSend,
  busy,
  onCancel,
  cancelLabel,
  sendDisabled,
  mode,
  onSetMode,
  modeLocked,
  workspaceLabel,
  onWorkspaceOpen,
  projectPickerOpen,
  accessLabel,
  accessFull,
  accessTitle,
}: {
  topNode?: ReactNode;
  paletteNode?: ReactNode;
  chipsNode?: ReactNode;
  menuNode?: ReactNode;
  modelNode?: ReactNode;
  menuTriggerNode?: ReactNode;
  healthNode?: ReactNode;
  routeLine?: ReactNode;
  input: string;
  inputRef: RefObject<HTMLTextAreaElement | null>;
  inputPlaceholder: string;
  inputAriaDescribedBy?: string;
  inputDisabled: boolean;
  onInputChange: (value: string) => void;
  onSend: () => void;
  busy: boolean;
  onCancel: () => void;
  cancelLabel: string;
  sendDisabled: boolean;
  mode: "plan" | "build";
  onSetMode: (mode: "plan" | "build") => void;
  modeLocked: boolean;
  workspaceLabel: string;
  onWorkspaceOpen: () => void;
  projectPickerOpen: boolean;
  accessLabel: string;
  accessFull: boolean;
  accessTitle: string;
}) {
  const [styleOpen, setStyleOpen] = useState(false);
  const styleRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!styleOpen) return;
    const onDocClick = (event: MouseEvent) => {
      if (!styleRef.current?.contains(event.target as Node)) setStyleOpen(false);
    };
    const onKey = (event: KeyboardEvent) => { if (event.key === "Escape") setStyleOpen(false); };
    document.addEventListener("mousedown", onDocClick);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDocClick);
      document.removeEventListener("keydown", onKey);
    };
  }, [styleOpen]);

  const styleLabel = mode === "plan" ? "Plan first" : "Build directly";

  return (
    <div className="ap-composer">
      {topNode}
      <div className="ap-ctxline">
        <button type="button" className="ap-ctx" onClick={onWorkspaceOpen}
          title="Execution environment and project for this conversation"
          aria-haspopup="dialog" aria-expanded={projectPickerOpen}>
          {workspaceLabel} ▾
        </button>
        <span className={`ap-access ${accessFull ? "gold" : ""}`} title={accessTitle}>{accessLabel}</span>
        <span className="ap-note">files stay on this computer until you send</span>
      </div>
      {paletteNode}
      {chipsNode}
      {menuNode}
      <div className="ap-inputrow">
        <textarea
          ref={inputRef}
          aria-label="Message Ask PAIGE"
          aria-describedby={inputAriaDescribedBy}
          placeholder={inputPlaceholder}
          value={input}
          onChange={(e) => onInputChange(e.target.value)}
          onKeyDown={(e) => { if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) { e.preventDefault(); onSend(); } }}
          disabled={inputDisabled}
          rows={2}
        />
      </div>
      <div className="ap-controls">
        {menuTriggerNode}
        <div className="ap-menu-anchor" ref={styleRef}>
          <button type="button" className="ap-btn icon" title="Working style — how Ask PAIGE executes"
            aria-haspopup="menu" aria-expanded={styleOpen}
            onClick={() => setStyleOpen((open) => !open)}>⚙</button>
          {styleOpen && (
            <div className="ap-menu" role="menu" aria-label="Working style">
              <button type="button" role="menuitemradio" aria-checked={mode === "plan"}
                className={`ap-menu-item ${mode === "plan" ? "on" : ""}`}
                onClick={() => { onSetMode("plan"); setStyleOpen(false); }}>
                Plan first<small>research and inspect, read-only</small>
              </button>
              <button type="button" role="menuitemradio" aria-checked={mode === "build"}
                className={`ap-menu-item ${mode === "build" ? "on" : ""}`}
                onClick={() => { onSetMode("build"); setStyleOpen(false); }}>
                Build directly<small>changes with broker approval</small>
              </button>
            </div>
          )}
        </div>
        {modelNode}
        {healthNode}
        {busy ? (
          <button type="button" className="ap-btn" onClick={onCancel}>{cancelLabel}</button>
        ) : (
          <button type="button" className="ap-send" aria-label="Send message" title="Send message (Enter)"
            onClick={onSend} disabled={sendDisabled}>
            <ArrowUpIcon width={18} height={18} aria-hidden />
          </button>
        )}
      </div>
      <span className="ap-controls" style={{ gap: 6, marginTop: 0 }}>
        <span className="ap-status-line" style={{ display: modeLocked ? "inline" : "none" }}>
          Working style: {styleLabel} · a workflow locks this for the run
        </span>
      </span>
      {routeLine}
    </div>
  );
}
