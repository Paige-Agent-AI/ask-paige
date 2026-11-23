import fs from "node:fs";
import path from "node:path";
import { createHash } from "node:crypto";

const source = "export function sum(values) {\n  return values.reduce((total, value) => total + value);\n}\n";
const baselineHash = createHash("sha256").update(source).digest("hex");
const command = "node --test --test-reporter=tap routing-eval.test.mjs";

const prompt = `PAIGE routing eval v1. Fix sum([]) in routing-eval.mjs so it returns zero. Read only routing-eval.mjs and routing-eval.test.mjs. Establish a failing baseline with ${command} in the pinned project. Save a Workbench plan with the exact file, cause and test command before proposing one targeted edit to routing-eval.mjs. The owner handles any edit approval. After a verified write, run the same test command once and report the actual result. Leave the test file as it is. Use no remote tools, packages, Git commands, or shell edits.\n`;

const tests = `import test from "node:test";
import assert from "node:assert/strict";
import { sum } from "./routing-eval.mjs";

test("the empty list sums to zero", () => assert.equal(sum([]), 0));
test("preserves a single value", () => assert.equal(sum([7]), 7));
test("sums positive and negative values", () => assert.equal(sum([9, -3, 4]), 10));
`;

const testHash = createHash("sha256").update(tests).digest("hex");

function prepare(root) {
  // The caller supplies a new directory. Never overwrite a previous eval or workspace.
  fs.mkdirSync(root);
  fs.writeFileSync(path.join(root, "routing-eval.mjs"), source, { flag: "wx" });
  fs.writeFileSync(path.join(root, "routing-eval.test.mjs"), tests, { flag: "wx" });
  return { project: path.resolve(root), prompt: prompt.trim(), initial_file_sha256: baselineHash, test_file_sha256: testHash };
}

function details(event) {
  try { return JSON.parse(event?.details ?? "{}") ?? {}; } catch { return {}; }
}

