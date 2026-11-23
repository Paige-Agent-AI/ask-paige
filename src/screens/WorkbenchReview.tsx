import { Button } from "../components/Button";
import type { ExecutionEvent } from "../types/chats";
import { readWordReport, WordReportPreview } from "./Workflows";

export function readReceipt(event: Pick<ExecutionEvent, "details">): Record<string, unknown> {
  try {
    const result: unknown = JSON.parse(event.details);
    return result && typeof result === "object" ? result as Record<string, unknown> : {};
  } catch {
    return {};
  }
}

export function ModelRoundFacts({ event }: { event: Pick<ExecutionEvent, "details"> }) {
  const value = readReceipt(event);
  const facts = [
    ["Gateway reported", [value.provider, value.reported_model].filter((item) => typeof item === "string").join(" · ")],
    ["First output", typeof value.first_output_ms === "number" ? `${value.first_output_ms.toLocaleString()} ms` : ""],
    ["Input tokens", value.prompt_tokens],
    ["Output tokens", value.completion_tokens],
    ["Cached input", value.cached_prompt_tokens],
    ["Tool calls returned", value.returned_tool_calls],
    ["HTTP status", value.http_status],
  ].filter(([, text]) => typeof text === "number" || (typeof text === "string" && text.length > 0));
  if (!facts.length) return null;
  return <dl className="ask-round-facts">
    {facts.map(([label, text]) => <div key={String(label)}><dt>{String(label)}</dt><dd>{String(text)}</dd></div>)}
  </dl>;
}

export function SearchResults({ event }: { event: ExecutionEvent }) {
  const { results } = readReceipt(event);
  if (!Array.isArray(results) || !results.length) return null;
  return <ol className="ask-search-results" aria-label="Web search results">
    {results.filter((item) => typeof item?.title === "string" && typeof item?.url === "string")
      .map(({ title, url, snippet }) => <li key={url}>
        <strong>{title}</strong>
        <code>{url}</code>
        {typeof snippet === "string" && snippet && <span>{snippet}</span>}
      </li>)}
  </ol>;
}

export function hasWritePreview(event: ExecutionEvent) {
  const { preview } = readReceipt(event);
  if (!preview || typeof preview !== "object") return false;
  return "before" in preview && typeof preview.before === "string"
    && "after" in preview && typeof preview.after === "string";
}

export function ApprovalCard({ event, reviewed, onReview }: {
  event: ExecutionEvent;
  reviewed: boolean;
  onReview: () => void;
}) {
  const { path, name } = readReceipt(event);
  const subject = typeof name === "string" ? name : typeof path === "string" ? path : event.tool;
  const preview = hasWritePreview(event);
  const word = event.tool === "create_word_report";
  return (
    <section className="ask-approval-card" aria-label="Owner approval">
      <div className="ask-approval-heading">
        <strong>{preview ? "File edit approval" : "Owner approval requested"}</strong>
        <span>{event.outcome === "awaiting_confirmation" ? "Confirm in Windows" : "Needs your approval"}</span>
      </div>
      <span className="ask-approval-subject" title={subject}>{subject}</span>
      {event.outcome === "awaiting_review" && <div className="ask-approval-actions">
        <Button id="ask-approval-review" size="sm" variant={reviewed ? "secondary" : "primary"} onClick={onReview}>
          {word ? "Review Word report" : preview ? "Review changes" : event.tool === "visual_inspection" ? "Review screenshot" : "Review action"}
        </Button>
      </div>}
    </section>
  );
}

export function ProposedAction({ event }: { event: ExecutionEvent }) {
  const { path, preview, arguments: args } = readReceipt(event);
  const versions = preview as { before: string; after: string };
  const word = preview && typeof preview === "object" && "report" in preview ? readWordReport(preview.report) : null;
  return (
    <section className="ask-artifact">
      <div><strong>{word ? "Proposed Word report" : hasWritePreview(event) ? "Proposed change" : "Proposed action"}</strong></div>
      {typeof path === "string" && <span className="ask-wrap-path">{path}</span>}
      {word ? <WordReportPreview report={word} /> : hasWritePreview(event)
        ? <ChangePreview before={versions.before} after={versions.after} />
        : args !== undefined && args !== null
          ? <pre className="ask-artifact-details">{JSON.stringify(args, null, 2)}</pre>
          : <p>{event.summary}</p>}
    </section>
  );
}

function ChangePreview({ before, after }: { before: string; after: string }) {
  const old = before.match(/[^\n]*\n|[^\n]+$/g) ?? [];
  const next = after.match(/[^\n]*\n|[^\n]+$/g) ?? [];
  let first = 0;
  while (first < old.length && first < next.length && old[first] === next[first]) first++;
  let suffix = 0;
  while (suffix < old.length - first && suffix < next.length - first
    && old[old.length - 1 - suffix] === next[next.length - 1 - suffix]) suffix++;
  const start = Math.max(0, first - 2);
  const endOld = old.length - suffix;
  const endNext = next.length - suffix;
  const lines = (source: string[], sign: string) => source.map((line) => sign + line.replace(/\r?\n$/, "")).join("\n");
  return (
    <>
      <pre className="ask-artifact-details ask-diff">
        <span>{lines(old.slice(start, first), "  ")}</span>{"\n"}
        <span className="ask-diff-remove">{lines(old.slice(first, endOld), "− ")}</span>{"\n"}
        <span className="ask-diff-add">{lines(next.slice(first, endNext), "+ ")}</span>{"\n"}
        <span>{lines(old.slice(endOld, endOld + 2), "  ")}</span>
      </pre>
      <details><summary>Exact proposed file</summary><pre className="ask-artifact-details">{after}</pre></details>
    </>
  );
}
