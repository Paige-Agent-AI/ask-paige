import { useState } from "react";
import type { SVGProps } from "react";
import { PaigeCommandMark } from "./brand/PaigeCommandMark";
import {
  ChevronLeftIcon,
  ChevronRightIcon,
  CpuIcon,
  FolderIcon,
  SettingsIcon,
} from "./Icons";
import "./Sidebar.css";

export type Screen =
  | "ask"
  | "projects"
  | "models"
  | "settings";

const CommandMarkNavIcon = (_props: SVGProps<SVGSVGElement>) => (
  <PaigeCommandMark className="sidebar-item-command-mark" plated={false} label={null} />
);

const items: { id: Screen; label: string; icon: React.FC<React.SVGProps<SVGSVGElement>> }[] = [
  { id: "ask", label: "Ask PAIGE", icon: CommandMarkNavIcon },
  { id: "projects", label: "Projects", icon: FolderIcon },
  { id: "models", label: "Models & Connections", icon: CpuIcon },
  { id: "settings", label: "Settings", icon: SettingsIcon },
];

export function Sidebar({
  active,
  onChange,
}: {
  active: Screen;
  onChange: (s: Screen) => void;
}) {
  const [expanded, setExpanded] = useState(true);

  return (
    <nav className={`sidebar ${expanded ? "expanded" : ""}`} aria-label="PAIGE Desktop">
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
              aria-label={item.label}
              aria-current={isActive ? "page" : undefined}
            >
              <span className="sidebar-icon">
                <Icon />
              </span>
              {expanded && <span className="sidebar-label">{item.label}</span>}
            </button>
          );
        })}
      </div>

    </nav>
  );
}
