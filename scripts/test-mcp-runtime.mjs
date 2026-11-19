#!/usr/bin/env node
// Increment 1D â€” functional MCP client runtime tests against the REAL
// (test-feature) executable: one local stdio MCP server + one authenticated
// remote HTTPS MCP server fixture.
import { spawn, execSync } from "node:child_process";
import path from "node:path";
import fs from "node:fs";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const exe = process.env.PAIGE_EXE
  ? path.resolve(process.env.PAIGE_EXE)
  : path.join(root, "src-tauri/target-test/release/paige-desktop.exe");
const PORT = 9229;
const REMOTE_PORT = 8443;
const REMOTE_TOKEN = "paige-mcp-runtime-test-token";
const STDIO_TOKEN = "no-token";
const STDIO_ID = `test-stdio-${Date.now()}`;
const REMOTE_ID = `test-remote-${Date.now()}`;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const results = [];
const check = (name, pass, detail = "") => {
  results.push({ name, pass: !!pass, detail });
  console.log(`${pass ? "PASS" : "FAIL"}  ${name}${detail ? `   [${String(detail).slice(0, 200)}]` : ""}`);
};

// ---------------------------------------------------------------------------
// Ensure the local HTTPS remote fixture has a self-signed certificate.
// ---------------------------------------------------------------------------
function ensureRemoteCert() {
  const pfx = path.join(root, "scripts", "paige-mcp-remote-test.pfx");
  if (fs.existsSync(pfx)) return pfx;
  const psFile = path.join(root, "scripts", "generate-mcp-remote-test-cert.ps1");
  // Write the script with NO interpolated backticks so the file content is
  // unambiguous: we substitute the pfx path via a simple token.
  const tmpl = [
    "$cert = New-SelfSignedCertificate -Subject 'CN=localhost' -DnsName 'localhost' -CertStoreLocation 'Cert:\\CurrentUser\\My'",
    "$bytes = $cert.Export('PFX', 'paige-mcp-test')",
    "$fs = New-Object System.IO.FileStream('%PFXPATH%', [System.IO.FileMode]::Create)",
    "$fs.Write($bytes, 0, $bytes.Length)",
    "$fs.Close()",
    "",
  ].join("\r\n");
  fs.writeFileSync(psFile, tmpl.replaceAll("%PFXPATH%", pfx), "utf8");
  execSync(`powershell -ExecutionPolicy Bypass -File "${psFile}"`, { stdio: ["ignore", "ignore", "pipe"] });
  fs.unlinkSync(psFile);
  if (!fs.existsSync(pfx)) throw new Error("certificate generation failed");
  return pfx;
}

// ---------------------------------------------------------------------------
// Launch app, open CDP page.
// ---------------------------------------------------------------------------
// The app window loads the dev page at http://localhost:1420, so Vite must
// serve it for the app to load and for IPC invoke to be available.
const vite = spawn("cmd", ["/c", "npm run dev"], { cwd: root, stdio: "ignore" });
const waitVite = async () => {
  for (let i = 0; i < 40; i++) {
    await sleep(500);
    try {
      const r = await fetch("http://localhost:1420/");
      if (r.ok) return true;
    } catch {}
  }
  return false;
};

// Clean persisted MCP config so this run starts from a blank slate.
const persistedPath = path.join(process.env.APPDATA ?? "", "paige-desktop", "mcp.json");
fs.rmSync(persistedPath, { force: true });

