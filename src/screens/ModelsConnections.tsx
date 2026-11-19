import { Button } from "../components/Button";
import { Card } from "../components/Card";
import { CpuIcon, LockIcon } from "../components/Icons";
import "./ModelsConnections.css";

export function ModelsConnections() {
  return (
    <div className="models-connections">
      <div className="models-connections-hero">
        <CpuIcon />
        <h2>Models & Connections</h2>
        <p>
          Truthful gateway, model and connection status for PAIGE Desktop's
          coding inference routes.
        </p>
      </div>

      <div className="models-connections-grid">
        <Card className="connection-card">
          <div className="connection-card-header">
            <h3>OmniRoute</h3>
            <span className="connection-status connection-status-pending">
              Not verified
            </span>
          </div>
          <p className="connection-card-desc">
            Owner-maintained private locally hosted router. Preferred engineering
            gateway for OpenCode and compatible coding clients.
          </p>
          <div className="connection-card-detail">
            <span>Endpoint</span>
            <span className="connection-card-value">Not configured</span>
          </div>
          <div className="connection-card-detail">
            <span>Qualified models</span>
            <span className="connection-card-value">Pending discovery</span>
          </div>
          <Button variant="secondary" size="sm" disabled>
            Verify connection
          </Button>
        </Card>

        <Card className="connection-card">
          <div className="connection-card-header">
            <h3>OpenRouter</h3>
            <span className="connection-status connection-status-pending">
              Not verified
            </span>
          </div>
          <p className="connection-card-desc">
            Direct OpenRouter connection as alternate gateway. Owner-maintained
            credits and spending limits.
          </p>
          <div className="connection-card-detail">
            <span>Key status</span>
            <span className="connection-card-value">Not configured</span>
          </div>
          <div className="connection-card-detail">
            <span>Spend cap</span>
            <span className="connection-card-value">Owner-managed</span>
          </div>
          <Button variant="secondary" size="sm" disabled>
            Verify connection
          </Button>
        </Card>

        <Card className="connection-card">
          <div className="connection-card-header">
            <h3>OpenCode</h3>
            <span className="connection-status connection-status-ready">
              Active client
            </span>
          </div>
          <p className="connection-card-desc">
            Current Desktop engineering client. Routing to OmniRoute or
            OpenRouter requires separate configuration and testing.
          </p>
          <div className="connection-card-detail">
            <span>Route</span>
            <span className="connection-card-value">Default provider</span>
          </div>
          <div className="connection-card-detail">
            <span>Compatibility</span>
            <span className="connection-card-value">To be determined</span>
          </div>
        </Card>

        <Card className="connection-card">
          <div className="connection-card-header">
            <h3>Codex</h3>
            <span className="connection-status connection-status-pending">
              Not verified
            </span>
          </div>
          <p className="connection-card-desc">
            Codex compatibility with custom OmniRoute base URLs and usage caps
            must be tested separately without disrupting authentication.
          </p>
          <div className="connection-card-detail">
            <span>Custom base URL</span>
            <span className="connection-card-value">Untested</span>
          </div>
          <Button variant="secondary" size="sm" disabled>
            Test compatibility
          </Button>
        </Card>
      </div>

      <Card className="models-connections-locked">
        <div className="models-connections-locked-icon">
          <LockIcon />
        </div>
        <h3>Inference gateway integration is not activated</h3>
        <p>
          Model routing, usage attribution, conversation continuity and safe
          recovery will be implemented in a separate authorized engineering
          increment. This screen shows the planned connection surface only.
        </p>
      </Card>
    </div>
  );
}
