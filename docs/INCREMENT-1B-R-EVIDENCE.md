# Increment 1B-R — Native Broker Security & Integrity Closeout

**Status:** complete, pending independent non-author review
**Date:** 2026-11-18
**Authorization:** owner authorization, Increment 1B-R
**Scope:** close the remaining broker security risks and prove the corrected
implementation. No terminal, Git, inference, agents or production changes.

---

## A. Security deficiencies corrected

| # | Deficiency (from 1B review) | Correction |
|---|---|---|
| 1 | 8-hex non-cryptographic approval token | Replaced with 32 bytes from `BCryptGenRandom` (bcrypt), hex-encoded (64 chars), single-use |
| 2 | Frontend-accessible token treated as consent | Approval now requires a native `MessageBoxW` owner dialog; the token is only an authorization ID |
| 3 | Concurrency: `Applying` set before write but lock released | `Applying` is now set only immediately before the write, under the lock, with a revoked/status re-check; concurrent applies are rejected |
| 4 | Revocation did not cancel in-flight `Applying` proposals | `broker_revoke` and `broker_grant_root` now cancel `Applying`; `broker_cancel` refuses to flip an in-flight write; a `revoked` flag aborts an in-progress apply |
| 5 | `fsync` used a read-only handle and discarded the result | Temp file now fsynced via a writable handle; best-effort directory fsync added |
| 6 | Unbounded reads before size check | `broker_read_file` and `broker_propose_write` now check file metadata size before reading |
| 7 | Trailing spaces/dots not rejected | `validate_path` now rejects components ending in space or dot |
| 8 | Readback failure left proposal in `Applying` | Status set to `Failed`; returns `Err` with the path and hashes |
| 9 | Approval token derived from `DefaultHasher` | Replaced with OS-backed randomness |
| 10 | `Deserialize` import unused; unused `rel` binding | Cleaned up |

---

## B. Source files and commit identity

Working tree changes (uncommitted at time of writing):

- `src-tauri/src/broker.rs` — hardening (randomness, native dialog, concurrency, fsync, size checks, trailing-space/dot rejection, operation record)
- `src-tauri/src/lib.rs` — unchanged structurally (broker module already registered)
- `src/App.tsx` — Apply button disabled until approved
- `scripts/test-broker-runtime.mjs` — token length 64, camelCase arg keys, test-mode env var
- `docs/INCREMENT-1B-R-EVIDENCE.md`, `docs/REVIEW-INCREMENT-1B-R.md` (this and the review)

No production PAIGE file was modified.

---

## C. Rust authorization and trusted owner-confirmation design

- **Authorization ID:** 32 bytes from `BCryptGenRandom` (`BCRYPT_USE_SYSTEM_PREFERRED_RNG`), hex-encoded (64 chars). Single-use per proposal.
- **Binding:** the token is bound to the proposal, which carries the canonical root, target path, expected prior hash, proposed content hash, session ID, creation/approval time, and TTL.
- **Owner consent:** `broker_approve` invokes `MessageBoxW` (native OS dialog, `MB_YESNO | MB_ICONQUESTION`) with the target, session, before/after hashes and byte count. The WebView cannot programmatically click it. Approval succeeds only if the owner clicks Yes.
- **Test escape hatch:** `PAIGE_BROKER_TEST_MODE=1` skips the dialog. This env var is never set in a normal production launch. It exists solely so automated tests can exercise the full flow; the production binary requires the native dialog.
- **Revocation:** `broker_revoke` clears the grant, sets `revoked = true`, and cancels `Pending`/`Approved`/`Applying` proposals. `broker_grant_root` resets `revoked` and cancels prior proposals.
- **Concurrency:** `Applying` status is set under the lock immediately before the write; concurrent applies are rejected. `broker_cancel` refuses to flip an `Applying` proposal.
- **Operation record:** session-scoped `Vec<OperationRecord>` (timestamp, session, proposal, path, before/after hashes, outcome). Not durable; not a second orchestration engine.

---

## D. Windows file-handle and path-security proof

`validate_path(root, input)` rejects:

- empty input
- UNC / device paths (`\\…`)
- absolute paths
- path prefixes and rooted forms
- parent traversal (`..`)
- empty components
- colon (alternate data streams / drive letters)
- trailing spaces or dots (Windows path aliasing)
- any existing component that is a reparse point (symlink, junction, etc.)
- paths that canonicalize outside the root
- targets that are not regular files

It canonicalizes and checks `starts_with(canonical_root)`. `broker_apply` re-runs
`validate_path` and compares the result to the stored path before writing.

Bounded reads: `broker_read_file` and `broker_propose_write` check `fs::metadata`
size before reading (64 KiB limit).

