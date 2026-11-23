import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { Button } from "../components/Button";
import type { Appearance } from "../App";
import { SkillDiscovery } from "../components/SkillDiscovery";
import type { InstalledSkill, SkillInfo } from "../skills";
import "./Settings.css";

const appearances: { value: Appearance; label: string }[] = [
  { value: "system", label: "System" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
];

export function Settings({ appearance, onAppearanceChange, enabledSkills, onSkillsChange, onUseSkill,
  skills, skillsError, onInstalled, onUseInstalled }: {
  appearance: Appearance;
  onAppearanceChange: (appearance: Appearance) => void;
  enabledSkills: string[];
  onSkillsChange: (ids: string[]) => void;
  onUseSkill: (id: string) => void;
  skills: SkillInfo[];
  skillsError: string;
  onInstalled: (skill: InstalledSkill) => void;
  onUseInstalled: (skill: InstalledSkill) => void;
}) {
  const skillGroups = [
    { name: "Installed from GitHub", entries: skills.filter((skill) => skill.repository) },
    { name: "Design", entries: skills.filter((skill) => skill.design) },
    { name: "Engineering and research", entries: skills.filter((skill) => !skill.design && !skill.repository) },
  ];
  const [version, setVersion] = useState<string | null>(null);
  useEffect(() => {
    let mounted = true;
    void getVersion().then((value) => { if (mounted) setVersion(value); }).catch(() => {});
    return () => { mounted = false; };
  }, []);

  return (
    <div className="settings">
      <section className="settings-section" aria-labelledby="settings-appearance">
        <div className="settings-section-title">
          <h2 id="settings-appearance">Appearance</h2>
          <p>Choose how PAIGE Desktop looks on this device.</p>
        </div>
        <div className="settings-row">
          <strong>Color theme</strong>
          <div className="settings-themes" role="group" aria-label="Color theme">
            {appearances.map(({ value, label }) => (
              <Button key={value} size="sm" variant={appearance === value ? "primary" : "ghost"}
                aria-pressed={appearance === value} onClick={() => onAppearanceChange(value)}>{label}</Button>
            ))}
          </div>
        </div>
      </section>

      <section className="settings-section" aria-labelledby="settings-skills">
        <div className="settings-section-title">
          <h2 id="settings-skills">Skills</h2>
          <p>Choose which skills appear in the composer, or use one in the current chat.</p>
        </div>
        <div className="settings-skill-library">
          <details className="settings-skill-discovery" id="settings-find-skills">
            <summary>Find and install skills</summary>
            <SkillDiscovery onInstalled={onInstalled} onUse={onUseInstalled} />
          </details>
          {skillsError && <p role="alert">Could not load installed skills. {skillsError}</p>}
        {skillGroups.filter(({ entries }) => entries.length > 0).map(({ name, entries }) => <div className="settings-skill-group" key={name}>
          <h3>{name}</h3>
          {entries.map((skill) => {
            const enabled = enabledSkills.includes(skill.id);
            return (
              <div className="settings-skill-row" key={skill.id}>
                <label className="settings-skill">
                  <input type="checkbox" checked={enabled} onChange={(event) => {
                    onSkillsChange(event.target.checked ? [...enabledSkills, skill.id] : enabledSkills.filter((id) => id !== skill.id));
                  }} aria-describedby={"settings-skill-" + skill.id} />
                  <span><strong>{skill.name}</strong><small id={"settings-skill-" + skill.id}>{skill.description}</small>
                    {skill.repository && <small>{skill.repository} · {skill.commit?.slice(0, 8)}</small>}</span>
                </label>
                <Button type="button" size="sm" variant="secondary" disabled={!enabled}
                  aria-label={"Use in chat for " + skill.name} onClick={() => onUseSkill(skill.id)}>Use in chat</Button>
              </div>
            );
          })}
        </div>)}
        </div>
      </section>

      <section className="settings-section" aria-labelledby="settings-workspace">
        <div className="settings-section-title">
          <h2 id="settings-workspace">Workspace access</h2>
          <p>Manage which local folders PAIGE can use and whether access is remembered.</p>
        </div>
        <div className="settings-row">
          <strong>Authorized projects</strong>
          <Button size="sm" onClick={() => window.dispatchEvent(new CustomEvent("paige-navigate", { detail: "projects" }))}>
            Manage project access
          </Button>
        </div>
      </section>

      <section className="settings-section" aria-labelledby="settings-connections">
        <div className="settings-section-title">
          <h2 id="settings-connections">Models &amp; Connections</h2>
          <p>Manage model gateways and connected services.</p>
        </div>
        <div className="settings-row">
          <strong>Providers and integrations</strong>
          <Button size="sm" onClick={() => window.dispatchEvent(new CustomEvent("paige-navigate", { detail: "models" }))}>
            Manage connections
          </Button>
        </div>
      </section>

      <section className="settings-section" aria-labelledby="settings-about">
        <div className="settings-section-title"><h2 id="settings-about">About</h2></div>
        <div className="settings-row"><strong>PAIGE Desktop</strong>{version && <span className="settings-value">Version {version}</span>}</div>
      </section>
    </div>
  );
}

