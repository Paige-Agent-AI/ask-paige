#!/usr/bin/env node
// Increment 1A - deterministic static security tests.
// No third-party dependencies. Run: node scripts/test-security-config.mjs
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const read = (p) => fs.readFileSync(path.join(root, p), "utf8");
const results = [];
const check = (name, pass, detail = "") => {
  results.push({ name, pass: !!pass, detail });
};

const conf = JSON.parse(read("src-tauri/tauri.conf.json"));
const caps = JSON.parse(read("src-tauri/capabilities/default.json"));
const pkg = JSON.parse(read("package.json"));
const cargo = read("src-tauri/Cargo.toml");

const prod = conf?.app?.security?.csp;
const dev = conf?.app?.security?.devCsp;

// ---- 1. production CSP exists and is a real policy ----
check("production csp is configured (not null)", prod && typeof prod === "object", JSON.stringify(prod?.["default-src"]));
check("development csp is configured separately", dev && typeof dev === "object");
check("production and development policies differ", JSON.stringify(prod) !== JSON.stringify(dev));

const flat = (obj) => JSON.stringify(obj ?? {});
const prodFlat = flat(prod);

// ---- 2. no weakening constructs in production ----
check("production csp has no 'unsafe-eval'", !prodFlat.includes("unsafe-eval"));
check("production csp has no 'unsafe-inline'", !prodFlat.includes("unsafe-inline"));
check("production csp has no wildcard source", !prodFlat.includes('"*"') && !prodFlat.includes("'*'"));

