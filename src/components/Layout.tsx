import type { ReactNode } from "react";
import { Sidebar, type Screen } from "./Sidebar";
import { Header } from "./Header";
import "./Layout.css";

export type { Screen };

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
      {active !== "ask" && <Sidebar active={active} onChange={onNavigate} />}
      <div className="app-main">
        {active !== "ask" && <Header title={title} subtitle={subtitle} />}
        <div className={active === "ask" ? "app-content app-content-work" : "app-content"}>{children}</div>
      </div>
    </div>
  );
}
