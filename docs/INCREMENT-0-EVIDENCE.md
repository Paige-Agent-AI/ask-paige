# Increment 0 — Toolchain, Scaffold and Native Build Evidence

**Status:** COMPLETE — awaiting independent non-author review
**Date:** 2026-11-18
**Authorization:** Owner authorization for Increment 0, issued 2026-11-17
**Scope:** Install approved prerequisites, verify toolchain, create isolated
repositories, scaffold Tauri 2 + React, record a reproducible native Cargo
compilation. **No Increment 1 work was performed.**

---

## 1. Device and privilege boundary

| Property | Value |
|---|---|
| Host | HP HP Pavilion Laptop 16-af0xxx |
| CPU | Intel Core Ultra 7 155U — 12 physical / 14 logical |
| RAM | 15.67 GB total; 2.61 GB free at build start |
| OS | Windows 11 Home, build 26300, 64-bit |
| User | PROJECTMOGUL\tonig |
| **Elevated shell** | **False** — no administrative privilege held by the build agent |
| WebView2 runtime | 157.0.4341.60 |
| Disk free, before install | 22.32 GB |
| Disk free, after everything | 11.93 GB |

The build agent never held administrator rights. Every privileged operation was
performed by an elevation prompt the owner approved directly.

---

## 2. Installed prerequisites and how they were verified

### 2.1 Microsoft C++ Build Tools

| Item | Value |
|---|---|
| Installer | `vs_BuildTools.exe`, 4,474,072 bytes |
| SHA-256 | `8AAF70D164808D313EE6226AF5D1E3B2DB43C60BE551BFEB2B70CE139D792C37` |
| Authenticode | **Valid**, `CN=Microsoft Corporation` |
| Source | `https://aka.ms/vs/17/release/vs_BuildTools.exe` |
| Product | Visual Studio Build Tools 2022, 17.14.37906.3 |
| Install path | `C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools` |
| MSVC toolset | 14.44.35207 |
| `link.exe` | `...\VC\Tools\MSVC\14.44.35207\bin\Hostx64\x64\link.exe` (3,252,616 bytes) |
| Workload | `Microsoft.VisualStudio.Workload.VCTools` with `--includeRecommended` |

### 2.2 Windows SDK

| Item | Value |
|---|---|
| Version | **10.0.26100.0** |
| Verification | `Lib\10.0.26100.0\um\x64\kernel32.lib` present |

### 2.3 Rust toolchain

| Item | Value |
|---|---|
| Installer | `rustup-init.exe`, 12,839,424 bytes |
| SHA-256 | `38027D51D69A5D05493627A0634DBEA52D6CD793C8F072D2373D3F4163EEC7DB` |
| Verification | Matched the official published hash at `static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe.sha256` |
| Authenticode | NotSigned — expected; rustup does not Authenticode-sign, hence hash verification against the official publication |
| `rustc` | 1.100.0 (0ef1bc386 2026-11-09) |
| `cargo` | 1.100.0 (3acc79c0c 2026-11-05) |
| `rustup` | 1.29.3 (edbc831ce 2026-11-02) |
| Default host | `x86_64-pc-windows-msvc` |
| Profile | minimal |
| PATH | `C:\Users\tonig\.cargo\bin` written to user PATH |

### 2.4 Linker proof (independent of the application)

A throwaway crate was compiled and executed to prove the MSVC linker works,
separately from any Tauri code:

```
cargo build            → exit 0, 7.1s
binary output          → "Hello, world!"
binary size            → 133,120 bytes
PE signature           → MZ OK
```

---

## 3. Incident log — mislocated Build Tools install

Recorded because it affected the machine and the owner's time.

**What happened.** The first install used `Start-Process -ArgumentList` with an
array. PowerShell joins array elements with spaces and does **not** quote them,
so `--installPath C:\Program Files (x86)\...` was parsed by the installer as
`--installPath C:\Program`. Build Tools installed to `C:\Program` — a root-level
directory no tooling expects — and the workload arguments after it were dropped.

**Why it was not caught immediately.** The install exited 0. A later verification
step searched only the path the agent *intended* to use, so it reported
`link.exe: NOT FOUND` even though a compiler existed at the wrong location.

**Correction.** The working compiler was identified at
`C:\Program\VC\Tools\MSVC\14.44.35207\bin\Hostx64\x64\cl.exe`, a linker probe
confirmed the toolchain could still build, and only then was the misplaced
instance uninstalled (`C:\Program` removed, disk returned to 22.51 GB) and
reinstalled at the default path using a single quoted argument string.

