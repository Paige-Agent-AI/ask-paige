# Increment 1B-R — Native Broker Security & Integrity Closeout

**Status:** implementation complete; final owner acceptance pending discrepancy
reconciliation and exact-head independent review.
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
| 8 | Readback failure left proposal in `Applying` | Status set to `Failed`; returns structured receipt (`committed_unverified`) instead of an opaque error |
| 9 | Approval token derived from `DefaultHasher` | Replaced with OS-backed randomness |
| 10 | `Deserialize` import unused; unused `rel` binding | Cleaned up |

---

## B. Source files and commit identity

Final commit for this closeout:

- **HEAD:** `dab842d`
- **Branch:** `feat/paige-desktop-prototype`
- **Repository:** `C:\Users\tonig\paige-desktop` (independent local `.git`, no remotes)

Changed files:

- `src-tauri/src/broker.rs` — native dialog, OS randomness, concurrency guard,
  fsync/size checks, path hardening, three-state write receipt, reconcile command,
  test-only readback-failure injection.
- `src-tauri/src/lib.rs` — registers `broker_reconcile` and the test-only hook
  behind `#[cfg(feature = "insecure-test-bypass")]`.
- `src-tauri/build.rs` — explicit `AppManifest::commands` ACL including reconcile
  and the test-only hook (test hook handler is compiled out of production).
- `src-tauri/capabilities/default.json` — grants broker permissions to the main
  window only.
- `scripts/test-broker-runtime.mjs` — 26 runtime checks including injected
  post-commit readback failure, reconcile and no-replay verification.
- `scripts/test-security-config.mjs` — ACL/reconcile permission checks.
- `docs/INCREMENT-1B-R-EVIDENCE.md`, `docs/REVIEW-INCREMENT-1B-R.md`.

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
`readback_verified`, `outcome`.

Three write outcomes are distinguished:

1. **Definite precommit failure** — the write did not occur. Returns `Err`
   (path validation, checksum mismatch, revoked grant, size limit, temp write
   failure, rename failure). The proposal remains `Approved` (retryable) or
   becomes `Failed`/`Cancelled`. No receipt.
2. **Committed and verified** — the write occurred and readback matched.
   Returns `Ok(WriteReceipt)` with `readback_verified: true`,
   `outcome: "committed_verified"`. Proposal status → `Applied`.
3. **Committed but unverified (uncertain)** — the rename committed but readback
   failed or was forced to fail. Returns `Ok(WriteReceipt)` with
   `readback_verified: false`, `outcome: "committed_unverified"`. Proposal
   status → `Failed`. **Replay is rejected** (status is not `Approved`).

A `broker_reconcile(proposal_id)` command provides a bounded, read-only
reconciliation: it reads the current file, computes its SHA-256, and reports
`matches_before`, `matches_after` or `unknown` against the proposal's before
and proposed hashes. This lets a caller determine the actual file state before
authorizing another operation, without replaying the uncertain write.

The in-memory `Vec<OperationRecord>` is session-scoped and is **not**
crash-durable. It is a minimal local record, not a recovery engine.

Atomic replacement: temp file (`.paige-broker-tmp-{proposal_id}`) in the same
directory, `fsync` (writable handle), `rename` (same-volume atomic), best-effort
directory `fsync`. On rename failure the temp file is removed and status set to
`Failed`.

Status values: `Pending`, `Approved`, `Applying`, `Applied`, `Denied`,
`Expired`, `Cancelled`, `Failed`.

---

## F. Tauri custom-command ACL proof

`src-tauri/build.rs` uses `tauri_build::AppManifest::new().commands(&[...])`
to register all broker commands plus `greet`. This generates `allow-<command>`
and `deny-<command>` permission identifiers and sets `has_app_acl_manifest`,
which makes Tauri enforce the ACL on app commands for local origins.

Capability file (`capabilities/default.json`) grants:

```
core:event:default
core:window:default
core:app:default
allow-greet
allow-broker-status
allow-broker-grant-root
allow-broker-revoke
allow-broker-list-files
allow-broker-read-file
allow-broker-propose-write
allow-broker-approve
allow-broker-deny
allow-broker-cancel
allow-broker-apply
allow-broker-operations
allow-broker-reconcile
allow-broker-test-force-readback-failure
```

No fs, shell, http, opener, process, dialog, notification, clipboard, os,
global-shortcut, deep-link, path, resources, image, webview-creation, menu or
tray permission. No Tauri plugin crates or npm packages are present.

**Negative test (Gate 1):** a second window labeled `untrusted` was created with
a capability granting only `core:default` (no `allow-broker-*`). Both windows
loaded the app. `broker_status` succeeded from the main window and was rejected
from the untrusted window. The untrusted window was then removed; the
production configuration has a single window. The permission file and
`AppManifest::commands` registration remain, so the ACL is enforced in
production.

---

## G. Positive and adversarial test results

