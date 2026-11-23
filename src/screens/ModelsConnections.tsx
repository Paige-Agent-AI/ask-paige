import { useState, type FormEvent, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "../components/Button";
import { useAsk } from "../hooks/useAsk";
import { useMcp, type UseMcpResult } from "../hooks/useMcp";
import { ModelRoundFacts } from "./WorkbenchReview";
import { ModelPicker } from "./ModelPicker";
import { PaigeCommandMark } from "../components/brand/PaigeCommandMark";
import type { McpServerStatus, OAuthProvider } from "../types/mcp";
import linearMark from "../assets/providers/linear.svg";
import supabaseMark from "../assets/providers/supabase.svg";
import githubMark from "../assets/providers/github.svg";
import omniMark from "../assets/providers/omniroute.png";
import routerLightMark from "../assets/providers/openrouter-light.svg";
import routerDarkMark from "../assets/providers/openrouter-dark.svg";
import "./ModelsConnections.css";

const views = ["Connections", "Models", "Receipts"] as const;
const receiptMillis = (timestamp: number) => timestamp < 100_000_000_000 ? timestamp * 1000 : timestamp;

const presets = [
  { id: "paige", name: "PAIGE Orchestrator", mark: undefined, description: "Browse and connect PAIGE specialists." },
  { id: "linear", name: "Linear", mark: linearMark, description: "Find, create and update issues and projects." },
  { id: "supabase", name: "Supabase", mark: supabaseMark, description: "Work with a project's database, documentation, and scoped log counts." },
  { id: "github", name: "GitHub", mark: githubMark, description: "Read, create and update issues in a repository." },
  { id: "notion", name: "Notion", mark: undefined, description: "Read a selected Notion page as your Second Brain." },
] as const;

function serverPreset(server: McpServerStatus): { id: OAuthProvider; readOnly: boolean } | undefined {
  if (server.oauth_provider) return { id: server.oauth_provider, readOnly: false };
  if (server.kind !== "remote") return;
  try {
    const url = new URL(server.display);
    if (url.origin === "https://mcp.linear.app" && (url.pathname === "/mcp" || url.pathname === "/mcp/readonly")) {
      return { id: "linear", readOnly: url.pathname === "/mcp/readonly" };
    }
    if (url.origin === "https://mcp.supabase.com" && url.pathname === "/mcp" && url.searchParams.get("project_ref")) {
      return { id: "supabase", readOnly: url.searchParams.getAll("read_only").includes("true") };
    }
  } catch {
    // Custom servers may have an invalid or non-URL display value.
  }
}

function ConnectionRow({ name, mark, darkMark, description, status, tone = "", action, children }: {
  name: string;
  mark?: string;
  darkMark?: string;
  description: string;
  status: string;
  tone?: string;
  action: ReactNode;
  children?: ReactNode;
}) {
  return (
    <div className="provider">
      <div className="provider-row">
        <span className={"provider-mark" + (name === "GitHub" ? " provider-mark-github" : "")} aria-hidden="true">
          {mark ? (
            <>
              <img className={darkMark ? "provider-mark-light" : undefined} src={mark} alt="" />
              {darkMark && <img className="provider-mark-dark" src={darkMark} alt="" />}
            </>
          ) : name === "PAIGE Orchestrator" ? <PaigeCommandMark plated={false} label={null} />
            : <span className="provider-initial">{name.slice(0, 1).toUpperCase()}</span>}
        </span>
        <div className="provider-identity">
          <h4>{name}</h4>
          <p title={description}>{description}</p>
        </div>
        <span className={"connection-status " + tone}>{status}</span>
        <div className="provider-action">{action}</div>
      </div>
      {children && <div className="provider-details">{children}</div>}
    </div>
  );
}

function ServerDetails({ server, mcp }: { server: McpServerStatus; mcp: UseMcpResult }) {
  const [busy, setBusy] = useState(false);

  async function change(operation: () => Promise<unknown>) {
    setBusy(true);
    try {
      await operation();
    } catch {
      // The MCP hook exposes the error and refreshes the saved configuration.
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
      <p className="provider-endpoint">{server.display}</p>
      <p className="connection-help">
        {server.state === "ready"
          ? server.capabilities.tools.length + " tools · " + server.capabilities.resources.length + " resources · " + server.capabilities.prompts.length + " prompts"
          : server.detail}
      </p>
      <div className="provider-buttons">
        {server.state === "ready" ? (
          <Button size="sm" disabled={busy} onClick={() => void change(() => mcp.disconnect(server.id))}>Disconnect</Button>
        ) : server.oauth_provider !== "github" && (
          <Button size="sm" variant="primary" disabled={busy} onClick={() => void change(() => mcp.connect(server.id))}>Connect</Button>
        )}
        {server.oauth_provider !== "github" && <Button size="sm" disabled={busy} onClick={() => void change(() => mcp.reconnect(server.id))}>Reconnect</Button>}
        <Button size="sm" variant="danger" disabled={busy} onClick={() => void change(() => mcp.revoke(server.id))}>Remove from PAIGE</Button>
      </div>
      {server.oauth_provider && server.state !== "ready" && <ProviderSignIn provider={server.oauth_provider} mcp={mcp} id={server.id} />}
    </>
  );
}

function ProviderSignIn({ provider, mcp, id, onSaved }: {
  provider: OAuthProvider;
  mcp: UseMcpResult;
  id?: string;
  onSaved?: (id: string) => void;
}) {
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState("");
  const name = presets.find((preset) => preset.id === provider)?.name ?? provider;

  async function signIn(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (busy) return;
    const target = String(new FormData(event.currentTarget).get("target") ?? "").trim();
    setBusy(true);
    setStatus("Finish signing in to " + name + " in your browser.");
    try {
      const connection = await mcp.signIn(provider, id, target);
      setStatus("");
      onSaved?.(connection.id);
    } catch (error) {
      setStatus(String(error));
    } finally {
      setBusy(false);
    }
  }

  async function cancel() {
    try {
      await mcp.cancelSignIn();
    } catch (error) {
      setStatus(String(error));
    }
  }

  return (
    <form className="provider-form" onSubmit={signIn}>
      {provider === "paige" && <p className="connection-help">For your Operator account, open Operator sign-in first. Then return here to authorize PAIGE Desktop in the same browser.</p>}
      {provider === "linear" && <p className="connection-help">Grant PAIGE Read and Write access to the Linear workspace you choose.</p>}
      {provider === "supabase" && <p className="connection-help">Authorize database and analytics read access in the browser. PAIGE will use the selected project and ask you to confirm SQL and migrations.</p>}
      {provider === "github" && <p className="connection-help">GitHub requests repo access. PAIGE’s issue tools use the repository below. GitHub sign-in lasts for this app session.</p>}
      {!id && provider === "supabase" && (
        <label>Project reference<input name="target" required pattern="[a-z0-9]{20}" placeholder="20-character project reference" disabled={busy} /></label>
      )}
      {provider === "notion" && <p className="connection-help">
        Sign in to the Notion workspace that contains your chosen page. Use /notion in a chat to read that page and see its receipt.
      </p>}
      {!id && provider === "notion" && (
        <label>Notion page link or ID<input name="target" required placeholder="https://www.notion.so/…" spellCheck={false} disabled={busy} /></label>
      )}
      {!id && provider === "github" && (
        <label>Repository<input name="target" required placeholder="owner/repository" spellCheck={false} disabled={busy} /></label>
      )}
      <div className="provider-buttons">
        {provider === "paige" && <Button type="button" size="sm" disabled={busy} onClick={() => {
          void invoke("mcp_paige_operator_login").catch((error) => setStatus(String(error)));
        }}>Open Operator sign-in</Button>}
        <Button type="submit" size="sm" variant="primary" disabled={busy}>
          {busy ? "Waiting for " + name + "…" : id ? "Sign in again" : "Sign in with " + name}
        </Button>
        {busy && <Button type="button" size="sm" onClick={() => void cancel()}>Cancel sign-in</Button>}
      </div>
      {status && <p className="connection-help" role="status">{status}</p>}
    </form>
  );
}

function McpSetup({ mcp, onSaved }: {
  mcp: UseMcpResult;
  onSaved: (id: string) => void;
}) {
  const [transport, setTransport] = useState<"remote" | "stdio">("remote");
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState("");

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (busy) return;
    const form = event.currentTarget;
    const fields = new FormData(form);
    const value = (field: string) => String(fields.get(field) ?? "").trim();
    const name = value("name");
    const id = value("id");
    const token = value("token");
    const tokenInput = form.elements.namedItem("token");
    if (tokenInput instanceof HTMLInputElement) tokenInput.value = "";
    setProblem("");
    setBusy(true);
    try {
      if (transport === "stdio") {
        await mcp.addStdio(id, name, value("command"), value("args").split(/\s+/).filter(Boolean), {}, true);
      } else {
        await mcp.addRemote(id, name, value("url"), token ? "Bearer " + token : undefined, undefined, true);
      }
      onSaved(id);
    } catch {
      // Add stores the configuration before connecting. A failed handshake can still leave a saved server.
      setProblem("Check the connection error below. If PAIGE saved the server, open Manage to reconnect or remove it.");
    } finally {
      setBusy(false);
    }
  }

  return (
    <form className="provider-form" onSubmit={submit}>
          <label>Transport
            <select value={transport} onChange={(event) => setTransport(event.target.value as "remote" | "stdio")}>
              <option value="remote">Remote HTTPS</option>
              <option value="stdio">Local process (stdio)</option>
            </select>
          </label>
          <div className="provider-fields">
            <label>Connection ID<input name="id" required /></label>
            <label>Display name<input name="name" required /></label>
          </div>
          {transport === "remote" ? (
            <label>Server URL<input name="url" type="url" placeholder="https://example.com/mcp" required /></label>
          ) : (
            <div className="provider-fields">
              <label>Command<input name="command" required /></label>
              <label>Arguments<input name="args" placeholder="Space-separated arguments" /></label>
            </div>
          )}
      {transport === "remote" && (
        <label>
          Bearer token (optional)
          <input name="token" type="password" autoComplete="off" spellCheck={false} />
        </label>
      )}
      {problem && <p className="connection-help" role="status">{problem}</p>}
      <div className="provider-buttons">
        <Button type="submit" size="sm" variant="primary" disabled={busy}>{busy ? "Connecting…" : "Add & connect"}</Button>
      </div>
    </form>
  );
}

function GatewayKey({ label, onConnect }: { label: string; onConnect: (key: string) => Promise<unknown> }) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const input = event.currentTarget.elements.namedItem("key");
    if (busy || !(input instanceof HTMLInputElement) || !input.value.trim()) return;
    const key = input.value.trim();
    input.value = "";
    setBusy(true);
    setError("");
    try {
      await onConnect(key);
    } catch (error) {
      setError(String(error));
    } finally {
      setBusy(false);
    }
  }

  return (
    <form className="provider-form" onSubmit={submit}>
      <label>{label}<input name="key" type="password" autoComplete="off" required /></label>
      <div className="provider-buttons">
        <Button size="sm" type="submit" variant="primary" disabled={busy}>{busy ? "Verifying…" : "Connect"}</Button>
      </div>
      {error && <p className="connection-error" role="alert">{error}</p>}
    </form>
  );
}