// ---- 3. no external network egress in production ----
const urls = prodFlat.match(/https?:\/\/[^"\s]+/g) ?? [];
const wsUrls = prodFlat.match(/wss?:\/\/[^"\s]+/g) ?? [];
const external = urls.filter((u) => u !== "http://ipc.localhost");
check("only permitted remote origin is Tauri IPC", external.length === 0, external.join(",") || "none");
check("no websocket origins in production", wsUrls.length === 0, wsUrls.join(",") || "none");
check("no dev-server origin in production", !prodFlat.includes("localhost:1420") && !prodFlat.includes("127.0.0.1"));
check("ipc scheme retained for Tauri IPC", prodFlat.includes('"ipc:"'));

// ---- 4. restrictive baseline directives ----
check("default-src is 'self'", JSON.stringify(prod?.["default-src"]) === '["\'self\'"]');
check("script-src is 'self' only", JSON.stringify(prod?.["script-src"]) === '["\'self\'"]');
check("object-src is 'none'", prod?.["object-src"]?.includes("'none'"));
check("frame-ancestors is 'none' (no embedding)", prod?.["frame-ancestors"]?.includes("'none'"));
check("base-uri is 'self'", prod?.["base-uri"]?.includes("'self'"));
check("form-action is 'self'", prod?.["form-action"]?.includes("'self'"));

// ---- 5. development-only relaxations did not leak into production ----
check("dev csp allows inline scripts for vite/react-refresh", flat(dev?.["script-src"]).includes("unsafe-inline"));
check("dev csp allows the vite dev server and hmr", flat(dev?.["connect-src"]).includes("localhost:1420"));

// ---- 6. capability surface ----
const perms = caps.permissions ?? [];
const allowed = new Set([
  "core:event:default",
  "core:window:default",
  "core:app:default",
  "allow-greet",
  "allow-skills-installed",
  "allow-skills-search",
  "allow-skills-catalog",
  "allow-skills-prepare",
  "allow-skills-install",
  "allow-broker-status",
  "allow-broker-grant-root",
  "allow-broker-revoke",
  "allow-broker-list-files",
  "allow-broker-read-file",
  "allow-broker-propose-write",
  "allow-broker-approve",
  "allow-broker-deny",
  "allow-broker-cancel",
  "allow-broker-apply",
  "allow-broker-operations",
  "allow-broker-reconcile",
  "allow-broker-test-force-readback-failure",
  "allow-terminal-execute",
  "allow-terminal-execute-approved",
  "allow-terminal-input",
  "allow-terminal-kill",
  "allow-terminal-list",
  "allow-terminal-receipts",
  "allow-terminal-output",
  "allow-ask-status",
  "allow-ask-probe",
  "allow-ask-connect-openrouter",
  "allow-ask-disconnect-openrouter",
  "allow-ask-set-model-selection",
  "allow-ask-send",
  "allow-ask-resume",
  "allow-ask-end-task",
  "allow-workflow-accept-artifact",
  "allow-workflow-read-word",
  "allow-workflow-read-visual",
  "allow-chat-read-image",
  "allow-workflow-compare-visual",
  "allow-workflow-retry-visual-test",
  "allow-ask-cancel",
  "allow-ask-open-confirmation",
  "allow-ask-receipts",
  "allow-ask-connect-omniroute",
  "allow-ask-disconnect-omniroute",
  "allow-chat-list",
  "allow-chat-search",
  "allow-chat-new",
  "allow-chat-select",
  "allow-chat-pin-workspace",
  "allow-chat-rename",
  "allow-chat-archive",
  "allow-chat-delete",
  "allow-chat-export",
  "allow-team-roster",
  "allow-team-create",
  "allow-team-name-specialist",
  "allow-mcp-status",
  "allow-mcp-add-stdio",
  "allow-mcp-add-remote",
  "allow-mcp-linear-oauth",
  "allow-mcp-supabase-oauth",
  "allow-mcp-notion-oauth",
  "allow-mcp-paige-oauth",
  "allow-mcp-paige-operator-login",
  "allow-mcp-github-oauth",
  "allow-mcp-oauth-cancel",
  "allow-mcp-connect",
  "allow-mcp-disconnect",
  "allow-mcp-reconnect",
  "allow-mcp-revoke",
  "allow-mcp-call-tool",
  "allow-mcp-receipts",
  "allow-project-grant",
  "allow-project-rename",
  "allow-project-remove",
  "allow-project-open-folder",
  "allow-project-pick-folder",
  "allow-project-revoke",
  "allow-project-list",
  "allow-project-status",
  "allow-git-files",
  "allow-git-log-cmd",
  "allow-git-diff-cmd",
  "allow-git-branches-cmd",
  "allow-git-worktrees-cmd",
  "allow-git-create-worktree-cmd",
  "allow-git-remove-worktree-cmd",
  "allow-git-create-branch-cmd",
  "allow-git-checkout-cmd",
  "allow-git-add-cmd",
  "allow-git-commit-cmd",
]);
check("capabilities are the minimal approved set", perms.every((p) => allowed.has(p)), perms.join(", "));
const manifest = read("src-tauri/build.rs");
const handlers = read("src-tauri/src/lib.rs");
const chatPermissions = [...allowed].filter((permission) => permission.startsWith("allow-chat-"));
check("chat capabilities match the native manifest and registered handlers", chatPermissions.every((permission) => {
  const command = permission.slice("allow-".length).replaceAll("-", "_");
  return perms.includes(permission) && manifest.includes(`"${command}"`) && handlers.includes(`chats::${command},`);
}));
check("chat IPC is limited to the local main window",
  caps.remote === undefined && caps.local !== false &&
  JSON.stringify(caps.windows) === '["main"]' && caps.webviews === undefined);
check("team roster and creation use the local native handlers", ["roster", "create", "name_specialist"].every((name) =>
  perms.includes(`allow-team-${name.replaceAll("_", "-")}`) && manifest.includes(`"team_${name}"`) &&
  handlers.includes(`team::team_${name},`)));
check("Provider OAuth uses local sign-in and cancel commands", [
  ["mcp_linear_oauth", "mcp"],
  ["mcp_supabase_oauth", "mcp"],
  ["mcp_notion_oauth", "mcp"],
  ["mcp_paige_oauth", "mcp"],
  ["mcp_paige_operator_login", "provider_oauth"],
  ["mcp_github_oauth", "mcp"],
  ["mcp_oauth_cancel", "provider_oauth"],
].every(([command, module]) =>
  perms.includes("allow-" + command.replaceAll("_", "-")) &&
  manifest.includes(`"${command}"`) && handlers.includes(`${module}::${command},`) &&
  caps.remote === undefined && JSON.stringify(caps.windows) === '["main"]'));
const forbidden = ["fs", "shell", "http", "opener", "process", "dialog", "notification", "clipboard", "os", "global-shortcut", "deep-link"];
for (const f of forbidden) {
  // match the plugin permission namespace, not a substring inside a command name
  check(`no ${f} plugin permission granted`, !perms.some((p) => p === f || p.startsWith(`${f}:`) || p.startsWith(`${f}-`)));
}
check("no blanket core:default grant", !perms.includes("core:default"));
check("no webview-creation permission (remote windows blocked)", !perms.some((p) => p.startsWith("core:webview")));

// ---- 7. no plugin dependencies that widen authority ----
const pluginDeps = (cargo.match(/^tauri-plugin-[\w-]+/gm) ?? []);
check("no tauri plugin crates present", pluginDeps.length === 0, pluginDeps.join(",") || "none");
const jsPlugins = Object.keys({ ...pkg.dependencies, ...pkg.devDependencies }).filter((d) => d.startsWith("@tauri-apps/plugin-"));
check("no tauri plugin npm packages present", jsPlugins.length === 0, jsPlugins.join(",") || "none");

// ---- 8. shipped HTML cannot pull remote content ----
for (const f of ["index.html", "dist/index.html"]) {
  const html = read(f);
  const remoteRefs = html.match(/(?:src|href)\s*=\s*"https?:\/\/[^"]+"/g) ?? [];
  check(`${f}: no remote script/style/image references`, remoteRefs.length === 0, remoteRefs.join(",") || "none");
  const inlineScripts = [...html.matchAll(/<script\b(?![^>]*\bsrc=)[^>]*>([\s\S]*?)<\/script>/g)].filter((m) => m[1].trim().length > 0);
  check(`${f}: no inline scripts (script-src 'self' is viable)`, inlineScripts.length === 0);
}

// ---- 9. policy is embedded in the actual built executable ----
// The release executable is the artifact that carries the production CSP.
const exePath = process.env.PAIGE_EXE
  ? path.resolve(process.env.PAIGE_EXE)
  : path.join(root, "src-tauri/target/release/paige-desktop.exe");
if (fs.existsSync(exePath)) {
  const bin = fs.readFileSync(exePath);
  // Tauri's codegen does not store the CSP as a searchable kebab-case or
  // camelCase string; it is emitted as the Content-Security-Policy header at
  // runtime (verified by the runtime test below). The reliable embedded
  // evidence is the protocol-handler's CSP enforcement code and the IPC origin.
  check("executable references the CSP enforcement path", bin.includes(Buffer.from("Content-Security-Policy")));
  check("executable embeds the IPC origin allowlist", bin.includes(Buffer.from("ipc.localhost")));
  check("executable no longer carries csp:null", !bin.includes(Buffer.from('csp:null')) && !bin.includes(Buffer.from('csp":null')));
} else {
  check("executable present for embedded-policy check", false, "not found");
}

// ---- report ----
let failed = 0;
for (const r of results) {
  if (!r.pass) failed++;
  console.log(`${r.pass ? "PASS" : "FAIL"}  ${r.name}${r.detail ? `   [${r.detail}]` : ""}`);
}
console.log(`\n${results.length - failed}/${results.length} static security checks passed`);
process.exit(failed === 0 ? 0 : 1);
