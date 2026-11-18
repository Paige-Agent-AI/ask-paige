# Independent Review — Increment 1B

**Reviewer:** separately spawned non-author agent
**Access:** read-only — source reads, git plumbing, running the specified test
suites
**Reviewer authored any of this work:** No

---

## Verdict

**ACCEPT WITH FINDINGS** — Increment 1B's native broker, IPC minimization, CSP and
tests are coherent and all advertised tests pass (5 Rust + 41 static + 10 CSP
runtime + 22 broker runtime), but several concurrency/failure-handling defects
and evidence overstatements should be recorded before closure.

---

## Findings and disposition

| # | Severity | Finding | Disposition |
|---|---|---|---|
| 1 | **High** | `broker_apply` sets `Applying` before I/O but failure paths (path mismatch, checksum mismatch, size, rename) left the proposal stuck in `Applying`. | **Fixed** — status is now set to `Applying` only immediately before the write; validation failures leave it `Approved` (retryable); write/readback failures set `Failed`. |
| 2 | **High** | Revocation did not cancel in-flight `Applying` proposals; `broker_cancel` could flip an `Applying` proposal mid-write. | **Fixed** — `broker_revoke` and `broker_grant_root` now cancel `Applying`; `broker_cancel` refuses to cancel an in-flight write; a `revoked` flag aborts an in-progress apply. |
| 3 | Medium | `fsync` used a read-only handle and discarded the result; no directory fsync. | **Partially fixed** — temp file now fsynced via a writable handle; best-effort directory fsync added. Crash durability remains best-effort. |
| 4 | Medium | No receipt on readback failure despite the evidence claiming `readback_verified` in a receipt. | **Fixed in wording** — `broker_apply` returns `Err`; evidence now states that. |
| 5 | Low | Approval TTL runs from proposal creation, not approval. | **Recorded** as a limitation. |
| 6 | Low | 8-hex approval token from a non-cryptographic hasher. | **Recorded.** |
| 7 | Low | Windows path edge cases (trailing spaces/dots, 8.3 short names) unhandled. | **Recorded.** |
| 8 | Low | Expired-approval timing not actually tested. | **Recorded.** |
| 9 | Info | "Forged out-of-scope IPC" only indirectly covered. | **Recorded.** |
| 10 | Info | Debug-vs-release caveat. | **Recorded** (and the 1A caveat was corrected). |

---

## Reviewer confirmations (summarised)

- Repository isolation, lab repo clean with README restored to `A39E33F4…`.
- Build artifact: 4,367,872 bytes, SHA-256 `E6BE9BB1…1B45`, unsigned, mtime after source.
- Rust unit tests: 5/5 pass.
- Static security: 41/41 pass. CSP runtime: 10/10 pass. Broker runtime: 22/22 pass.
- Broker architecture: single `Mutex<BrokerState>`, allowlist = lab root only, read/write
  distinguished, no plugins, no second state system.
- Capabilities: only `core:event:default`, `core:window:default`, `core:app:default`.
- CSP: production and devCsp distinct; production restrictive.
- UI: only `greet` and `broker_*` invokes; no direct filesystem access.

---

## Not verifiable by the reviewer

- Production PAIGE repository untouched (outside this repo).
- Whether all ten broker commands register at runtime (exercised by CDP invocation
  of `greet`/`broker_*` and reported as passing).
- Exact build flags/environment.
- Owner authorization provenance.

---

## Reviewer independence statement

I did not author, co-author or assist with this increment. I reviewed read-only:
source reads, git plumbing, and the specified test suites (which leave the lab
repository restored to its original hash). No files were modified.
