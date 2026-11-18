#!/usr/bin/env node
// Increment 1B - functional broker tests against the REAL release executable.
// Return values from Rust are snake_case (serde default); invoke ARGUMENTS must
// be camelCase (Tauri converts to the snake_case Rust parameter names).
import { spawn, execSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const exe = process.env.PAIGE_EXE
  ? path.resolve(process.env.PAIGE_EXE)
  : path.join(root, "src-tauri/target/release/paige-desktop.exe");
const PORT = 9227;
const LAB = "C:\\Users\\tonig\\paige-desktop-lab";
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const results = [];
const check = (name, pass, detail = "") => {
  results.push({ name, pass: !!pass, detail });
  console.log(`${pass ? "PASS" : "FAIL"}  ${name}${detail ? `   [${String(detail).slice(0, 200)}]` : ""}`);
};

const child = spawn(exe, [], {
  env: { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT}` },
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

  const s0 = await invoke("broker_status");
  check("1. initial status: not granted", s0.granted === false, JSON.stringify(s0));

  const g = await invoke("broker_grant_root", { root: LAB, read: true, write: true });
  check("2. grant root succeeds", g.granted === true && g.read === true && g.write === true && g.root === LAB, JSON.stringify(g));

  const list = await invoke("broker_list_files");
  check("3. list files succeeds and includes README.md", Array.isArray(list) && list.some((f) => f.rel_path === "README.md"), JSON.stringify(list)?.slice(0, 120));

  const read = await invoke("broker_read_file", { path: "README.md" });
  check("4. read succeeds with content and sha256", typeof read.content === "string" && typeof read.sha256 === "string" && read.sha256.length === 64, `size=${read.size}`);
  const originalContent = read.content;
  const originalSha = read.sha256;

  const prop = await invoke("broker_propose_write", { path: "README.md", newContent: "BROKER RUNTIME TEST\n", expectedSha256: originalSha });
  check("5. propose write returns proposal + token", prop.proposal_id && prop.approval_token && prop.approval_token.length === 8, `id=${prop.proposal_id}`);

  const badApprove = await invoke("broker_approve", { proposalId: prop.proposal_id, approvalToken: "000000" });
  check("6. wrong approval token rejected", badApprove?.__err, JSON.stringify(badApprove));

  const goodApprove = await invoke("broker_approve", { proposalId: prop.proposal_id, approvalToken: prop.approval_token });
  check("7. correct approval token accepted", goodApprove === "approved", String(goodApprove));

  const receipt = await invoke("broker_apply", { proposalId: prop.proposal_id });
  check("8. apply commits with readback_verified", receipt?.readback_verified === true && receipt?.after_sha256 !== receipt?.before_sha256, JSON.stringify(receipt));

  const after = await invoke("broker_read_file", { path: "README.md" });
  check("9. readback content and checksum match", after.content === "BROKER RUNTIME TEST\n" && after.sha256 === receipt?.after_sha256, `sha=${after.sha256}`);

  const reapply = await invoke("broker_apply", { proposalId: prop.proposal_id });
  check("10. re-apply of applied proposal rejected", reapply?.__err, JSON.stringify(reapply));

  const rev = await invoke("broker_revoke");
  check("11. revoke succeeds", rev.granted === false && rev.read === false && rev.write === false, JSON.stringify(rev));

  const listAfter = await invoke("broker_list_files");
  check("12. list denied after revoke", listAfter?.__err, JSON.stringify(listAfter));

  const readAfter = await invoke("broker_read_file", { path: "README.md" });
  check("13. read denied after revoke", readAfter?.__err, JSON.stringify(readAfter));

  await invoke("broker_grant_root", { root: LAB, read: true, write: true });
  for (const bad of ["../secret.txt", "C:\\Windows\\System32\\drivers\\etc\\hosts", "\\\\server\\share\\x.txt", "sub/../../etc/passwd", "README.md:stream"]) {
    const r = await invoke("broker_read_file", { path: bad });
    check(`14. unauthorized path denied: ${bad}`, r?.__err, r?.__err ?? "NO ERROR (BAD)");
  }

  const prop2 = await invoke("broker_propose_write", { path: "README.md", newContent: "SECOND\n", expectedSha256: after.sha256 });
  await invoke("broker_deny", { proposalId: prop2.proposal_id });
  const deniedApply = await invoke("broker_apply", { proposalId: prop2.proposal_id });
  check("15. denied proposal cannot be applied", deniedApply?.__err, JSON.stringify(deniedApply));

  const prop3 = await invoke("broker_propose_write", { path: "README.md", newContent: "THIRD\n", expectedSha256: after.sha256 });
  await invoke("broker_cancel", { proposalId: prop3.proposal_id });
  const cancelledApply = await invoke("broker_apply", { proposalId: prop3.proposal_id });
  check("16. cancelled proposal cannot be applied", cancelledApply?.__err, JSON.stringify(cancelledApply));

  const cur = await invoke("broker_read_file", { path: "README.md" });
  const restoreProp = await invoke("broker_propose_write", { path: "README.md", newContent: originalContent, expectedSha256: cur.sha256 });
  if (restoreProp && restoreProp.proposal_id) {
    await invoke("broker_approve", { proposalId: restoreProp.proposal_id, approvalToken: restoreProp.approval_token });
    const restoreReceipt = await invoke("broker_apply", { proposalId: restoreProp.proposal_id });
    const restored = await invoke("broker_read_file", { path: "README.md" });
    check("17. original content restored", restored.content === originalContent && restored.sha256 === originalSha, `sha=${restored.sha256}`);
  } else {
    check("17. original content restored", false, "could not create restore proposal: " + JSON.stringify(restoreProp));
  }

  await invoke("broker_revoke");
  const finalStatus = await invoke("broker_status");
  check("18. final status: not granted", finalStatus.granted === false, JSON.stringify(finalStatus));

  const failed = results.filter((r) => !r.pass).length;
  console.log(`\n${results.length - failed}/${results.length} broker runtime checks passed`);
  exitCode = failed === 0 ? 0 : 1;
} catch (err) {
  console.error("RUNTIME TEST ERROR:", err.message);
  exitCode = 1;
} finally {
  try { ws?.close(); } catch {}
  try { execSync(`taskkill /PID ${child.pid} /T /F`, { stdio: "ignore" }); } catch {}
  process.exit(exitCode);
}
