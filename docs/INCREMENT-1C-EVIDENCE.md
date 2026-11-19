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

> **Count reconciliation (2026-11-19 correction):** the original closeout
> incorrectly reported **119/119**. The accurate original total was
> **109/109** (19 + 41 + 10 + 26 + 13). The miscount was an arithmetic error
> in the closeout report; no suite was omitted and no test evidence was
> missing. After the 1C corrections below, the suite grew to **115/115**.

| Suite | Result (original) | Result (after corrections) |
|---|---|---|
| Rust unit tests (`cargo test --lib`) | **19/19 pass** | **22/22 pass** |
| Static security tests (`npm run test:security`) | **41/41 pass** | **41/41 pass** |
| CSP runtime tests (`npm run test:security:runtime`) | **10/10 pass** | **10/10 pass** |
| Broker runtime tests (`npm run test:broker:runtime`) | **26/26 pass** | **26/26 pass** |
| Terminal runtime tests (`npm run test:terminal:runtime`) | **13/13 pass** | **16/16 pass** |
| **Total** | **109/109** | **115/115** |

---

## F. Production-mode executable

| Property | Value |
|---|---|
| Build command | `npm run tauri build` (full bundle) |
| Source commit | corrections applied within Increment 1C |
| Artifact | `src-tauri/target/release/paige-desktop.exe` |
| Size | **4,503,040 bytes** |
| SHA-256 | `25F0E67FECED57F8D230277760075C66C0A57860D6D1E5A3152D126D467D12F8` |
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
reviewer (verdict: ACCEPT WITH FINDINGS). Full report:
`docs/REVIEW-INCREMENT-1C.md`.

### 1C correction pass (owner-directed, 2026-11-19)

The owner directed a bounded reconciliation within Increment 1C. All six
corrections were applied:

1. **Regression count reconciled.** The closeout's "119/119" was an
   arithmetic error; the accurate original total was **109/109**. No suite
   was omitted and no test evidence was missing.
2. **Missing-root handling fixed.** `resolve_cwd` fails closed with a
   structured error when no authorized root is available — never panics,
   never launches. Covered by a new unit test.
3. **Kill result verified.** `taskkill`'s exit status is checked; a failed
   kill is reported truthfully as `kill_failed: <error>` in the exit event,
   session status and receipt (`SessionStatus::KillFailed`), instead of
   being claimed as `killed`.
4. **Quoted-argument parsing resolved.** The frontend tokenizer handles
   Windows-style double-quoted arguments (including escaped quotes); the
   backend continues to receive a plain argv array with no shell
   intermediary, so the allowlist and injection posture are unchanged.
   Verified end-to-end by a new runtime check
   (`node -e "console.log('quoted arg test')"`).
5. **Receipt accuracy corrected.** `stdout_lines` / `stderr_lines` are real
   per-session counts from the reader threads; receipts carry them.
   Expired approval-token garbage collection (`sweep_expired_pending`) was
   also added and is unit-tested.
6. **Rebuild + re-review.** Full rebuild, all suites re-run (115/115), and a
   fresh independent non-author review of the corrected code was obtained
   (see `docs/REVIEW-INCREMENT-1C-CORRECTION.md`).

The reviewed code commit is `b6bf0bc` (corrections); documentation-only
commits after it do not alter code.

---

## I. Remaining limitations

- Output buffering is limited to 10,000 lines per session.
- stdin interaction is implemented at the backend but not exposed in the UI for
  Increment 1C.
- The process engine is reused conceptually for future MCP `stdio` connections,
  but the MCP protocol layer is not yet implemented.
- Production binary is unsigned (prototype phase).

---

## J. Status

Increment 1C is complete, tested, reviewed and accepted as part of the amended
roadmap. The next authorized implementation outcome will be **1E — Intelligent
Ask PAIGE Chat / Engineering Inference UX** when the owner activates it.
