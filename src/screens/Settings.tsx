import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "../components/Button";
import type { Appearance } from "../App";
import { SkillDiscovery } from "../components/SkillDiscovery";
import type { InstalledSkill, SkillInfo } from "../skills";
import "./Settings.css";

type ScopeMode = "observe" | "selected" | "full";

interface ScopeGrant {
  id: string;
  root: string;
  read: boolean;
  write: boolean;
  entity_id: string;
  entity_name: string;
  granted_at: number;
  confirmed_via: string;
  full_access?: boolean;
}

interface ScopeStatus {
  mode: ScopeMode;
  grants: ScopeGrant[];
  store_unsupported: boolean;
  profile_root: string | null;
}

const scopeEntities = [
  { id: "personal", name: "Personal" },
  { id: "paige-agent-ai", name: "PAIGE Agent AI Inc." },
  { id: "coreconnect", name: "CoreConnect Technology Inc." },
];

const scopeModes: { value: ScopeMode; label: string; detail: string }[] = [
  { value: "observe", label: "Observe", detail: "Read-only inspection inside granted roots. No writes, no terminal, no effectful tools." },
  { value: "selected", label: "Selected Resources", detail: "Only the roots you grant below, each with its own read/write permissions." },
  { value: "full", label: "Full Computer Access", detail: "Ordinary operations across your user profile and added roots, within your Windows account's own permissions. UAC prompts still apply; sensitive actions still ask." },
];

function ComputerAccess() {
  const [scope, setScope] = useState<ScopeStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [entity, setEntity] = useState("personal");
  const [customName, setCustomName] = useState("");

  const refresh = async () => {
    try { setScope(await invoke<ScopeStatus>("scope_status")); setError(null); }
    catch (e) { setError(String(e)); }
  };
  useEffect(() => { void refresh(); }, []);

  const run = async (action: () => Promise<ScopeStatus>) => {
    if (busy) return;
    setBusy(true); setError(null);
    try {
      setScope(await action());
      // The rail's trust dot follows real scope changes, not navigation.
      window.dispatchEvent(new CustomEvent("paige-scope-changed"));
    }
    catch (e) { setError(String(e)); }
    finally { setBusy(false); }
  };

  const addRoot = () => void run(async () => {
    const root = await invoke<string | null>("project_pick_folder");
    if (!root) return await invoke<ScopeStatus>("scope_status");
    const chosen = scopeEntities.find((option) => option.id === entity);
    const name = entity === "custom"
      ? customName.trim()
      : chosen?.name ?? "Personal";
    return await invoke<ScopeStatus>("scope_add_root", {
      root, read: true, write: true,
      entityId: entity === "custom" ? undefined : entity,
      entityName: name,
    });
  });

  const activeMode = scope?.mode;
  // Until the scope status resolves, no mode is presented as active.
  const effectiveMode = activeMode;
  return (
    <section className="settings-section" aria-labelledby="settings-computer-access">
      <div className="settings-section-title">
        <h2 id="settings-computer-access">Computer Access</h2>
        <p>The native authority PAIGE runs under. Every file, terminal and tool check consults this one owner-granted scope.</p>
      </div>
      <div className="settings-scope-body">
      {scope?.store_unsupported && (
        <p className="settings-scope-error" role="alert">
          The saved scope registry could not be read and was preserved. Remove scope.json in the app config folder to start a fresh one.
        </p>
      )}
      <div className="settings-row" role="group" aria-label="Computer Access mode">
        <strong>Mode</strong>
        <div className="settings-themes">
          {scopeModes.map(({ value, label, detail }) => (
            <Button key={value} size="sm" variant={effectiveMode === value ? "primary" : "ghost"}
              aria-pressed={effectiveMode === value} title={detail} disabled={busy}
              onClick={() => void run(() => invoke<ScopeStatus>("scope_set_mode", { mode: value }))}>
              {label}
            </Button>
          ))}
        </div>
      </div>
      <p className="settings-scope-detail">
        {scopeModes.find((m) => m.value === effectiveMode)?.detail}
      </p>
      <div className="settings-scope-block">
        <div className="settings-scope-head">
          <strong>Granted roots</strong>
          <div className="settings-scope-actions">
            <label>
              Workspace
              <select value={entity} onChange={(e) => setEntity(e.target.value)} aria-label="Owning workspace for new roots">
                {scopeEntities.map((option) => (
                  <option key={option.id} value={option.id}>{option.name}</option>
                ))}
                <option value="custom">Another workspace…</option>
              </select>
            </label>
            {entity === "custom" && (
              <input value={customName} maxLength={80} placeholder="Workspace name"
                aria-label="New workspace name" onChange={(e) => setCustomName(e.target.value)} />
            )}
            <Button size="sm" onClick={addRoot} disabled={busy || (entity === "custom" && !customName.trim())}>Add root…</Button>
          </div>
        </div>
        <ul className="settings-scope-list">
          {(scope?.grants ?? []).map((grant) => (
            <li key={grant.id}>
              <span className="settings-scope-root" title={grant.root}>{grant.root}</span>
              <span className="settings-scope-entity" title={`Workspace: ${grant.entity_name} (${grant.entity_id})`}>
                {grant.entity_name}
              </span>
              <span className="settings-scope-rights" aria-label="Granted permissions">
                {grant.read && <em>read</em>}
                {grant.write && <em>write</em>}
                {grant.full_access && <em title="Created by Full Computer Access; withdrawn when you leave that mode">full scope</em>}
              </span>
              <Button size="sm" variant="ghost" disabled={busy}
                onClick={() => void run(() => invoke<ScopeStatus>("scope_remove_root", { id: grant.id }))}>
                Remove
              </Button>
            </li>
          ))}
          {scope && scope.grants.length === 0 && (
            <li className="settings-scope-empty">No roots are granted. PAIGE cannot touch any folder until you add one.</li>
          )}
        </ul>
      </div>
      <div className="settings-row">
        <strong>Emergency stop</strong>
        <Button size="sm" variant="secondary" disabled={busy}
          onClick={() => {
            if (window.confirm("Stop PAIGE's file and terminal access now? This switches to Observe mode and stops running terminal work. Any granted roots stay listed; if you later switch back to Selected Resources, their permissions return.")) {
              void run(() => invoke<ScopeStatus>("scope_revoke_all"));
            }
          }}>
          Revoke active access
        </Button>
      </div>
      {error && <p className="settings-scope-error" role="alert">{error}</p>}
      </div>
    </section>
  );
}

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
          <p>Choose how Ask PAIGE looks on this device.</p>
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

      <ComputerAccess />

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
        <div className="settings-row"><strong>Ask PAIGE</strong>{version && <span className="settings-value">Version {version}</span>}</div>
      </section>
    </div>
  );
}

