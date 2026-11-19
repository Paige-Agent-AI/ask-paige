import { useState } from "react";
import { Button } from "../components/Button";
import { Card } from "../components/Card";
import { CpuIcon, LockIcon } from "../components/Icons";
import { useAsk } from "../hooks/useAsk";
import "./ModelsConnections.css";

export function ModelsConnections() {
  const ask = useAsk();
  const [keyInput, setKeyInput] = useState("");
  const [connecting, setConnecting] = useState(false);

  const omniroute = ask.status?.gateways.find((g) => g.gateway === "omniroute");
  const openrouter = ask.status?.gateways.find((g) => g.gateway === "openrouter-direct");

  const connect = async () => {
    if (!keyInput.trim()) return;
    setConnecting(true);
    const ok = await ask.connectOpenrouter(keyInput.trim());
    if (ok) setKeyInput("");
    setConnecting(false);
  };

  return (
    <div className="models-connections">
      <div className="models-connections-hero">
        <CpuIcon />
        <h2>Models & Connections</h2>
        <p>
          Truthful gateway, model and connection status for PAIGE Desktop's
          engineering inference routes.
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
                Connect with the owner's OpenRouter key. The connection is verified
                live against the OpenRouter catalog before it is stored.
              </p>
            </>
          )}
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
            <span className="connection-status connection-status-pending">{ask.receipts.length}</span>
          </div>
          <div className="connection-receipts">
            {ask.receipts.slice(-8).reverse().map((r, i) => (
              <div key={i} className="connection-receipt">
                <code>{r.tool}</code>
                <span className="connection-receipt-meta">
                  {r.model} · {r.outcome} · {r.summary.slice(0, 80)}
                </span>
              </div>
            ))}
            {ask.receipts.length === 0 && (
              <p className="connection-card-note">No inference receipts yet this session.</p>
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
          harness used only by automated tests. This is not a general provider
          registry. Native MCP client connections arrive in Increment 1D.
        </p>
      </Card>
    </div>
  );
}
