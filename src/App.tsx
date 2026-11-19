import { useEffect, useState } from "react";
import { Layout, type Screen } from "./components/Layout";
import { AskPaige } from "./screens/AskPaige";
import { Projects } from "./screens/Projects";
import { Files } from "./screens/Files";
import { Approvals } from "./screens/Approvals";
import { Activity } from "./screens/Activity";
import { AgentTeam } from "./screens/AgentTeam";
import { Terminal } from "./screens/Terminal";
import { ModelsConnections } from "./screens/ModelsConnections";
import { Settings } from "./screens/Settings";
import "./styles/global.css";

const screenTitles: Record<Screen, { title: string; subtitle?: string }> = {
  ask: { title: "Ask PAIGE" },
  projects: { title: "Projects & Workspace" },
  files: { title: "Files" },
  approvals: { title: "Approvals" },
  activity: { title: "Activity & Receipts" },
  agents: { title: "Agent Team" },
  terminal: { title: "Terminal" },
  models: { title: "Models & Connections" },
  settings: { title: "Settings" },
};

function App() {
  const [screen, setScreen] = useState<Screen>("ask");

  useEffect(() => {
    const handler = (e: Event) => {
      const detail = (e as CustomEvent).detail as Screen;
      if (detail && screenTitles[detail]) {
        setScreen(detail);
      }
    };
    window.addEventListener("paige-navigate", handler);
    return () => window.removeEventListener("paige-navigate", handler);
  }, []);

  const meta = screenTitles[screen];

  return (
    <Layout active={screen} onNavigate={setScreen} title={meta.title} subtitle={meta.subtitle}>
      {screen === "ask" && <AskPaige />}
      {screen === "projects" && <Projects />}
      {screen === "files" && <Files />}
      {screen === "approvals" && <Approvals />}
      {screen === "activity" && <Activity />}
      {screen === "agents" && <AgentTeam />}
      {screen === "terminal" && <Terminal />}
      {screen === "models" && <ModelsConnections />}
      {screen === "settings" && <Settings />}
    </Layout>
  );
}

export default App;