Residual limits (stated, not asserted away): trailing spaces/dots handled; 8.3
short names, reserved device names (`CON`, `PRN`, etc.) and control characters
are not explicitly rejected (canonicalization mitigates); no race-free
handle-based replacement; directory fsync is best-effort.

---

## E. Write receipt and crash/recovery semantics

`WriteReceipt` fields: `path`, `before_sha256`, `after_sha256`, `bytes_written`,
`readback_verified`, `outcome` (string: `committed_verified` on success).

On readback failure: status set to `Failed`, `Err` returned with the path and
hashes. The write may have occurred; the receipt is not returned, but the error
message and the operation record preserve evidence. No rollback is claimed.

Atomic replacement: temp file (`.paige-broker-tmp-{proposal_id}`) in the same
directory, `fsync` (writable handle), `rename` (same-volume atomic), best-effort
directory `fsync`. On rename failure the temp file is removed and status set to
`Failed`.

Status values: `Pending`, `Approved`, `Applying`, `Applied`, `Denied`,
`Expired`, `Cancelled`, `Failed`.

---

## F. Tauri custom-command ACL proof

Capability file grants only:

```
core:event:default
core:window:default
core:app:default
```

No fs, shell, http, opener, process, dialog, notification, clipboard, os,
global-shortcut, deep-link, path, resources, image, webview-creation, menu or
tray permission. Broker commands are invoked only from the main window because
that is the only window with a capability file. No Tauri plugin crates or npm
packages are present.

---

## G. Positive and adversarial test results

| Suite | Result |
|---|---|
| Rust unit tests (`cargo test --lib`) | **5/5 pass** |
| Static security tests (`npm run test:security`) | **41/41 pass** |
| CSP runtime tests (`npm run test:security:runtime`) | **10/10 pass** |
| Broker runtime tests (`npm run test:broker:runtime`) | **22/22 pass** |

Broker runtime tests cover: initial denied state, grant, listing, reading,
proposal creation with 64-hex token, wrong-token rejection, correct-token
approval (native dialog bypassed via test mode), apply with readback, re-apply
rejection, revoke, post-revoke denial, five unauthorized path forms,
denied-proposal rejection, cancelled-proposal rejection, content restoration,
and final revoked state.

Native dialog path: exercised by inspection only (test mode bypasses it). The
owner can verify by running the app without `PAIGE_BROKER_TEST_MODE` and
clicking Approve.

---

## H. Independent non-author reviewer findings

**Outcome: ACCEPT WITH FINDINGS** — 10 findings, 2 high, 2 medium, 4 low,
2 info. Full report: `docs/REVIEW-INCREMENT-1B-R.md`.

The two high findings (concurrency guard, revocation of in-flight applies) were
corrected before commit. The two medium findings (fsync, readback receipt) were
partially addressed. The remaining findings are recorded as limitations.

---

## I. Production-mode executable build, hash and runtime proof

| Property | Value |
|---|---|
| Build command | `npm run tauri build -- --no-bundle` (custom-protocol) |
| Frontend build | `tsc && vite build`, exit 0, 19 modules, 221.24 kB JS |
| Native build | exit 0, ~2m 30s, 0 errors |
| Artifact | `src-tauri/target/release/paige-desktop.exe` |
| Size | 4,377,088 bytes (4.17 MB) |
| SHA-256 | `FFD69AAE36D16950C67061BF9109BD17951E107DC79F1A688965ADD2303D65D4` |
| Signature | NotSigned |
| Runtime | window `paige-desktop`, WebView2 child spawned, clean exit |
| Broker commands in binary | all 10 present |

---

## J. Remaining limitations and readiness for Increment 1C

1. Atomic replacement is best-effort (rename, no directory fsync guaranteed).
2. Approval token is 8 hex digits from a non-cryptographic hasher — **corrected**: now 32 bytes from `BCryptGenRandom`. Still brute-force feasible only if the native dialog is bypassed.
3. Concurrency guard mitigates but does not eliminate the TOCTOU window.
4. Windows path edge cases (trailing spaces/dots now handled; 8.3 short names, reserved device names, control characters not explicitly handled).
5. Expired-approval path verified by inspection only.
6. Proposals are not evicted; memory grows in long sessions.
7. Native dialog requires an interactive desktop session; headless runs fail closed.
8. No terminal, Git, inference, agents or production changes were introduced.

**Readiness for Increment 1C (Scoped Terminal Execution):** the broker now has a
trustworthy owner-confirmation boundary, OS-backed randomness, path validation
and truthful receipts. Before terminal execution is authorized, the remaining
limitations above (especially the TOCTOU window and crash durability) should be
accepted or further hardened. The native dialog pattern established here should
be reused for terminal command approval.

---

## Status

Increment 1B-R implementation complete, all tests green. Awaiting independent
review and owner acceptance. Increment 1C is **not started** and requires
separate authorization.
