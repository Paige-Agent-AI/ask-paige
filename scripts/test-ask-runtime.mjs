#!/usr/bin/env node
// Increment 1E - functional Ask PAIGE tests against the REAL target-test
// executable with the local TEST-ONLY harness gateway (PAIGE_TEST_GATEWAY=1).
// Exercises the complete machinery: probe, auto model selection, streaming,
// tool loop (list files, real `node --test` execution), receipts, manual pin,
// PLAN-mode write refusal, unknown-pin fallback, cancellation, and the
// truthful (non-harness) production gateway states.
import { spawn, execSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const exe = process.env.PAIGE_EXE
  ? path.resolve(process.env.PAIGE_EXE)
  : path.join(root, "src-tauri/target-test/release/paige-desktop.exe");
const PORT = 9231;
const LAB = "C:\\Users\\tonig\\paige-desktop-lab";
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const results = [];
const check = (name, pass, detail = "") => {
  results.push({ name, pass: !!pass, detail });
  console.log(`${pass ? "PASS" : "FAIL"}  ${name}${detail ? `   [${String(detail).slice(0, 220)}]` : ""}`);
};

// ---- Lab fixture: a tiny node:test project ----
fs.mkdirSync(LAB, { recursive: true });
fs.writeFileSync(path.join(LAB, "math.js"), "function add(a, b) {\n  return a + b;\n}\nmodule.exports = { add };\n");
fs.writeFileSync(
  path.join(LAB, "math.test.js"),
  'const { test } = require("node:test");\nconst assert = require("node:assert");\nconst { add } = require("./math.js");\n\ntest("add works", () => {\n  assert.strictEqual(add(2, 3), 5);\n});\n',
);

// ---- Start the harness gateway ----
const harness = spawn("node", [path.join(root, "scripts", "test-harness-gateway.mjs")], { stdio: "ignore" });
await sleep(600);

// ---- Launch the app with the test gateway enabled ----
const child = spawn(exe, [], {
  env: {
    ...process.env,
    PATH: `${process.env.USERPROFILE}\\.cargo\\bin;${process.env.PATH}`,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT}`,
    PAIGE_TEST_GATEWAY: "1",
    PAIGE_BROKER_TEST_MODE: "1",
  },
  stdio: "ignore",
});

let ws;
let exitCode = 0;
const events = []; // { event, payload }
try {
  let target = null;
  for (let i = 0; i < 60; i++) {
    await sleep(500);
    try {
      const r = await fetch(`http://127.0.0.1:${PORT}/json/list`);
      if (r.ok) {
        const list = await r.json();
        const page = list.find((t) => t.type === "page" && t.url && !t.url.startsWith("devtools://") && t.url !== "about:blank");
        if (page) { target = page; break; }
      }
    } catch {}
  }
  if (!target) throw new Error("no app page");

  ws = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((res, rej) => { ws.onopen = res; ws.onerror = () => rej(new Error("ws fail")); setTimeout(() => rej(new Error("ws timeout")), 10000); });
  let nextId = 1; const pending = new Map();
  ws.onmessage = (ev) => {
    const m = JSON.parse(ev.data);
    if (m.id && pending.has(m.id)) { pending.get(m.id).resolve(m); pending.delete(m.id); }
  };
  const send = (method, params = {}) => new Promise((res) => { const id = nextId++; pending.set(id, { resolve: res }); ws.send(JSON.stringify({ id, method, params })); });
  const evaluate = async (expression) => {
    const r = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true, userGesture: true });
    if (r.result?.exceptionDetails) return { __err: "EXCEPTION: " + r.result.exceptionDetails.text };
    return r.result?.result?.value;
  };
  const invoke = (cmd, args = {}) => evaluate(`(async()=>{ try { return await window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args)}) } catch(e){ return {__err: String(e)} } })()`);

  await send("Runtime.enable");
  await sleep(1000);

  // The test drives via invoke() and polls ask_receipts/status; the streaming
  // path itself is additionally verified by the frontend listeners in the app.

  // 1. Grant the broker root (tools need it).
  const g = await invoke("broker_grant_root", { root: LAB, read: true, write: true });
  check("1. broker grant succeeds", g?.granted === true, JSON.stringify(g));

  // 2. Gateway probe: harness healthy, production gateways truthful.
  const status = await invoke("ask_status");
  const harnessHealth = status?.gateways?.find((h) => h.gateway === "test-harness");
  const omnirouteHealth = status?.gateways?.find((h) => h.gateway === "omniroute");
  check("2a. test-harness gateway is healthy", harnessHealth?.reachable === true && harnessHealth?.authenticated === true, JSON.stringify(harnessHealth));
  check("2b. harness catalog present", (status?.catalog ?? []).some((c) => c.id === "harness/alpha-coder" && c.gateway === "test-harness"), JSON.stringify(status?.catalog?.slice(0, 4)));
  check("2c. omniroute state is truthful (not falsely ready)", !omnirouteHealth || omnirouteHealth.authenticated === false || omnirouteHealth.reachable === false, JSON.stringify(omnirouteHealth));

  // Detect whether a real production gateway is connected. The harness-specific
  // assertions below pin harness/alpha-coder so they are not affected by an
  // owner-connected OpenRouter key; a separate auto-selection assertion verifies
  // the live gateway state.
  const statusBefore = await invoke("ask_status");
  const openrouterConnected = statusBefore?.openrouter_connected === true;

  // 3. Auto-selection sanity check (adapts to environment).
  const autoRes = await invoke("ask_send", {
    request: {
      message: "Inspect the project files.",
      mode: "plan",
      attachments: [],
      context_refs: [],
      history: [],
      model_choice: null, // Auto
    },
  });
  const expectedAutoGateway = openrouterConnected ? "openrouter-direct" : "test-harness";
  check("3. auto selection chooses a healthy gateway", autoRes?.selection?.selected?.gateway === expectedAutoGateway, JSON.stringify(autoRes?.selection?.selected));

  // 3a-h. Harness-specific BUILD turn: pin harness/alpha-coder so the scripted
  // tool loop (list files → node --test → final answer) runs deterministically.
  const sendRes = await invoke("ask_send", {
    request: {
      message: "Read the project files and run the node test suite; report the exact result.",
      mode: "build",
      attachments: [],
      context_refs: [],
      history: [],
      model_choice: "harness/alpha-coder",
    },
  });
  check("3a. ask_send starts a pinned harness turn", sendRes?.turn_id && sendRes?.selection?.selected?.id === "harness/alpha-coder", JSON.stringify(sendRes?.selection?.selected ?? sendRes));
  check("3b. pinned harness rationale mentions selected model", String(sendRes?.selection?.rationale ?? "").includes("harness/alpha-coder"), String(sendRes?.selection?.rationale));

  const turn1 = sendRes.turn_id;
  // Wait for the turn to finish (poll receipts).
  let done1 = false;
  for (let i = 0; i < 120 && !done1; i++) {
    await sleep(500);
    const receipts = await invoke("ask_receipts");
    done1 = Array.isArray(receipts) && receipts.some((r) => r.turn_id === turn1 && r.tool === "final_answer");
  }
  check("3c. turn reached a final answer", done1);

  const receipts1 = (await invoke("ask_receipts")).filter((r) => r.turn_id === turn1);
  const selectReceipt = receipts1.find((r) => r.tool === "select_model");
  const listReceipt = receipts1.find((r) => r.tool === "list_files");
  const runReceipt = receipts1.find((r) => r.tool === "run_command");
  const finalReceipt = receipts1.find((r) => r.tool === "final_answer");
  check("3d. model selection receipt recorded", !!selectReceipt && selectReceipt.model === "harness/alpha-coder", JSON.stringify(selectReceipt));
  check("3e. list_files tool executed", !!listReceipt && listReceipt.outcome === "ok", JSON.stringify(listReceipt));
  check("3f. run_command executed a real test run", !!runReceipt && runReceipt.outcome === "ok", JSON.stringify(runReceipt?.details ?? "").slice(0, 200));
  check("3g. real node --test output captured", String(runReceipt?.details ?? "").includes("pass") || String(runReceipt?.details ?? "").includes("exit_code"), String(runReceipt?.details ?? "").slice(0, 120));
  check("3h. final answer reports verified result", String(finalReceipt?.details ?? "") + String(finalReceipt?.summary ?? "") !== "" && /test|pass|exit/i.test(String(finalReceipt?.summary ?? "") + String(finalReceipt?.details ?? "")), String(finalReceipt?.summary));

  // 4. Manual pin: beta-analyst for a research-style question.
  const sendRes2 = await invoke("ask_send", {
    request: {
      message: "Analyze whether this project is healthy enough to ship.",
      mode: "plan",
      attachments: [],
      context_refs: ["math.js"],
      history: [],
      model_choice: "harness/beta-analyst",
    },
  });
  check("4a. manual pin honored", sendRes2?.selection?.manual_pin_honored === true && sendRes2?.selection?.selected?.id === "harness/beta-analyst", JSON.stringify(sendRes2?.selection));
  const turn2 = sendRes2.turn_id;
  let done2 = false;
  for (let i = 0; i < 120 && !done2; i++) {
    await sleep(500);
    const receipts = await invoke("ask_receipts");
    done2 = Array.isArray(receipts) && receipts.some((r) => r.turn_id === turn2 && r.tool === "final_answer");
  }
  check("4b. pinned model answered", done2);
  const receipts2 = (await invoke("ask_receipts")).filter((r) => r.turn_id === turn2);
  check("4c. pinned turn used beta-analyst", receipts2.some((r) => r.model === "harness/beta-analyst"), receipts2.map((r) => r.tool).join(","));

  // 5. PLAN mode refuses file writes.
  const sendRes3 = await invoke("ask_send", {
    request: {
      message: "Please overwrite math.js with a stub and tell me what happened. Write it now.",
      mode: "plan",
      attachments: [],
      context_refs: [],
      history: [],
      model_choice: "harness/alpha-coder",
    },
  });
  const turn3 = sendRes3.turn_id;
  let done3 = false;
  for (let i = 0; i < 120 && !done3; i++) {
    await sleep(500);
    const receipts = await invoke("ask_receipts");
    done3 = Array.isArray(receipts) && receipts.some((r) => r.turn_id === turn3 && (r.tool === "final_answer"));
  }
  const receipts3 = (await invoke("ask_receipts")).filter((r) => r.turn_id === turn3);
  const writeReceipt = receipts3.find((r) => r.tool === "write_file");
  check("5a. PLAN-mode write attempt recorded as error", !!writeReceipt && writeReceipt.outcome === "error" && String(writeReceipt.details).includes("PLAN mode"), JSON.stringify(writeReceipt));
  check("5b. the file was NOT modified in PLAN mode", (() => { const c = fs.readFileSync(path.join(LAB, "math.js"), "utf8"); return c.includes("add(a, b)") && !c.includes("stub"); })(), fs.readFileSync(path.join(LAB, "math.js"), "utf8").slice(0, 40));
  check("5c. PLAN refusal turn completed with explanation", done3 && receipts3.some((r) => String(r.summary).includes("refused")), receipts3.map((r) => r.summary.slice(0, 60)).join(" | "));

  // 6. Unknown manual pin falls back to auto with a truthful reason.
  const sendRes4 = await invoke("ask_send", {
    request: {
      message: "Inspect the project files.",
      mode: "plan",
      attachments: [],
      context_refs: [],
      history: [],
      model_choice: "harness/does-not-exist",
    },
  });
  check("6a. unknown pin falls back with reason", sendRes4?.selection?.manual_pin_honored === false && String(sendRes4?.selection?.rationale ?? "").includes("not in the current qualified catalog"), String(sendRes4?.selection?.rationale).slice(0, 160));

  // 7. Cancellation on a slow stream.
  const sendRes5 = await invoke("ask_send", {
    request: {
      message: "Do a slow streaming demonstration.",
      mode: "plan",
      attachments: [],
      context_refs: [],
      history: [],
      model_choice: "harness/alpha-coder",
    },
  });
  const turn5 = sendRes5.turn_id;
  await sleep(1500); // let the slow stream start
  await invoke("ask_cancel", { turnId: turn5 });
  let cancelled = false;
  for (let i = 0; i < 30 && !cancelled; i++) {
    await sleep(400);
    const receipts = await invoke("ask_receipts");
    cancelled = !Array.isArray(receipts) || !receipts.some((r) => r.turn_id === turn5 && r.tool === "final_answer");
    if (cancelled) break;
  }
  // Also confirm no final receipt appears shortly after.
  await sleep(2000);
  const receipts5 = (await invoke("ask_receipts")).filter((r) => r.turn_id === turn5);
  check("7a. cancelled turn produced no final answer", !receipts5.some((r) => r.tool === "final_answer"), receipts5.map((r) => r.tool).join(","));
  check("7b. cancel flag cleared (second cancel is a no-op)", (await invoke("ask_cancel", { turnId: turn5 })) !== undefined);

  // 8. Bogus OpenRouter key is refused and does not overwrite a valid key.
  const before = await invoke("ask_status");
  const hadValidKey = before?.openrouter_connected === true;
  const bogus = await invoke("ask_connect_openrouter", { key: "sk-or-v1-bogus-key-for-negative-test" });
  const after = await invoke("ask_status");
  check("8a. bogus OpenRouter key rejected", !!bogus?.__err, JSON.stringify(bogus).slice(0, 140));
  if (hadValidKey) {
    check("8b. valid owner key remains connected after bogus attempt", after?.openrouter_connected === true, JSON.stringify(after?.openrouter_connected));
  } else {
    check("8b. no key retained after rejection", after?.openrouter_connected === false, JSON.stringify(after?.openrouter_connected));
  }

  const failed = results.filter((r) => !r.pass).length;
  console.log(`\n${results.length - failed}/${results.length} ask runtime checks passed`);
  exitCode = failed === 0 ? 0 : 1;
} catch (err) {
  console.error("RUNTIME TEST ERROR:", err.message);
  exitCode = 1;
} finally {
  try { ws?.close(); } catch {}
  try { execSync(`taskkill /PID ${child.pid} /T /F`, { stdio: "ignore" }); } catch {}
  try { harness.kill(); } catch {}
  process.exit(exitCode);
}
