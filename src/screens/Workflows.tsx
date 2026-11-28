import { useEffect, useState } from "react";
import { invoke } from "../adapter/core";
import { Button } from "../components/Button";
import type { Chat, ExecutionEvent, WorkflowRun, WorkflowTemplate } from "../types/chats";

export const workflowTemplates = {
  engineering_v1: {
    name: "Engineering change",
    description: "Inspect a worktree, propose an edit, and capture the diff and test results.",
    prompt: "Describe the change and how you want it tested.",
    steps: ["Inspect the isolated worktree", "Propose and approve a targeted change", "Capture the diff and test receipts", "Independent review and owner acceptance"],
  },
  visual_qa_v1: {
    name: "Visual QA",
    description: "Correct a design issue and compare the before and after screenshots.",
    prompt: "Describe what to inspect and how to test the correction.",
    steps: ["Inspect an authorized screenshot", "Propose and verify the correction", "Compare before and after", "Independent review and owner acceptance"],
  },
  research_v1: {
    name: "Research to finished artifact",
    description: "Read official sources and create an editable Word report.",
    prompt: "Ask about Rust tests, Node.js tests, Tauri commands, or React state.",
    steps: ["Read official sources", "Review the proposed Word report", "Verify and preview the saved file", "Owner acceptance"],
  },
} satisfies Record<WorkflowTemplate, { name: string; description: string; prompt: string; steps: string[] }>;

function receiptDetails(event: ExecutionEvent): Record<string, unknown> {
  try { return JSON.parse(event.details) ?? {}; }
  catch { return {}; }
}

interface WordReport {
  title: string;
  blocks: { style: "heading" | "bullet" | "paragraph"; text: string }[];
  sources: { title: string; url: string; sha256: string }[];
}

export function readWordReport(value: unknown): WordReport | null {
  if (!value || typeof value !== "object") return null;
  const { title, blocks, sources } = value as Partial<WordReport>;
  if (typeof title !== "string" || !Array.isArray(blocks) || !Array.isArray(sources)
    || !blocks.every((block) => typeof block?.text === "string" && ["heading", "bullet", "paragraph"].includes(block.style))
    || !sources.every((source) => typeof source?.title === "string" && typeof source.url === "string"
      && typeof source.sha256 === "string")) return null;
  return { title, blocks, sources };
}

export function WordReportPreview({ report }: { report: WordReport }) {
  return (
    <article className="ask-word-report" aria-label="Word report preview">
      <h2>{report.title}</h2>
      {report.blocks.map(({ style, text }, index) => {
        if (style === "heading") return <h3 key={index}>{text}</h3>;
        if (style === "bullet") return <p className="ask-word-bullet" key={index}>• {text}</p>;
        return <p key={index}>{text}</p>;
      })}
      <h3>Sources</h3>
      <ol>{report.sources.map(({ title, url }) => <li key={url}>
        <span>{title}</span><small>{url}</small>
      </li>)}</ol>
    </article>
  );
}

function wordPreview(events: ExecutionEvent[]) {
  const event = [...events].reverse().find((item) => item.kind === "tool" && item.tool === "create_word_report"
    && ["committed_verified", "target_verified"].includes(item.outcome));
  if (!event) return null;
  const write = receiptDetails(event);
  const report = readWordReport(write.report);
  if (!report || write.readback_verified !== true || typeof write.path !== "string"
    || typeof write.after_sha256 !== "string") return null;
  return { path: write.path, sha: write.after_sha256, report };
}

function researchPreview(events: ExecutionEvent[]) {
  const latestFirst = [...events].reverse();
  const writeIndex = latestFirst.findIndex((event) => event.kind === "tool" && event.tool === "write_file"
    && ["committed_verified", "target_verified"].includes(event.outcome));
  if (writeIndex === -1) return null;
  const write = receiptDetails(latestFirst[writeIndex]);
  const read = latestFirst.slice(0, writeIndex)
    .filter((event) => event.kind === "tool" && event.tool === "read_file" && event.outcome === "ok")
    .map(receiptDetails).find((result) => result.path === write.path);
  if (write.readback_verified !== true || typeof write.path !== "string" || typeof write.after_sha256 !== "string"
    || read?.sha256 !== write.after_sha256 || typeof read.content !== "string") return null;
  return { path: write.path, sha: write.after_sha256, content: read.content };
}

