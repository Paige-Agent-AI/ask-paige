import type { ExecutionEvent } from "../types/chats";
import { readReceipt } from "./WorkbenchReview";

export function isWorkbenchPlan(event: ExecutionEvent) {
  return event.kind === "plan" && event.tool === "submit_workbench_plan" && event.outcome === "ok";
}

type PlanEvidence = {
  project: string;
  plan: {
    goal: string;
    approach: string;
    files: string[];
    test_command: string;
    expected_result: string;
    research_note: string;
  };
  inspected: { path: string; sha256: string }[];
  sources: { title: string; url: string; sha256: string }[];
};

export function WorkbenchPlan({ events, preparing }: { events: ExecutionEvent[]; preparing: boolean }) {
  const accepted = events.slice().reverse().find(isWorkbenchPlan);
  if (!accepted) {
    const issue = events.slice().reverse().find((event) => event.kind === "plan" && event.outcome === "error");
    return <p className="ask-sidepanel-empty" role={preparing ? "status" : undefined}>
      {issue?.summary ?? (preparing
        ? "PAIGE is gathering evidence for this task."
        : "Saved plans for this conversation appear here.")}
    </p>;
  }
  const { plan, project, inspected, sources } = readReceipt(accepted) as unknown as PlanEvidence;
  if (!plan || !Array.isArray(plan.files) || !Array.isArray(inspected) || !Array.isArray(sources)) {
    return <p className="ask-sidepanel-empty">This saved plan could not be read.</p>;
  }

  const result = events.slice().reverse().find((event) =>
    event.turn_id === accepted.turn_id && event.timestamp >= accepted.timestamp &&
    event.kind === "tool" && event.tool === "run_command" &&
    readReceipt(event).command === plan.test_command);
  const verification = result ? readReceipt(result) : undefined;
  const exit = verification?.exit_code;

  return <article className="ask-plan" aria-label="Saved Workbench plan">
    <header>
      <h2>{plan.goal}</h2>
      <p className="ask-wrap-path">{project}</p>
    </header>
    <section aria-label="Approach">
      <h3>Approach</h3>
      <p>{plan.approach}</p>
      <ul>{plan.files.map((file) => <li key={file}><code>{file}</code></li>)}</ul>
    </section>
    <section aria-label="Verification">
      <h3>Verification</h3>
      <code className="ask-plan-command">{plan.test_command}</code>
      <p>{plan.expected_result}</p>
      {verification?.timed_out === true
        ? <p className="ask-plan-result">The latest test run timed out.</p>
        : typeof exit === "number" && <p className="ask-plan-result">Latest test exit code {exit}</p>}
    </section>
    <section aria-label="Evidence">
      <h3>Evidence</h3>
      <ul>{inspected.map(({ path, sha256 }) => <li key={path}>
        <code>{path}</code><small title={sha256}>File read · {sha256.slice(0, 12)}</small>
      </li>)}</ul>
      <p>{plan.research_note}</p>
      {sources.length > 0 && <ul>{sources.map((source) => <li key={source.url}>
        <strong>{source.title}</strong><small className="ask-wrap-path">{source.url}</small>
      </li>)}</ul>}
    </section>
  </article>;
}
