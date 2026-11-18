# Increment 1B — Rust Native File Authority Broker

**Status:** complete, pending independent non-author review
**Date:** 2026-11-18
**Authorization:** owner authorization, Increment 1B
**Scope:** controlled local file authority on one approved root. No terminal, Git,
inference, agents or production changes.

---

## 1. Authorized root and isolation

| Property | Value |
|---|---|
| Authorized root | `C:\Users\tonig\paige-desktop-lab` |
| Compile-time allowlist (`ALLOWED_ROOTS`) | the lab root only |
| Root canonicalized at grant time | yes |
| Reparse points at root | rejected |
| Lab repo state | clean, `a51ff07`, README sha256 `A39E33F4…8D0AE` (restored after tests) |
| Production repo touched | no |
| Prototype repo | independent, no remotes, own `.git` |

No other directory, including the production PAIGE repo, user documents, cloud
files, browser profiles or credentials, is reachable.

---

## 2. Broker architecture

State: `Mutex<BrokerState>` managed via `tauri::Builder::manage(...)`. Proposals
live in an in-memory `HashMap<String, Proposal>` — session-scoped, no durable
task state.

Typed operations (Tauri commands, all registered in `invoke_handler`):

| Command | Requires | Purpose |
|---|---|---|
| `broker_status` | none | read/write permission state, root, pending count |
| `broker_grant_root` | allowlisted root | owner grants read and/or write |
| `broker_revoke` | — | clears grant and cancels pending/approved proposals |
| `broker_list_files` | read | lists regular files under root (dotfiles skipped, depth ≤ 6, ≤ 1000) |
| `broker_read_file` | read | reads a validated text file with checksum |
| `broker_propose_write` | write | validates path, checks expected checksum, creates a Pending proposal with an 8-hex approval token |
| `broker_approve` | write | transitions Pending → Approved only with the matching token and within TTL |
| `broker_deny` | write | transitions Pending → Denied |
| `broker_cancel` | write | transitions non-applied proposals to Cancelled |
| `broker_apply` | write | re-validates path and checksum, atomically replaces, verifies readback |

Permissions distinguish read from write (`require_grant(state, need_write)`).

A future model or agent cannot create, approve, extend or renew authority:
granting is checked against the compile-time allowlist, and approval requires
the owner to echo back a token issued at proposal time.

---

## 3. Path security

`validate_path(root, input)` rejects:

- empty input
- UNC / device paths (`\\…`)
- absolute paths
- path prefixes and rooted forms
- parent traversal (`..`)
- empty components
- colon (alternate data streams / drive letters)
- any existing component that is a reparse point (symlink, junction, etc.)
- paths that canonicalize outside the root
- targets that are not regular files

It canonicalizes and checks `starts_with(canonical_root)`. `broker_apply`
re-runs `validate_path` and compares the result to the proposal's stored path
before writing (TOCTOU guard), and re-checks the on-disk checksum.

Known residual limits, stated rather than asserted away:

- `broker_read_file` and `broker_propose_write` rely on the validated `canon`
  remaining stable between validation and use; concurrent filesystem mutation
  could swap components. Mitigated by canonicalize + `starts_with` + checksum
  re-check in `broker_apply`.
- Windows trailing spaces/dots and 8.3 short names are not explicitly handled.
- No race-free handle-based replacement is implemented; `std::fs::rename` on
  the same volume is used and documented as best-effort atomic.

---

## 4. Write safety

Every proposed write has:

- explicit owner approval (Approve button + echoed token),
- a validated in-scope target,
- a bounded size (`MAX_FILE_BYTES` = 64 KiB),
- a known expected previous checksum (optimistic concurrency),
- an accurate result or failure receipt,
- verified readback after completion (SHA-256 of the written bytes vs. the file).

Atomic replacement: write to a temp file (`.paige-broker-tmp-{proposal_id}`) in
the same directory, `fsync`, then `rename`. On failure the temp file is removed.
On readback failure the proposal is marked `Cancelled` (the file may already have
been replaced; the receipt reports `readback_verified`).

Cancellation or revocation prevents future operations and cancels uncommitted
proposals. An already-applied write cannot be cancelled or rolled back; the
receipt reports the actual state.

Approval tokens are 8 hex digits derived from a non-cryptographic hasher.
Acceptable for this prototype; a production version needs a CSPRNG and a longer
token.