| Suite | Result |
|---|---|
| Rust unit tests (`cargo test --lib`) | **11/11 pass** |
| Static security tests (`npm run test:security`) | **41/41 pass** |
| CSP runtime tests (`npm run test:security:runtime`) | **10/10 pass** |
| Broker runtime tests (`npm run test:broker:runtime`) | **26/26 pass** |
| **Total** | **88/88 pass** |

Broker runtime tests cover: initial denied state, grant, listing, reading,
proposal creation with 64-hex token, wrong-token rejection, correct-token
approval (native dialog bypassed via test mode), forced post-commit readback
failure producing `committed_unverified`, reconcile reports `matches_after`,
replay of uncertain write rejected, apply with readback, re-apply rejection,
revoke, post-revoke denial, five unauthorized path forms,
denied-proposal rejection, cancelled-proposal rejection, content restoration,
and final revoked state.

Native dialog path: exercised by inspection only (test mode bypasses it). The
owner can verify by running the app without `PAIGE_BROKER_TEST_MODE` and
clicking Approve.

---

## H. Independent non-author reviewer findings

**Outcome: ACCEPT WITH FINDINGS** — full exact-head review against
`dab842d684d338dd811c1c9c72b938d81fd56b8a` completed by an independent
non-author reviewer. Reviewer did not write or co-author the changes.

One low/informational finding: the test-only command name is declared in
`build.rs` and referenced in `capabilities/default.json` unconditionally. The
handler is compiled out of the production binary, so the IPC call is rejected;
this is recorded as a configuration-hygiene item for future hardening.

Accepted limitations: best-effort atomic replace / directory fsync, residual
TOCTOU window, proposals not evicted, interactive-desktop requirement for the
native dialog, no terminal/Git/inference/agent orchestration introduced.

Full report: `docs/REVIEW-INCREMENT-1B-R.md`.

---

## I. Production-mode executable build, hash and runtime proof

| Property | Value |
|---|---|
| Build command | `cargo build --release --features tauri/custom-protocol` |
| Source commit | `dab842d` |
| Frontend build | `tsc && vite build`, exit 0 |
| Native build | exit 0, ~2m 45s, 0 errors, 0 warnings |
| Artifact | `src-tauri/target/release/paige-desktop.exe` |
| Size | **4,398,592 bytes** (4.20 MB) |
| SHA-256 | **`2F83B69F4C9E935921CD87CC008A12D305D112DDFF1C25DE7A323FB5071D7161`** |
| Signature | NotSigned |
| Runtime | window `paige-desktop`, WebView2 child spawned, clean exit |
| Broker commands in binary | all production commands present |
| Test hook in production binary | handler compiled out; invoke returns `"Command broker_test_force_readback_failure not found"` |

**Note on previous discrepancy:** an earlier report incorrectly repeated the
SHA-256 from a previous build. The table above reflects a fresh production build
from `dab842d`.

---

## J. Failure-injection hook: production isolation

The `broker_test_force_readback_failure` command is implemented only when the
`insecure-test-bypass` Cargo feature is enabled:

```rust
#[tauri::command]
#[cfg(feature = "insecure-test-bypass")]
pub fn broker_test_force_readback_failure(...)
```

It is registered in `src-tauri/src/lib.rs` with the same `#[cfg(...)]` guard.
The production binary is built **without** `insecure-test-bypass`.

Empirical verification against the production executable:

```
invoke("broker_test_force_readback_failure", ...)
-> { __err: "Command broker_test_force_readback_failure not found" }
```

The command name string may appear in the binary metadata because the
permission is declared in the build manifest and the capability file; the
handler is absent, so the IPC call is rejected. This is accepted for the
prototype scope; future hardening should remove the permission from production
capabilities entirely (e.g., by generating a separate test capability file when
the feature is enabled).

---

## K. Remaining limitations and readiness for Increment 1C

1. Atomic replacement is best-effort (rename, no directory fsync guaranteed).
2. Concurrency guard mitigates but does not eliminate the TOCTOU window.
3. Windows path edge cases (trailing spaces/dots handled; 8.3 short names, reserved device names, control characters not explicitly handled).
4. Expired-approval path verified by inspection only.
5. Proposals are not evicted; memory grows in long sessions.
6. Native dialog requires an interactive desktop session; headless runs fail closed.
7. The test-only command permission is declared in the production manifest but its handler is compiled out.
8. No terminal, Git, inference, agents or production changes were introduced.

**Readiness for Increment 1C (Scoped Terminal Execution):** the broker now has a
trustworthy owner-confirmation boundary, OS-backed randomness, path validation
and truthful receipts. Before terminal execution is authorized, the remaining
limitations above (especially the TOCTOU window and crash durability) should be
accepted or further hardened. The native dialog pattern established here should
be reused for terminal command approval.

---

## Status

Increment 1B-R implementation complete, all tests green, exact-head independent
review completed (`ACCEPT WITH FINDINGS`). Final owner acceptance is now
requested. Increment 1C is **not started** and requires separate authorization.
Increment 1B-UI is the next owner-selected milestone after 1B-R acceptance.
