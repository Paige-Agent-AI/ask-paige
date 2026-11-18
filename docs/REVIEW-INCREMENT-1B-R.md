# Independent Review — Increment 1B-R (exact-head)

**Reviewer:** independent non-author subagent  
**Repo:** `C:\Users\tonig\paige-desktop`  
**Branch:** `feat/paige-desktop-prototype`  
**Reviewed commit (exact HEAD):** `dab842d684d338dd811c1c9c72b938d81fd56b8a`  
**Access:** read-only — source inspection, git plumbing, test execution, hash verification  
**Reviewer authored any of this work:** No  

---

## 1. Independence statement

I confirm that I did **not** write or co-author the changes in Increment 1B-R. The commit `dab842d` was authored by **PAIGE Desktop Builder <builder@paige.local>** before this review session. During this review I performed only read-only inspection, test execution, and hash verification; I did not create, modify, delete, move, or commit any files.

---

## 2. Per-file inspection summary

| File | Summary |
|---|---|
| `src-tauri/src/broker.rs` | Native file-authority broker. Compile-time root allowlist (`C:\Users\tonig\paige-desktop-lab` only). Secure 32-byte approval tokens from `BCryptGenRandom`. Native `MessageBoxW` owner-approval dialog, with test bypass gated by the `insecure-test-bypass` Cargo feature **and** `PAIGE_BROKER_TEST_MODE=1`. Strong path validation (traversal, absolute/UNC, ADS, reparse points, reserved device names, control chars, trailing spaces/dots, 8.3 aliases). Write flow uses propose → approve → apply, checksum binding, atomic temp-file + rename, readback verification, and a `committed_unverified` receipt path. Reconcile command added. Test hook `broker_test_force_readback_failure` is `#[cfg(feature = "insecure-test-bypass")]` gated. |
| `src-tauri/src/lib.rs` | Registers all production broker commands plus `greet`. The test hook is registered under `#[cfg(feature = "insecure-test-bypass")]`. State managed as `Mutex<BrokerState>`. |
| `src-tauri/build.rs` | Declares the Tauri app-command ACL. The test hook command name is listed **unconditionally**, even though its handler is compiled out in production. This is a configuration-hygiene issue, not an exploitable flaw. |
| `src-tauri/capabilities/default.json` | Minimal grants: core event/window/app defaults and the explicit broker command allows. The test-hook permission `allow-broker-test-force-readback-failure` is granted here. Because the handler is absent from production, the permission is effectively inert. |
| `src-tauri/tauri.conf.json` | Production CSP is strict (`'self'` for default/script/style, `data:` only for img/font, `ipc:` / `http://ipc.localhost` for connect, `'none'` for object/media/frame-ancestors). Dev CSP is separate and relaxed. No external network egress in production. |
| `scripts/test-broker-runtime.mjs` | 26 runtime checks against the real executable (test-mode binary). Exercises grant, list, read, propose, approve (bad/good token), apply, committed-but-unverified/reconcile, replay rejection, deny, cancel, revoke, path traversal/ADS/UNC/absolute blocking, and content restoration. |
| `scripts/test-security-config.mjs` | 41 static checks: CSP policy shape, no `unsafe-eval`/`unsafe-inline`/wildcards, no external origins, minimal capability set, no forbidden Tauri plugins, no remote references in HTML, embedded CSP evidence in binary. |
| `scripts/test-csp-runtime.mjs` | 10 runtime CSP checks via CDP: app renders, local assets load, Tauri IPC works, external HTTPS fetch blocked, remote script blocked, inline script blocked, inline event handler blocked, external WebSocket blocked, no fs/shell/http bridge exposed. |

---

## 3. Regression test totals

| Suite | Result |
|---|---|
| `cd src-tauri && cargo test --lib` | **11 passed, 0 failed** |
| `npm run test:security` | **41/41 passed** |
| `npm run test:security:runtime` | **10/10 passed** |
| `npm run test:broker:runtime` (test-mode binary) | **26/26 passed** |

Test-mode binary used: `C:\Users\tonig\paige-desktop\src-tauri\target-test\release\paige-desktop.exe`  
Built with `insecure-test-bypass`; verified to contain `PAIGE_BROKER_TEST_MODE` logic.

---

## 4. Production binary verification

| Property | Value |
|---|---|
| Path | `C:\Users\tonig\paige-desktop\src-tauri\target\release\paige-desktop.exe` |
| Size | **4,398,592 bytes** |
| SHA-256 | **`2F83B69F4C9E935921CD87CC008A12D305D112DDFF1C25DE7A323FB5071D7161`** |
| Contains `PAIGE_BROKER_TEST_MODE` | **No** |
| `broker_test_force_readback_failure` invoke against production binary | **`{"__err":"Command broker_test_force_readback_failure not found"}`** |

The test-only hook is isolated from the production binary.

---

## 5. Verdict

**ACCEPT WITH FINDINGS**

No material unresolved security vulnerability was found within the Increment 1B-R prototype scope. The findings below are configuration hygiene items or accepted prototype limitations.

---

## 6. Findings and accepted limitations

### Findings
1. **Test-only command declared in production manifest/capability (LOW / informational).**  
   `src-tauri/build.rs` and `src-tauri/capabilities/default.json` list/reference `broker_test_force_readback_failure` unconditionally. The command handler is compiled out of the production binary, so IPC calls return `"Command ... not found"`. For future hardening, the manifest entry and capability grant should be generated only when `insecure-test-bypass` is enabled (e.g., via a separate test capability).

### Accepted limitations (explicitly scoped to this prototype)
2. Atomic file replacement uses rename; directory `fsync` is best-effort.
3. The `Applying` concurrency guard mitigates but does not fully eliminate the TOCTOU window.
4. Proposals are never evicted; memory use grows over long sessions.
5. Native owner dialog requires an interactive desktop session; headless runs fail closed.
6. No terminal, Git, inference, agent orchestration, or broad filesystem/shell access is introduced.
7. Working tree contains an uncommitted modification to `docs/INCREMENT-1B-R-EVIDENCE.md`. This was present before the review; I did not modify it.

---

**Review completed:** All required inspections and regression suites executed against the exact final commit `dab842d`. No files were modified. No commits were made.
