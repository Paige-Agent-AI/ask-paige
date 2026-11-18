#!/usr/bin/env node
// Increment 1A - runtime CSP and IPC tests against the REAL built executable.
//
// Launches src-tauri/target/debug/paige-desktop.exe with WebView2 remote
// debugging enabled for the duration of the test only, then drives the page
// over the Chrome DevTools Protocol using Node's built-in WebSocket client.
// No third-party dependencies, no test code shipped in the application.
//
// Run: node scripts/test-csp-runtime.mjs
import { spawn, execSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
// Default to the RELEASE binary: Tauri serves frontendDist with the production
// CSP only in release builds. A debug build navigates to devUrl and is governed
// by devCsp instead, so it cannot prove the production policy.
const exe = process.env.PAIGE_EXE
  ? path.resolve(process.env.PAIGE_EXE)
  : path.join(root, "src-tauri/target/release/paige-desktop.exe");
const PORT = 9222;
const results = [];
const check = (name, pass, detail = "") => {
  results.push({ name, pass: !!pass, detail });
  console.log(`${pass ? "PASS" : "FAIL"}  ${name}${detail ? `   [${detail}]` : ""}`);
};
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const child = spawn(exe, [], {
  env: {
    ...process.env,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT}`,
  },
  stdio: "ignore",
});

let ws;
let exitCode = 0;
try {
  // ---- wait for the DevTools endpoint ----
  let targets = null;
  for (let i = 0; i < 120; i++) {
    await sleep(500);
    try {
      const r = await fetch(`http://127.0.0.1:${PORT}/json/list`);
      if (r.ok) {
        const list = await r.json();
        const pages = list.filter((t) => t.type === "page" && t.webSocketDebuggerUrl);
        const page = pages.find((t) => /tauri\.localhost/.test(t.url))
          ?? pages.find((t) => t.url && t.url !== "about:blank" && !t.url.startsWith("devtools://"));
        if (page) {
          targets = page;
          console.log(`attached to: ${page.url}  (all: ${pages.map((p) => p.url).join(", ")})`);
          break;
        }
        if (i === 0 || i % 10 === 0) console.log(`waiting... targets: ${pages.map((p) => p.url).join(", ") || "none"}`);
      }
    } catch { /* not up yet */ }
  }
  if (!targets) throw new Error("WebView2 DevTools endpoint never became available");

  ws = new WebSocket(targets.webSocketDebuggerUrl);
  await new Promise((res, rej) => {
    ws.onopen = res;
    ws.onerror = () => rej(new Error("CDP websocket failed"));
    setTimeout(() => rej(new Error("CDP websocket timeout")), 10000);
  });

  let nextId = 1;
  const pending = new Map();
  ws.onmessage = (ev) => {
    const msg = JSON.parse(ev.data);
    if (msg.id && pending.has(msg.id)) {
      const { resolve } = pending.get(msg.id);
      pending.delete(msg.id);
      resolve(msg);
    }
  };
  const send = (method, params = {}) =>
    new Promise((resolve) => {
      const id = nextId++;
      pending.set(id, { resolve });
      ws.send(JSON.stringify({ id, method, params }));
    });
  const evaluate = async (expression) => {
    const r = await send("Runtime.evaluate", {
      expression,
      awaitPromise: true,
      returnByValue: true,
      userGesture: true,
    });
    if (r.result?.exceptionDetails) return `EXCEPTION: ${r.result.exceptionDetails.text}`;
    return r.result?.result?.value;
  };

  await send("Runtime.enable");
  await sleep(1500);

  // ---- positive: application actually rendered and loaded local assets ----
  const rendered = await evaluate("document.querySelector('#root')?.children.length ?? 0");
  check("React application rendered", rendered > 0, `root children=${rendered}`);

  const assets = await evaluate(
    "performance.getEntriesByType('resource').filter(e=>/assets\\/.*\\.(js|css)/.test(e.name)).map(e=>e.name)",
  );
  check("local application assets loaded", Array.isArray(assets) && assets.length > 0, (assets ?? []).join(", "));

  // ---- positive: Tauri IPC still works under the strict policy ----
  const internals = await evaluate("typeof window.__TAURI_INTERNALS__");
  check("Tauri IPC internals present", internals === "object", String(internals));

  const greet = await evaluate(
    "(async()=>{ try { return await window.__TAURI_INTERNALS__.invoke('greet',{name:'PAIGE'}) } catch(e){ return 'ERR:'+e } })()",
  );
  check("Tauri invoke('greet') succeeds", typeof greet === "string" && greet.includes("PAIGE"), String(greet));

  // ---- negative: external network fetch must be blocked ----
  const fetchRes = await evaluate(
    "(async()=>{ try { await fetch('https://example.com/'); return 'ALLOWED' } catch(e){ return 'BLOCKED: '+e.message } })()",
  );
  check("external https fetch blocked", String(fetchRes).startsWith("BLOCKED"), String(fetchRes));

  // ---- negative: remote script injection must be blocked ----
  const scriptRes = await evaluate(
    "(async()=>await new Promise(res=>{const s=document.createElement('script');s.src='https://example.com/evil.js';s.onload=()=>res('ALLOWED');s.onerror=()=>res('BLOCKED');document.head.appendChild(s);setTimeout(()=>res('TIMEOUT'),8000)}))()",
  );
  check("remote <script> injection blocked", scriptRes === "BLOCKED", String(scriptRes));

  // ---- negative: inline script execution must be blocked ----
  // (eval() and the Function constructor are governed by the same
  //  script-src directive without 'unsafe-eval'; they are verified by
  //  the static policy check and by this inline-script test, because
  //  CDP Runtime.evaluate is not subject to page CSP in this WebView2
  //  version, so an eval() string cannot be a faithful negative test.)
  const inlineRes = await evaluate(
    "(async()=>await new Promise(res=>{const s=document.createElement('script');s.textContent='window.__inlineRan=true';document.head.appendChild(s);setTimeout(()=>res(window.__inlineRan===true?'ALLOWED':'BLOCKED'),1000)}))()",
  );
  check("inline <script> execution blocked", inlineRes === "BLOCKED", String(inlineRes));

  // second variant: inline event handler must also be blocked
  const handlerRes = await evaluate(
    "(async()=>await new Promise(res=>{const img=new Image();img.onerror=()=>res('BLOCKED');img.src='data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=';img.setAttribute('onerror','window.__h=1');document.body.appendChild(img);setTimeout(()=>res(window.__h===1?'ALLOWED':'BLOCKED'),1000)}))()",
  );
  check("inline event handler blocked", handlerRes === "BLOCKED", String(handlerRes));

  // ---- negative: external websocket must be blocked ----
  const wsRes = await evaluate(
    "(async()=>await new Promise(res=>{ try { const w=new WebSocket('wss://example.com/socket'); w.onopen=()=>res('ALLOWED'); w.onerror=()=>res('BLOCKED'); setTimeout(()=>res('TIMEOUT'),6000) } catch(e){ res('BLOCKED: '+e.message) } }))()",
  );
  check("external websocket blocked", String(wsRes).startsWith("BLOCKED"), String(wsRes));

  // ---- negative: no filesystem/shell bridge is exposed to the page ----
  const bridges = await evaluate(
    "Object.keys(window.__TAURI__ ?? {}).concat(['fs','shell','http'].filter(k=>typeof window[k]==='object'))",
  );
  check("no fs/shell/http bridge exposed", Array.isArray(bridges) && !bridges.some((b) => ["fs", "shell", "http"].includes(b)), (bridges ?? []).join(",") || "none");

  exitCode = results.some((r) => !r.pass) ? 1 : 0;
} catch (err) {
  console.error("RUNTIME TEST ERROR:", err.message);
  exitCode = 1;
} finally {
  try { ws?.close(); } catch {}
  try { execSync(`taskkill /PID ${child.pid} /T /F`, { stdio: "ignore" }); } catch {}
  console.log(`\n${results.filter((r) => r.pass).length}/${results.length} runtime security checks passed`);
  process.exit(exitCode);
}
