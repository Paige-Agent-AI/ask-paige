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
| Commit | `435671f72ced5561339ce648000076b92f102d00` |
| Author | PAIGE Desktop Builder `<builder@paige.local>` |
| Files | 39 |
| `git-common-dir` | `.git` — **own repository, not a worktree of production** |
| Remotes | **none** |

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

## 6. Reproduction

```powershell
git clone --branch feat/paige-desktop-prototype <repo> paige-desktop
cd paige-desktop
npm install
npm run build
cd src-tauri
$env:CARGO_BUILD_JOBS = "2"
cargo build
```

Expected artifact: `src-tauri\target\debug\paige-desktop.exe`.
`Cargo.lock` is committed so dependency resolution is pinned.

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

**Nothing in this increment demonstrates:** bounded local file access, terminal
execution, agent-to-agent handoff, independent review enforcement, approval
gating, or recovery from an interrupted task. Those are Increment 1 and later.

---

## 8. Negative results worth recording

- No write of any kind to `Paige-Agent-AI` or any existing worktree.
- No modification of `Paige-desktop-deploy`.
- No production deploy, no installer published, no device privilege expanded.
- No credential created, read or stored — no OpenRouter key exists yet.
- No change to NEXUS Vibe Studio or PAIGE Model Fabric.

---

## 9. Independent review

Reviewer requirements: separately spawned, did not author or assist this
implementation, read-only access, findings reported to the owner who remains
final approver.

Review outcome is recorded separately in `docs/REVIEW-INCREMENT-0.md`.
