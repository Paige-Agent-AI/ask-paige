import { Button } from "../components/Button";
import { Card } from "../components/Card";
import { LockIcon, UsersIcon } from "../components/Icons";
import "./AgentTeam.css";

const agents = [
  {
    id: "coding",
    name: "Coding Agent",
    role: "Implements and reviews code in isolated worktrees.",
    status: "not configured",
  },
  {
    id: "research",
    name: "Research Agent",
    role: "Browses documentation and produces source-cited reports.",
    status: "not configured",
  },
  {
    id: "review",
    name: "Independent Reviewer",
    role: "Non-author security and correctness review.",
    status: "spawned on demand",
  },
];

export function AgentTeam() {
  return (
    <div className="agent-team">
      <div className="agent-team-hero">
        <UsersIcon />
        <h2>Agent Team</h2>
        <p>
          Coordinate coding, research, testing and independent-review agents on
          owner-selected engineering objectives.
        </p>
      </div>

      <div className="agent-team-grid">
        {agents.map((a) => (
          <Card key={a.id} className="agent-card">
            <div className="agent-card-header">
              <h3>{a.name}</h3>
              <span className={`agent-card-status agent-card-status-${a.status === "spawned on demand" ? "ready" : "pending"}`}>
                {a.status}
              </span>
            </div>
            <p>{a.role}</p>
            <Button variant="secondary" size="sm" disabled>
              Configure
            </Button>
          </Card>
        ))}
      </div>

      <Card className="agent-team-locked">
        <div className="agent-team-locked-icon">
          <LockIcon />
        </div>
        <h3>Multi-agent orchestration is not activated</h3>
        <p>
          Agent team coordination requires separate owner authorization and
          integration with PAIGE's orchestration layer. This screen prepares the
          UI only.
        </p>
      </Card>
    </div>
  );
}
