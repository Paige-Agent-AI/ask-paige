# Independent Review — Increment 1B-R (final)

**Reviewer:** separately spawned non-author agent
**Access:** read-only — source reads, git plumbing, running the four specified
test suites
**Reviewer authored any of this work:** No

---

## Verdict

**ACCEPT WITH FINDINGS** — the 1B-R closeout is functionally complete and all
four specified test suites pass, but the native owner-confirmation boundary
remains unexercised by automation and the evidence document still carried stale
hash/test-count/feature-name claims (corrected before commit).

---

## Findings and disposition

| # | Severity | Finding | Disposition |
|---|---|---|---|
| 1 | **High** | Native owner-confirmation boundary only proven by inspection; no automated test exercises the dialog. | **Recorded as limitation.** The production binary lacks the bypass string (verified), so a production build requires the dialog. Manual verification by the owner is required. |
| 2 | **Medium** | `broker_apply` releases the state lock between the `Applying` transition and the commit; a `broker_revoke` issued after the single pre-write `revoked` check cannot stop the commit. | **Recorded.** The evidence wording already says "prevents uncommitted changes where possible." |
| 3 | **Medium** | Readback failure returns `Err` with no `WriteReceipt`, despite the evidence implying a receipt on failure. | **Recorded.** Evidence wording corrected to state that `broker_apply` returns `Err` on readback failure. |
| 4 | Low | `validate_path` does not reject 8.3 short names reliably (heuristic), reserved device names with extensions (handled via stem split), control characters. | **Partially addressed** — trailing spaces/dots, reserved names, control chars and an 8.3 heuristic are now rejected. |
| 5 | Low | The `insecure-test-bypass` feature name is generic and could be enabled accidentally. | **Recorded.** Name is deliberately alarming; CI should deny it. |
| 6 | Low | `broker_operations` exposes the full in-memory operation log to any window capable of invoking it. | **Recorded.** Mitigated by capability scoping to `main`. |
| 7 | Medium | Evidence doc was stale (hash, test counts, feature name). | **Fixed** — hash `DC21919D…`, 11/11 unit tests, `insecure-test-bypass`. |
| 8 | Info | `cargo test --lib` runs debug profile; runtime tests use release binary. | **Recorded.** |
| 9 | Info | `target-test/` (~4.38 GB) is untracked and consumes disk. | **Recorded.** |
| 10 | Info | `broker_grant_root` cancels `Applying` proposals on re-grant, but an in-flight apply continues. | **Recorded.** |

---

## Reviewer confirmations (summarised)

- Repository isolation, lab repo clean, README restored to `A39E33F4…`.
- Build artifact: 4,378,624 bytes, SHA-256 `DC21919DEC1BE86E42E97E2981B0B27C1971CC64CCE9BFF45469F14224822116`, unsigned, mtime after source.
- Test-mode bypass compiled out of the production binary (string absent); present in the test-mode binary.
- Rust unit tests: 11/11 pass. Static: 41/41. CSP runtime: 10/10. Broker runtime: 22/22.
- Cryptographic authorization: `BCryptGenRandom` (32 bytes), native `MessageBoxW` unless `insecure-test-bypass` feature.
- Path security: trailing spaces/dots, reserved device names (with extensions), control chars, 8.3 aliases, UNC, ADS, traversal all rejected.
- Write safety: size pre-check, atomic replace, readback verification, status transitions, concurrency guard, revoked flag, TTL from approval.
- IPC: minimal capability set, no plugin perms.
- UI: Apply button disabled until approved.
- Lab repo: clean, README hash matches.

---

## Not verifiable by the reviewer

- Interactive `MessageBoxW` appearance and owner clickability.
- OS-level fsync/crash durability.
- Whether any production launcher sets `PAIGE_BROKER_TEST_MODE`.
- "No production writes" beyond the local broker allowlist.
- Whether shipping an unsigned binary is acceptable under release policy.
- Expired-approval path and broker runtime behavior against the final production binary.

---

## Reviewer independence statement

I did not author, co-author or assist with this increment. I reviewed the
uncommitted working tree directly (including `broker.rs`, `lib.rs`, `App.tsx`,
capability config and test scripts), ran the four specified test suites
read-only, and made no file modifications. My findings are based solely on
source code and observed test output.
