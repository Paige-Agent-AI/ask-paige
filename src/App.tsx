import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Layout, type Screen } from "./components/Layout";
import { AskPaige } from "./screens/AskPaige";
import { Projects } from "./screens/Projects";
import { ModelsConnections } from "./screens/ModelsConnections";
import { Settings } from "./screens/Settings";
import "./styles/global.css";
import { availableSkills, readEnabledSkills, saveEnabledSkills, type InstalledSkill } from "./skills";

export type Appearance = "light" | "dark" | "system";

function savedAppearance(): Appearance {
  const saved = localStorage.getItem("paige.appearance");
  return saved === "light" || saved === "dark" || saved === "system" ? saved : "system";
}

const screenTitles: Record<Screen, { title: string; subtitle?: string }> = {
  ask: { title: "Ask PAIGE" },
  projects: { title: "Projects & Workspace" },
  models: { title: "Models & Connections" },
  settings: { title: "Settings" },
};

function App() {
  const [screen, setScreen] = useState<Screen>("ask");
  const [appearance, setAppearance] = useState<Appearance>(savedAppearance);
  const [enabledSkills, setEnabledSkills] = useState(readEnabledSkills);
  const [requestedSkillId, setRequestedSkillId] = useState<string | null>(null);
  const [installedSkills, setInstalledSkills] = useState<InstalledSkill[]>([]);
  const [skillsError, setSkillsError] = useState("");
  const skills = [...availableSkills, ...installedSkills.map((skill) => ({ ...skill, design: false }))];
  useEffect(() => {
    let mounted = true;
    void invoke<InstalledSkill[]>("skills_installed").then((found) => {
      if (mounted) setInstalledSkills(found);
    }).catch((error) => { if (mounted) setSkillsError(String(error)); });
    return () => { mounted = false; };
  }, []);
  const changeSkills = setEnabledSkills;
  useEffect(() => { saveEnabledSkills(enabledSkills); }, [enabledSkills]);
  const useSkillInChat = (id: string) => {
    setRequestedSkillId(id);
    setScreen("ask");
  };
  const registerInstalled = (skill: InstalledSkill) => {
    setInstalledSkills((current) => [...current.filter((item) => item.id !== skill.id), skill]
      .sort((a, b) => a.name.localeCompare(b.name)));
    setEnabledSkills((current) => current.includes(skill.id) ? current : [...current, skill.id]);
  };
  const useInstalledInChat = (skill: InstalledSkill) => {
    registerInstalled(skill);
    useSkillInChat(skill.id);
  };

  const finishSkillRequest = useCallback(() => setRequestedSkillId(null), []);

  useEffect(() => {
    const system = window.matchMedia("(prefers-color-scheme: dark)");
    const resolve = () => {
      document.documentElement.dataset.theme = appearance === "system"
        ? system.matches ? "dark" : "light"
        : appearance;
    };
    resolve();
    localStorage.setItem("paige.appearance", appearance);
    system.addEventListener("change", resolve);
    return () => system.removeEventListener("change", resolve);
  }, [appearance]);

  useEffect(() => {
    const handler = (e: Event) => {
      const detail = (e as CustomEvent).detail as Screen;
      if (detail && Object.prototype.hasOwnProperty.call(screenTitles, detail)) setScreen(detail);
    };
    window.addEventListener("paige-navigate", handler);
    return () => window.removeEventListener("paige-navigate", handler);
  }, []);

  const meta = screenTitles[screen];

  return (
    <Layout active={screen} onNavigate={setScreen} title={meta.title} subtitle={meta.subtitle}>
      <div className="app-conversation" hidden={screen !== "ask"}>
        <AskPaige visible={screen === "ask"} enabledSkills={enabledSkills} skills={skills}
          onInstalled={registerInstalled} onUseInstalled={useInstalledInChat}
          requestedSkillId={requestedSkillId} onSkillRequestHandled={finishSkillRequest}
          onNavigate={setScreen}
          onToggleTheme={() => setAppearance(document.documentElement.dataset.theme === "light" ? "dark" : "light")} />
      </div>
      {screen === "projects" && <Projects />}
      {screen === "models" && <ModelsConnections />}
      {screen === "settings" && <Settings appearance={appearance} onAppearanceChange={setAppearance}
        enabledSkills={enabledSkills} onSkillsChange={changeSkills} onUseSkill={useSkillInChat}
        skills={skills} skillsError={skillsError} onInstalled={registerInstalled} onUseInstalled={useInstalledInChat} />}
    </Layout>
  );
}

export default App;
