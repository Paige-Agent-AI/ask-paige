import type { ExecutionEvent } from "../types/chats";

export type RunStep = {
  title: string;
  detail?: string;
  tone: "neutral" | "success" | "warning" | "error";
  tab: "Plan" | "Changes" | "Terminal" | "Activity";
};

function facts(event: ExecutionEvent): Record<string, unknown> {
  try {
    const value: unknown = JSON.parse(event.details);
    if (value && typeof value === "object") return value as Record<string, unknown>;
  } catch { /* Older receipts may have plain-text details. */ }
  return {};
}

function testCommand(command: unknown) {
  return typeof command === "string" && /(?:^|[/\\\s])(?:cargo(?:\.exe)?\s+test\b|npm(?:\.cmd)?\s+(?:run\s+)?test\b|node(?:\.exe)?\s+--test\b|pytest\b|go\s+test\b|dotnet\s+test\b)/i.test(command);
}

function countPassing(output: unknown) {
  if (typeof output !== "string") return undefined;
  const suites = [...output.matchAll(/test result:\s*ok\.\s*(\d+) passed\b/g)];
  if (suites.length) return suites.reduce((total, match) => total + Number(match[1]), 0);
  const node = output.match(/^# pass\s+(\d+)\s*$/m);
  if (node) return Number(node[1]);
}

export function runStep(event: ExecutionEvent): RunStep | undefined {
  if (event.kind === "plan" && event.tool === "submit_workbench_plan" && event.outcome === "ok")
    return { title: "Plan saved", tone: "success", tab: "Plan" };
  if (event.kind === "tool" && event.tool === "write_file" && event.outcome === "committed_verified") {
    const { path } = facts(event);
    return { title: "Edit verified", detail: typeof path === "string" ? path : undefined, tone: "success", tab: "Changes" };
  }
  if (event.kind === "tool" && event.tool === "run_command") {
    const { command, exit_code: code, stdout, timed_out: timedOut, visual_css_verified: visualVerified } = facts(event);
    const tests = testCommand(command);
    if (typeof code !== "number" || timedOut)
      return { title: timedOut ? "Command timed out" : "Command result recorded", tone: timedOut || event.outcome === "error" ? "error" : "neutral", tab: "Terminal" };
    if (visualVerified === false)
      return { title: "Test file verification failed", detail: "Exit code " + code, tone: "error", tab: "Terminal" };
    if (code === 0 && event.outcome === "error")
      return { title: "Command reported an error", detail: "Exit code 0", tone: "error", tab: "Terminal" };
    const passed = code === 0;
    const count = tests && passed ? countPassing(stdout) : undefined;
    const title = tests
      ? passed ? count === 0 ? "Test command exited without passing tests" : "Test command passed" : "Test command failed"
      : passed ? "Command finished" : "Command failed";
    return {
      title,
      detail: count && count > 0 ? String(count) + " passed · Exit code " + code : "Exit code " + code,
      tone: passed ? count === 0 ? "warning" : "success" : "error",
      tab: "Terminal",
    };
  }
  if (event.kind === "tool" && event.tool === "git_diff" && event.outcome === "ok")
    return { title: "Diff recorded", tone: "success", tab: "Changes" };
  if (event.kind === "handoff")
    return { title: "Specialist handoff", detail: event.summary, tone: "neutral", tab: "Activity" };
  if (event.kind === "approval" && event.outcome === "declined")
    return { title: "Approval declined", detail: event.summary, tone: "warning", tab: "Activity" };
  if (event.tool === "task_paused" && event.outcome === "paused")
    return { title: "Checkpoint saved", tone: "warning", tab: "Activity" };
  if (event.tool === "workflow_batch_continued" || event.tool === "workflow_restart_test")
    return { title: "Continued from saved work", tone: "neutral", tab: "Activity" };
  if (event.tool === "turn_ended" && event.outcome === "cancelled")
    return { title: "Task stopped", tone: "neutral", tab: "Activity" };
  return undefined;
}

export function liveRunLabel(event: ExecutionEvent | undefined, tool: string | undefined, starting: boolean) {
  if (starting) return "Starting your request";
  if (event?.tool === "workflow_batch_continued") return "Continuing from the saved checkpoint";
  if (event?.tool === "workflow_restart_test") return "Preparing the saved tests";
  if (event?.tool === "model_recovery" && event.outcome === "retrying") return "Retrying the model from saved results";
  if (tool === "run_command") return "Running the command in your project";
  if (tool === "list_files") return "Checking the project structure";
  if (tool === "read_file") return "Inspecting the project files";
  if (tool === "git_diff" || tool === "git_log" || tool === "git_status") return "Reviewing the repository";
  if (tool === "write_file") return "Applying the approved edit";
  if (tool === "submit_workbench_plan") return "Saving the plan";
  if (event?.kind === "handoff") return "Coordinating with a specialist";
  return "Working on your request";
}
