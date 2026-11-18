# Storage Audit and Closeout — 2026-11-18

**Scope:** read-only disk and cache inventory of the Windows development machine
before further PAIGE Desktop builds.

**Method:** directory-size enumeration, registry uninstall-key reads, process /
service / scheduled-task queries, and file-attribute analysis to separate real
local bytes from cloud placeholders.

**Actions taken by the auditing agent:** none. No deletion, uninstall, cache
clean, settings change or build-artifact removal was performed or authorized.

Paths below use `%USERPROFILE%` in place of the account-specific profile path.
No personal filenames, client names, document titles or credential material are
included.

---

## 1. Baseline free space for the next increment

| Measurement | Value |
|---|---|
| Free space reported during audit | **40.80 GB** |
| Closeout re-measure, sample 1 (13:32:08) | **40.78 GB** |
| Closeout re-measure, sample 2 (13:32:28) | **40.78 GB** |
| Delta over 20 s | −0.2 MB → **stable** |
| Disk used / total | 911.89 / 952.67 GB |

**Baseline for the next development increment: 40.80 GB free** (40.78 GB on
closeout verification, stable within measurement noise).

Earlier in the same session free space was **9.63 GB**. The increase happened
concurrently with the audit and is not attributable to the auditing agent.

---

## 2. Attribution of removals observed during the audit

| Observation | First seen | Later seen | Attributable to this agent? |
|---|---|---|---|
| Recycle Bin contents | 1.97 GB | 0.00 GB | **No** |
| `Roaming\Cursor` directory | 20.34 GB | absent | **No** |
| Hermes MSIX package | present | absent | **No** |
| Free space | 9.63 GB | 40.80 GB | **No** |

**Evidence for that statement.** All five audit scripts written by the agent
were re-read and scanned for destructive verbs (`Remove-Item`, `rmdir`, `del`,
`Clear-RecycleBin`, `Uninstall-*`, `Stop-Process`, `Set-ItemProperty`,
`powercfg`, `vssadmin delete`, `Dism`). **Zero matches in all five.**

The only `Remove-Item` calls this agent has issued anywhere in Increment 0 are
scoped to its own temporary working directories:

| Script | Target |
|---|---|
| `install-rust.ps1` | its own `linker-probe` temp directory |
| `reinstall-vs.ps1` | its own `linker-probe2` temp directory |
| `scaffold-repos.ps1` | its own scaffold staging directory |

No user file, application, cache or repository was ever a target.

**Attribution — confirmed by the owner.** After this closeout was first
written, the owner stated: *"I deleted those things myself."* The Recycle Bin
emptying, the `Roaming\Cursor` removal and the Hermes MSIX uninstall were
**owner actions**, performed manually and independently of the auditing agent.
The free-space increase from 9.63 GB to 40.80 GB is therefore owner-initiated
recovery, not agent recovery.

For the record: **Windows Storage Sense is enabled** on this machine
(`StoragePolicy 01 = 1`), which can empty the Recycle Bin and temporary files
automatically. It cannot uninstall applications, so it was never a sufficient
explanation for the Cursor or Hermes removals — hence the owner confirmation.

---

## 3. Hermes finding

Hermes is the **Nous Research Hermes Agent CLI** — an agent harness, not a set
of downloaded model weights.

| Component | State at closeout |
|---|---|
| MSIX package | already absent |
| `Roaming\Hermes` | 0.01 GB leftover |
| `Local\hermes` | 0.02 GB leftover (skills, history file) |
| Processes / services / scheduled tasks | none running |
| Local model weights (`.gguf`, `.safetensors`, `.onnx`, `.pt`, `.ckpt`) | **none found** |
| HuggingFace / torch / whisper caches | absent |
| Ollama, LM Studio, GPT4All, Jan, AnythingLLM | absent |

**Recoverable from Hermes: ~0.03 GB.** There is no multi-GB model cache on this
machine. `hermes.dll` inside Office and OneDrive is an unrelated Microsoft
telemetry library, not Hermes.

## 4. Cloud content is not disk usage

| Folder | Logical size | Actual local bytes |
|---|---|---|
| OneDrive | 176.52 GB | **0.17 GB** |
| iCloudPhotos | 577.60 GB | **0.00 GB** |
| iCloudDrive | 4.61 GB | **0.00 GB** |

Roughly 758 GB of apparent usage is cloud placeholders. It is not reclaimable
and must not be deleted to "free space" — that risks real data for zero local
gain. All cloud-synchronized content is preserved.

## 5. Candidates identified but NOT actioned

Listed for the record only. Owner declined further cleanup.

| Candidate | Size | Note |
|---|---|---|
| npm cache | 6.49 GB | Regenerable |
| User temp | 2.51 GB | Partly in use |
| Installer temp directories | ~2.8 GB | Stale updater leftovers |
| CapCut | 24.21 GB | Contains user project data — export first |
| AI CLI session histories | ~34 GB | History loss, not a build dependency |
| OmniRoute installations | 4.95 GB | **Preserved** — optional future engineering gateway |
| Browser profile | 22.44 GB | Active; cache-only clearing would be safe |
| Video files in Downloads | 171.02 GB | Personal content — archive, never delete |
| `node_modules` in dated work copies | ~200 GB | Regenerable, needs a dated manifest |
| Pagefile | 23.14 GB allocated, 9.62 GB peak | System setting, needs admin + reboot |
| Hibernation file | 6.27 GB | Would disable hibernate / Fast Startup |

## 6. Explicitly preserved

Windows `Installer` directory (MSI repository), `WinSxS` (DISM only), MSVC Build
Tools, Windows SDK, Rust, Node, Git, OpenCode, OmniRoute, both PAIGE Desktop
repositories, all production PAIGE worktrees, credentials, engineering
histories, creative assets and cloud-synchronized content.

`paige-desktop\src-tauri\target` (5.54 GB) was left intact; the compiled
executable of record is still present.

## 7. Cleanup tasks remaining active

| Check | Result |
|---|---|
| Agent audit/cleanup scripts still running | **none** (the only match was the closeout script inspecting itself) |
| Agent-created scheduled tasks | **none** |
| Storage-related scheduled tasks present | stock Windows tasks only (`SilentCleanup`, `StorageSense`, `StartComponentCleanup`, etc.), all in `Ready` state, none created by this agent |
| Storage Sense | enabled, pre-existing; left unchanged |

## 8. Outcome

Storage remediation is **complete for current development needs**. Free space
exceeds the 25–30 GB target. No remote build is required; the local Rust/MSVC
toolchain is adequate.

Nothing in this document authorizes further cleanup. Increment 1 remains
unstarted pending separate owner authorization.