export function ModelsConnections() {
  const ask = useAsk();
  const mcp = useMcp();
  const [view, setView] = useState<(typeof views)[number]>("Connections");
  const [open, setOpen] = useState<string | null>(null);
  const [catalogQuery, setCatalogQuery] = useState("");
  const [gatewayFilter, setGatewayFilter] = useState("");
  const [checking, setChecking] = useState(false);
  const [receiptLimit, setReceiptLimit] = useState(12);
  const omniroute = ask.status?.gateways.find((gateway) => gateway.gateway === "omniroute");
  const openrouter = ask.status?.gateways.find((gateway) => gateway.gateway === "openrouter-direct");
  const catalog = ask.status?.catalog ?? [];
  const receipts = [...ask.receipts, ...mcp.receipts].sort((a, b) => receiptMillis(b.timestamp) - receiptMillis(a.timestamp));
  const registeredServers = mcp.servers.map((server) => ({ server, preset: serverPreset(server) }));
  const gatewayName = (id: string) => ask.status?.gateways.find((gateway) => gateway.gateway === id)?.label ?? id;
  const gatewayOptions = [...new Set(catalog.map((model) => model.gateway))];
  const filteredModels = catalog.filter((model) => (!gatewayFilter || model.gateway === gatewayFilter)
    && catalogQuery.trim().toLowerCase().split(/\s+/).every((term) =>
      (model.id + " " + gatewayName(model.gateway)).toLowerCase().includes(term)));
  const defaultModel = ask.status?.selection === "manual" ? ask.status.manual_pin : ask.status ? "auto" : null;
  const refresh = async () => {
    setChecking(true);
    try { await Promise.all([ask.refresh(), mcp.refresh()]); }
    finally { setChecking(false); }
  };

  function disclosure(id: string, label: string, name: string) {
    return (
      <Button size="sm" aria-label={(open === id ? "Close" : label) + ", " + name} aria-expanded={open === id}
        onClick={() => setOpen(open === id ? null : id)}>{open === id ? "Close" : label}</Button>
    );
  }

  function savedRow(server: McpServerStatus, mark?: string, readOnly = false) {
    const status = server.state === "ready" ? "Connected" : server.state === "auth_needed" ? "Sign-in needed"
      : server.state === "unsupported" ? "Unsupported" : server.state === "error" ? "Connection failed"
      : server.state === "connecting" ? "Connecting…" : "Disconnected";
    const tone = server.state === "ready" ? "connection-ready"
      : server.state === "error" || server.state === "unsupported" ? "connection-offline"
      : server.state === "auth_needed" ? "connection-attention" : "";
    const id = "server:" + server.id;
    const description = readOnly ? "Read-only endpoint · " + server.display
      : server.oauth_provider ? "Browser sign-in · " + server.display : server.display;
    return (
      <ConnectionRow key={id} name={server.name} mark={mark}
        description={description} status={status}
        tone={tone} action={disclosure(id, server.state === "ready" || server.state === "connecting" ? "Manage" : "Fix connection", server.name)}>
        {open === id && <ServerDetails server={server} mcp={mcp} />}
      </ConnectionRow>
    );
  }

  return (
    <div className="models-connections">
      <div className="connection-toolbar">
      <nav className="connection-nav" aria-label="Models and connections views">
        {views.map((name) => (
          <button key={name} type="button" aria-pressed={view === name} onClick={() => { setView(name); setOpen(null); }}>{name}</button>
        ))}
      </nav>
      <Button type="button" size="sm" variant="ghost" disabled={checking || mcp.refreshing} onClick={() => void refresh()}>
        {checking || mcp.refreshing ? "Checking status…" : "Refresh status"}
      </Button>
      </div>
      {view === "Connections" && (
        <>
          <section className="provider-section" aria-labelledby="integration-providers">
            <header className="provider-section-header">
              <div>
                <h3 id="integration-providers">Integrations</h3>
                <p>Connect tools and workspaces to PAIGE Desktop.</p>
              </div>
              {disclosure("custom", "Add custom server", "for MCP")}
            </header>
            <div className="provider-list">
              {presets.flatMap((preset) => {
                const saved = registeredServers.filter((entry) => entry.preset?.id === preset.id);
                const rows = saved.map(({ server, preset: connection }) => savedRow(server, preset.mark, connection?.readOnly));
                const configured = saved.some((entry) => entry.server.oauth_provider === preset.id);
                if (!configured) {
                  rows.push(
                    <ConnectionRow key={"setup:" + preset.id} {...preset} status={mcp.refreshing && !mcp.servers.length ? "Checking…" : mcp.error && !mcp.servers.length ? "Status unavailable" : "Not connected"}
                      action={disclosure(preset.id, saved.length ? "Set up browser sign-in" : "Connect", preset.name)}>
                      {open === preset.id && <ProviderSignIn provider={preset.id} mcp={mcp} onSaved={(id) => setOpen("server:" + id)} />}
                    </ConnectionRow>,
                  );
                }
                return rows;
              })}
              {registeredServers.filter((entry) => !entry.preset).map(({ server }) => savedRow(server))}
            </div>
            {open === "custom" && <div className="provider-custom"><McpSetup mcp={mcp} onSaved={(id) => setOpen("server:" + id)} /></div>}
            {mcp.error && <p className="connection-error" role="alert">{mcp.error}</p>}
          </section>

          <section className="provider-section" aria-labelledby="gateway-providers">
            <header className="provider-section-header">
              <div>
                <h3 id="gateway-providers">Model gateways</h3>
                <p>Choose where PAIGE sends model requests.</p>
              </div>
            </header>
            <div className="provider-list">
              <ConnectionRow name="OmniRoute" mark={omniMark} description="Preferred local gateway"
                status={!omniroute ? ask.error ? "Status unavailable" : "Checking…" : !omniroute.reachable ? "Offline" : omniroute.authenticated ? "Ready" : "Key needed"}
                tone={omniroute?.reachable && omniroute.authenticated ? "connection-ready" : omniroute && !omniroute.reachable ? "connection-offline" : omniroute ? "connection-attention" : ""}
                action={disclosure("omni", omniroute?.reachable && !omniroute.authenticated ? "Add key" : "Manage", "OmniRoute")}>
                {open === "omni" && (
                  <>
                    <p className="provider-endpoint">http://127.0.0.1:20128/v1</p>
                    <p className="connection-help">{omniroute?.detail ?? "Checking the gateway…"}</p>
                    {omniroute?.authenticated
                      ? <Button size="sm" onClick={ask.disconnectOmniroute}>Disconnect</Button>
                      : <GatewayKey label="OmniRoute endpoint key" onConnect={ask.connectOmniroute} />}
                  </>
                )}
              </ConnectionRow>
              <ConnectionRow name="OpenRouter" mark={routerLightMark} darkMark={routerDarkMark} description="Alternate model gateway"
                status={!openrouter ? ask.status || ask.error ? "Status unavailable" : "Checking…"
                  : !openrouter.reachable ? "Offline" : openrouter.authenticated ? "Ready" : "Key not verified"}
                tone={openrouter?.reachable && openrouter.authenticated ? "connection-ready"
                  : openrouter && !openrouter.reachable ? "connection-offline" : openrouter ? "connection-attention" : ""}
                action={disclosure("router", openrouter?.reachable && !openrouter.authenticated ? "Add key" : "Manage", "OpenRouter")}>
                {open === "router" && (
                  <>
                    {openrouter?.detail && <p className="connection-help">{openrouter.detail}</p>}
                    {ask.status?.openrouter_connected ? (
                      <>
                        <p className="connection-help">{openrouter?.catalog_count ?? 0} live models</p>
                        <Button size="sm" onClick={ask.disconnectOpenrouter}>Disconnect</Button>
                      </>
                    ) : <GatewayKey label="OpenRouter API key" onConnect={ask.connectOpenrouter} />}
                    {ask.error && <p className="connection-error" role="alert">{ask.error}</p>}
                  </>
                )}
              </ConnectionRow>
            </div>
          </section>
        </>
      )}
      {view === "Models" && (
        <section className="provider-section">
          <div className="provider-section-header connection-default-model">
            <div>
              <h3>Default model</h3>
              <p>Chats use this setting until you choose a model in that chat.</p>
            </div>
            <ModelPicker status={ask.status} choice={defaultModel} disabled={!ask.status}
              onChange={(choice) => ask.setModelSelection(choice === "auto" ? "auto" : "manual", choice === "auto" ? null : choice)} />
          </div>
          {ask.error && <p className="connection-error" role="alert">{ask.error}</p>}
          <header className="provider-section-header"><h3>Qualified models</h3><span className="connection-status">{filteredModels.length} of {catalog.length}</span></header>
          {catalog.length > 0 && <div className="connection-catalog-controls">
            <input aria-label="Search models" type="search" placeholder="Search models or providers" value={catalogQuery}
              onChange={(event) => setCatalogQuery(event.target.value)} />
            <select aria-label="Filter by gateway" value={gatewayFilter} onChange={(event) => setGatewayFilter(event.target.value)}>
              <option value="">All gateways</option>
              {gatewayOptions.map((gateway) => <option key={gateway} value={gateway}>{gatewayName(gateway)}</option>)}
            </select>
          </div>}
          <div className="provider-list">
            {filteredModels.map((model) => (
              <div className="connection-record" key={model.gateway + "::" + model.id}>
                <code>{model.id}</code>
                <span>{gatewayName(model.gateway)} · Tier {model.coding_tier}/5 · {Math.round(model.context_bytes / 1024)} KB{model.tools && " · Tools"}</span>
              </div>
            ))}
            {!filteredModels.length && <p className="connection-record">{catalog.length ? "No models match these filters." : ask.status ? "Connect a model gateway to load its catalog." : ask.error ? "The model catalog is unavailable." : "Checking model gateways…"}</p>}
          </div>
        </section>
      )}
      {view === "Receipts" && (
        <section className="provider-section">
          <header className="provider-section-header"><h3>Recent receipts</h3><span className="connection-status">{Math.min(receiptLimit, receipts.length)} of {receipts.length}</span></header>
          <div className="provider-list">
            {receipts.slice(0, receiptLimit).map((receipt, index) => (
              <details className="connection-receipt" key={receipt.timestamp + ":" + index}>
                <summary className="connection-record">
                  <strong>{receipt.tool.replace(/_/g, " ")}</strong>
                  <span>{receipt.outcome.replace(/_/g, " ")} · {new Date(receiptMillis(receipt.timestamp))
                    .toLocaleString([], { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" })}</span>
                </summary>
                <div className="connection-receipt-body">
                  <p>{receipt.summary}</p>
                  <small>{"connection_id" in receipt ? receipt.connection_id : [receipt.model, receipt.gateway].filter(Boolean).join(" · ")}</small>
                  {receipt.tool === "model_round" && <ModelRoundFacts event={receipt} />}
                </div>
              </details>
            ))}
            {!receipts.length && <p className="connection-record">Model and MCP receipts appear here after a request runs.</p>}
          </div>
          {receiptLimit < receipts.length && <Button size="sm" variant="ghost" onClick={() => setReceiptLimit((limit) => limit + 12)}>Show more receipts</Button>}
        </section>
      )}
    </div>
  );
}
