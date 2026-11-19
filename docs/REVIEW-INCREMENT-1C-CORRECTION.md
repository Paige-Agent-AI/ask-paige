# Independent Review — Increment 1C Correction Pass (exact-head)

**Reviewer:** independent non-author subagent  
**Repo:** `C:\Users\tonig\paige-desktop`  
**Branch:** `feat/paige-desktop-prototype`  
**Reviewed commit (exact HEAD):** `8b94e5b305e8c13039679f07d165a16bb3528344`  
**Diff base:** `e73b045c7051a8479980acd162eef8b56fb6fcbe`  
**Reviewer authored any of this work:** No  

---

## Per-correction verification (diff `e73b045..8b94e5b`)

**Scope of diff:** exactly 5 files — `src-tauri/src/terminal.rs`,
`screens/Terminal.tsx`, `scripts/test-terminal-runtime.mjs`, plus two new docs
files. No changes to `build.rs`, `capabilities/default.json`, `tauri.conf.json`,
or any Cargo manifest — **no new permissions, no plugin crates, no security
posture weakening.**

1. **Regression count reconciliation** — VERIFIED. Evidence doc records the
   corrected arithmetic: original total **109/109 (19+41+10+26+13)**;
   the 119/119 miscount is explicitly flagged as an arithmetic error.
2. **Fail-closed missing root** — VERIFIED. `resolve_cwd()` uses
   `allowed_roots.first().cloned().ok_or_else(...)` — never indexes
   `allowed_roots[0]` unconditionally. Returns a structured error
   ("execution refused (fail closed)"). `spawn_and_manage` propagates via `?`,
   so execution is refused without panic. Two new unit tests cover this.
3. **Verified kill result** — VERIFIED. `kill_process_tree` returns `Result`;
   the `ControlMsg::Kill` handler captures the error into `kill_error`; final
   status maps to `SessionStatus::KillFailed` / `"kill_failed: <e>"` instead of
   falsely reporting `"killed"`. `terminal_list` exposes `kill_failed`.
4. **Quoted-argument parsing** — VERIFIED. The tokenizer in `Terminal.tsx`
   handles double quotes and backslash-escaped quotes, emitting a plain
   `args[]` array passed to `TerminalExecuteRequest`. Backend uses
   `Command::new(&resolved_exe).args(&request.args)` — no shell is ever invoked;
   `resolve_executable` still rejects explicit paths, metacharacters and shell
   builtins. No allowlist weakening.
5. **Receipt counts + token GC** — VERIFIED. `stdout_lines`/`stderr_lines`
   fields added to `ManagedSession`, incremented in the actual reader threads,
   and copied into `TerminalReceipt` (replacing the hardcoded approximate
   zeros). `sweep_expired_pending()` retains only unexpired pending tokens and
   is called at the top of `terminal_execute`; unit test confirms expired
   removal / fresh retention.
6. **New runtime checks** — VERIFIED present: 7b (node receipt records real
   `stdout_lines >= 1`) and 8a/8b (spaced argument reaches the process intact).
   Both pass live.
7. **No cloud inference, provider activation, MCP implementation, Git
   mutation, or production integration** — CONFIRMED.

---

## Regression test totals (re-run by reviewer)

| Suite | Total |
|---|---|
| Rust unit tests (`cargo test --lib`) | **22/22** |
| Static security (`test:security`) | **41/41** |
| CSP runtime (`test:security:runtime`) | **10/10** |
| Broker runtime (`test:broker:runtime`) | **26/26** |
| Terminal runtime (`test:terminal:runtime`) | **16/16** |
| **Arithmetic sum** | **115/115** ✔ |

---

## Production binary verification

| Property | Value |
|---|---|
| Path | `src-tauri/target/release/paige-desktop.exe` |
| Size | **4,503,040 bytes** |
| SHA-256 | `25F0E67FECED57F8D230277760075C66C0A57860D6D1E5A3152D126D467D12F8` |
| `PAIGE_BROKER_TEST_MODE` string | **Absent** (full-byte scan) |
| Build currency | Binary built before commit `8b94e5b`; the terminal runtime suite ran against this exact binary and passed correction-specific checks 7b and 8a/8b, proving the binary embeds the corrected code. No source changes after the build. |

---

## Verdict

**ACCEPT WITH FINDINGS**

Findings:
1. **(Minor)** One uncommitted documentation-only edit existed at review time
   (review-commit-hash reference in the evidence doc); committed afterward as
   `92e6cff` — no code impact.
2. **(Observation)** In the tokenizer, a backslash before a quote toggles
   quoting but the backslash itself is retained in the argument. Conservative
   Windows-style simplification; not a security issue.
3. **(Observation)** `sweep_expired_pending` runs only on `terminal_execute`,
   not on approval/list paths — expired tokens may linger in memory until the
   next execute call. Acceptable for this bounded correction.

No findings affect the security posture, correctness of the six directed
corrections, or the honesty of the reported test totals. All six owner-directed
corrections are correctly implemented and independently verified.