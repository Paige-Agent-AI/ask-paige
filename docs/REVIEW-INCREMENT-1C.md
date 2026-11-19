# Independent Review — Increment 1C (exact-head)

**Reviewer:** independent non-author subagent  
**Repo:** `C:\Users\tonig\paige-desktop`  
**Branch:** `feat/paige-desktop-prototype`  
**Reviewed commit (exact HEAD):** `e73b045c7051a8479980acd162eef8b56fb6fcbe`  
**Working tree status:** clean  
**Reviewer authored any of this work:** No  

---

## 1. Exact reviewed SHA

```text
e73b045c7051a8479980acd162eef8b56fb6fcbe
```

---

## 2. Independence statement

I did not author or co-author the changes under review. This is an independent, exact-head review of commit `e73b045`.

---

## 3. Per-area summary

### `src-tauri/src/terminal.rs`
- **Allowlist:** Hard-coded `ALLOWED_COMMANDS` table with per-command `TerminalRisk` (Read / StandardWrite / Privileged). Unknown commands default to `Privileged`.
- **Risk classification:** Extra blocklist `PRIVILEGED_NAMES` rejects shell/interpreter/system tooling (`cmd`, `powershell`, `bash`, `wsl`, `curl`, `wget`, etc.). `git clean` and `git reset` are explicitly privileged.
- **Path resolution:** `resolve_executable` searches only `PATH`/`PATHEXT` and rejects any command containing `/` or `\`; no explicit paths allowed.
- **CWD validation:** `validate_cwd` requires an absolute path, canonicalizes it, and confines execution to `ALLOWED_ROOTS` (`C:\Users\tonig\paige-desktop-lab`) or a subdirectory.
- **Approval flow:** Privileged commands return an `ApprovalRequired` response with a 32-byte token generated via `BCryptGenRandom` and a 300-second TTL. Tokens are single-use and expire on approval.
- **Process spawn/management:** Spawns with `std::process::Command::new(resolved_exe).args(...)` — argv array, no shell intermediary. Uses a dedicated manager thread plus stdout/stderr reader threads.
- **Output streaming:** Emits `terminal-output` events and retains the last 10,000 lines per session.
- **Kill / receipts:** Kill sends `taskkill /F /T /PID`; receipts record command line, cwd, exit code, and timestamps.
- **No duplicate engine:** Only this process engine exists; the file broker does not spawn processes.

### `src-tauri/src/lib.rs` & `src-tauri/build.rs`
All eight terminal commands (`terminal_execute`, `terminal_execute_approved`, `terminal_input`, `terminal_kill`, `terminal_list`, `terminal_receipts`, `terminal_output`) are registered in both files, matching the capability entries.

### `src-tauri/capabilities/default.json`
Permissions are limited to `core:event:default`, `core:window:default`, `core:app:default`, `allow-greet`, the broker commands, and the terminal commands. **No** `fs`, `shell`, `http`, `opener`, `process`, `dialog`, `notification`, `clipboard`, `os`, `global-shortcut`, `deep-link`, or `core:default` plugin grants are present.

### `src-tauri/Cargo.toml`
Dependencies are `tauri`, `serde`, `serde_json`, `sha2`, `hex`, and `tauri-build`. **No Tauri plugin crates** are added.

### Frontend terminal UI
- `src/hooks/useTerminal.ts` — manages listeners for `terminal-output`/`terminal-exit`, exposes `execute`, `approve`, `sendInput`, and `kill`.
- `src/screens/Terminal.tsx` / `Terminal.css` — functional terminal screen with command input, output scrollback, kill button, approval prompt, status panel, and keyboard handling.

### `scripts/test-terminal-runtime.mjs`
Runtime tests exercise the real release executable via the Chrome DevTools protocol: node/cargo execution, privileged-command approval gate, unknown-command rejection, CWD outside root rejection, process kill, and receipt enumeration.

### `docs/ROADMAP.md`
Controlling sequence is recorded as **1C → 1E → 1D → 1F → 1G → 1H**. Only 1C is active; queued outcomes are not implemented.

---

## 4. Regression suite results

| Suite | Result |
|---|---|
| `cargo test --lib` | **19 passed, 0 failed** |
| `npm run test:security` | **41/41 passed** |
| `npm run test:security:runtime` | **10/10 passed** |
| `npm run test:broker:runtime` | **26/26 passed** |
| `npm run test:terminal:runtime` | **13/13 passed** |
| **Total** | **119/119 checks passed** |

---

## 5. Production binary verification

| Property | Value |
|---|---|
| Path | `src-tauri/target/release/paige-desktop.exe` |
| Size | **4,501,504 bytes** |
| SHA-256 | `DE1395B5C9569F728AAD9EDD3DEA071022B90A3F9E477375E144566A2A115467` |
| `PAIGE_BROKER_TEST_MODE` present? | **No** |

---

## 6. Screenshot review

- `.impeccable/review/screen-terminal-ready.png` — Shows the Terminal screen with the scoped-execution welcome text, an input prompt, and a **ready** status panel. Confirms the terminal UI is rendered and functional.
- `.impeccable/review/screen-terminal-node-version.png` — Shows the result of running `node --version` (`v24.18.0`) followed by `Process exited (exit code 0)`. Confirms real command execution and output capture.

Both screenshots demonstrate a functional terminal.

---

## 7. Scope / unauthorized-activation check

No backend implementation of cloud inference, provider activation, MCP protocol, Git mutation, or production integration was started:
- `src-tauri/src` contains no references to OpenRouter, OpenAI, Anthropic, MCP/stdio clients, HTTP request crates, or Git operations.
- `Cargo.toml` includes no networking/plugin crates.
- The only matches in `src/screens` are placeholder UI labels/strings; there is no wiring behind them.

---

## 8. Findings / limitations

1. **Receipt line counts are approximate zeros.** `TerminalReceipt.stdout_lines` and `stderr_lines` are hard-coded to `0`. Acceptable for 1C; should be populated or removed later.
2. **Missing root fallback could panic.** If none of the `ALLOWED_ROOTS` canonicalize successfully at startup, `spawn_and_manage` will panic when it falls back to `allowed_roots[0]` for an omitted `cwd`. Low risk while the lab directory exists.
3. **Expired approval tokens are not garbage-collected.** Tokens remain in the `pending` map until used; TTL is only enforced at approval time. Usage-bound and not a security issue.
4. **Frontend command parser does not handle quoting.** Acknowledged as acceptable for Increment 1C.
5. **Kill result is ignored.** The manager thread marks the session `killed` even if `taskkill` fails.
6. **Production binary is unsigned**, consistent with the prototype phase.

---

## Verdict

**ACCEPT WITH FINDINGS**

Increment 1C meets its scoped-terminal objectives: allowlisted commands, root-confined execution, risk-based approval, safe process spawning without shell injection, output streaming/kill/receipts, and a functional frontend. All regression suites pass, the production binary does not contain the test-mode string, and no unauthorized cloud/inference/MCP/Git/production integration has been started. The findings are minor and do not block acceptance.