**Cost.** Three elevation prompts were declined before one was approved; the
agent stopped retrying after the second decline and asked the owner rather than
repeatedly re-raising dialogs.

**Lesson applied to later scripts.** Native tools that write to stderr
(`rustup`, `npm`, `git`) are invoked with error preference `Continue` and their
results judged by `$LASTEXITCODE`, not by PowerShell's stderr trapping. A
`$ErrorActionPreference='Stop'` script had already aborted a successful rustup
run on a harmless `info:` line.

---

## 4. Repositories created

### 4.1 Prototype — `C:\Users\tonig\paige-desktop`

| Property | Value |
|---|---|
| Branch | `feat/paige-desktop-prototype` |
| Commit 1 (scaffold) | `435671f72ced5561339ce648000076b92f102d00` — 39 files |
| Commit 2 (evidence + versions) | `f532907f84115c4ab2b29818908bf917d2df2dd6` — 41 files |
| Commit 3 | post-review corrections (see §10); the commit containing this document |
| Author | PAIGE Desktop Builder `<builder@paige.local>` |
| `git-common-dir` | `.git` — **own repository, not a worktree of production** |
| Remotes | **none** |

`src-tauri/Cargo.lock` enters at commit 2, so dependency resolution is pinned
from that commit onward, not from commit 1.

Independence from the production repository is machine-verified: `git rev-parse
--git-common-dir` returns `.git` locally, and `git remote -v` returns nothing.
The production `Paige-Agent-AI`, all its existing worktrees and
`Paige-desktop-deploy` were not modified.

### 4.2 Lab — `C:\Users\tonig\paige-desktop-lab`

| Property | Value |
|---|---|
| Branch | `main` |
| Commit | `a51ff07ef55db0bf82b015305cf24558b9a9e31e` |
| Files | 1 |
| Remotes | **none** |
| `git-common-dir` | `.git` |

---

## 5. Build evidence

### 5.1 Frontend

```
npm run build  (tsc && vite build)
19 modules transformed
dist/index.html                 0.48 kB
dist/assets/index-*.css         1.39 kB
dist/assets/index-*.js        221.16 kB  (gzip 69.26 kB)
built in 142ms                 exit 0
dist: 6 files, 225.9 KB
```

### 5.2 Native

```
CARGO_BUILD_JOBS=2  cargo build   (src-tauri)
Finished dev profile in 5m 04s    exit 0
warning lines: 0    error lines: 0
```

### 5.3 Artifact

| Property | Value |
|---|---|
| Path | `src-tauri\target\debug\paige-desktop.exe` |
| Size | 12.41 MB |
| SHA-256 | `8F7DF40FCAE497BEA1C891AEBB9FA57B5568B7742F9E04E22BC3402EDE71E20F` |
| PE header | MZ OK |
| Version | 0.1.0 |
| Signature | **NotSigned** — expected for a local debug build; not a release artifact |

### 5.4 Runtime probe

The compiled application was launched and observed, then stopped:

| Property | Value |
|---|---|
| Process | alive and responding after 8s |
| Window title | `paige-desktop` |
| Window handle | 330074 |
| Working set | 34.2 MB |
| Threads | 10 |
| Child process | `msedgewebview2.exe` — the WebView2 host actually booted |
| Shutdown | process gone 2s after stop; local kill path works |

Note: a system-wide `msedgewebview2` process count is **not** valid evidence,
because other applications on this machine also use WebView2. The decisive
observation is the child process parented to pid 34936.

### 5.5 Footprint

| Item | Size |
|---|---|
| `src-tauri/target/` | 3.91 GB |
| `node_modules/` | 85.3 MB |
| Net disk consumed, whole increment | 10.39 GB (22.32 → 11.93 GB free) |

Exceeds the earlier 11.7 GB projection by roughly 1 GB. Disk headroom is
adequate but no longer generous; a release build with LTO would consume
further space.

---

### 5.6 Post-review rebuild (current artifact of record)

After the independent review, the opener plugin was removed and the source
corrected. The rebuild is the artifact that matches HEAD of this document.

| Property | Value |
|---|---|
| `cargo build` | exit 0, **2m 30s**, warning lines: 0, error lines: 0 |
| Frontend | `tsc && vite build` exit 0, 19 modules, 221.24 kB JS (gzip 69.27 kB), 225 ms |
| Artifact | `src-tauri\target\debug\paige-desktop.exe` |
| Size | 12,572,160 bytes (11.99 MB) |
| SHA-256 | `F7FB2D4172E38FF08385C87262BF73DFCD6BA18B89215C2BADFE12C2ADF4E357` |
| Signature | NotSigned |
| `opener` in `Cargo.lock` | 0 matches |
| `opener` in `package-lock.json` | 0 matches |
| Disk free after | 9.64 GB |

