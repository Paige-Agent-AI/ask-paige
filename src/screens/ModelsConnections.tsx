import { useState } from "react";
import { Button } from "../components/Button";
import { Card } from "../components/Card";
import { CpuIcon, LockIcon } from "../components/Icons";
import { useAsk } from "../hooks/useAsk";
import { useMcp } from "../hooks/useMcp";
import "./ModelsConnections.css";

export function ModelsConnections() {
  const ask = useAsk();
  const mcp = useMcp();
  const [keyInput, setKeyInput] = useState("");
  const [connecting, setConnecting] = useState(false);

  // MCP add-server form state
  const [mcpTab, setMcpTab] = useState<"stdio" | "remote">("stdio");
  const [mcpId, setMcpId] = useState("");
  const [mcpName, setMcpName] = useState("");
  const [mcpCommand, setMcpCommand] = useState("");
  const [mcpArgs, setMcpArgs] = useState("");
  const [mcpUrl, setMcpUrl] = useState("");
  const [mcpToken, setMcpToken] = useState("");
  const [mcpAdding, setMcpAdding] = useState(false);

  const omniroute = ask.status?.gateways.find((g) => g.gateway === "omniroute");
  const openrouter = ask.status?.gateways.find((g) => g.gateway === "openrouter-direct");

  const connect = async () => {
    if (!keyInput.trim()) return;
    setConnecting(true);
    const ok = await ask.connectOpenrouter(keyInput.trim());
    if (ok) setKeyInput("");
    setConnecting(false);
  };

  const addMcp = async () => {
    if (!mcpId.trim() || !mcpName.trim()) return;
    setMcpAdding(true);
    try {
      if (mcpTab === "stdio") {
        if (!mcpCommand.trim()) return;
        const args = mcpArgs
          .trim()
          .split(/\s+/)
          .filter((a) => a.length > 0);
        await mcp.addStdio(mcpId.trim(), mcpName.trim(), mcpCommand.trim(), args, {}, true);
      } else {
        if (!mcpUrl.trim()) return;
        const authHeader = mcpToken.trim() ? `Bearer ${mcpToken.trim()}` : undefined;
        await mcp.addRemote(mcpId.trim(), mcpName.trim(), mcpUrl.trim(), authHeader, undefined, true);
      }
      setMcpId("");
      setMcpName("");
      setMcpCommand("");
      setMcpArgs("");
      setMcpUrl("");
      setMcpToken("");
    } finally {
      setMcpAdding(false);
    }
  };

  const statusClass = (state: string) => {
    if (state === "ready") return "connection-status connection-status-ready";
    if (state === "offline" || state === "unsupported") return "connection-status connection-status-off";
    return "connection-status connection-status-pending";
  };

  return (
    <div className="models-connections">
      <div className="models-connections-hero">
        <CpuIcon />
        <h2>Models & Connections</h2>
        <p>
          Truthful gateway, model and connection status for PAIGE Desktop's
          engineering inference routes and MCP servers.
        </p>
      </div>

      <div className="models-connections-grid">
        <Card className="connection-card">
          <div className="connection-card-header">
            <h3>OmniRoute</h3>
            <span
              className={
                omniroute?.reachable
                  ? omniroute?.authenticated
                    ? "connection-status connection-status-ready"
                    : "connection-status connection-status-pending"
                  : "connection-status connection-status-off"
              }
            >
              {omniroute?.reachable
                ? omniroute?.authenticated
                  ? "Ready"
                  : "Auth needed"
                : "Offline"}
            </span>
          </div>
          <p className="connection-card-desc">
            Owner-maintained local engineering gateway (preferred route) at
            127.0.0.1:20128. Streams Claude/OpenAI-compatible traffic to the
            owner's qualified upstream models.
          </p>
          <div className="connection-card-detail">
            <span>Endpoint</span>
            <span className="connection-card-value">http://127.0.0.1:20128/v1</span>
          </div>
          <div className="connection-card-detail">
            <span>Status</span>
            <span className="connection-card-value">{omniroute?.detail ?? "probing…"}</span>
          </div>
          <div className="connection-card-detail">
            <span>Catalog</span>
            <span className="connection-card-value">
              {omniroute ? `${omniroute.catalog_count} live` : "—"}
            </span>
          </div>
          {omniroute && !omniroute.authenticated && (
            <p className="connection-card-note">
              The OmniRoute server is installed and startable. Its stored upstream
              credential could not be decrypted (STORAGE_ENCRYPTION_KEY missing
              from the OmniRoute environment), so inference returns a truthful
              upstream 401. Restore the owner's STORAGE_ENCRYPTION_KEY in the
              OmniRoute .env to reactivate this gateway.
            </p>
          )}
        </Card>

        <Card className="connection-card">
          <div className="connection-card-header">
            <h3>OpenRouter</h3>
            <span
              className={
                ask.status?.openrouter_connected
                  ? "connection-status connection-status-ready"
                  : "connection-status connection-status-pending"
              }
            >
              {ask.status?.openrouter_connected ? "Connected" : "Not connected"}
            </span>
          </div>
          <p className="connection-card-desc">
            Direct OpenRouter connection as the alternate gateway. Owner-maintained
            credits and spending limits. The key is stored encrypted at rest
            (Windows DPAPI) outside the repository and never exposed to the page.
          </p>
          {ask.status?.openrouter_connected ? (
            <>
              <div className="connection-card-detail">
                <span>Catalog</span>
                <span className="connection-card-value">{openrouter?.catalog_count ?? "—"} live models</span>
              </div>
              <div className="connection-ok">Connected — key verified live against OpenRouter's authenticated credits endpoint and stored DPAPI-encrypted on this machine.</div>
              <Button variant="secondary" size="sm" onClick={ask.disconnectOpenrouter}>
                Disconnect
              </Button>
            </>
          ) : (
            <>
              <div className="connection-keyrow">
                <input
                  type="password"
                  className="connection-keyinput"
                  placeholder="sk-or-v1-…"
                  value={keyInput}
                  onChange={(e) => setKeyInput(e.target.value)}
                />
                <Button variant="primary" size="sm" onClick={connect} disabled={connecting || !keyInput.trim()}>
                  {connecting ? "Verifying…" : "Connect"}
                </Button>
              </div>
              <p className="connection-card-note">
                Connect with the owner's OpenRouter key. The key is verified live
                against OpenRouter's authenticated credits endpoint before it is
                stored (the public model catalog cannot validate a key).
              </p>
              {ask.error && <div className="connection-error">{ask.error}</div>}
              {!ask.error && connecting && <div className="connection-ok">Verifying the key against OpenRouter…</div>}
            </>
          )}
        </Card>

        <Card className="connection-card mcp-card">
          <div className="connection-card-header">
            <h3>MCP Servers</h3>
            <span className="connection-status connection-status-pending">{mcp.servers.length}</span>
          </div>
          <p className="connection-card-desc">
            Native Model Context Protocol client connections. Add local stdio
            servers or remote HTTPS servers, discover tools/resources/prompts,
            and execute tools with verifiable receipts.
          </p>

          <div className="mcp-servers">
            {mcp.servers.map((srv) => (
              <div key={srv.id} className="mcp-server-row">
                <div className="mcp-server-meta">
                  <code>{srv.name}</code>
                  <span className={statusClass(srv.state)}>{srv.state}</span>
                </div>
                <div className="mcp-server-detail">{srv.display}</div>
                <div className="mcp-server-detail">
                  {srv.state === "ready"
                    ? `${srv.capabilities.tools.length} tools · ${srv.capabilities.resources.length} resources · ${srv.capabilities.prompts.length} prompts`
                    : srv.detail}
                </div>
                <div className="mcp-server-actions">
                  {srv.state === "ready" ? (
                    <Button variant="secondary" size="sm" onClick={() => mcp.disconnect(srv.id)}>
                      Disconnect
                    </Button>
                  ) : (
                    <Button variant="primary" size="sm" onClick={() => mcp.connect(srv.id)}>
                      Connect
                    </Button>
                  )}
                  <Button variant="secondary" size="sm" onClick={() => mcp.reconnect(srv.id)}>
                    Reconnect
                  </Button>
                  <Button variant="danger" size="sm" onClick={() => mcp.revoke(srv.id)}>
                    Revoke
                  </Button>
                </div>
              </div>
            ))}
            {mcp.servers.length === 0 && (
              <p className="connection-card-note">No MCP servers configured yet.</p>
            )}
          </div>

          <div className="mcp-add">
            <div className="mcp-tabs">
              <button className={mcpTab === "stdio" ? "active" : ""} onClick={() => setMcpTab("stdio")} type="button">
                stdio
              </button>
              <button className={mcpTab === "remote" ? "active" : ""} onClick={() => setMcpTab("remote")} type="button">
                remote HTTPS
              </button>
            </div>
            {mcpTab === "stdio" ? (
              <>
                <div className="connection-keyrow">
                  <input
                    className="connection-keyinput"
                    placeholder="id"
                    value={mcpId}
                    onChange={(e) => setMcpId(e.target.value)}
                  />
                  <input
                    className="connection-keyinput"
                    placeholder="display name"
                    value={mcpName}
                    onChange={(e) => setMcpName(e.target.value)}
                  />
                </div>
                <div className="connection-keyrow">
                  <input
                    className="connection-keyinput"
                    placeholder="command (e.g. node)"
                    value={mcpCommand}
                    onChange={(e) => setMcpCommand(e.target.value)}
                  />
                  <input
                    className="connection-keyinput"
                    placeholder="args (space separated)"
                    value={mcpArgs}
                    onChange={(e) => setMcpArgs(e.target.value)}
                  />
                </div>
              </>
            ) : (
              <>
                <div className="connection-keyrow">
                  <input
                    className="connection-keyinput"
                    placeholder="id"
                    value={mcpId}
                    onChange={(e) => setMcpId(e.target.value)}
                  />
                  <input
                    className="connection-keyinput"
                    placeholder="display name"
                    value={mcpName}
                    onChange={(e) => setMcpName(e.target.value)}
                  />
                </div>
                <div className="connection-keyrow">
                  <input
                    className="connection-keyinput"
                    placeholder="https://host/mcp"
                    value={mcpUrl}
                    onChange={(e) => setMcpUrl(e.target.value)}
                  />
                  <input
                    type="password"
                    className="connection-keyinput"
                    placeholder="bearer token"
                    value={mcpToken}
                    onChange={(e) => setMcpToken(e.target.value)}
                  />
                </div>
              </>
            )}
            <Button
              variant="primary"
              size="sm"
              onClick={addMcp}
              disabled={
                mcpAdding ||
                !mcpId.trim() ||
                !mcpName.trim() ||
                (mcpTab === "stdio" ? !mcpCommand.trim() : !mcpUrl.trim())
              }
            >
              {mcpAdding ? "Adding…" : "Add & connect"}
            </Button>
          </div>

          {mcp.error && <div className="connection-error">{mcp.error}</div>}
        </Card>

        <Card className="connection-card">
          <div className="connection-card-header">
            <h3>Qualified models</h3>
            <span className="connection-status connection-status-pending">
              {(ask.status?.catalog ?? []).length} candidates
            </span>
          </div>
          <div className="connection-models">
            {(ask.status?.catalog ?? []).map((c) => (
              <div key={c.id} className="connection-model">
                <code>{c.id}</code>
                <span className="connection-model-meta">
                  {c.gateway} · tier {c.coding_tier}/5 · {Math.round(c.context_bytes / 1024)}KB
                  {c.tools ? " · tools" : ""}
                </span>
              </div>
            ))}
            {(ask.status?.catalog ?? []).length === 0 && (
              <p className="connection-card-note">No qualified models yet — connect a gateway.</p>
            )}
          </div>
        </Card>

        <Card className="connection-card">
          <div className="connection-card-header">
            <h3>Receipts</h3>
            <span className="connection-status connection-status-pending">{ask.receipts.length + mcp.receipts.length}</span>
          </div>
          <div className="connection-receipts">
            {[...ask.receipts, ...mcp.receipts]
              .sort((a, b) => b.timestamp - a.timestamp)
              .slice(0, 12)
              .map((r, i) => (
                <div key={i} className="connection-receipt">
                  <code>{"connection_id" in r ? `mcp:${r.tool}` : r.tool}</code>
                  <span className="connection-receipt-meta">
                    {"connection_id" in r ? (r as { connection_id: string }).connection_id : r.model} · {r.outcome} · {r.summary.slice(0, 80)}
                  </span>
                </div>
              ))}
            {ask.receipts.length === 0 && mcp.receipts.length === 0 && (
              <p className="connection-card-note">No inference or MCP receipts yet this session.</p>
            )}
          </div>
        </Card>
      </div>

      <Card className="models-connections-locked">
        <div className="models-connections-locked-icon">
          <LockIcon />
        </div>
        <h3>Gateway scope</h3>
        <p>
          PAIGE Desktop's inference layer is a fixed table: OmniRoute (preferred)
          and direct OpenRouter (alternate), plus an explicitly labeled local
          harness used only by automated tests. MCP connections are managed as
          independent client attachments with per-server credentials and
          receipts — not a second orchestration registry.
        </p>
      </Card>
    </div>
  );
}
