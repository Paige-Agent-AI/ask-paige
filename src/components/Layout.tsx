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
    <div className="app-shell">
      <Sidebar active={active} onChange={onNavigate} />
      <div className="app-main">
        <Header title={title} subtitle={subtitle} />
        <div className="app-content">{children}</div>
      </div>
    </div>
  );
}
