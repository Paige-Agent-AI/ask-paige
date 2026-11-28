import { useCallback, useEffect, useState, type ReactNode } from "react";
import { invoke } from "../adapter/core";
import { PaigeCommandMark } from "./brand/PaigeCommandMark";
import { CpuIcon, FolderIcon, SettingsIcon } from "./Icons";
import type { Screen } from "./Layout";
import "./Rail.css";

type ScopeMode = "observe" | "selected" | "full";

const navItems: { id: Screen; label: string; icon: () => ReactNode }[] = [
  { id: "ask", label: "Ask PAIGE — conversations and work", icon: () => <PaigeCommandMark className="rail-command-glyph" plated={false} label={null} /> },
  { id: "projects", label: "Projects — workspaces and folders", icon: () => <FolderIcon /> },
  { id: "models", label: "Models & Connections", icon: () => <CpuIcon /> },
];

const modeDot: Record<ScopeMode, { className: string; title: string }> = {
  full: { className: "rail-dot rail-dot-full", title: "Computer Access: Full" },
  selected: { className: "rail-dot rail-dot-selected", title: "Computer Access: Selected resources" },
  observe: { className: "rail-dot rail-dot-observe", title: "Computer Access: Observe (read-only)" },
};

/**
 * Region A — the persistent navigation rail. One home for navigation, the
 * brand mark, and the global trust status with an always-reachable emergency
 * stop (the existing native scope revoke; no new authority surface).
 */
export function Rail({ active, onNavigate }: { active: Screen; onNavigate: (s: Screen) => void }) {
  const [mode, setMode] = useState<ScopeMode | null>(null);

  const refreshStatus = useCallback(() => {
    void invoke<{ mode: ScopeMode }>("scope_status")
      .then((status) => setMode(status.mode))
      .catch(() => setMode(null));
  }, []);

  useEffect(() => {
    refreshStatus();
    const handler = () => refreshStatus();
    window.addEventListener("paige-navigate", handler);
    window.addEventListener("paige-scope-changed", handler);
    return () => {
      window.removeEventListener("paige-navigate", handler);
      window.removeEventListener("paige-scope-changed", handler);
    };
  }, [refreshStatus]);

  const emergencyStop = () => {
    if (window.confirm("Stop PAIGE's file and terminal access now? This switches to Observe mode and stops running terminal work. Granted workspaces stay listed in Projects; their permissions return if you switch back.")) {
      void invoke("scope_revoke_all").then(refreshStatus).catch(() => refreshStatus());
    }
  };

  const dot = mode ? modeDot[mode] : { className: "rail-dot", title: "Computer Access status unavailable" };

  return (
    <nav className="rail" aria-label="Ask PAIGE">
      <div className="rail-brand">
        <PaigeCommandMark className="rail-command-mark" label="Ask PAIGE" />
      </div>
      <div className="rail-items">
        {navItems.map((item) => {
          const Icon = item.icon;
          const isActive = active === item.id;
          return (
            <button
              key={item.id}
              className={`rail-item ${isActive ? "active" : ""}`}
              onClick={() => onNavigate(item.id)}
              title={item.label}
              aria-label={item.label}
              aria-current={isActive ? "page" : undefined}
            >
              <Icon />
            </button>
          );
        })}
      </div>
      <div className="rail-end">
        <button
          className="rail-item"
          onClick={() => onNavigate("settings")}
          title="Profile and Settings"
          aria-label="Profile and Settings"
          aria-current={active === "settings" ? "page" : undefined}
        >
          <SettingsIcon />
        </button>
        <span className={dot.className} title={dot.title} aria-label={dot.title} role="img" />
        <button
          className="rail-stop"
          onClick={emergencyStop}
          title="Emergency stop — revoke PAIGE's active file and terminal access"
          aria-label="Emergency stop — revoke active access"
        >
          <span aria-hidden>⏹</span>
        </button>
      </div>
    </nav>
  );
}
