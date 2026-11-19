import { Button } from "../components/Button";
import { Card } from "../components/Card";
import { FolderIcon, LockIcon } from "../components/Icons";
import "./Projects.css";

const projects = [
  {
    id: "lab",
    name: "PAIGE Desktop Lab",
    path: "C:\\Users\\tonig\\paige-desktop-lab",
    status: "Authorized",
    description: "Isolated sandbox for broker demonstrations and recovery tests.",
  },
];

export function Projects() {
  return (
    <div className="projects">
      <div className="projects-toolbar">
        <Button variant="primary" disabled>
          Open local folder
        </Button>
        <Button variant="secondary" disabled>
          Clone repository
        </Button>
      </div>

      <div className="projects-grid">
        {projects.map((p) => (
          <Card key={p.id} className="project-card">
            <div className="project-card-header">
              <FolderIcon />
              <div>
                <h3 className="project-card-name">{p.name}</h3>
                <p className="project-card-path">{p.path}</p>
              </div>
            </div>
            <p className="project-card-desc">{p.description}</p>
            <div className="project-card-footer">
              <span className="project-card-status project-card-status-active">
                {p.status}
              </span>
              <Button variant="secondary" size="sm">
                Open
              </Button>
            </div>
          </Card>
        ))}

        <Card className="project-card project-card-placeholder">
          <div className="project-card-header">
            <LockIcon />
            <div>
              <h3 className="project-card-name">Add a project</h3>
              <p className="project-card-path">Local folders and Git repos</p>
            </div>
          </div>
          <p className="project-card-desc">
            Project discovery, Git clone and worktree management require
            separate authorization.
          </p>
          <div className="project-card-footer">
            <span className="project-card-status project-card-status-pending">
              Not available
            </span>
            <Button variant="ghost" size="sm" disabled>
              Coming soon
            </Button>
          </div>
        </Card>
      </div>
    </div>
  );
}