function artifactAccepted(events: ExecutionEvent[]) {
  return events.some((event) => event.tool === "workflow_acceptance" && event.outcome === "accepted");
}

function visualTestVerified(events: ExecutionEvent[]) {
  return events.some((event) => {
    if (event.kind !== "tool" || event.tool !== "run_command" || event.outcome !== "ok") return false;
    const receipt = receiptDetails(event);
    return receipt.exit_code === 0 && receipt.timed_out === false && receipt.visual_css_verified === true;
  });
}

function runStatus(chat: Chat, run: WorkflowRun, running: boolean, events: ExecutionEvent[]): string {
  if (running) {
    const latest = events[events.length - 1];
    return latest?.kind === "approval" && ["awaiting_review", "awaiting_confirmation"].includes(latest.outcome)
      ? "Awaiting approval" : "Running";
  }
  if (chat.resume?.turn_id === run.turn_id) {
    if (chat.resume.blocked) return "Needs attention";
    if (chat.resume.restart_waiting) return chat.resume.restart_ready ? "Ready to verify" : "Waiting for restart";
    return "Ready to resume";
  }
  if (run.visual && events.some((event) => event.tool === "visual_comparison" && event.outcome === "verified")) {
    return "Comparison ready for owner review";
  }
  if (run.visual) {
    if (events.some((event) => event.tool === "visual_proposal" && event.outcome === "proposed")) return "Fix proposed · Review only";
    if (visualTestVerified(events)) return "Tests passed · Awaiting After screenshot";
    const result = [...events].reverse().find((event) => event.tool === "run_command" && event.kind === "tool");
    if (result) {
      const receipt = receiptDetails(result);
      if (typeof receipt.exit_code === "number" || receipt.timed_out === true) return "Visual QA verification failed";
    }
  }
  if (events.some((event) => event.tool === "turn_ended" && event.outcome === "failed")) return "Failed";
  if (events.some((event) => event.tool === "turn_ended" && event.outcome === "cancelled")) return "Cancelled";
  if (artifactAccepted(events)) return "Artifact accepted";
  if (events.some((event) => event.tool === "final_answer" && event.outcome === "test_failed")) return "Tests failed";
  if (run.template === "engineering_v1" && events.some((event) =>
    event.kind === "workflow" && event.tool === "final_answer" && event.outcome === "complete")) {
    return "Tests passed · Independent review pending";
  }
  if (events.some((event) => event.tool === "final_answer" && event.outcome === "complete")) {
    if (run.template === "research_v1") return (run.word_report ? wordPreview(events) : researchPreview(events))
      ? "Ready for your review" : "Reply saved · Preview needed";
    const latestFirst = [...events].reverse();
    const edits = latestFirst.filter((event) => event.kind === "tool" && event.tool === "write_file"
      && ["committed_verified", "target_verified"].includes(event.outcome) && receiptDetails(event).readback_verified === true);
    const finalEdit = edits[0];
    const afterEdit = finalEdit ? latestFirst.slice(0, latestFirst.indexOf(finalEdit)) : [];
    const tests = afterEdit.find((event) => event.kind === "tool" && event.tool === "run_command");
    const diff = afterEdit.find((event) => event.kind === "tool" && event.tool === "git_diff" && event.outcome === "ok");
    const diffs = diff ? receiptDetails(diff).diffs : null;
    const normalizePath = (input: string) => input.replace(/\\/g, "/").toLowerCase();
    const workspace = normalizePath(run.workspace_root).replace(/\/$/, "");
    const includesEveryEdit = Array.isArray(diffs) && edits.every((edit) => {
      const editedPath = receiptDetails(edit).path;
      return typeof editedPath === "string" && diffs.some((item) =>
        typeof item?.path === "string" && `${workspace}/${normalizePath(item.path)}` === normalizePath(editedPath));
    });
    if (tests?.outcome === "error") return "Reply saved · Tests failed";
    if (tests?.outcome === "ok" && receiptDetails(tests).exit_code === 0 && includesEveryEdit) {
      return "Independent review pending";
    }
    return "Reply saved · Verification needed";
  }
  return "Interrupted before completion";
}