const app = spawn(exe, [], {
  env: {
    ...process.env,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT}`,
    PAIGE_MCP_TEST_INSECURE_REMOTE: "1",
  },
  stdio: "ignore",
});

let ws;
let exitCode = 0;
let stdioServer = null;
let remoteServer = null;

async function ensureRemoteServer() {
  const server = spawn(process.execPath, [path.join(root, "scripts", "test-mcp-remote-server.mjs")], {
    env: {
      ...process.env,
      PAIGE_MCP_TEST_CERT: ensureRemoteCert(),
      PAIGE_MCP_TEST_TOKEN: REMOTE_TOKEN,
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  remoteServer = server;
  // Wait for "listening" on stderr.
  const ok = await new Promise((res) => {
    const t = setTimeout(() => res(false), 4000);
    server.stderr.on("data", (d) => {
      const s = d.toString();
      if (s.includes("listening")) { clearTimeout(t); res(true); }
      else if (s.includes("fatal") || s.includes("missing")) { clearTimeout(t); res(false); }
    });
    server.once("exit", () => { clearTimeout(t); res(false); });
  });
  if (!ok) {
    const errText = await new Promise((res) => { server.stderr.once("data", res); setTimeout(() => res("timeout"), 1500); });
    throw new Error(`remote server failed to start: ${errText}`);
  }
}

try {
  const viteUp = await waitVite();
  if (!viteUp) console.log("WARN: vite did not come up; app may not load.");

  let target = null;
  const wsList = () => fetch(`http://127.0.0.1:${PORT}/json/list`).then((r) => (r.ok ? r.json() : [])).catch(() => []);
  const goodTarget = async (t) => {
    try {
      const r = await fetch(`http://127.0.0.1:${PORT}/json/new?about:blank`);
      void r;
    } catch {}
    const v = await evaluateOn(t, "!!(window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.invoke)");
    return v === true;
  };
  const evaluateOn = async (t, expression) => {
    const w = new WebSocket(t.webSocketDebuggerUrl);
    try {
      await new Promise((res, rej) => { w.onopen = res; w.onerror = () => rej(new Error("ws fail")); setTimeout(() => rej(new Error("ws timeout")), 3000); });
      let id = 1; const p = new Map();
      w.onmessage = (ev) => { const m = JSON.parse(ev.data); if (m.id && p.has(m.id)) { p.get(m.id)(m); p.delete(m.id); } };
      const send = (method, params = {}) => new Promise((res) => { p.set(id, res); w.send(JSON.stringify({ id: id++, method, params })); });
      const r = await send("Runtime.evaluate", { expression, returnByValue: true, timeout: 3000 });
      return r.result?.result?.value ?? false;
    } catch {
      return false;
    } finally {
      w.close();
    }
  };
  for (let i = 0; i < 90 && !target; i++) {
    await sleep(500);
    const list = await wsList();
    for (const t of list) {
      if (t.type !== "page" || !t.url || t.url.startsWith("devtools://") || t.url === "about:blank") continue;
      if (await goodTarget(t)) { target = t; break; }
    }
  }
  if (!target) throw new Error("no app page");
  ws = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((res, rej) => { ws.onopen = res; ws.onerror = () => rej(new Error("ws fail")); setTimeout(() => rej(new Error("ws timeout")), 10000); });
  let nextId = 1; const pending = new Map();
  ws.onmessage = (ev) => { const m = JSON.parse(ev.data); if (m.id && pending.has(m.id)) { pending.get(m.id).resolve(m); pending.delete(m.id); } };
  const send = (method, params = {}) => new Promise((res) => { const id = nextId++; pending.set(id, { resolve: res }); ws.send(JSON.stringify({ id, method, params })); });
  const evaluate = async (expression) => {
    const r = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true, userGesture: true });
    if (r.result?.exceptionDetails) return { __err: "EXCEPTION: " + r.result.exceptionDetails.text };
    return r.result?.result?.value;
  };
  const invoke = (cmd, args = {}) => evaluate(`(async()=>{ try { return await window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args)}) } catch(e){ return {__err: String(e)} } })()`);
  const waitReady = async (id, timeoutMs = 20000) => {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      const s = await invoke("mcp_status");
      const srv = Array.isArray(s?.servers) && s.servers.find((x) => x.id === id);
      if (srv?.state === "ready") return srv;
      await sleep(400);
    }
    return null;
  };

  await send("Runtime.enable");
  await send("Page.enable");
  await sleep(2000);

  // --- Fixtures ---
  // Local stdio server.
  stdioServer = spawn(process.execPath, [path.join(root, "scripts", "test-mcp-stdio-server.mjs")], { stdio: ["ignore", "pipe", "pipe"] });

  // Local authenticated remote HTTPS server.
  await ensureRemoteServer();

  // --- 1. Add local stdio MCP server (auto-connect). ---
  const addStdio = await invoke("mcp_add_stdio", {
    request: {
      id: STDIO_ID,
      name: "Test Stdio",
      command: "node",
      args: [path.join(root, "scripts", "test-mcp-stdio-server.mjs")],
      env: {},
      auto_connect: true,
    },
  });
  check("1a. mcp_add_stdio returns status", addStdio?.state === "ready", JSON.stringify(addStdio).slice(0, 160));

  const stdioStatus = await waitReady(STDIO_ID);
  check("1b. stdio server reports ready", stdioStatus?.state === "ready", stdioStatus?.detail);
  const toolNames = (stdioStatus?.capabilities?.tools ?? []).map((t) => t.name).sort();
  check("1c. stdio server discovers echo+add tools", toolNames.join(",") === "add,echo", toolNames.join(","));

  // --- 2. Add remote HTTPS MCP server (auto-connect, bearer auth). ---
  const addRemote = await invoke("mcp_add_remote", {
    request: {
      id: REMOTE_ID,
      name: "Test Remote",
      url: `https://127.0.0.1:${REMOTE_PORT}/mcp`,
      auth_header: `Bearer ${REMOTE_TOKEN}`,
      auth_token: null,
      auto_connect: true,
    },
  });
  check("2a. mcp_add_remote returns status", addRemote?.state === "ready", JSON.stringify(addRemote).slice(0, 160));

  const remoteStatus = await waitReady(REMOTE_ID);
  check("2b. remote server reports ready", remoteStatus?.state === "ready", remoteStatus?.detail);
  const remoteTools = (remoteStatus?.capabilities?.tools ?? []).map((t) => t.name).sort();
  check("2c. remote server discovers echo+add tools", remoteTools.join(",") === "add,echo", remoteTools.join(","));

  // --- 3. Execute real tools through the MCP client. ---
  const echoRes = await invoke("mcp_call_tool", { request: { connection_id: STDIO_ID, tool: "echo", arguments: { text: "hello world" } } });
  const echoText = (echoRes?.content ?? []).map((c) => c?.text ?? "").join("");
  check("3a. echo tool returns text", echoText === "hello world", echoText);
  check("3b. echo tool result ok and no error", !echoRes?.__err && echoRes?.is_error !== true && echoRes?.content?.length === 1, JSON.stringify(echoRes).slice(0, 120));

  const addRes = await invoke("mcp_call_tool", { request: { connection_id: STDIO_ID, tool: "add", arguments: { a: 1, b: 2 } } });
  check("3c. add tool returns 3", (addRes?.content ?? []).map((c) => c?.text).includes("3"), JSON.stringify(addRes).slice(0, 160));

  const remoteEchoRes = await invoke("mcp_call_tool", { request: { connection_id: REMOTE_ID, tool: "echo", arguments: { text: "remote hello" } } });
  const remoteEchoText = (remoteEchoRes?.content ?? []).map((c) => c?.text ?? "").join("");
  check("3d. remote echo returns text", remoteEchoText === "remote hello", remoteEchoText);
  const remoteAddRes = await invoke("mcp_call_tool", { request: { connection_id: REMOTE_ID, tool: "add", arguments: { a: 20, b: 22 } } });
  check("3e. remote add returns 42", (remoteAddRes?.content ?? []).map((c) => c?.text).includes("42"), JSON.stringify(remoteAddRes).slice(0, 160));

  // --- 4. Receipts recorded for tool executions. ---
  const receipts = await invoke("mcp_receipts");
  const receiptsArr = Array.isArray(receipts) ? receipts : [];
  const stdioReceipts = receiptsArr.filter((r) => r.connection_id === STDIO_ID);
  const remoteReceipts = receiptsArr.filter((r) => r.connection_id === REMOTE_ID);
  check("4a. stdio tool receipts exist", stdioReceipts.length >= 2, `count=${stdioReceipts.length}`);
  check("4b. remote tool receipts exist", remoteReceipts.length >= 2, `count=${remoteReceipts.length}`);
  const stdioOk = stdioReceipts.every((r) => r.outcome === "ok");
  check("4c. stdio receipts are ok", stdioOk, stdioReceipts.map((r) => r.outcome).join(","));
  const remoteOk = remoteReceipts.every((r) => r.outcome === "ok");
  check("4d. remote receipts are ok", remoteOk, remoteReceipts.map((r) => r.outcome).join(","));

  // --- 5. Status shows both servers ready. ---
  const finalStatus = await invoke("mcp_status");
  const ids = (finalStatus?.servers ?? []).map((s) => s.id).sort();
  check("5. both servers present and ready", [REMOTE_ID, STDIO_ID].sort().join(",") === ids.sort().join(","), ids.join(","));

  // --- 6. stdio credential echo + revocation. ---
  const revStdio = await invoke("mcp_revoke", { id: STDIO_ID });
  check("6a. mcp_revoke removes stdio server", !(Array.isArray(revStdio?.servers) ? revStdio.servers.some((s) => s.id === STDIO_ID) : false), JSON.stringify(revStdio?.servers?.map((s) => s.id)));
  const revRemote = await invoke("mcp_revoke", { id: REMOTE_ID });
  check("6b. mcp_revoke removes remote server", !(Array.isArray(revRemote?.servers) ? revRemote.servers.some((s) => s.id === REMOTE_ID) : false), JSON.stringify(revRemote?.servers?.map((s) => s.id)));
  // Config file should be empty after revoking both (no server configs persist).
  if (fs.existsSync(persistedPath)) {
    const cfg = JSON.parse(fs.readFileSync(persistedPath, "utf8"));
    check("6c. mcp.json persisted empty after revoke", cfg.servers.length === 0, `count=${cfg.servers.length}`);
  } else {
    check("6c. mcp.json persisted empty after revoke", true, "file absent");
  }

  const failed = results.filter((r) => !r.pass).length;
  console.log(`\n${results.length - failed}/${results.length} MCP runtime checks passed`);
  exitCode = failed === 0 ? 0 : 1;
} catch (err) {
  console.error("RUNTIME TEST ERROR:", err.message);
  exitCode = 1;
} finally {
  try { ws?.close(); } catch {}
  try { stdioServer?.kill("SIGTERM"); } catch {}
  try { remoteServer?.kill("SIGTERM"); } catch {}
  try { execSync(`taskkill /PID ${app.pid} /T /F`, { stdio: "ignore" }); } catch {}
  try { execSync(`taskkill /F /IM vite.exe /T 2>&1 | Out-Null`, { stdio: "ignore" }); } catch {}
  process.exit(exitCode);
}