**Runtime check on this exact artifact:** launched, alive and responding after
8s, window title `paige-desktop`, window handle 1639608, working set 26.6 MB,
10 threads, child `msedgewebview2.exe` (pid 24868) present, stopped cleanly 2s
after the kill request. Transcript: `docs/evidence/runtime-probe-postreview.log`.

The earlier artifact (`8F7DF40F…`, 13,016,576 bytes, §5.3) was produced before
these corrections and is superseded. Both hashes are retained so the change is
auditable rather than silently overwritten.

Note: the full recompile of `tauri`/`wry`/`tao` increased on-disk usage by
roughly 2.3 GB of incremental artifacts. Disk headroom is now 9.64 GB.

---

## 6. Reproduction

```powershell
git clone --branch feat/paige-desktop-prototype <repo> paige-desktop
cd paige-desktop
npm ci                                  # lockfile-exact, not "npm install"
npm run build                           # tsc && vite build -> dist/
cd src-tauri
$env:CARGO_BUILD_JOBS = "2"
cargo build
```

Recorded environment for the build in this document:

| Tool | Version |
|---|---|
| Node | v24.18.0 |
| npm | 11.16.0 |
| Rust channel | pinned by `rust-toolchain.toml` to 1.100.0 |
| MSVC toolset | 14.44.35207 |
| Windows SDK | 10.0.26100.0 |

Expected artifact: `src-tauri\target\debug\paige-desktop.exe`.

**Limits of this claim.** "Reproducible" here means the committed lockfiles and
toolchain pin describe a build that another machine can attempt, not that the
output is bit-identical. Rust builds are not bit-for-bit reproducible by
default, and no provenance attestation exists. Raw build logs are preserved in
`docs/evidence/` as self-reported transcripts.

---

## 7. Known limitations — stated, not hidden

| # | Limitation | Impact |
|---|---|---|
| 1 | `tauri.conf.json` ships `"csp": null` | **No Content Security Policy.** No defense-in-depth for rendered agent or retrieved content. Must be addressed before any untrusted content is displayed. |
| 2 | 5 high-severity npm advisories (`picomatch` ReDoS via `fdir`/`tinyglobby` → `vite`) | Development/build-time dependency chain only; `npm audit fix --force` proposes downgrading Vite 8 → 6.2.7, a breaking change. Not applied. |
| 3 | Debug build only | No release/LTO build, no installer bundle produced. Installer signing and publication remain later-phase requirements. |
| 4 | Binary unsigned | Compilation is not code signing. |
| 5 | Window is the stock template shell | No PAIGE UI surface yet — Ask PAIGE, Files, Terminal, Agent Roster etc. are Increment 1+. |
| 6 | No Rust native broker | Least-privilege file/process enforcement does not exist yet. |
| 7 | No inference wiring | No OpenRouter client, no credential storage, no spend enforcement. |
| 8 | No agent protocol, no task state | No TASK_ASSIGNED / HANDOFF_* / REVIEW_RESULT signals, no orchestration adapter. |
| 9 | Sequential execution not yet implemented | The Phase A concurrency limit of one active worker is a design ruling, not yet code. |
| 10 | RAM tight during build | 2.61 GB free at start; build succeeded with `CARGO_BUILD_JOBS=2`. This is the basis for the one-worker Phase A ruling. |
| 11 | ~~`opener:default` granted `reveal_item_in_dir` unscoped~~ | **Removed after independent review.** Plugin dropped from `Cargo.toml`, `lib.rs`, `capabilities/default.json` and `package.json`; confirmed absent from both lockfiles. Capability set is now `core:default` only. |
| 12 | `TAURI_DEV_HOST`, if set, binds Vite/HMR to a non-local interface | Dev-only stock template behaviour. Do not set it on untrusted networks. |
| 13 | Frontend and Tauri crate versions are caret/tilde ranges | Nothing was pinned. `package-lock.json` and `Cargo.lock` pin resolution for `npm ci` / `cargo build`; `rust-toolchain.toml` pins only the Rust channel. |

**Nothing in this increment demonstrates:** bounded local file access, terminal
execution, agent-to-agent handoff, independent review enforcement, approval
gating, or recovery from an interrupted task. Those are Increment 1 and later.

---