export function mergeWorkflowEvidence(saved: ExecutionEvent[], live: ExecutionEvent[]): ExecutionEvent[] {
  if (!live.length) return saved;
  const identity = (event: ExecutionEvent) => JSON.stringify([
    event.timestamp, event.kind, event.tool, event.outcome, event.details,
  ]);
  const known = new Set(saved.map(identity));
  return [...saved, ...live.filter((event) => !known.has(identity(event)))].sort((a, b) => a.timestamp - b.timestamp);
}

export function VisualScreenshot({ chatId, turnId, stage, onDisplay }: {
  chatId: string;
  turnId: string;
  stage: "before" | "after";
  onDisplay?: () => void;
}) {
  const label = stage === "before" ? "Before" : "After";
  return <VerifiedPng chatId={chatId} turnId={turnId} stage={stage} label={label}
    alt={`${label} screenshot for this Visual QA run`} onDisplay={onDisplay} />;
}

export function VerifiedPng({ chatId, turnId, stage, label, alt, onDisplay }: {
  chatId: string;
  turnId: string;
  stage?: "before" | "after";
  label: string;
  alt: string;
  onDisplay?: () => void;
}) {
  const [image, setImage] = useState<string>();
  const [error, setError] = useState<string>();
  useEffect(() => {
    let disposed = false;
    setImage(undefined);
    setError(undefined);
    const reader = new FileReader();
    reader.onload = () => {
      if (!disposed && typeof reader.result === "string") setImage(reader.result);
    };
    reader.onerror = () => {
      if (!disposed) setError("Could not display this PNG.");
    };
    const read = stage
      ? invoke<ArrayBuffer>("workflow_read_visual", { chatId, turnId, stage })
      : invoke<ArrayBuffer>("chat_read_image", { chatId, turnId });
    read
      .then((bytes) => { if (!disposed) reader.readAsDataURL(new Blob([bytes], { type: "image/png" })); })
      .catch((reason) => { if (!disposed) setError(String(reason)); });
    return () => {
      disposed = true;
      if (reader.readyState === FileReader.LOADING) reader.abort();
    };
  }, [chatId, turnId, stage]);
  return <figure className={stage ? "ask-visual-shot" : "ask-visual-shot ask-chat-image"}>
    <figcaption>{label}</figcaption>
    {image ? <img src={image} alt={alt} onLoad={onDisplay} onError={() => {
      setImage(undefined);
      setError("Could not display this PNG.");
    }} /> : <small role="status">{error ?? "Loading verified PNG…"}</small>}
  </figure>;
}