function summary(recordFile, turn, expectedModel) {
  const document = JSON.parse(fs.readFileSync(recordFile, "utf8").replace(/^\uFEFF/, ""));
  const chat = document.chats.find((candidate) => {
    const first = candidate.messages[0];
    return first?.role === "user" && first.turn_id === turn && first.text === prompt.trim();
  });
  if (!chat) throw Error("That turn does not use the exact PAIGE routing eval prompt.");
  const events = chat.execution.filter((event) => event.turn_id === turn);
  const modelRounds = events.filter((event) => event.tool === "model_round");
  const selected = events.find((event) => event.tool === "select_model");
  const routeMatched = selected?.gateway === "omniroute" && selected.model === expectedModel &&
    modelRounds.length > 0 && modelRounds.every((event) =>
      event.gateway === "omniroute" && event.model === expectedModel);
  const attempts = modelRounds.map((event) => ({
    ...details(event), route: event.gateway, requested_model: event.model, outcome: event.outcome,
  }));
  const relativeFile = (event) => {
    const target = details(event).path;
    return typeof target === "string" ? path.relative(chat.project_roots[0], target) : null;
  };
  const scopedFile = (event) => relativeFile(event) === "routing-eval.mjs";
  const reads = events.filter((event) => event.kind === "tool" && event.tool === "read_file");
  const expectedFiles = ["routing-eval.mjs", "routing-eval.test.mjs"];
  const scopeMatched = reads.length >= 2 && reads.every((event) => expectedFiles.includes(relativeFile(event))) &&
    expectedFiles.every((file) => reads.some((event) => event.outcome === "ok" && relativeFile(event) === file)) &&
    events.filter((event) => event.kind === "tool_start" || event.kind === "tool").every((event) =>
      ["read_file", "submit_workbench_plan", "write_file", "run_command"].includes(event.tool));
  const read = events.findIndex((event) => event.tool === "read_file" && event.outcome === "ok" &&
    scopedFile(event) && details(event).sha256 === baselineHash);
  const readTests = events.findIndex((event) => event.tool === "read_file" && event.kind === "tool" && event.outcome === "ok" &&
    relativeFile(event) === "routing-eval.test.mjs" && details(event).sha256 === testHash);
  const plan = events.findIndex((event) => event.tool === "submit_workbench_plan" && event.outcome === "ok" &&
    details(event).plan?.test_command === command &&
    JSON.stringify(details(event).plan?.files) === JSON.stringify(["routing-eval.mjs"]));
  const write = events.findIndex((event) => event.tool === "write_file" && event.outcome === "committed_verified" &&
    scopedFile(event) && details(event).before_sha256 === baselineHash && details(event).readback_verified === true);
  const writeIdentity = details(events[write]).proposal_id;
  const confirmed = typeof writeIdentity === "string" && events.slice(plan + 1, write).some((event) =>
    event.tool === "write_file" && event.kind === "approval" && event.outcome === "confirmed" &&
    details(event).proposal_id === writeIdentity && scopedFile(event));
  const testEvents = events.map((event, index) => ({ event, index, data: details(event) }))
    .filter(({ event, data }) => event.tool === "run_command" && event.kind === "tool" &&
      data.command === command && typeof data.cwd === "string" &&
      path.relative(chat.project_roots[0], data.cwd) === "" && data.timed_out === false);
  const baseline = testEvents.find(({ index, data }) => read >= 0 && readTests >= 0 &&
    index > Math.max(read, readTests) && index < plan &&
    data.exit_code === 1 && /# fail 1\b/.test(data.stdout));
  const verified = testEvents.find(({ index, data }) => index > write && data.exit_code === 0 &&
    /# pass 3\b/.test(data.stdout) && /# fail 0\b/.test(data.stdout));
  const writeCount = events.filter((event) => event.tool === "write_file" && event.outcome === "committed_verified").length;
  const requestedWrites = events.filter((event) => event.tool === "write_file" && event.kind === "tool_start").length;
  const requestedCommands = events.filter((event) => event.tool === "run_command" && event.kind === "tool_start").length;
  const complete = events.some((event) => event.tool === "final_answer" && event.outcome === "complete");
  const measurements = (field) => {
    const numbers = attempts.map((item) => item[field]).filter(Number.isFinite);
    return { total: numbers.length ? numbers.reduce((total, value) => total + value, 0) : null, observed_rounds: numbers.length };
  };
  const first = attempts.map((item) => item.first_output_ms).filter(Number.isFinite).sort((a, b) => a - b);
  return {
    turn, project: chat.project_roots[0], expected_model: expectedModel, route_matched: routeMatched,
    passed: routeMatched && scopeMatched && read >= 0 && read < plan && plan < write &&
      Boolean(baseline && verified && confirmed && complete) && writeCount === 1 && requestedWrites === 1 &&
      testEvents.length === 2 && requestedCommands === 2,
    rounds: attempts.length, verified_writes: writeCount, test_runs: testEvents.length,
    slowest_first_output_ms: first.at(-1) ?? null, prompt_tokens: measurements("prompt_tokens"),
    completion_tokens: measurements("completion_tokens"), cached_prompt_tokens: measurements("cached_prompt_tokens"),
    reported_cost_usd: measurements("cost_usd"), attempts,
  };
}

const [action, ...args] = process.argv.slice(2);
if (action === "prepare" && args.length === 1) {
  console.log(JSON.stringify(prepare(args[0]), null, 2));
} else if (action === "compare" && args.length === 5) {
  const [comparisonModel, baselineFile, baselineTurn, comparisonFile, comparisonTurn] = args;
  if (!comparisonModel || comparisonModel === "auto/best-free") throw Error("Name a distinct coding combo.");
  const results = [summary(baselineFile, baselineTurn, "auto/best-free"), summary(comparisonFile, comparisonTurn, comparisonModel)];
  if (path.relative(results[0].project, results[1].project) === "") throw Error("Use a fresh fixture directory for each route.");
  console.log(JSON.stringify({ results }, null, 2));
  if (!results.every((result) => result.passed)) process.exitCode = 1;
} else {
  throw Error("Usage: node scripts/eval-paige-routing.mjs prepare <new-dir> | compare <coding-combo-model-id> <chats.json> <turn> <chats.json> <turn>");
}
