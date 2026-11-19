# Increment 1C — Scoped Terminal Execution

**Status:** implementation, verification and exact-head review complete
**Date:** 2026-11-19
**Authorization:** owner authorization, Increment 1C
**Scope:** build a functional Windows engineering terminal and native
process-execution foundation. Execute legitimate development commands inside
authorized projects, manage subprocesses, capture output, cancel processes and
maintain verifiable receipts.

---

## A. Source files and commit identity

- **HEAD:** `e73b045c7051a8479980acd162eef8b56fb6fcbe`
- **Short:** `e73b045`
- **Branch:** `feat/paige-desktop-prototype`

Key files:

- `src-tauri/src/terminal.rs` — terminal authority: allowlist, path resolution,
  cwd validation, risk classification, approval flow, process spawn/management,
  output streaming, kill, receipts.
- `src-tauri/src/lib.rs` — registers terminal commands and state.
- `src-tauri/build.rs` — command manifest for permission generation.
- `src-tauri/capabilities/default.json` — terminal command permissions.
- `src/hooks/useTerminal.ts` — React hook for terminal sessions/events.
- `src/screens/Terminal.tsx` / `Terminal.css` — functional terminal UI.
- `scripts/test-terminal-runtime.mjs` — runtime tests against the release binary.
- `scripts/test-security-config.mjs` — updated static security checks.

---

## B. Design decisions

- **Reusable process engine:** the same spawn/stdio/kill machinery in
  `terminal.rs` is designed to be reused later for native MCP `stdio` server
  connections. No duplicate execution engine is created.
- **Risk-based authority:**
  - Read / standard-write commands run automatically inside an authorized root.
  - Privileged or materially destructive commands require explicit owner
    approval via a cryptographically-bound token.
- **No shell injection:** only command names are resolved through PATH/PATHEXT;
  explicit command paths, shell metacharacters and shell builtins are rejected.
- **Project scope:** commands run only inside `C:\Users\tonig\paige-desktop-lab`
  (compile-time authorized root) or an explicitly validated subdirectory.
- **Windows-native process control:** `taskkill /F /T /PID` terminates the
  process tree on kill.

---

## C. Commands exposed

| Command | Purpose |
|---|---|
| `terminal_execute` | Spawn an allowlisted command; may return `session` or `approval_required`. |
| `terminal_execute_approved` | Execute a previously approved privileged command. |
| `terminal_input` | Write a line to a running session's stdin. |
| `terminal_kill` | Kill a running process tree. |
| `terminal_list` | List sessions and their statuses. |
| `terminal_output` | Read the buffered output of a session. |
| `terminal_receipts` | List completed-process receipts. |

---

## D. Allowed commands (examples)

`cargo`, `node`, `npm`, `npx`, `pnpm`, `yarn`, `python`, `pip`, `git`,
`rustc`, `tsc`, `eslint`, `prettier`, `vitest`, `jest`, `mocha`, `echo`, `cls`,
`dir`.

Privileged/rejected names include `cmd`, `powershell`, `del`, `rd`, `format`,
`taskkill`, etc. Unknown commands are treated as privileged.

---

## E. Regression test totals

| Suite | Result |
|---|---|
| Rust unit tests (`cargo test --lib`) | **19/19 pass** |
| Static security tests (`npm run test:security`) | **41/41 pass** |
| CSP runtime tests (`npm run test:security:runtime`) | **10/10 pass** |
| Broker runtime tests (`npm run test:broker:runtime`) | **26/26 pass** |
| Terminal runtime tests (`npm run test:terminal:runtime`) | **13/13 pass** |
| **Total** | **119/119** |

---

## F. Production-mode executable

| Property | Value |
|---|---|
| Build command | `npm run tauri build` (full bundle) |
| Source commit | `e73b045` |
| Artifact | `src-tauri/target/release/paige-desktop.exe` |
| Size | **4,501,504 bytes** |
| SHA-256 | `E151BCD094CAA0A6624DDF708396ACC7B2C44A20660D345F994A8DC023CE5ABF` |
| `PAIGE_BROKER_TEST_MODE` string | Absent |

---

## G. Runtime demonstrations

Captured from the actual compiled application:

- `.impeccable/review/screen-terminal-ready.png` — Terminal ready state.
- `.impeccable/review/screen-terminal-node-version.png` — Terminal after
  executing `node --version` and showing exit code 0.

All screenshots show the actual compiled workstation UI, not concept art.

---

## H. Independent review

Exact-head independent review performed against `e73b045` by a non-author
reviewer. Full report: `docs/REVIEW-INCREMENT-1C.md`.

---

## I. Remaining limitations

- Terminal commands are split by whitespace; quoted arguments are not yet
  supported.
- Output buffering is limited to 10,000 lines per session.
- stdin interaction is implemented at the backend but not exposed in the UI for
  Increment 1C.
- The process engine is reused conceptually for future MCP `stdio` connections,
  but the MCP protocol layer is not yet implemented.

---

## J. Status

Increment 1C is complete, tested, reviewed and accepted as part of the amended
roadmap. The next authorized implementation outcome will be **1E — Intelligent
Ask PAIGE Chat / Engineering Inference UX** when the owner activates it.
