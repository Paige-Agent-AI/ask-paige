# Increment 1A — Tauri Application Security Foundation

**Status:** complete, pending independent non-author review
**Date:** 2026-11-18
**Authorization:** owner authorization, Increment 1A
**Scope:** harden the Tauri shell (CSP + capabilities), assess the npm
advisories, add deterministic security tests. No file/terminal/agent work.

---

## 1. Security vulnerabilities addressed

| # | Vulnerability | Before | After |
|---|---|---|---|
| 1 | `csp: null` — no Content Security Policy | `null` | restrictive object policy (§3) |
| 2 | Blanket `core:default` capability — included `core:process:*` (exit/restart), `core:webview:*` (create-webview-window), `core:menu`, `core:tray`, `core:image`, `core:resources`, `core:path` | `core:default` | `core:event:default`, `core:window:default`, `core:app:default` only |
| 3 | `tauri-plugin-opener` present — granted unscoped `reveal_item_in_dir` (any filesystem path in Explorer) and default URL schemes | present | removed from `Cargo.toml`, `lib.rs`, `capabilities/default.json`, `package.json`; 0 matches in both lockfiles |
| 4 | 5 high npm advisories (picomatch ReDoS via vite dev chain) | 5 high | **0** (resolved in the post-review lockfile update; see §5) |
| 5 | No security tests existed | none | 41 static + 10 runtime tests |

---

## 2. Exact files changed in this increment

```
src-tauri/tauri.conf.json          CSP object (prod + devCsp), security section
src-tauri/capabilities/default.json capability reduced: core:default -> 3 core groups
package.json                       added test/test:security/test:security:runtime scripts
scripts/test-security-config.mjs   NEW - 41 static checks
scripts/test-csp-runtime.mjs       NEW - 10 runtime checks via WebView2 CDP
```

**Attribution note.** The `tauri-plugin-opener` removal (from `Cargo.toml`,
`lib.rs`, `capabilities/default.json` and `package.json`) and the
`rel="noopener noreferrer"` additions to `src/App.tsx` were already made in
commit `0fc0b2d` (Increment 0 review response). This increment carried them
forward and added the CSP, capability reduction, tests and evidence. The end
state is as listed above; the earlier commit is the origin of the opener
removal.

No change to `index.html`, `vite.config.ts`, Rust command surface (`greet`
only), or any production PAIGE file.

---

## 3. CSP and permissions before / after

### Content Security Policy

**Before:** `"csp": null`

**After (production):**

```
default-src 'self'
script-src 'self'
style-src 'self'
img-src 'self' data:
font-src 'self' data:
connect-src 'self' ipc: http://ipc.localhost
object-src 'none'
media-src 'none'
worker-src 'self'
base-uri 'self'
form-action 'self'
frame-ancestors 'none'
```

