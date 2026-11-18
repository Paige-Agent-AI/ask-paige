# Independent Review — Increment 0

**Reviewer:** separately spawned non-author agent (session `ses_e10f94014ffenieveDqfRH65tp`)
**Reviewer access:** read-only — git plumbing, file reads, `Get-FileHash`,
`Get-AuthenticodeSignature`, CIM/registry queries, `npm audit --json`
**Reviewer authored any of this work:** No
**Subject:** `C:\Users\tonig\paige-desktop` @ `f532907`, Increment 0
**Review date:** 2026-11-18

---

## Verdict

**ACCEPT WITH FINDINGS** — The repository is genuinely independent, the working
tree is clean, and the claimed binary matches the SHA-256 exactly, but the
evidence document misstates the resolved React/Tauri versions, carries a stale
commit/file table, asserts an unverifiable zero-warning build, omits a broad
`opener:default` grant, and claims no production writes while a production
commit and push landed at 09:19:24–09:19:41.

---

## Reviewer confirmations (summarised from the reviewer's verified list)

- **Repository independence** — `git-common-dir` is `.git`; `.git` is a real
  directory; no `.git/commondir`, no `objects/info/alternates`; no remotes;
  `git worktree list` shows only this repo. Not a production worktree.
- **Commit integrity** — `435671f` (39 files) and `f532907` (41 files) exist as
  claimed; parentage correct; clean tree; HEAD stable during review.
- **Artifact** — `paige-desktop.exe` is 13,016,576 bytes and its SHA-256 is an
  exact match for the claimed value; PE machine `0x8664`; `NotSigned`; source
  and `dist` mtimes precede the exe.
- **Version resolution** — React resolves **19.3.1**, `tauri` crate resolves
  **2.12.7**; the README's "2.7.6" is `tauri-build`, not `tauri`.
- **Security configuration** — `"csp": null` confirmed; `capabilities/default.json`
  granted `core:default` + `opener:default`; no fs/shell/http/dialog permissions.
- **Secret hygiene** — no credential, key, token or `.env` file tracked or
  present; ignore rules cover the expected patterns.
- **Source behaviour** — stock Tauri template: one `greet` command, no fetch,
  no network, no process/fs access, no telemetry, no `unsafe`.
- **Installer provenance** — both installer hashes reproduce, Microsoft
  Authenticode valid, and the rustup hash matches the value published at
  `static.rust-lang.org`.
- **npm advisories** — exactly 5 high, 0 critical, matching limitation 2.

---

## Findings and disposition

| # | Severity | Finding | Disposition |
|---|---|---|---|
| 1 | **Major** | "No write of any kind to `Paige-Agent-AI`" and "no change to PAIGE Model Fabric" were false: production `origin/main` received `320c33d` (Model Fabric revert, +74/−240, author *Mr. Mogul Maker*) at 09:19:24, pushed 09:19:41 — 21 s after this increment's commit. | **Accepted.** Claim rescoped in the evidence document. Independently re-verified: `320c33d` is an ancestor of `origin/main`, authored by a different identity, produced by the concurrent Model Fabric session's worktree. Reported to the owner as a production change during the increment window. |
| 2 | **Major** | "Pinned versions" table was inaccurate: React 19.1.0 → actually 19.3.1; Tauri 2.7.6 → actually `tauri` 2.12.7 (`2.7.6` is `tauri-build`); Vite 8 → 8.3.8; caret ranges mean nothing is pinned. | **Fixed.** README now states resolved versions and says explicitly that ranges are not pins. `rust-toolchain.toml` added to pin the Rust channel. |
| 3 | **Major** | `opener:default` granted `allow-open-url`, `allow-reveal-item-in-dir` (unscoped — any filesystem path), and default URL schemes. Unnecessary: the UI uses plain anchors, never the opener API. | **Fixed.** Plugin removed from `Cargo.toml`, `lib.rs`, `capabilities/default.json` and `package.json`. Rebuild confirms it is gone from both lockfiles. |
| 4 | Minor | "No credential created, read or stored" is a machine-wide claim the repo cannot support. | **Fixed.** Rescoped to "this repository contains no credential", with the search method cited. |
| 5 | Minor | Commit/file table was stale (HEAD is `f532907`, 41 files; `Cargo.lock` entered at that commit). | **Fixed.** Table now lists both commits. |
| 6 | Minor | "warning lines: 0" was not independently checkable; no build log retained. | **Fixed.** Raw build logs committed under `docs/evidence/`. |
| 7 | Minor | Referenced `docs/REVIEW-INCREMENT-0.md` did not exist. | **Fixed.** This document. |
| 8 | Minor | Reproduction recipe under-specified (no `npm ci`, Node/npm unrecorded, no Rust pin, bash-style env syntax). | **Fixed.** Uses `npm ci`, records Node/npm, pins Rust via `rust-toolchain.toml`. |
| 9 | Observation | README implied an existing orchestration-client integration. | **Fixed.** Reworded as future intent. |
| 10 | Observation | External anchors lacked `rel="noopener noreferrer"`. | **Fixed.** Added to all three. |
| 11 | Observation | `.gitignore` missed `*.p12`, `*.pfx`, `*.jks`, `.npmrc`, `id_rsa*`. | **Fixed.** Added. |
| 12 | Observation | `TAURI_DEV_HOST` can bind Vite to a non-local interface. | **Recorded** in limitations; dev-only template behaviour. |
| 13 | Observation | Stock Cargo metadata (`description = "A Tauri App"`, `authors = ["you"]`). | **Fixed.** |

---

## Items the reviewer could not verify

These are recorded rather than disputed. They are properties of the machine and
session rather than of the repository, and closing them fully would require
retained transcripts:

- Historical disk/RAM figures at build start.
- Build timings and the zero-warning count (now partially addressed by
  committed logs, which are self-reported by this agent).
- The runtime probe observations (pid, window handle, WebView2 child).
- Machine-wide absence of credentials.
- Whether the 09:19:41 push triggered any deployment.
- Attribution of `320c33d` — author identity and content indicate the Model
  Fabric session, not this increment, but session logs were not available.
- UAC/elevation history.

---

## Reviewer independence statement (verbatim)

> I did not author, co-author, or assist with this increment, and I had no role
> in creating `C:\Users\tonig\paige-desktop`, its commits, evidence document, or
> build artifact. I reviewed it as a separately spawned non-author using
> read-only access: git plumbing (`rev-parse`, `log`, `ls-files`, `status`,
> `show`, `reflog`, `fsck`, `worktree list`, `config`), PowerShell reads
> (`Get-Content`, `Get-ChildItem`, `Get-FileHash`, `Get-AuthenticodeSignature`,
> CIM/registry queries), direct file reads, `npm audit --json` (a registry
> read), and a `curl` fetch of the official rustup SHA-256. I did not run
> builds, create branches or commits, or modify/delete/copy any file; the
> prototype working tree remained clean and HEAD remained `f532907` throughout.
> I also queried the production repository read-only; the production commit and
> ref writes I report are timestamped 09:19:14–09:19:41, before my first query
> of that repository, and I made no changes to its tracked content.

---

## Ownership note

The owner remains the final approver. Reviewer acceptance does not constitute
owner acceptance, and a passing review is not evidence that the delivered scope
is correct — only that the recorded claims were checked against the repository.
