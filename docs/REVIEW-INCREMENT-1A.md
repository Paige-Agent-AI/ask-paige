# Independent Review — Increment 1A

**Reviewer:** separately spawned non-author agent
**Access:** read-only — file reads, git plumbing, running the two security test
scripts, `npm audit --json`, `npm ls picomatch`, hashing, signature inspection
**Reviewer authored any of this work:** No

---

## Verdict

**ACCEPT WITH FINDINGS** — all core security claims (CSP shape and minimality,
capability reduction, plugin removal, HTML cleanliness, npm audit 0, binary hash,
signature and mtime, tests) were verified against the working tree and the
release artifact, but the evidence document has attribution and documentation
gaps and one runtime test overstates what it proves.

---

## Findings and disposition

| # | Severity | Finding | Disposition |
|---|---|---|---|
| 1 | minor | §2 "Exact files changed" attributed the opener removal and `rel="noopener noreferrer"` to Increment 1A, but they landed in commit `0fc0b2d`. | **Fixed** — §2 now states the attribution correctly. |
| 2 | minor | The dev-mode caveat (debug builds use `devUrl`/`devCsp`; the Increment 0 window probe proved only that a window opened) was not recorded in the evidence doc. | **Fixed** — recorded in §4 caveats and §8 risks. |
| 3 | **major** | Runtime test #10 "no fs/shell/http bridge exposed" only checks `window` globals; it does not prove absence of authority. | **Fixed** — renamed to "no fs/shell/http bridge exposed on page globals" and the caveat is stated in the evidence. Authority absence remains proven by `capabilities/default.json` and the static tests. |
| 4 | observation | Static "embedded-policy" checks are weak: one finds the framework string `Content-Security-Policy` in the binary, the other is a substring check. | **Recorded.** The meaningful enforcement proof is the runtime test set. |
| 5 | observation | The claim "No `npm audit fix --force` was run" cannot be verified from the repository. | **Fixed** — reworded to state what is verifiable (no forced upgrade or downgrade in the lockfile). |
| 6 | minor | Native build warning/error count, duration and free-disk figures are not verifiable from the repo. | **Recorded.** |
| 7 | observation | Runtime tests rely on timeouts and CDP `Runtime.evaluate`, which bypasses page CSP in this WebView2 version. | **Recorded.** The DOM-injection tests are the meaningful proof. |

---

## Reviewer confirmations (summarised)

- Production CSP is an object with `default-src 'self'`, `script-src 'self'`,
  `style-src 'self'`, `object-src 'none'`, `frame-ancestors 'none'`, `base-uri
  'self'`, `form-action 'self'`; no `unsafe-eval`, `unsafe-inline`, wildcard or
  external origin; `devCsp` present and distinct.
- Capability is exactly `core:event:default`, `core:window:default`,
  `core:app:default`; no `core:default` or any fs/shell/http/opener/process/…
  permission.
- No `tauri-plugin-*` or `@tauri-apps/plugin-*` anywhere; `lib.rs` registers
  only `greet`.
- `index.html` and `dist/index.html` have no remote references and no inline
  scripts.
- `npm audit --json`: 0 vulnerabilities; `picomatch@4.0.7` patched; absent from
  `dist/`.
- Static tests: 41/41 pass. Runtime tests: 10/10 pass, attached to
  `http://tauri.localhost/` (production page), not `about:blank`.
- Release executable exists, 4,290,560 bytes, SHA-256
  `C83CB1F519B94DE0867E5E598EF6FA4C0D36CD4B953FD39E4A672FD21CB22A75`,
  unsigned, mtime after the source changes.
- No credential files, no remote, no fs/process/network/shell code added.

---

## Not verifiable by the reviewer

- Native build warning/error count, duration and free-disk figures.
- Whether `npm audit fix --force` was avoided (process history not in repo).
- Whether any writes occurred to the production PAIGE tree (outside this repo).
- Exact commit in which the opener removal landed (history indicates `0fc0b2d`).

---

## Reviewer independence statement

I did not author, co-author or assist with this increment. I reviewed it
read-only: file reads, git plumbing, running the two security test scripts,
`npm audit --json`, `npm ls picomatch`, SHA-256 hashing, Authenticode
signature inspection and filesystem checks. No builds were run and no file was
modified.
