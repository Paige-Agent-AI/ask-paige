#!/usr/bin/env node
// Runs only against the test-feature Windows executable and a disposable project.
import assert from "node:assert/strict";
import { execFileSync, spawn } from "node:child_process";
import fs from "node:fs";
import net from "node:net";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const exe = process.env.PAIGE_CONTINUATION_TEST_EXE ?? path.join(repo, "src-tauri/target-test/release/paige-desktop.exe");
const dir = fs.mkdtempSync(path.join(repo, "src-tauri/target-test/continuation-"));
const project = path.join(dir, "project");
const settings = path.join(dir, "settings");
const reservation = net.createServer();
await new Promise(resolve => reservation.listen(0, "127.0.0.1", resolve));
const port = reservation.address().port;
await new Promise(resolve => reservation.close(resolve));
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
fs.mkdirSync(project);
fs.mkdirSync(settings);
for (let index = 1; index <= 10; index++) {
  fs.writeFileSync(path.join(project, `continuation-progress-${index}.txt`), `Independent input ${index}.\n`);
}

async function until(read, label, attempts = 160) {
  for (let attempt = 0; attempt < attempts; attempt++) {
    const result = await read();
    if (result) return result;
    await sleep(250);
  }
  throw Error(`Timed out waiting for ${label}`);
}

const identity = await fetch("http://127.0.0.1:41011/v1/models").catch(() => null);
assert.equal(identity, null, "The isolated test gateway port must be unused");
const harness = spawn(process.execPath, [path.join(repo, "scripts/test-harness-gateway.mjs")], { stdio: "ignore", windowsHide: true });
let app;
let socket;

async function launch() {
  assert.equal(await fetch(`http://127.0.0.1:${port}/json/list`).catch(() => null), null,
    "The isolated WebView debug port must be unused");
  app = spawn(exe, [], {
    env: {
      ...process.env,
      PAIGE_TEST_CONFIG_DIR: settings,
      PAIGE_TEST_REMEMBER_FIXTURE: "1",
      PAIGE_TEST_GATEWAY: "1",
      PAIGE_BROKER_TEST_MODE: "1",
      WEBVIEW2_USER_DATA_FOLDER: path.join(dir, "webview"),
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
    },
    stdio: "ignore",
  });
  const page = await until(async () => {
    try {
      const response = await fetch(`http://127.0.0.1:${port}/json/list`);
      return (await response.json()).find(entry => entry.url === "http://tauri.localhost/");
    } catch {
      return null;
    }
  }, "the isolated packaged PAIGE webview");
  socket = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    socket.addEventListener("open", resolve, { once: true });
    socket.addEventListener("error", reject, { once: true });
  });
  let counter = 0;
  const replies = new Map();
  socket.addEventListener("message", ({ data }) => {
    const reply = JSON.parse(data);
    const resolve = replies.get(reply.id);
    if (resolve) {
      replies.delete(reply.id);
      resolve(reply);
    }
  });
  return async (name, args = {}) => {
    const id = ++counter;
    const result = await new Promise(resolve => {
      replies.set(id, resolve);
      socket.send(JSON.stringify({ id, method: "Runtime.evaluate", params: {
        awaitPromise: true, returnByValue: true,
        expression: `(async () => { try { return { value: await window.__TAURI_INTERNALS__.invoke(${JSON.stringify(name)}, ${JSON.stringify(args)}) }; } catch (error) { return { error: String(error) }; } })()`,
      } }));
    });
    return result.result?.result?.value;
  };
}

function stopApp() {
  socket?.close();
  if (app?.pid && app.exitCode === null) {
    try { execFileSync("taskkill", ["/PID", String(app.pid), "/T", "/F"], { stdio: "ignore", windowsHide: true }); }
    catch (error) { if (app.exitCode === null) throw error; }
  }
  app = undefined;
  socket = undefined;
}

function store() {
  try { return JSON.parse(fs.readFileSync(path.join(settings, "chats.json"), "utf8")); }
  catch (error) { if (error.code === "ENOENT") return null; throw error; }
}