## 8. Production non-interference — scoped correctly

An earlier version of this section asserted "no write of any kind" to the
production repository and "no change to PAIGE Model Fabric". **That claim was
wrong**, and the independent reviewer rejected it. Corrected statement:

**What this increment's agent did:** no `git` write operation of any kind in
`C:\Users\tonig\Paige-Agent-AI`, in any of its eight linked worktrees, or in
`C:\Users\tonig\Paige-desktop-deploy`. All work on those repositories in this
session was read-only inspection (`rev-parse`, `log`, `status`, `worktree
list`, file reads). Identity used for this increment's commits is
`PAIGE Desktop Builder <builder@paige.local>`.

**What happened anyway, concurrently:** at **2026-11-18 09:19:24**, 21 seconds
after this increment's commit, commit `320c33dbd5b52a2e588ecbd3a980921c81f93087`
was created in the Model Fabric session's worktree
`.../worktree/f7aba1/main-current` on branch
`integration/ant-166-exp10-edge-reconciliation`, and `origin/main` was updated
by push at **09:19:41** to that commit.

| Property | Value |
|---|---|
| Commit | `320c33dbd5b52a2e588ecbd3a980921c81f93087` |
| Author | Mr. Mogul Maker `<232259757+mrmogulmaker-bot@users.noreply.github.com>` |
| Subject | revert OpenRouter primary Chat transport, restore working Spine routing |
| Files | `model-fabric.ts`, `openai-responses.ts`, `openrouter-models.ts`, `paige-turn/route.ts`, `paige-turn/route.test.ts`, `provider-failure.ts` |
| Diff | +74 / −240 |
| `origin/main` | now at `320c33d` |

This touched the customer-facing Model Fabric, which this charter places out of
scope for this increment. It was **not** produced by this agent — the author
identity, the worktree, and the subject line all belong to the concurrent Model
Fabric session. It is recorded here because "the production routes were
unchanged during Increment 0" is **not true**, and the owner needs to know that
rather than read a clean claim.

**Other statements that remain true:**

- No production deploy performed by this agent.
- No installer built, signed or published.
- No device privilege expanded; this agent never held administrator rights.
- No credential created, stored or read by this increment. This repository
  contains no `.env`, key, token or credential file, and no secret pattern was
  found in any tracked file. (This is a statement about **this repository**,
  not about the machine.)
- No NEXUS Vibe Studio repository was accessed or modified.

---

## 9. Independent review

Reviewer requirements: separately spawned, did not author or assist this
implementation, read-only access, findings reported to the owner who remains
final approver.

**Outcome: ACCEPT WITH FINDINGS** — 5 verified problem areas (3 major) across
13 findings. Full report, reviewer independence statement and per-finding
disposition are in `docs/REVIEW-INCREMENT-0.md`. The corrections arising from
that review are described in §10.

Reviewer acceptance is not owner acceptance.

---

## 10. Corrections made after independent review
The independent reviewer returned ACCEPT WITH FINDINGS. The following were
changed in response; the review of record is `docs/REVIEW-INCREMENT-0.md`.

| Change | Reason |
|---|---|
| Removed `tauri-plugin-opener` from `Cargo.toml`, `lib.rs`, `capabilities/default.json`, `package.json` | The grant exposed unscoped `reveal_item_in_dir` and default URL schemes that the UI never used. Capability set is now `core:default` only. |
| Added `rel="noopener noreferrer"` to all external anchors | Defense in depth in the webview. |
| Added `rust-toolchain.toml` pinning channel 1.100.0 | The review found no Rust pin, so "reproducible" was overstated. |
| Added `*.p12`, `*.pfx`, `*.jks`, `.npmrc`, `id_rsa*` to `.gitignore` | Credential-pattern coverage. |
| Replaced stock Cargo metadata (`description`, `authors`) | Traceability. |
| Corrected README version claims; marked orchestration-client role as future intent | The review found React resolved 19.3.1 (not 19.1.0) and the `tauri` crate 2.12.7 (not 2.7.6 — that is `tauri-build`), and that no contract client exists yet. |
| Rescoped §8 production claims; recorded the concurrent `320c33d` Model Fabric commit and push | The original absolute claim was false. |
| Rescoped the credential claim to this repository | A repo cannot prove a machine-wide negative. |
| Committed raw build logs under `docs/evidence/` | "Zero warnings" was otherwise unverifiable. |
| Changed reproduction to `npm ci` and recorded Node/npm | Lockfile-exact installs. |

The rebuild that follows these changes was re-verified: see §5.6.