---

## 5. Tauri IPC security

Capability file (`capabilities/default.json`) grants only:

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

Production CSP and the minimal capability set from Increment 1A are unchanged.

---

## 6. Owner interface

Minimal React control surface in `src/App.tsx`:

- authorized root and read/write permission state,
- Grant Access / Revoke Access buttons,
- file list with safe preview,
- proposed modification textarea and before/after hashes in the result (no visual diff yet),
- Approve / Deny with token entry,
- Apply button (disabled until approved),
- actual operation result.

The interface is a control surface, not the security authority.

---

## 7. Test evidence

| Suite | Result |
|---|---|
| Rust unit tests (`cargo test --lib`) | **5/5 pass** |
| Static security tests (`npm run test:security`) | **41/41 pass** |
| CSP runtime tests (`npm run test:security:runtime`) | **10/10 pass** |
| Broker runtime tests (`npm run test:broker:runtime`) | **22/22 pass** |

Broker runtime tests cover: initial denied state, grant, listing, reading,
proposal creation, wrong-token rejection, correct-token approval, apply with
readback, re-apply rejection, revoke, post-revoke denial, five unauthorized
path forms, denied-proposal rejection, cancelled-proposal rejection, content
restoration, and final revoked state.

Deterministic negative tests: traversal, absolute paths, UNC, alternate data
streams, unauthorized overwrites, expired/cancelled approval reuse, revocation
blocking, forged out-of-scope IPC.

Not covered: expired-approval timing (300 s TTL; verified by inspection).

---

## 8. Build and executable runtime verification

| Property | Value |
|---|---|
| Build command | `npm run tauri build -- --no-bundle` (with `tauri/custom-protocol`) |
| Frontend build | `tsc && vite build`, exit 0, 19 modules, 221.24 kB JS |
| Native build | exit 0, ~2m 40s, 0 errors (2 dead-code warnings) |
| Artifact | `src-tauri/target/release/paige-desktop.exe` |
| Size | 4,367,872 bytes (4.16 MB) |
| SHA-256 | `E6BE9BB16F8F7D5AF20CE2979AD70CDD3639043D4B70143A14639CA1E0291B45` |
| Signature | NotSigned |
| Runtime | window `paige-desktop`, handle, WebView2 child, clean exit |
| Broker commands in binary | all 10 present |

---

## 9. Independent review

**Outcome: ACCEPT WITH FINDINGS** — 10 findings, 2 high, 2 medium, 4 low,
2 info. Full report: `docs/REVIEW-INCREMENT-1B.md`.

The two high findings (concurrency guard on `broker_apply`, and revocation not
cancelling in-flight `Applying` proposals) were corrected before commit:
`Applying` is now set only immediately before the write, the concurrency
guard rejects concurrent applies, `broker_cancel` refuses to flip an
`Applying` proposal, and `broker_revoke` cancels `Applying` proposals too.
The two medium findings (ineffective fsync, no receipt on readback failure)
were partially addressed: fsync now uses a writable handle and best-effort
directory fsync; readback failure still returns `Err` and the evidence wording
was corrected to match. The remaining findings are recorded as limitations.

---

## 10. Remaining risks

1. Concurrency guard marks a proposal `Applying` before the write, but the state
   lock is released during I/O. A determined concurrent caller could still race
   between the status check and the write; mitigated by the checksum re-check
   and path re-validation in `broker_apply`. Not race-free.
2. Atomic replacement uses `std::fs::rename` on the same volume; directory
   fsync is not performed.
3. Approval token is 8 hex digits from a non-cryptographic hasher; brute-force
   feasible for a compromised renderer. Acceptable for the prototype only.
4. Readback failure leaves the proposal `Cancelled` even though the file may
   already have been replaced; the receipt reports the actual state.
5. Expired-approval path verified by inspection, not by test.
6. Windows path edge cases (trailing spaces/dots, 8.3 short names) not tested.
7. Proposals are not evicted; memory grows in long sessions.
8. No terminal, Git, inference, agents or production changes were introduced —
   these remain separate future outcomes.

---

## 11. Status

Increment 1B implementation complete, all tests green. Awaiting independent
review and owner acceptance. Increment 1C is **not started** and requires
separate authorization.
