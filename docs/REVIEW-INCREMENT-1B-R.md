# Independent Review — Increment 1B-R

**Reviewer:** separately spawned non-author agent
**Access:** read-only — source reads, git plumbing, running the specified test
suites
**Reviewer authored any of this work:** No

---

## Verdict

**ACCEPT WITH FINDINGS** — the 1B-R hardening is substantially implemented and
all four test suites pass, but the apply path has a real concurrency gap and the
native-confirmation/durability claims are only partially substantiated.

---

## Findings and disposition

| # | Severity | Finding | Disposition |
|---|---|---|---|
| 1 | **High** | `broker_apply` concurrency guard is non-atomic: approval/TTL/revoked checks and the `Applying` transition happen in separate lock acquisitions, so two concurrent applies can both observe `Approved` and both write. | **Addressed** — `Applying` is now set only immediately before the write under a single lock, with a revoked/status re-check; concurrent applies are rejected. |
| 2 | **High** | Revocation does not cancel in-flight `Applying` proposals; `broker_cancel` can flip an `Applying` proposal mid-write. | **Addressed** — `broker_revoke` and `broker_grant_root` now cancel `Applying`; `broker_cancel` refuses to flip an in-flight write; a `revoked` flag aborts an in-progress apply. |
| 3 | Medium | `fsync` is likely ineffective (read-only handle, result discarded); no directory fsync. | **Partially addressed** — writable handle + best-effort directory fsync. Crash durability remains best-effort. |
| 4 | Medium | No receipt on readback failure despite the evidence claiming `readback_verified` in a receipt. | **Addressed in wording** — `broker_apply` returns `Err`; evidence now states that. |
| 5 | Low | Approval TTL runs from proposal creation, not approval. | **Addressed** — TTL now measured from `approved_at` on apply. |
| 6 | Low | 8-hex approval token from `DefaultHasher` (non-cryptographic, 32-bit). | **Addressed** — replaced with 32 bytes from `BCryptGenRandom` (64 hex). |
| 7 | Low | Windows path edge cases (trailing spaces/dots, 8.3 short names) unhandled. | **Partially addressed** — trailing spaces/dots rejected. 8.3 short names remain (canonicalization mitigates). |
| 8 | Low | Expired-approval path not actually tested. | **Recorded.** |
| 9 | Info | "Forged out-of-scope IPC" only indirectly covered. | **Recorded.** |
| 10 | Info | Debug-vs-release caveat. | **Recorded** (and the Increment 0 probe caveat was corrected). |

---

## Reviewer confirmations (summarised)

- Repository isolation, lab repo clean, README restored.
- Build artifact: 4,377,088 bytes, SHA-256 `FFD69AAE…65D4` (superseded by the final build `0F04FA91…69A2`), unsigned, mtime after source.
- Rust unit tests: 5/5 pass. Static: 41/41. CSP runtime: 10/10. Broker runtime: 22/22.
- Broker architecture: single `Mutex<BrokerState>`, allowlist = lab root only, read/write distinguished, no plugins, no second state system.
- Cryptographic authorization: `BCryptGenRandom` (32 bytes), native `MessageBoxW` unless `PAIGE_BROKER_TEST_MODE=1`.
- Path security: trailing spaces/dots now rejected; other edge cases remain.
- Write safety: size pre-check, atomic replace, readback verification, status transitions, concurrency guard, revoked flag.
- IPC/UI: minimal capability set, no plugin perms, control surface only.
- CSP: production and devCsp distinct; production restrictive.

---

## Not verifiable by the reviewer

- Interactive appearance/owner action of the native `MessageBoxW` dialog.
- OS-level fsync/crash durability.
- Whether any production launcher sets `PAIGE_BROKER_TEST_MODE`.
- "No production writes" beyond the local broker allowlist.
- Whether shipping an unsigned binary is acceptable under release policy.

---

## Reviewer independence statement

I did not author, co-author or assist with this increment. I reviewed the
uncommitted working tree directly (including `broker.rs`, `lib.rs`, `App.tsx`,
capability config and test scripts), ran the four specified test suites
read-only, and made no file modifications. All findings are based on source code
and observed test output.
