import { useState } from "react";
import reactLogo from "./assets/react.svg";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

type BrokerStatus = {
  granted: boolean;
  root: string | null;
  read: boolean;
  write: boolean;
  pending_proposals: number;
  session_id: string;
  allowed_roots: string[];
};

type FileEntry = { rel_path: string; size: number };

type FileRead = { path: string; content: string; sha256: string; size: number };

type ProposalInfo = {
  proposal_id: string;
  path: string;
  current_sha256: string;
  approval_token: string;
  expires_in_seconds: number;
  new_size: number;
};

type WriteReceipt = {
  path: string;
  before_sha256: string;
  after_sha256: string;
  bytes_written: number;
  readback_verified: boolean;
};

const LAB_ROOT = "C:\\Users\\tonig\\paige-desktop-lab";

function App() {
  const [greetMsg, setGreetMsg] = useState("");
  const [name, setName] = useState("");

  // --- File authority broker state ---
  const [status, setStatus] = useState<BrokerStatus | null>(null);
  const [files, setFiles] = useState<FileEntry[]>([]);
  const [preview, setPreview] = useState<FileRead | null>(null);
  const [selected, setSelected] = useState<string>("");
  const [proposed, setProposed] = useState("");
  const [proposal, setProposal] = useState<ProposalInfo | null>(null);
  const [approved, setApproved] = useState(false);
  const [tokenInput, setTokenInput] = useState("");
  const [result, setResult] = useState<string>("");
  const [busy, setBusy] = useState(false);

  async function greet() {
    setGreetMsg(await invoke("greet", { name }));
  }

  async function refreshStatus() {
    const s = (await invoke("broker_status")) as BrokerStatus;
    setStatus(s);
  }

  async function grant() {
    setBusy(true);
    setResult("");
    try {
      const s = (await invoke("broker_grant_root", { root: LAB_ROOT, read: true, write: true })) as BrokerStatus;
      setStatus(s);
      setResult("Granted read+write on " + (s.root ?? ""));
    } catch (e) {
      setResult("Grant failed: " + String(e));
    } finally {
      setBusy(false);
    }
  }

  async function revoke() {
    setBusy(true);
    setResult("");
    try {
      const s = (await invoke("broker_revoke")) as BrokerStatus;
      setStatus(s);
      setFiles([]);
      setPreview(null);
      setProposal(null);
      setApproved(false);
      setResult("Access revoked. Subsequent operations will be denied.");
    } catch (e) {
      setResult("Revoke failed: " + String(e));
    } finally {
      setBusy(false);
    }
  }

  async function listFiles() {
    setBusy(true);
    setResult("");
    try {
      const f = (await invoke("broker_list_files")) as FileEntry[];
      setFiles(f);
      setResult("Listed " + f.length + " files.");
    } catch (e) {
      setResult("List denied: " + String(e));
    } finally {
      setBusy(false);
    }
  }

  async function readFile(rel: string) {
    setBusy(true);
    setResult("");
    try {
      const r = (await invoke("broker_read_file", { path: rel })) as FileRead;
      setPreview(r);
      setSelected(rel);
      setProposed(r.content);
      setResult("Read " + r.path + " (" + r.size + " bytes).");
    } catch (e) {
      setResult("Read denied: " + String(e));
    } finally {
      setBusy(false);
    }
  }

  async function propose() {
    if (!preview) return;
    setBusy(true);
    setResult("");
    try {
      const p = (await invoke("broker_propose_write", {
        path: selected,
        newContent: proposed,
        expectedSha256: preview.sha256,
      })) as ProposalInfo;
      setProposal(p);
      setApproved(false);
      setResult("Proposal " + p.proposal_id + " created. Approval code: " + p.approval_token);
    } catch (e) {
      setResult("Propose denied: " + String(e));
    } finally {
      setBusy(false);
    }
  }

  async function approve() {
    if (!proposal) return;
    setBusy(true);
    setResult("");
    try {
      const r = await invoke("broker_approve", {
        proposalId: proposal.proposal_id,
        approvalToken: tokenInput,
      });
      setResult("Approval: " + String(r));
      setApproved(true);
      setTokenInput("");
    } catch (e) {
      setResult("Approve failed: " + String(e));
    } finally {
      setBusy(false);
    }
  }

  async function deny() {
    if (!proposal) return;
    setBusy(true);
    setResult("");
    try {
      const r = await invoke("broker_deny", { proposalId: proposal.proposal_id });
      setResult("Deny: " + String(r));
      setProposal(null);
      setApproved(false);
    } catch (e) {
      setResult("Deny failed: " + String(e));
    } finally {
      setBusy(false);
    }
  }

  async function applyWrite() {
    if (!proposal) return;
    setBusy(true);
    setResult("");
    try {
      const r = (await invoke("broker_apply", { proposalId: proposal.proposal_id })) as WriteReceipt;
      setResult(
        "Applied. before=" + r.before_sha256.slice(0, 12) + " after=" + r.after_sha256.slice(0, 12) + " readback_verified=" + r.readback_verified,
      );
      setProposal(null);
      setApproved(false);
      await readFile(selected);
    } catch (e) {
      setResult("Apply failed: " + String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="container">
      <h1>Welcome to Tauri + React</h1>

      <div className="row">
        <a href="https://vite.dev" target="_blank" rel="noopener noreferrer">
          <img src="/vite.svg" className="logo vite" alt="Vite logo" />
        </a>
        <a href="https://tauri.app" target="_blank" rel="noopener noreferrer">
          <img src="/tauri.svg" className="logo tauri" alt="Tauri logo" />
        </a>
        <a href="https://react.dev" target="_blank" rel="noopener noreferrer">
          <img src={reactLogo} className="logo react" alt="React logo" />
        </a>
      </div>
      <p>Click on the Tauri, Vite, and React logos to learn more.</p>

      <form
        className="row"
        onSubmit={(e) => {
          e.preventDefault();
          greet();
        }}
      >
        <input
          id="greet-input"
          onChange={(e) => setName(e.currentTarget.value)}
          placeholder="Enter a name..."
        />
        <button type="submit">Greet</button>
      </form>
      <p>{greetMsg}</p>

      <hr />

      <section>
        <h2>File Authority Broker (Increment 1B)</h2>
        <p>
          Authorized root: <code>{LAB_ROOT}</code>
        </p>
        <p>
          Status:{" "}
          {status
            ? status.granted
              ? "GRANTED (read=" + status.read + ", write=" + status.write + ")"
              : "NOT GRANTED"
            : "unknown"}
          {status ? "  session=" + status.session_id : ""}
        </p>
        <div className="row">
          <button onClick={grant} disabled={busy}>Grant Access</button>
          <button onClick={revoke} disabled={busy}>Revoke Access</button>
          <button onClick={listFiles} disabled={busy}>List Files</button>
          <button onClick={refreshStatus} disabled={busy}>Refresh Status</button>
        </div>

        {files.length > 0 && (
          <div>
            <h3>Files</h3>
            <ul>
              {files.map((f) => (
                <li key={f.rel_path}>
                  <button onClick={() => readFile(f.rel_path)} disabled={busy}>
                    {f.rel_path}
                  </button>{" "}
                  ({f.size} bytes)
                </li>
              ))}
            </ul>
          </div>
        )}

        {preview && (
          <div>
            <h3>Preview / Proposed Modification</h3>
            <p>
              <code>{preview.path}</code> sha256={preview.sha256.slice(0, 16)}…
            </p>
            <textarea
              rows={8}
              cols={80}
              value={proposed}
              onChange={(e) => setProposed(e.target.value)}
            />
            <div className="row">
              <button onClick={propose} disabled={busy}>Propose Change</button>
            </div>
          </div>
        )}

        {proposal && (
          <div>
            <h3>Pending Proposal</h3>
            <p>id={proposal.proposal_id}</p>
            <p>path={proposal.path}</p>
            <p>
              current sha256={proposal.current_sha256.slice(0, 16)}… new size=
              {proposal.new_size} bytes
            </p>
            <p>
              <strong>Approval code: {proposal.approval_token}</strong> (enter it below to approve)
            </p>
            <input
              value={tokenInput}
              onChange={(e) => setTokenInput(e.target.value)}
              placeholder="approval code"
            />
            <div className="row">
              <button onClick={approve} disabled={busy}>Approve</button>
              <button onClick={deny} disabled={busy}>Deny</button>
              <button onClick={applyWrite} disabled={busy || !approved}>Apply (after approval)</button>
            </div>
          </div>
        )}

        {result && <p>Result: {result}</p>}
      </section>
    </main>
  );
}

export default App;