function VisualEvidence({ chat, run, events, deciding, onCompare, onRetry }: {
  chat: Chat;
  run: WorkflowRun;
  events: ExecutionEvent[];
  deciding: boolean;
  onCompare: (path: string | null) => void;
  onRetry: () => void;
}) {
  const [after, setAfter] = useState("");
  const verified = (tool: string) => events.find((event) => event.tool === tool && event.outcome === "verified");
  const before = verified("visual_before");
  const inspection = verified("visual_inspection");
  const comparison = verified("visual_comparison");
  const observation = inspection && receiptDetails(inspection).observation;
  const changed = comparison && receiptDetails(comparison).changed_pixels;
  const passed = visualTestVerified(events);
  const lastCommandEvent = [...events].reverse().find((event) => event.tool === "run_command" &&
    (event.kind === "tool" || event.outcome === "test_dispatch_pending"));
  const launchError = lastCommandEvent && receiptDetails(lastCommandEvent).error;
  const canRetry = !passed && !chat.resume && !chat.archived &&
    typeof launchError === "string" && launchError.startsWith("failed to spawn process:");
  return <>
    {typeof observation === "string" && <p className="ask-visual-observation">{observation}</p>}
    {before && <div className="ask-visual-pair">
      <VisualScreenshot chatId={chat.id} turnId={run.turn_id} stage="before" />
      {comparison && <VisualScreenshot chatId={chat.id} turnId={run.turn_id} stage="after" />}
    </div>}
    {typeof changed === "number" && <small>{changed.toLocaleString()} pixels changed at the same screenshot size.</small>}
    {canRetry && <Button size="sm" variant="secondary" disabled={deciding} onClick={onRetry}>
      Retry planned test
    </Button>}
    {passed && !comparison && <form className="ask-visual-fields" onSubmit={(event) => {
      event.preventDefault();
      if (after.trim() && !deciding && !chat.archived) onCompare(after.trim());
    }}>
      <label>After screenshot
        <input placeholder="screenshots/after.png" value={after} disabled={deciding || chat.archived}
          onChange={(event) => setAfter(event.target.value)} />
      </label>
      <small>Open the updated build at the same window size and scale. For an existing PNG, enter its worktree-relative path.</small>
      {run.visual?.capture_window && <Button size="sm" variant="secondary" type="button"
        disabled={deciding || chat.archived} onClick={() => onCompare(null)}>
        Capture PAIGE window as After
      </Button>}
      <Button size="sm" variant="secondary" type="submit" disabled={deciding || chat.archived || !after.trim()}>
        Compare screenshots
      </Button>
    </form>}
  </>;
}

export function WorkflowDetails({ chat, run, running, events, deciding, onAccept, onSaveWord, onCompareVisual, onRetryVisualTest }: {
  chat: Chat;
  run: WorkflowRun;
  running: boolean;
  events: ExecutionEvent[];
  deciding: boolean;
  onAccept: () => void;
  onSaveWord: (filename: string) => void;
  onCompareVisual: (path: string | null) => void;
  onRetryVisualTest: () => void;
}) {
  const template = workflowTemplates[run.template];
  const preview = run.template === "research_v1" && !run.word_report ? researchPreview(events) : null;
  const word = run.word_report ? wordPreview(events) : null;
  const replied = events.some((event) => event.tool === "final_answer" && event.outcome === "complete");
  const canAccept = replied && !artifactAccepted(events);
  return (
    <section className="ask-workflow-summary" aria-label="Workflow details">
      <div className="ask-artifact-meta"><strong>{template.name}</strong><span>{run.selected_from_chat ? "Selected from chat" : "Version 1"}</span></div>
      <span role="status">{runStatus(chat, run, running, events)}</span>
      <small title={run.workspace_root}>{run.workspace_root}</small>
      <details>
        <summary>Template steps</summary>
        <ol>{template.steps.map((step) => <li key={step}>{step}</li>)}</ol>
      </details>
      {run.template === "engineering_v1" && <small>Independent review requires a verified reviewer handoff.</small>}
      {run.visual && <VisualEvidence key={run.turn_id} chat={chat} run={run} events={events}
        deciding={deciding || running} onCompare={onCompareVisual} onRetry={onRetryVisualTest} />}
      {word && <>
        <div className="ask-word-actions">
          <Button size="sm" variant="secondary" disabled={deciding || running}
            onClick={() => onSaveWord(word.path.split(/[\\/]/).pop() || "report.docx")}>Save Word copy</Button>
          {canAccept && <Button size="sm" variant="secondary" disabled={deciding || running || chat.archived}
            onClick={onAccept}>Accept this version</Button>}
        </div>
        <WordReportPreview report={word.report} />
        <details>
          <summary>File verification</summary>
          <small>{word.path}</small>
          <small>SHA-256 {word.sha}</small>
        </details>
      </>}
      {preview && <>
        <details>
          <summary>Open Markdown preview</summary>
          <small>{preview.path}</small>
          <small>SHA-256 {preview.sha}</small>
          <pre className="ask-artifact-details">{preview.content}</pre>
        </details>
        {canAccept && <Button size="sm" variant="secondary"
          disabled={deciding || running || chat.archived} onClick={onAccept}>Accept this version</Button>}
      </>}
    </section>
  );
}
