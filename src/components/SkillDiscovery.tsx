import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "./Button";
import type { InstalledSkill } from "../skills";
import "./SkillDiscovery.css";

interface Candidate {
  repository: string;
  skill: string | null;
  description: string | null;
}
export interface SkillSearch {
  source: string;
  results: Candidate[];
  installed: InstalledSkill[];
}
interface Catalog {
  repository: string;
  commit: string;
  manifests: string[];
}
interface FileRecord {
  path: string;
  size: number;
  sha256: string;
}
interface Preview {
  token: string;
  skill: InstalledSkill;
  files: FileRecord[];
  guidance: string;
}
interface InstallReceipt {
  skill: InstalledSkill;
  files: FileRecord[];
  installed_at: number;
}

function skillLabel(path: string, repo: string) {
  const parts = path.split("/");
  return parts.length > 1 ? parts[parts.length - 2] : repo.split("/").pop();
}

export function SkillDiscovery({ initial, onInstalled, onUse }: {
  initial?: SkillSearch;
  onInstalled: (skill: InstalledSkill) => void;
  onUse: (skill: InstalledSkill) => void;
}) {
  const [input, setInput] = useState("");
  const [search, setSearch] = useState(initial);
  const [catalog, setCatalog] = useState<Catalog>();
  const [preview, setPreview] = useState<Preview>();
  const [receipt, setReceipt] = useState<InstallReceipt>();
  const [pending, setPending] = useState("");
  const [error, setError] = useState("");

  async function perform(label: string, action: () => Promise<void>) {
    setError("");
    setPending(label);
    try { await action(); } catch (error) { setError(String(error)); }
    finally { setPending(""); }
  }

  async function review(repo: Catalog, manifest: string, expectedName?: string) {
    const data = await invoke<Preview>("skills_prepare", {
      repository: repo.repository, commit: repo.commit, manifest,
    });
    if (expectedName && data.skill.name !== expectedName) {
      throw new Error("SKILL.md has a different name from the search result. Choose the intended package.");
    }
    setPreview(data);
  }

  const busy = pending !== "";
  return (
    <div className="skill-finder">
      {!initial && (
        <form className="skill-finder-search" onSubmit={(event) => {
          event.preventDefault();
          if (busy || !input.trim()) return;
          setSearch(undefined);
          setCatalog(undefined);
          setPreview(undefined);
          setReceipt(undefined);
          void perform("Searching for skills…", async () => {
            setSearch(await invoke<SkillSearch>("skills_search", { input: input.trim() }));
          });
        }}>
          <label>
            <span>Search public GitHub skills</span>
            <input type="search" placeholder="Skill topic, name, or GitHub repository" maxLength={220}
              value={input} onChange={(event) => setInput(event.target.value)} />
          </label>
          <Button type="submit" size="sm" disabled={busy || !input.trim()}>Search</Button>
        </form>
      )}
      {receipt ? (
        <div className="skill-finder-preview">
          <strong role="status">Installed {receipt.skill.name}</strong>
          <p>{receipt.skill.repository} · {receipt.files.length} files verified</p>
          <details>
            <summary>Installation receipt</summary>
            <p>Commit <code>{receipt.skill.commit}</code></p>
            <ul className="skill-finder-file-list">{receipt.files.map((file) => (
              <li key={file.path}><code>{file.path}</code><small>SHA-256 {file.sha256}</small></li>
            ))}</ul>
          </details>
          <div className="skill-finder-actions">
            <Button type="button" size="sm" variant="ghost" onClick={() => {
              setReceipt(undefined);
              setCatalog(undefined);
            }}>Back to results</Button>
            <Button type="button" size="sm" variant="primary" onClick={() => onUse(receipt.skill)}>Use in chat</Button>
          </div>
        </div>
      ) : preview ? (
        <div className="skill-finder-preview">
          <strong>{preview.skill.name}</strong>
          <p>{preview.skill.description}</p>
          <dl>
            <dt>GitHub</dt><dd>{preview.skill.repository}</dd>
            <dt>Commit</dt><dd><code>{preview.skill.commit}</code></dd>
            <dt>Package</dt><dd><code>{preview.skill.manifest}</code></dd>
          </dl>
          <details open>
            <summary>Review {preview.files.length} files</summary>
            <ul className="skill-finder-file-list">{preview.files.map((file) => (
              <li key={file.path}><code>{file.path}</code><small>{file.size.toLocaleString()} bytes</small></li>
            ))}</ul>
          </details>
          <details>
            <summary>Read SKILL.md</summary>
            <pre tabIndex={0}>{preview.guidance}</pre>
          </details>
          <p>PAIGE saves the whole package locally. Scripts stay inactive during installation.</p>
          <div className="skill-finder-actions">
            <Button type="button" size="sm" variant="ghost" disabled={busy} onClick={() => {
              setPreview(undefined);
              setError("");
            }}>Back</Button>
            <Button type="button" size="sm" variant="primary" disabled={busy} onClick={() => {
              const { token } = preview;
              setPreview(undefined);
              void perform("Waiting for Windows approval…", async () => {
                const verified = await invoke<InstallReceipt>("skills_install", { token });
                setReceipt(verified);
                onInstalled(verified.skill);
              });
            }}>Install with Windows approval</Button>
          </div>
        </div>
      ) : catalog ? (
        <>
          <div className="skill-finder-result-heading">
            <strong>{catalog.repository}</strong>
            <Button type="button" size="sm" variant="ghost" disabled={busy} onClick={() => {
              setCatalog(undefined);
              setError("");
            }}>Back to results</Button>
          </div>
          {!catalog.manifests.length && <p>This repository has no supported SKILL.md files.</p>}
          {catalog.manifests.length > 0 && <ul className="skill-finder-results">
            {catalog.manifests.map((path) => <li key={path}>
              <span><strong>{skillLabel(path, catalog.repository)}</strong><small>{path}</small></span>
              <Button type="button" size="sm" disabled={busy} aria-label={"Review " + path}
                onClick={() => void perform("Downloading the pinned package…", () => review(catalog, path))}>Review files</Button>
            </li>)}
          </ul>}
        </>
      ) : search && (
        <>
          <p>{search.source}</p>
          {!search.results.length && !search.installed.length && <p>No matching skills found.</p>}
          <ul className="skill-finder-results">
            {search.installed.map((skill) => <li key={skill.id}>
              <span><strong>{skill.name}</strong><small>Installed · {skill.repository}</small></span>
              <Button type="button" size="sm" disabled={busy} onClick={() => onUse(skill)}>Use in chat</Button>
            </li>)}
            {search.results.map((item) => <li key={item.repository + "@" + (item.skill ?? "")}>
              <span><strong>{item.skill ?? item.repository}</strong><small>
                {item.skill ? item.repository : item.description}</small></span>
              <Button type="button" size="sm" disabled={busy}
                aria-label={"Open " + (item.skill ? item.skill + " from " : "") + item.repository}
                onClick={() => void perform("Checking GitHub…", async () => {
                  const repo = await invoke<Catalog>("skills_catalog", { repository: item.repository });
                  setCatalog(repo);
                  const matches = item.skill ? repo.manifests.filter((path) =>
                    skillLabel(path, repo.repository) === item.skill) : repo.manifests;
                  const paths = matches.length ? matches : repo.manifests.filter((path) => path === "SKILL.md");
                  if (item.skill && paths.length === 0) {
                    throw new Error("This repository does not list that skill at the current commit. Browse its other skills below.");
                  }
                  setCatalog({ ...repo, manifests: paths });
                  if (paths.length === 1) {
                    setPending("Downloading the pinned package…");
                    try { await review(repo, paths[0], item.skill ?? undefined); }
                    catch (error) { setCatalog(repo); throw error; }
                  }
                })}>Open</Button>
            </li>)}
          </ul>
        </>
      )}
      {pending && <p role="status">{pending}</p>}
      {error && <p className="skill-finder-error" role="alert">{error}</p>}
    </div>
  );
}