**Development (`devCsp`), separate and stricter-only-where-needed:** adds
`'unsafe-inline'` for scripts/styles (required by Vite's React Refresh preamble)
and the localhost dev server + HMR websocket origins. Production policy is never
relaxed to accommodate development tools.

### Capability

**Before:** `["core:default"]` (blanket — fs/path/process/webview/menu/tray/image/resources all included)

**After:** `["core:event:default", "core:window:default", "core:app:default"]`

No fs, shell, http, opener, process, dialog, notification, clipboard, os,
global-shortcut, deep-link, path, resources, image, webview-creation or
menu/tray permissions remain.

---

## 4. Security test results

### Methodological caveats

- `Runtime.evaluate` over CDP is **not subject to page CSP** in this WebView2
  version. The eval/Function-constructor negative tests were therefore replaced
  with DOM-injection tests (inline `<script>` and inline event handler), which
  are the meaningful behavioral proof of `script-src 'self'` without
  `'unsafe-inline'`.
- The static "embedded-policy" checks are configuration-level, not a proof that
  the string is embedded in the binary. The meaningful enforcement proof is the
  runtime test set below.
- Debug builds navigate to `devUrl` and are governed by `devCsp`; only the
  release binary (`cargo build` invoked via `npm run tauri build --no-bundle`)
  exercises the production policy. This also corrects the Increment 0 runtime
  probe, which proved only that a window opened, not that the UI rendered.
- Test #10 ("no fs/shell/http bridge exposed") checks only that no authority
  bridge is exposed on `window` globals. It does **not** prove absence of
  authority; that is enforced by `capabilities/default.json` and `lib.rs` and is
  verified by the static tests.

### Static — 41/41 PASS

`npm run test:security` (`scripts/test-security-config.mjs`): policy shape and
restrictiveness, dev/prod separation, capability minimality, no plugin
dependencies, no remote references in shipped HTML, no inline scripts in `dist/`,
and IPC-origin presence in the built executable.

### Runtime — 10/10 PASS (against the production release binary)

`npm run test:security:runtime` (`scripts/test-csp-runtime.mjs`) drives the real
`target/release/paige-desktop.exe` over the Chrome DevTools Protocol:

| Test | Result |
|---|---|
| React application rendered (`#root` has children) | PASS |
| Local application assets loaded (`/assets/*.js`, `*.css`) | PASS |
| Tauri IPC internals present | PASS |
| `invoke('greet')` returns the expected greeting | PASS |
| External `https://example.com` fetch blocked | PASS |
| Remote `<script>` injection blocked | PASS |
| Inline `<script>` execution blocked | PASS |
| Inline event handler blocked | PASS |
| External websocket blocked | PASS |
| No `fs`/`shell`/`http` bridge exposed on page globals | PASS | (authority absence proven by capabilities + static tests)

Note on `eval()` / `Function constructor`: these are governed by the same
`script-src` directive without `'unsafe-eval'`. They cannot be tested
faithfully via `Runtime.evaluate` because CDP evaluation bypasses page CSP in
this WebView2 version; the inline-script and inline-handler tests exercise the
same directive and are the meaningful behavioral proof.

---

## 5. npm advisory assessment

**Current state: `npm audit` reports 0 vulnerabilities.**

The five advisories reported during Increment 0 (picomatch ReDoS,
GHSA-fc5q-39xc-vjpf, via `fdir` → `tinyglobby` → `vite` →
`@vitejs/plugin-react`) no longer reproduce: `npm audit --json` now reports 0
vulnerabilities, and `npm ls picomatch` resolves to 4.0.7 (patched). The
lockfile was updated during the Increment 0 post-review rebuild (`0fc0b2d`),
which moved picomatch to 4.0.7. Whether `npm audit fix --force` was invoked
cannot be verified from this repository; no forced upgrade or downgrade is
recorded in the lockfile, and no dependency was pinned.

Reachability assessment of the original advisory:

- **Affected packages:** `picomatch`, `fdir`, `tinyglobby`, `vite`,
  `@vitejs/plugin-react`.
- **Reachability:** build-time / dev-server only. None of these identifiers
  appear in `dist/`; they are not shipped in the built application.
- **Severity in context:** a ReDoS in a dev-time globbing helper. No
  production exposure, no remote attacker path in a shipped desktop app.
- **Action taken:** lockfile update only. No code change required.

---

## 6. Build and executable runtime verification

| Property | Value |
|---|---|
| Build command | `npm run tauri build -- --no-bundle` |
| Frontend build | `tsc && vite build`, exit 0, 19 modules, 221.24 kB JS |
| Native build | exit 0, 2m 48s, 0 warnings, 0 errors |
| Artifact | `src-tauri/target/release/paige-desktop.exe` |
| Size | 4,290,560 bytes (4.09 MB) |
| SHA-256 | `C83CB1F519B94DE0867E5E598EF6FA4C0D36CD4B953FD39E4A672FD21CB22A75` |
| Signature | NotSigned (unsigned development build) |
| `--no-bundle` used | yes — no installer/MSI was produced or published |

This release binary serves its frontend from `http://tauri.localhost/` and
enforces the production CSP as a response header, which is what the runtime
test above exercises.

---

## 7. Independent review

**Outcome: ACCEPT WITH FINDINGS** — 7 findings, 1 major. Full report:
`docs/REVIEW-INCREMENT-1A.md`.

All three actionable findings were corrected before commit (attribution,
dev-mode caveat, test-#10 overstatement). The remaining four are observations or
unverifiable process-history items and are recorded as such.

Reviewer acceptance is not owner acceptance.

---

## 8. Remaining risks

1. `csp: null` is gone, but **CSP is a policy, not a sandbox**. Any future
   code that adds `'unsafe-eval'` or remote origins weakens it silently; the
   static test guards against the current configuration only.
2. `core:window:default` still grants window-control methods. Acceptable for
   this template; should be re-scoped per window once multiple windows exist.
3. No native broker yet — filesystem and terminal authority is effectively
   none, which is correct for this increment but must be designed carefully in 1B.
4. The npm audit result is a snapshot of the registry at build time; it must be
   re-run before any future release.
5. Disk now 39.31 GB free after the release build. Adequate.

---

## 9. Status

Increment 1A implementation complete, tests green. Awaiting independent review
and owner acceptance. Increment 1B (Rust native device authority broker) is
**not started** and requires separate authorization.
