import { useEffect, useState, type FormEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "../components/Button";
import type { Chat, TeamMember } from "../types/chats";
import type { McpStatus } from "../types/mcp";

function hasChatModelRoute(schema: Record<string, unknown>) {
  const route = (schema?.properties as Record<string, unknown> | undefined)?.model_route as
    { type?: string; properties?: { gateway?: { type?: string }; model?: { type?: string } } } | undefined;
  return route?.type === "object" && route.properties?.gateway?.type === "string" &&
    route.properties?.model?.type === "string";
}

export function TeamPicker({ chatId, project, model, onCreate, onCancel }: {
  chatId: string | null;
  project: string | null;
  model: string;
  onCreate: (chat: Chat) => void;
  onCancel: () => void;
}) {
  const [status, setStatus] = useState<McpStatus | null>(null);
  const [connection, setConnection] = useState("");
  const [members, setMembers] = useState<TeamMember[] | null>(null);
  const [selected, setSelected] = useState<string[]>([]);
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState("");
  const [addingNamed, setAddingNamed] = useState(false);
  const [namedError, setNamedError] = useState("");
  const busy = creating || addingNamed;
  const candidates = status?.servers.filter((server) => server.state === "ready" &&
    server.capabilities.tools.some((tool) => tool.name === "list_subagents")) ?? [];
  const activeConnection = connection || (candidates.length === 1 ? candidates[0].id : "");
  const server = candidates.find((candidate) => candidate.id === activeConnection);
  const routeReady = server?.capabilities.tools.some((tool) =>
    tool.name === "delegate_to_subagent" && hasChatModelRoute(tool.input_schema));

  useEffect(() => {
    let current = true;
    void invoke<McpStatus>("mcp_status").then((value) => {
      if (current) setStatus(value);
    }).catch((reason) => { if (current) setError(String(reason)); });
    return () => { current = false; };
  }, []);

  useEffect(() => {
    if (!activeConnection) return;
    let current = true;
    void invoke<TeamMember[]>("team_roster", { connectionId: activeConnection }).then((value) => {
      if (current) setMembers(value);
    }).catch((reason) => { if (current) setError(String(reason)); });
    return () => { current = false; };
  }, [activeConnection]);

  const create = async () => {
    setCreating(true);
    setError("");
    try {
      onCreate(await invoke<Chat>("team_create", { connectionId: activeConnection, slugs: selected, chatId }));
    } catch (reason) {
      setError(String(reason));
      setCreating(false);
    }
  };

  const addNamed = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!chatId || !project || busy) return;
    const fields = new FormData(event.currentTarget);
    setAddingNamed(true);
    setNamedError("");
    try {
      const chat = await invoke<Chat>("team_name_specialist", {
        chatId,
        name: String(fields.get("name") ?? ""),
        role: String(fields.get("role") ?? ""),
      });
      onCreate(chat);
    } catch (reason) {
      setNamedError(String(reason));
      setAddingNamed(false);
    }
  };

  return (
    <section className="ask-team-picker" aria-labelledby="ask-team-heading">
      <header>
        <h2 id="ask-team-heading">Add specialists</h2>
        <p>Specialists use this chat’s model setting. <strong>{model}</strong></p>
      </header>
      <form className="ask-team-named" onSubmit={addNamed} aria-labelledby="ask-team-named-title">
        <h3 id="ask-team-named-title">Name a specialist for this chat</h3>
        {chatId && project ? (
          <>
            <p>Choose a name and a role. Use the specialist’s @handle in this chat to hand off a task.</p>
            <label>Name<input name="name" maxLength={48} required placeholder="Example: Test Reviewer" disabled={busy} /></label>
            <label>Role<input name="role" maxLength={500} required placeholder="Example: Check the change and run regression tests" disabled={busy} /></label>
            {namedError && <p role="alert">{namedError}</p>}
            <Button type="submit" variant="primary" disabled={busy}>
              {addingNamed ? "Adding specialist…" : "Add named specialist"}
            </Button>
          </>
        ) : <p>Pin a project to the chat to add a named specialist.</p>}
      </form>
      {!status && !error && <p role="status">Checking PAIGE connections…</p>}
      {status && candidates.length === 0 && (
        <div className="ask-team-connect">
          <p>Sign in to browse the shared PAIGE specialist roster.</p>
          <Button onClick={() => {
            onCancel();
            window.dispatchEvent(new CustomEvent("paige-navigate", { detail: "models" }));
          }}>
            Models &amp; Connections
          </Button>
        </div>
      )}
      {candidates.length > 1 && (
        <label className="ask-team-connection">
          Connection
          <select value={activeConnection} disabled={busy} onChange={(event) => {
            setMembers(null);
            setSelected([]);
            setError("");
            setConnection(event.target.value);
          }}>
            <option value="">Choose a connection</option>
            {candidates.map((server) => <option key={server.id} value={server.id}>{server.name}</option>)}
          </select>
        </label>
      )}
      {server && !routeReady && <p role="status">Live handoffs need an orchestrator that supports the chat’s model route.</p>}
      {activeConnection && !members && !error && <p role="status">Loading specialists…</p>}
      {members?.length === 0 && <p>This connection has no prompt-only specialists available.</p>}
      {members && members.length > 0 && (
        <fieldset className="ask-team-members" disabled={busy}>
          <legend className="ask-team-legend">Choose two to six specialists</legend>
          {members.map((member) => (
            <label key={member.slug} className="ask-team-member">
              <input type="checkbox" checked={selected.includes(member.slug)}
                disabled={selected.length === 6 && !selected.includes(member.slug)}
                onChange={(event) => setSelected(event.target.checked
                  ? [...selected, member.slug]
                  : selected.filter((slug) => slug !== member.slug))} />
              <span><strong>{member.name}</strong><small>@{member.slug}</small></span>
            </label>
          ))}
        </fieldset>
      )}
      {error && <p role="alert">{error}</p>}
      <footer>
        <Button onClick={onCancel} disabled={busy}>Cancel</Button>
        <Button variant="primary" onClick={create} disabled={selected.length < 2 || busy}>
          {creating ? "Adding…" : "Add to chat"}
        </Button>
      </footer>
    </section>
  );
}
