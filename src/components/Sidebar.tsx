import { useState } from "react";
import type { SVGProps } from "react";
import { PaigeCommandMark } from "./brand/PaigeCommandMark";
import {
  ActivityIcon,
  ChevronLeftIcon,
  ChevronRightIcon,
  CpuIcon,
  FolderIcon,
  SettingsIcon,
  ShieldCheckIcon,
  TerminalIcon,
  UsersIcon,
} from "./Icons";
import "./Sidebar.css";

export type Screen =
  | "ask"
  | "projects"
  | "files"
  | "approvals"
  | "activity"
  | "agents"
  | "terminal"
  | "models"
  | "settings";

const CommandMarkNavIcon = (_props: SVGProps<SVGSVGElement>) => (
  <PaigeCommandMark className="sidebar-item-command-mark" plated={false} label={null} />
);

const items: { id: Screen; label: string; icon: React.FC<React.SVGProps<SVGSVGElement>> }[] = [
  { id: "ask", label: "Ask PAIGE", icon: CommandMarkNavIcon },
  { id: "projects", label: "Projects", icon: FolderIcon },
  { id: "files", label: "Files", icon: FileIcon },
  { id: "approvals", label: "Approvals", icon: ShieldCheckIcon },
  { id: "activity", label: "Activity", icon: ActivityIcon },
  { id: "agents", label: "Agent Team", icon: UsersIcon },
  { id: "terminal", label: "Terminal", icon: TerminalIcon },
  { id: "models", label: "Models & Connections", icon: CpuIcon },
  { id: "settings", label: "Settings", icon: SettingsIcon },
];

function FileIcon(props: React.SVGProps<SVGSVGElement>) {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      width="20"
      height="20"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      {...props}
    >
      <path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" />
      <path d="M14 2v4a2 2 0 0 0 2 2h4" />
    </svg>
  );
}

export function Sidebar({
  active,
  onChange,
}: {
  active: Screen;
  onChange: (s: Screen) => void;
}) {
  const [expanded, setExpanded] = useState(false);

  return (
    <nav className={`sidebar ${expanded ? "expanded" : ""}`}>
      <div className="sidebar-header">
        <div className="sidebar-mark">
          <PaigeCommandMark className="sidebar-command-mark" label="PAIGE" />
          {expanded && <span className="sidebar-mark-text">PAIGE</span>}
        </div>
        <button
          className="sidebar-toggle"
          onClick={() => setExpanded((e) => !e)}
          aria-label={expanded ? "Collapse sidebar" : "Expand sidebar"}
          title={expanded ? "Collapse" : "Expand"}
        >
          {expanded ? <ChevronLeftIcon /> : <ChevronRightIcon />}
        </button>
      </div>

      <div className="sidebar-items">
        {items.map((item) => {
          const Icon = item.icon;
          const isActive = active === item.id;
          return (
            <button
              key={item.id}
              className={`sidebar-item ${isActive ? "active" : ""}`}
              onClick={() => onChange(item.id)}
              title={item.label}
            >
              <span className="sidebar-icon">
                <Icon />
              </span>
              {expanded && <span className="sidebar-label">{item.label}</span>}
            </button>
          );
        })}
      </div>

      <div className="sidebar-footer">
        {expanded && <span className="sidebar-version">PAIGE Desktop · 1B-UI</span>}
      </div>
    </nav>
  );
}