try {
  await until(() => fetch("http://127.0.0.1:41011/v1/models").catch(() => null), "gateway fixture");
  let invoke = await launch();
  await until(async () => {
    const status = await invoke("ask_status");
    return status?.value?.gateways.some(gateway => gateway.gateway === "test-harness" && gateway.authenticated);
  }, "the new WebView to initialize its isolated test gateway");
  assert.equal((await invoke("broker_grant_root", { root: project, read: true, write: true })).value?.granted, true);
  async function startBuildFixture(message) {
    return until(async () => {
      const result = await invoke("ask_send", { request: {
        message, mode: "build", model_choice: "test-harness::harness/alpha-coder", persist_chat: true,
      } });
      if (result.error?.startsWith("Could not acquire the active turn lock:")) return null;
      assert.ok(result.value?.chat_id, result.error);
      return result.value;
    }, "the cutoff fixture to start");
  }

  const automatic = await startBuildFixture("Verify recovery after a model cutoff.");
  const recovered = await until(() => {
    const chat = store()?.chats.find(item => item.id === automatic.chat_id);
    return chat?.execution.some(event => ["final_answer", "task_paused", "turn_ended"].includes(event.tool)) && chat;
  }, "automatic recovery to finish");
  assert.match(recovered.messages.at(-1).text, /Automatic recovery preserved the verified edit/);
  const recoveryAttempts = recovered.execution.filter(event => event.tool === "model_recovery")
    .map(event => JSON.parse(event.details));
  assert.deepEqual(recoveryAttempts.map(({ cause, attempt }) => [cause, attempt]), [
    ["output_limit", 1],
    ["stream_ended_early", 2],
    ["incomplete_tool_call", 1],
    ["inconsistent_tool_finish", 2],
  ]);
  assert.equal(recovered.execution.filter(event => event.outcome === "committed_verified").length, 1);
  assert.equal(recovered.execution.filter(event => event.outcome === "already_applied").length, 1);
  assert.equal(recovered.execution.filter(event => event.tool === "task_paused").length, 0);
  assert.equal(fs.readFileSync(path.join(project, "cutoff-fixture.txt"), "utf8"), "One verified edit before cutoff.\n");
  assert.equal(fs.existsSync(path.join(project, "cutoff-must-not-exist.txt")), false);
  console.log("PASS: output limit, missing finish and truncated arguments recovered in the same turn; cutoff actions discarded and verified edit preserved.");

  const replyRecovery = await startBuildFixture("Verify recovery from an empty and a printed tool reply. Call the native tool.");
  const usable = await until(() => {
    const chat = store()?.chats.find(item => item.id === replyRecovery.chat_id);
    return chat?.recorded_turn_ids.includes(replyRecovery.turn_id) && chat;
  }, "an empty reply and printed tool call to recover in the same turn");
  assert.deepEqual(usable.execution.filter(event => event.tool === "model_recovery")
    .map(event => JSON.parse(event.details).cause), ["empty_reply", "printed_tool_call"]);
  assert.equal(usable.execution.filter(event => event.tool === "read_file" && event.outcome === "ok").length, 1);
  assert.equal(usable.execution.some(event => ["write_file", "task_paused"].includes(event.tool)), false);
  assert.match(usable.messages.at(-1).text, /read with the native tool/);
  assert.equal(fs.existsSync(path.join(project, "printed-call-must-not-exist.txt")), false);
  console.log("PASS: an empty reply and printed tool markup recovered to one native read without dispatching the printed write.");

  const bounded = await startBuildFixture("Verify bounded cutoff recovery.");
  const paused = await until(() => {
    const chat = store()?.chats.find(item => item.id === bounded.chat_id);
    return chat?.execution.some(event => event.tool === "task_paused") && chat;
  }, "recovery to pause at its limit");
  assert.equal(paused.execution.filter(event => event.tool === "model_recovery").length, 2);
  assert.equal(paused.execution.filter(event => event.tool === "model_round" && event.outcome === "incomplete").length, 3);
  assert.equal(paused.execution.some(event => event.tool === "write_file"), false);
  const boundedCheckpoint = store().checkpoints[bounded.chat_id];
  assert.equal(boundedCheckpoint.rounds, 0);
  assert.equal(boundedCheckpoint.in_flight, null);
  assert.deepEqual(boundedCheckpoint.pending, []);
  assert.equal(fs.existsSync(path.join(project, "cutoff-must-not-exist.txt")), false);
  console.log("PASS: repeated cutoffs stop after two recovery attempts and leave an effect-free checkpoint.");

  const started = await startBuildFixture("Verify the disposable coding continuation fixture.");
  assert.ok(started?.chat_id);
  await until(async () => {
    const saved = store()?.checkpoints?.[started.chat_id];
    const fixture = await fetch("http://127.0.0.1:41011/fixtures/continuation");
    return saved?.rounds === 11 && !saved.pending.length && !saved.in_flight && (await fixture.json()).held;
  }, "eleven durable tool rounds");
  const target = path.join(project, "continuation-fixture.txt");
  assert.equal(fs.readFileSync(target, "utf8"), "One verified coding edit.\n");
  const beforeRestart = fs.statSync(target).mtimeMs;
  stopApp();
  await until(async () => await fetch(`http://127.0.0.1:${port}/json/list`).catch(() => null) === null, "the previous app to exit");
  invoke = await launch();
  const saved = await until(async () => {
    const result = await invoke("chat_list");
    return result?.value?.chats.find(chat => chat.id === started.chat_id && chat.resume);
  }, "the reopened WebView to read the saved checkpoint");
  assert.equal(saved.resume.turn_id, started.turn_id);
  assert.equal(saved?.resume?.rounds, 11);
  assert.match((await invoke("ask_resume", { chatId: started.chat_id })).error ?? "", /Authorize the saved workspace/);
  assert.equal((await invoke("broker_grant_root", { root: project, read: true, write: true })).value?.granted, true);
  const resumed = (await invoke("ask_resume", { chatId: started.chat_id })).value;
  assert.equal(resumed?.turn_id, started.turn_id);
  const completed = await until(() => {
    const chat = store()?.chats.find(item => item.id === started.chat_id);
    return chat?.recorded_turn_ids.includes(started.turn_id) && chat;
  }, "the resumed task to finish");
  assert.equal(completed.messages.filter(message => message.turn_id === started.turn_id).length, 2);
  assert.equal(completed.execution.filter(event => event.outcome === "committed_verified").length, 1);
  assert.equal(completed.execution.filter(event => event.outcome === "already_applied").length, 1);
  assert.equal(store().checkpoints?.[started.chat_id], undefined);
  assert.equal(fs.statSync(target).mtimeMs, beforeRestart);
  console.log("PASS: eleven progressing tool rounds, forced restart, same chat and turn, workspace reauthorization, verified edit never rewritten.");

  const sendPlan = message => until(async () => {
    const started = await invoke("ask_send", { request: {
      message, mode: "plan", model_choice: "test-harness::harness/alpha-coder", persist_chat: true,
    } });
    if (started.error?.startsWith("Could not acquire the active turn lock:")) return null;
    assert.ok(started.value?.chat_id, started.error ?? "Plan chat must start");
    return started.value;
  }, "the previous turn to settle");
  const waitForPlan = (turn, label) => until(() => {
    const chat = store()?.chats.find(item => item.id === turn.chat_id);
    return chat?.recorded_turn_ids.includes(turn.turn_id) && chat;
  }, label);

  const repeated = await sendPlan("Verify the repeated-read stop fixture.");
  const stopped = await waitForPlan(repeated, "the duplicate-result loop to stop");
  const repeatedReads = stopped.execution.filter(event => event.kind === "tool" && event.tool === "read_file");
  assert.equal(repeatedReads.length, 4);
  assert.ok(repeatedReads.every(event => event.outcome === "ok"));
  assert.equal(new Set(repeatedReads.map(event => event.details)).size, 1);
  assert.equal(stopped.execution.at(-1).outcome, "failed");
  assert.match(stopped.messages.at(-1).text, /didn’t produce any new results/);
  assert.equal(stopped.messages.at(-1).turn_id, repeated.turn_id);
  console.log("PASS: repeated identical reads stopped after three rounds without progress and saved a reply.");

  const limited = await sendPlan("Verify the closing reply at the tool limit.");
  const finished = await waitForPlan(limited, "a closing reply without tools");
  const verifiedReads = finished.execution.filter(event => event.kind === "tool" && event.tool === "read_file");
  assert.equal(verifiedReads.length, 8);
  assert.ok(verifiedReads.every(event => event.outcome === "ok"));
  assert.equal(finished.execution.at(-1).outcome, "answered_at_limit");
  assert.match(finished.messages.at(-1).text, /The closing reply used those verified reads/);
  console.log("PASS: eight progressing Plan rounds finished with a tool-free answer.");

  const partial = await sendPlan("Verify the partial report at the tool limit.");
  const partialChat = await waitForPlan(partial, "a partial report kept at the limit");
  const partialReads = partialChat.execution.filter(event => event.kind === "tool" && event.tool === "read_file");
  assert.equal(partialReads.length, 8, "only the eight verified reads may execute");
  assert.ok(partialReads.every(event => event.outcome === "ok"));
  assert.equal(partialChat.execution.at(-1).outcome, "answered_at_limit");
  assert.match(partialChat.messages.at(-1).text, /Partial verified findings from the eight reads/);
  assert.match(partialChat.messages.at(-1).text, /The ninth file was not checked/);
  console.log("PASS: a tool call at the limit still returned the model's real partial report instead of the canned reply.");

  const pinned = await until(async () => {
    const result = await invoke("chat_pin_workspace", { root: project });
    if (result.error?.startsWith("Could not acquire the active turn lock:")) return null;
    assert.ok(result.value?.id, result.error);
    return result.value;
  }, "a new pinned chat");
  const named = await invoke("team_name_specialist", {
    chatId: pinned.id, name: "Test Reviewer", role: "Verify the pinned project",
  });
  assert.equal(named.value?.specialists[0]?.slug, "test-reviewer", named.error);
  assert.match((await invoke("ask_send", { request: {
    chat_id: pinned.id, mode: "plan", message: "@unknown Inspect the project.",
  } })).error ?? "", /existing @handle/);
  const delegated = await invoke("ask_send", { request: {
    chat_id: pinned.id, mode: "plan", model_choice: "test-harness::harness/alpha-coder",
    message: "@test-reviewer Delegate the fixture reads to a supporting specialist. Have them call read_file for the fixture files.",
  } });
  assert.ok(delegated.value, delegated.error);
  const specialistChat = await waitForPlan(delegated.value, "the lead to reply after the supporting specialist");
  const handoffs = specialistChat.execution.filter(event => event.kind === "handoff");
  assert.deepEqual(handoffs.map(event => [event.actor, event.outcome]), [
    ["@test-reviewer", "assigned"], ["@fixture-verifier", "assigned"],
    ["@fixture-verifier", "completed"], ["@test-reviewer", "completed"],
  ]);
  for (const event of handoffs) {
    assert.equal(event.gateway, delegated.value.selection.selected.gateway);
    assert.equal(event.model, delegated.value.selection.selected.id);
    assert.equal(JSON.parse(event.details).workspace, project);
  }
  const childReads = specialistChat.execution.filter(event => event.kind === "tool" && event.tool === "read_file");
  assert.equal(childReads.length, 7);
  assert.ok(childReads.every(event => event.actor === "@fixture-verifier" && event.outcome === "ok"));
  assert.match(specialistChat.messages.at(-1).text, /lead reviewed the supporting specialist/);
  assert.deepEqual(specialistChat.specialists.map(member => member.slug), ["test-reviewer"]);
  console.log("PASS: named specialist inherited the route and pinned project, delegated once, saved seven broker receipts and completed the chat at the round limit.");

  const planned = await until(async () => {
    const started = await invoke("ask_send", { request: {
      chat_id: pinned.id, mode: "build", model_choice: "test-harness::harness/alpha-coder",
      message: "Verify the Workbench Plan file proposal gate.",
    } });
    if (started.error?.startsWith("Could not acquire the active turn lock:")) return null;
    assert.ok(started.value, started.error);
    return started.value;
  }, "the specialist turn to settle");
  const plannedChat = await waitForPlan(planned, "the Workbench Plan fixture to finish");
  const events = plannedChat.execution.filter(event => event.turn_id === planned.turn_id);
  const refused = events.findIndex(event => event.kind === "tool" && event.tool === "write_file" && event.outcome === "error");
  const read = events.findIndex(event => event.kind === "tool" && event.tool === "read_file" && event.outcome === "ok");
  const plan = events.findIndex(event => event.kind === "plan" && event.tool === "submit_workbench_plan" && event.outcome === "ok");
  const committed = events.findIndex(event => event.kind === "tool" && event.tool === "write_file" && event.outcome === "committed_verified");
  assert.ok(refused >= 0 && refused < read && read < plan && plan < committed, JSON.stringify(events.map(event => [event.kind, event.tool, event.outcome])));
  assert.match(events[refused].summary, /submit a Workbench plan/);
  assert.deepEqual(JSON.parse(events[plan].details).plan.files, ["continuation-progress-1.txt"]);
  assert.equal(JSON.parse(events[plan].details).inspected[0].sha256, JSON.parse(events[read].details).sha256);
  assert.equal(events.filter(event => event.outcome === "committed_verified").length, 1);
  assert.equal(fs.readFileSync(path.join(project, "continuation-progress-1.txt"), "utf8"), "Plan-verified fixture edit.\n");
  console.log("PASS: native broker refused an early edit, then saved verified inspection and plan before allowing one edit.");

  const origin = path.join(dir, "engineering-repo");
  const worktree = path.join(dir, "engineering-worktree");
  fs.mkdirSync(path.join(origin, "src"), { recursive: true });
  fs.mkdirSync(path.join(origin, "checks"));
  for (let number = 1; number <= 64; number++) {
    fs.writeFileSync(path.join(origin, "checks", `check-${number}.txt`), `Distinct evidence ${number}.\n`);
  }
  fs.writeFileSync(path.join(origin, "Cargo.toml"), '[package]\nname = "engineering-fixture"\nversion = "0.1.0"\nedition = "2021"\n');
  const code = "pub fn fixture_value() -> u32 {\n    41\n}\n#[test]\nfn expected_value() { assert_eq!(fixture_value(), 42); }\n";
  fs.writeFileSync(path.join(origin, "src", "lib.rs"), code);
  const git = (...args) => execFileSync("git", args, { cwd: origin, stdio: "pipe", windowsHide: true });
  git("init", "-b", "main");
  git("add", ".");
  git("-c", "user.name=PAIGE fixture", "-c", "user.email=fixture@example.invalid",
    "-c", "commit.gpgsign=false", "commit", "-m", "Initialize Engineering fixture");
  git("worktree", "add", "--detach", worktree, "HEAD");
  const opened = await until(async () => {
    const result = await invoke("project_open_folder", { root: worktree, rememberAccess: true });
    if (result.error?.startsWith("Could not acquire the active turn lock:")) return null;
    assert.ok(result.value?.remembered_roots?.includes(worktree), result.error ?? JSON.stringify(result));
    return result.value;
  }, "isolated remembered Engineering worktree");
  const engineeringChat = (await invoke("chat_pin_workspace", { root: opened.root })).value;
  assert.ok(engineeringChat?.id);
  const engineeringRequest = await invoke("ask_send", { request: {
    chat_id: engineeringChat.id, mode: "build", model_choice: "test-harness::harness/alpha-coder",
    message: "Verify the durable Engineering restart workflow.",
    workflow: { template: "engineering_v1", workspace_root: opened.root, restart_before_tests: true },
  } });
  assert.ok(engineeringRequest.value, engineeringRequest.error);
  const engineeringTurn = engineeringRequest.value.turn_id;
  const getEngineering = () => store()?.chats.find(chat => chat.id === engineeringChat.id);
  const engineeringEvents = chat => chat?.execution.filter(event => event.turn_id === engineeringTurn);
  const atRestart = await until(() => {
    const chat = getEngineering();
    return engineeringEvents(chat)?.some(event => event.tool === "task_paused") && chat;
  }, "Engineering’s two automatic batches and pre-test restart boundary", 600);
  const beforeEvents = engineeringEvents(atRestart);
  assert.deepEqual(beforeEvents.filter(event => event.tool === "workflow_batch_continued")
    .map(event => JSON.parse(event.details).rounds), [32, 64]);
  assert.equal(beforeEvents.filter(event => event.outcome === "committed_verified").length, 1);
  assert.equal(beforeEvents.some(event => event.tool === "run_command"), false);
  await until(async () => {
    const result = await invoke("ask_resume", { chatId: engineeringChat.id });
    if (result.error?.startsWith("Could not acquire the active turn lock:")) return null;
    assert.match(result.error ?? "", /Restart PAIGE/);
    return true;
  }, "same-process restart gate to reject Resume");
  assert.match(fs.readFileSync(path.join(worktree, "src", "lib.rs"), "utf8"), /    42/);
  const engineeringFileTime = fs.statSync(path.join(worktree, "src", "lib.rs")).mtimeMs;
  stopApp();
  await until(async () => await fetch(`http://127.0.0.1:${port}/json/list`).catch(() => null) === null, "Engineering app to exit");
  invoke = await launch();
  const afterRestart = await until(() => {
    const chat = getEngineering();
    return chat?.recorded_turn_ids.includes(engineeringTurn) && chat;
  }, "startup to run the saved offline test without Resume");
  const afterEvents = engineeringEvents(afterRestart);
  const tests = afterEvents.filter(event => event.tool === "run_command" && event.kind === "tool");
  assert.equal(afterEvents.filter(event => event.tool === "workflow_restart_test").length, 1);
  assert.equal(afterEvents.filter(event => event.outcome === "committed_verified").length, 1);
  assert.equal(tests.length, 1);
  assert.equal(JSON.parse(tests[0].details).exit_code, 0);
  assert.match(JSON.parse(tests[0].details).stdout, /1 passed; 0 failed/);
  assert.equal(fs.statSync(path.join(worktree, "src", "lib.rs")).mtimeMs, engineeringFileTime);
  assert.equal(store().checkpoints?.[engineeringChat.id], undefined);
  console.log("PASS: Engineering crossed rounds 32 and 64, wrote once, relaunched, and ran one passing Cargo test in the same chat and turn without Resume.");

  const testArgs = { command: "cargo", args: ["test", "--offline", "--manifest-path", "Cargo.toml"], cwd: opened.root };
  const testOnly = await until(async () => {
    const started = await invoke("ask_send", { request: {
      chat_id: engineeringChat.id, mode: "build", model_choice: "test-harness::harness/alpha-coder",
      message: "This turn is tests only.\n" + JSON.stringify(testArgs),
      workflow: { template: "engineering_v1", workspace_root: opened.root },
    } });
    if (started.error?.startsWith("Could not acquire the active turn lock:")) return null;
    assert.ok(started.value, started.error);
    return started.value;
  }, "the explicit native test-only turn to start");
  const testOnlyChat = await until(() => {
    const chat = getEngineering();
    return chat?.recorded_turn_ids.includes(testOnly.turn_id) && chat;
  }, "the test-only command to complete");
  const testOnlyEvents = testOnlyChat.execution.filter(event => event.turn_id === testOnly.turn_id);
  assert.equal(testOnlyEvents.some(event => ["model_round", "write_file"].includes(event.tool)), false);
  const testOnlyResults = testOnlyEvents.filter(event => event.tool === "run_command" && event.kind === "tool");
  assert.equal(testOnlyResults.length, 1);
  assert.equal(JSON.parse(testOnlyResults[0].details).exit_code, 0);
  assert.equal(JSON.parse(testOnlyResults[0].details).cwd, opened.root);
  assert.equal(JSON.parse(testOnlyResults[0].details).command, "cargo test --offline --manifest-path Cargo.toml");
  assert.equal(fs.statSync(path.join(worktree, "src", "lib.rs")).mtimeMs, engineeringFileTime);
  console.log("PASS: a test-only Engineering turn used the exact owner arguments and saved one passing native receipt without rewriting the file.");
} finally {
  stopApp();
  harness.kill();
}
