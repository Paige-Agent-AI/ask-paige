#!/usr/bin/env node
// Increment 1C - functional terminal tests against the REAL release executable.
import { spawn, execSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const exe = process.env.PAIGE_EXE
  ? path.resolve(process.env.PAIGE_EXE)
  : path.join(root, "src-tauri/target/release/paige-desktop.exe");
const PORT = 9228;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const results = [];
const check = (name, pass, detail = "") => {
  results.push({ name, pass: !!pass, detail });
  console.log(`${pass ? "PASS" : "FAIL"}  ${name}${detail ? `   [${String(detail).slice(0, 200)}]` : ""}`);
};

const child = spawn(exe, [], {
  env: {
    ...process.env,
    PATH: `${process.env.USERPROFILE}\\.cargo\\bin;${process.env.PATH}`,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT}`,
  },
  stdio: "ignore",
});

let ws;
let exitCode = 0;
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
  await new Promise((res, rej) => {
    ws.onopen = res;
    ws.onerror = () => rej(new Error("ws fail"));
    setTimeout(() => rej(new Error("ws timeout")), 10000);
  });
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

  const waitForExit = async (sessionId, timeoutMs = 20000) => {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      const list = await invoke("terminal_list");
      const sess = Array.isArray(list) && list.find((s) => s.session_id === sessionId);
      if (!sess || !sess.status || sess.status !== "running") {
        return sess;
      }
      await sleep(300);
    }
    return null;
  };

  const getOutput = async (sessionId) => {
    const out = await invoke("terminal_output", { request: { session_id: sessionId } });
    return Array.isArray(out) ? out : [];
  };

  // 1. node --version runs and prints a version.
  const r1 = await invoke("terminal_execute", { request: { command: "node", args: ["--version"] } });
  check("1a. node --version starts a session", r1?.type === "session" && r1?.session_id, JSON.stringify(r1));
  if (r1?.session_id) {
    const sess1 = await waitForExit(r1.session_id);
    check("1b. node --version exits", sess1 && !sess1.status.startsWith("running"), JSON.stringify(sess1));
    const out1 = await getOutput(r1.session_id);
    const nodeLine = out1.find((o) => o.stream === "stdout" && o.line.startsWith("v"));
    check("1c. node --version prints version", !!nodeLine, out1.map((o) => o.line).join(" | ").slice(0, 120));
    check("1d. node --version exit code 0", sess1?.status === "exited:0", sess1?.status);
  }

  // 2. cargo --version runs.
  const r2 = await invoke("terminal_execute", { request: { command: "cargo", args: ["--version"] } });
  check("2a. cargo --version starts a session", r2?.type === "session" && r2?.session_id, JSON.stringify(r2));
  if (r2?.session_id) {
    const sess2 = await waitForExit(r2.session_id);
    check("2b. cargo --version exits cleanly", sess2?.status === "exited:0", sess2?.status);
  }

  // 3. Privileged command requires approval.
  const r3 = await invoke("terminal_execute", { request: { command: "cmd", args: ["/c", "echo", "bad"] } });
  check("3. privileged cmd requires approval", r3?.type === "approval_required" && r3?.token?.length > 0, JSON.stringify(r3));

  // 4. Unknown command fails.
  const r4 = await invoke("terminal_execute", { request: { command: "paige-fake-command-xyz", args: [] } });
  check("4. unknown command rejected", r4?.__err, JSON.stringify(r4));

  // 5. CWD outside authorized root is rejected.
  const r5 = await invoke("terminal_execute", { request: { command: "node", args: ["--version"], cwd: "C:\\Windows" } });
  check("5. cwd outside root rejected", r5?.__err, JSON.stringify(r5));

  // 6. Kill a running process.
  const r6 = await invoke("terminal_execute", { request: { command: "node", args: ["-e", "setInterval(()=>{}, 10000)"] } });
  check("6a. long-running node starts", r6?.type === "session" && r6?.session_id, JSON.stringify(r6));
  if (r6?.session_id) {
    await sleep(500);
    const killRes = await invoke("terminal_kill", { sessionId: r6.session_id });
    check("6b. kill accepted", !killRes?.__err, JSON.stringify(killRes));
    const sess6 = await waitForExit(r6.session_id, 10000);
    check("6c. process status is killed", sess6?.status === "killed", sess6?.status);
  }

  // 7. Receipts include the completed sessions with real output counts.
  const receipts = await invoke("terminal_receipts");
  check("7. receipts exist", Array.isArray(receipts) && receipts.length >= 2, `count=${receipts?.length}`);
  const nodeReceipt = Array.isArray(receipts) && receipts.find((r) => r.command_line && r.command_line.startsWith("node --version"));
  check("7b. node receipt records stdout line count", nodeReceipt && typeof nodeReceipt.stdout_lines === "number" && nodeReceipt.stdout_lines >= 1, JSON.stringify(nodeReceipt));

  // 8. Arguments containing spaces are passed as a single argv entry.
  const r8 = await invoke("terminal_execute", { request: { command: "node", args: ["-e", "console.log('quoted arg test')"] } });
  check("8a. node -e with spaced arg starts", r8?.type === "session" && r8?.session_id, JSON.stringify(r8));
  if (r8?.session_id) {
    for (let i = 0; i < 60; i++) {
      await sleep(300);
      const list = await invoke("terminal_list");
      const s = Array.isArray(list) && list.find((x) => x.session_id === r8.session_id);
      if (s && s.status !== "running") break;
    }
    const out8 = await getOutput(r8.session_id);
    const found = out8.some((o) => o.line.includes("quoted arg test"));
    check("8b. spaced argument reaches the process intact", found, out8.map((o) => o.line).join(" | ").slice(0, 120));
  }

  const failed = results.filter((r) => !r.pass).length;
  console.log(`\n${results.length - failed}/${results.length} terminal runtime checks passed`);
  exitCode = failed === 0 ? 0 : 1;
} catch (err) {
  console.error("RUNTIME TEST ERROR:", err.message);
  exitCode = 1;
} finally {
  try { ws?.close(); } catch {}
  try { execSync(`taskkill /PID ${child.pid} /T /F`, { stdio: "ignore" }); } catch {}
  process.exit(exitCode);
}
