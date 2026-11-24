import type { ReactNode } from "react";
import { Rail } from "./Rail";
import { Header } from "./Header";
import "./Layout.css";

export type Screen = "ask" | "projects" | "models" | "settings";

/**
 * The permanent four-region application shell.
 * Region A: the persistent Rail (always present, every destination).
 * Region B: the contextual left sidebar — Ask PAIGE carries its own inside the
 *           work surface; later stages provide it for other destinations.
 * Region C: the main workspace (this component's children + Header).
 * Region D: the optional Work inspector — Ask PAIGE's existing workbench docks
 *           collapsed by default; later stages generalize the slot.
 */
export function Layout({
  active,
  onNavigate,
  title,
  subtitle,
  children,
}: {
  active: Screen;
  onNavigate: (s: Screen) => void;
  title: string;
  subtitle?: string;
  children: ReactNode;
}) {
  return (
    <div className={active === "ask" ? "app-shell" : "app-shell app-shell-tools"}>
      <Rail active={active} onNavigate={onNavigate} />
      <div className="app-main">
        {active !== "ask" && <Header title={title} subtitle={subtitle} />}
        <div className={active === "ask" ? "app-content app-content-work" : "app-content"}>{children}</div>
      </div>
    </div>
  );
}
