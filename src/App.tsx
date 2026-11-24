import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Layout, type Screen } from "./components/Layout";
import { AskPaige } from "./screens/AskPaige";
import { Projects } from "./screens/Projects";
import { ModelsConnections } from "./screens/ModelsConnections";
import { Settings } from "./screens/Settings";
import "./styles/global.css";
import { availableSkills, readEnabledSkills, saveEnabledSkills, type InstalledSkill } from "./skills";

export type Appearance = { mode: "light" | "dark"; auto: boolean };
export type StartupExperience = "cinematic" | "express" | "instant";

function savedAppearance(): Appearance {
  // Honest migration of the previous single-value preference: "system" becomes
  // "match Windows automatically"; light/dark become the chosen theme. No
  // silent change — the stored value keeps its meaning under the new model.
  const saved = localStorage.getItem("paige.appearance");
  const systemDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
  if (saved === "light" || saved === "dark") return { mode: saved, auto: false };
  if (saved === "system") return { mode: systemDark ? "dark" : "light", auto: true };
  return { mode: systemDark ? "dark" : "light", auto: true };
}

function savedStartup(): StartupExperience {
  const saved = localStorage.getItem("paige.startup");
  return saved === "express" || saved === "instant" ? saved : "cinematic";
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
      document.documentElement.dataset.theme = appearance.auto
        ? system.matches ? "dark" : "light"
        : appearance.mode;
    };
    resolve();
    localStorage.setItem("paige.appearance", appearance.auto ? "system" : appearance.mode);
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

  // Branded cinematic awakening: the original Ask PAIGE identity reveal — one
  // gold light, the actual Command Mark, the wordmark — presented while the
  // application initializes underneath. It never blocks readiness, invents no
  // progress, and auto-enters the shell. Three experiences: cinematic (~4s),
  // express (~1.2s), instant (none). Skip and Escape enter immediately;
  // reduced-motion users see the same information as calm crossfades.
  const [startup, setStartup] = useState<StartupExperience>(savedStartup);
  useEffect(() => { localStorage.setItem("paige.startup", startup); }, [startup]);
  const [awakening, setAwakening] = useState(startup !== "instant");
  const awakenMs = startup === "cinematic" ? 4000 : 1200;
  const enterNow = useCallback(() => {
    setAwakening(false);
  }, []);
  useEffect(() => {
    if (startup === "instant") return;
    const dismiss = setTimeout(enterNow, awakenMs);
    const onKey = (event: KeyboardEvent) => { if (event.key === "Escape") enterNow(); };
    window.addEventListener("keydown", onKey);
    return () => {
      clearTimeout(dismiss);
      window.removeEventListener("keydown", onKey);
    };
  }, [startup, awakenMs, enterNow]);

  return (
    <Layout active={screen} onNavigate={setScreen} title={meta.title} subtitle={meta.subtitle}>
      {awakening && (
        <div className={`app-awakening mode-${startup}`} role="presentation">
          <div className="awakening-atmosphere" aria-hidden="true">
            <div className="awakening-core" />
            <div className="awakening-haze" />
          </div>
          <div className="awakening-stage">
            <svg className="awakening-mark" viewBox="0 0 48 48" fill="none" aria-hidden="true">
              <polygon className="awakening-slash" points="21,13.6 30.5,13.6 21,34.4 11.5,34.4" />
              <circle className="awakening-orb" cx="34.5" cy="30.5" r="5.5" />
            </svg>
            <div className="awakening-name">Ask <b>PAIGE</b></div>
            <div className="awakening-tag">Your intelligent Windows workstation</div>
          </div>
          <div className="awakening-actions">
            <button type="button" onClick={enterNow}>Skip — Enter now</button>
          </div>
        </div>
      )}
      <div className="app-conversation" hidden={screen !== "ask"}>
        <AskPaige visible={screen === "ask"} enabledSkills={enabledSkills} skills={skills}
          onInstalled={registerInstalled} onUseInstalled={useInstalledInChat}
          requestedSkillId={requestedSkillId} onSkillRequestHandled={finishSkillRequest}
          onNavigate={setScreen}
          onToggleTheme={() => setAppearance({
            mode: document.documentElement.dataset.theme === "light" ? "dark" : "light",
            auto: false,
          })} />
      </div>
      {screen === "projects" && <Projects />}
      {screen === "models" && <ModelsConnections />}
      {screen === "settings" && <Settings appearance={appearance} onAppearanceChange={setAppearance}
        startup={startup} onStartupChange={setStartup}
        enabledSkills={enabledSkills} onSkillsChange={changeSkills} onUseSkill={useSkillInChat}
        skills={skills} skillsError={skillsError} onInstalled={registerInstalled} onUseInstalled={useInstalledInChat} />}
    </Layout>
  );
}

export default App;
