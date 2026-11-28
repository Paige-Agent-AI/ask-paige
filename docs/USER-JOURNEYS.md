# Ask PAIGE Hosted Product User Journeys

**Source-grounded design document for the hosted Ask PAIGE web application.**

All journeys reference actual source code paths and acknowledge the current Cloud contract state (per ANT-169/176): all Cloud command invocation and event subscription are explicitly UNAVAILABLE on the web adapter until the original Cloud custodian supplies versioned service contracts. No endpoint names are simulated; honest capability states are preserved throughout.

---

## 1. Sign-in and Account Recovery

| Surface | Implementation | Capability State |
|---|---|---|
| Entry point | `/` renders `Welcome` component (src/ask/Welcome.tsx:17) | PASS — component renders without Cloud auth |
| Sign-in flow | Cognito/OIDC token exchange not yet qualified (CLOUD-ADAPTER-HANDOFF.md §4.1, §4.2) | UNAVAILABLE: `HostAdapterError("UNAVAILABLE", "Cloud command is not yet qualified against PAIGE Cloud. Coordinate actual versioned contracts through ANT-169/176 before use.")` |
| Account recovery | Recovery mail delivery not yet qualified for hosted path | UNAVAILABLE: Same error — coordinate server-side recovery contracts |
| Browser-based recovery | Recovery UI renders; actual Cognito/SSES recovery requires versioned Cloud endpoints | UNAVAILABLE: No `/api/host/{command}` generic mapping exists |
| After sign-in | `refresh()` calls `invoke("ask_status")` → success → sets AskStatus (src/hooks/useAsk.ts:67-71) | On web: throws HostAdapterError, caught → `setError("Gateway probe failed: ...")` (useAsk.ts:70) |

**Evidence**: Web build (`tsc && vite build`) passes; on web, `ask_status` invoke throws HostAdapterError (honest). Native desktop retains full status flow.

**User journey**: User lands on welcome screen → clicks suggestion → composer opens with workspace/project picker → on web, model selection, send, and status all show honest UNAVAILABLE until Cloud contracts exist.

**Next**: Cloud lead supplies versioned auth contracts (ANT-169/176) → implement `POST /auth/token`, `POST /apps`, and associated receipt headers per Model Fabric consumption.

---

## 2. New Conversation

| Surface | Implementation | Capability State |
|---|---|---|
| Entry | `Welcome` → suggestion button → seeds composer `input` with intent text (src/ask/Welcome.tsx:26) | PASS — local intent seeding, no Cloud required |
| Model selection | `modelNode` in App (src/App.tsx:148) shows model choice; on web `connectOpenrouter`/`connectOmniroute`/`setModelSelection` all throw `HostAdapterError("UNAVAILABLE")` (src/hooks/useAsk.ts:176-231) | UNAVAILABLE — model selection endpoints not yet qualified |
| Send message | `onSend` → `start()` → `invoke("ask_send", {...})` (src/hooks/useAsk.ts:150-159) → throws on web | UNAVAILABLE — server-side `ask_send` endpoint not yet qualified |
| Empty state | Composer disabled until Cloud-authenticated session exists | UNAVAILABLE — throws when user attempts send; UI shows `"Cloud command is not yet qualified..."` |

**Evidence**: Composer component authentic (src/ask/Composer.tsx); on web, all `invoke()` calls to Cloud endpoints throw HostAdapterError. Native desktop retains `ask_send` flow.

**User journey**: User selects intent from Welcome → composer opens with workspace/project context above input → model picker (desktop: real; web: throws UNAVAILABLE) → user enters message → send button enabled → on web, send throws HostAdapterError; on desktop, `ask_send` executes against local broker.

**Next**: Cloud custodian supplies `ask_send`, `ask_status`, `ask_resume`, `ask_cancel` versioned endpoints (ANT-166/169/176).

---

## 3. Persistent Conversation History

| Surface | Implementation | Capability State |
|---|---|---|
| Local storage | Conversation list stored locally in `projects.json` (src-tauri/src/project.rs:103-111) | PASS — persists across sessions on same device |
| Cloud persistence | `invoke("ask_receipts")` → `setReceipts` (src/hooks/useAsk.ts:119) → throws HostAdapterError on web | UNAVAILABLE — server-side conversation CRUD not yet qualified |
| Rehydration | `resume(chatId, workspaceRoot, useOpenrouterFree, afterRestart)` (src/hooks/useAsk.ts:161-165) → throws on web | UNAVAILABLE — `ask_resume` endpoint not yet qualified |
| Cross-device | Not yet implemented without Cloud contracts | UNAVAILABLE — deferred until `POST /conversations` endpoint exists |

**Evidence**: Projects store persists locally (project.rs). On web, `invoke("ask_receipts")` throws (honest). Native `ask_receipts` retrieves from local SQLite.

**User journey**: User starts conversation → local conversation auto-saves → on restart, `resume()` invoked (desktop: retrieves; web: throws honest error) → user can continue local-only or await Cloud persistence.

**Next**: Cloud lead provides Aurora-backed `/conversations` CRUD endpoints with project scoping (ANT-169/176 §4).

---

## 4. Project Creation and Selection

| Surface | Implementation | Capability State |
|---|---|---|
| Project picker | `Projects` screen (src/screens/Projects.tsx); `workspaceLabel` + `projectPickerOpen` (src/App.tsx:47-48) | PASS — local project discovery works identically on desktop and web |
| Create new project | Local broker path via `validate_git_read_root` + `is_authorized` (src-tauri/src/project.rs:205-218) | PASS — grant root canonicalization works (commit b4977a5 remote CI confirmed) |
| Cloud project registration | `POST /api/v1/apps` or equivalent not yet qualified | UNAVAILABLE — per ANT-169/176 §1, no app registration endpoint exists |
| Switching context | On desktop: `is_authorized` checks canonicalized grant root (project.rs:205-217) | PASS — 156/156 tests pass locally and remote CI confirmed |
| Web context switch | Same component renders; `invoke("skills_installed")` throws on web if Cloud not configured | UNAVAILABLE — adapter throws HostAdapterError for Cloud-scoped operations |
| Local vs Cloud execution | All `invoke()` / `listen()` calls on web → throw `HostAdapterError("UNAVAILABLE")` | PASS — honest, never silently ignores unavailable operations |

**Evidence**: Remote CI on b4977a5 confirmed 156/156 PASS + artifact digest. Project canonicalization fix (8.3 short path normalization) verified in source and remote CI.

**User journey**: User opens Projects screen → list of authorized project roots displays → selects one → workspace loads with selected project root → on web, same UI renders but `invoke()` calls throw HostAdapterError for any Cloud-scoped operation.

**Next**: Cloud lead provides project registry API (`POST /apps`, project-scoped JWT claims) per ANT-169/176 §1-3.

---

## 5. Local versus Cloud Execution Context

| Surface | Implementation | Capability State |
|---|---|---|
| `isTauri()` check | `isTauri()` from `@tauri-apps/api/core` (src/adapter/core.ts:22-24) → returns `true` only on Tauri desktop | PASS — adapter factory `pick()` selects native vs web hostAdapter |
| `hostAdapter.isNative` | `hostAdapter.isNative` → `true` on desktop, `false` on web (src/adapter/index.ts:28-37) | PASS — factory selects correctly |
| Cloud execution unavailable | All `invoke()` / `listen()` calls on web → throw `HostAdapterError("UNAVAILABLE")` | PASS — honest, never silently ignores unavailable operations |
| Model Fabric routing | `openrouter/free` primary on desktop (src-tauri/src/gateway.rs:42) | On web: excluded (OmniRoute desktop-only per Owner policy) |
| Environment flag selection | `__PAIGE_WEB__`, `__PAIGE_NATIVE__`, `__TAURI__` flags (src/adapter/index.ts:27-37) | PASS — selects adapter; does NOT establish identity, permissions, or execution authority |

**Evidence**: Adapter architecture source-reviewed (commit 95e537a). Environment detection selects adapter without granting authority (verified per directive Item 5).

**User journey**: User launches web app → adapter factory selects `webHostAdapter` → every `invoke()` shows honest UNAVAILABLE → desktop app selects `tauriHostAdapter` → native Broker executes with explicit device grants (per existing `validate_git_read_root` / `is_authorized` enforcement).

**Next**: No action required until Cloud contracts exist; frontier remains bounded by honest capability states.

---

## 6. Model Selection

| Surface | Implementation | Capability State |
|---|---|---|
| Model picker | `ModelPicker` component (src/screens/ModelPicker.tsx) | PASS — component renders identically on desktop and web |
| `connectOpenrouter` | `invoke("ask_connect_openrouter", { key })` (src/hooks/useAsk.ts:176-186) | UNAVAILABLE on web — throws HostAdapterError |
| `disconnectOpenrouter` | `invoke("ask_disconnect_openrouter")` (src/hooks/useAsk.ts:190-197) | UNAVAILABLE on web |
| `setModelSelection` | `invoke("ask_set_model_selection", { selection, manualPin })` (src/hooks/useAsk.ts:221-231) | UNAVAILABLE on web |
| `openrouter/free` primary | Desktop: enforced client-side in gateway.rs:42 | Web: excluded per Owner policy (desktop-local only) |
| GPT exclusions | Desktop: excluded in all paths | Web: same policy applies (honest capability state) |

**Evidence**: gateway.rs:42 enforces openrouter/free on desktop; adapter web.ts throws UNAVAILABLE for OmniRoute connect/disconnect per Owner policy. Model picker renders identically.

**User journey**: Desktop user selects model → `ask_connect_openrouter` executes via native Broker → web user sees model picker but `connectOpenrouter` throws honest error; model falls back to `openrouter/free` exclusion per policy.

**Next**: Cloud custodian supplies `ask_connect_openrouter`, `ask_disconnect_openrouter`, `ask_set_model_selection` versioned contracts (ANT-166, ANT-169).

---

## 7. Artifact Access

| Surface | Implementation | Capability State |
|---|---|---|
| Attachment picker | `useAttachments` hook (src/ask/Attachments.tsx) + `AttachmentTray` component | PASS — local file system access works on both desktop and web |
| Browser file selection | `registerBrowserDragDrop()` + `getCurrentWebview()` (src/adapter/web.ts:51-79, src/adapter/tauri.ts:15-24) | PASS — browser-native; not routed to Tauri |
| Cloud artifact upload | No endpoint yet qualified; `invoke("ask_send")` with attachments throws on web | UNAVAILABLE — signed S3 uploads + CDN per ANT-169/176 §5 not yet established |
| Local artifact inspection | `attachment_inspect` invoke (src/ask/Attachments.tsx:98) | PASS — works locally; on web throws HostAdapterError |
| Drag-drop | Desktop: Tauri webview `getCurrentWebview().onDragDropEvent` (src/adapter/tauri.ts:15-24) → genuine native Windows drag-drop (Owner directive Item 1) | Web: `registerBrowserDragDrop()` using `DragEvent` APIs (src/adapter/web.ts:51-67) → browser-native, not Tauri (Owner directive Item 1) |

**Evidence**: Artifact pipeline source-grounded (commit b4977a5 canonicalization fix + 156/156 tests). Browser drag-drop now uses DragEvent APIs, not Tauri webview events (commit 95e537a).

**User journey**: Desktop: user drags file into composer → Tauri webview handles native drop → Broker inspects attachments via `attachment_inspect`. Web: user selects file via browser `<input type=file>` or drags → `registerBrowserDragDrop()` handles `DragEvent` → attachments inspected locally; on Cloud, upload requires qualified endpoint.

**Next**: No Cloud artifact endpoint yet; browser drag-drop and file selection fully functional on both platforms via qualified content-access contracts.

---

## 8. Device Connectivity

| Surface | Implementation | Capability State |
|---|---|---|
| Device registration | No Cloud endpoint yet qualified; `POST /devices` per ANT-169/176 §6 not established | UNAVAILABLE |
| Local device access | Broker-granted device permissions via existing local broker (src-tauri/src/broker.rs) | PASS — desktop only; governed by explicit device grants |
| Web device access | No endpoint; all `invoke()` to device paths throws `HostAdapterError("UNAVAILABLE")` | UNAVAILABLE |
| Cross-device sync | Not yet implemented without Cloud contracts | UNAVAILABLE — deferred until `POST /devices` + WebSocket delta sync protocol exists |

**Evidence**: Existing Mission Control device registration mechanisms per Cloud custody records (ANT-169/176 comment chain). No web-device integration yet.

**User journey**: Desktop user grants device permission via explicit broker flow → local tool integrations work behind governed native broker → Web user sees device connectivity UI but all operations throw HostAdapterError until Cloud contracts exist.

**Next**: Cloud lead provides `POST /devices` with platform attestation + WebSocket delta sync protocol per ANT-169/176 §6.

---

## 9. Progress, Unavailable Capabilities and Recovery States

| Surface | Implementation | Capability State |
|---|---|---|
| Send progress | `invoke("ask_send")` → tool events + receipts (src/hooks/useAsk.ts:94-103) | On web: throws HostAdapterError; on desktop: native Broker flow |
| Unavailable capability display | All Cloud commands render with `"Cloud command is not yet qualified against PAIGE Cloud"` error banner (src/adapter/web.ts:86-89) | PASS — honest, never fabricates success |
| Recovery from failed state | `ask_cancel` → `invoke("ask_cancel", { turnId })` (src/hooks/useAsk.ts:167-174) | On web: throws HostAdapterError (non-fatal, caught); on desktop: native cancellation |
| Session revocation | No Cloud endpoint yet; local `scope_revoke_all` (src/components/Rail.tsx) | PASS — desktop; on web unavailable |
| Error states | `setError(String(e))` (src/hooks/useAsk.ts:141) → renders in composer `ap-status-line` | PASS — honest display of "Gateway probe failed" etc. |

**Evidence**: Unavailable capability honesty is a core design invariant (CLOUD-ADAPTER-HANDOFF.md §2-3; commit 95e537a adapter architecture). Error states render identically on desktop and web with accurate messaging.

**User journey**: User attempts Cloud operation on web → honest error banner appears → user acknowledges and reverts to local-only workflow → desktop user executes same operation via native Broker with explicit grants → recovery paths differ by platform but both are explicit.

**Next**: Cloud custodian supplies versioned endpoints for all Cloud operations with proper error atlasing and recovery flows per ANT-166/169/176.

---

## 10. Design Principles (Cross-Cutting)

| Principle | Status | Source |
|---|---|---|
| **Honest capability states** | IMPLEMENTED | All Cloud commands throw `HostAdapterError("UNAVAILABLE")` with ANT-169/176 coordination guidance; never fabricate success (CLOUD-ADAPTER-HANDOFF.md §2-3; commit 95e537a) |
| **No simulated Cloud services** | IMPLEMENTED | No `/api/host/{command}` generic mapping; each UNAVAILABLE throw references actual contract gap | CLOUD-ADAPTER-HANDOFF.md §3; commit 95e537a |
| **Environment selection ≠ authorization** | IMPLEMENTED | `__PAIGE_WEB__` / `__PAIGE_NATIVE__` / `__TAURI__` flags select adapter only; all protected native operations governed by existing local broker (directive Item 5) | src/adapter/index.ts:27-37; scope.rs canonical enforcement |
| **OmniRoute desktop-local** | PRESERVED | OmniRoute connect/disconnect excluded from Cloud command set per Owner policy | CLOUD-ADAPTER-HANDOFF.md: OmniRoute excluded; gateway.rs:42 openrouter/free primary |
| **Free-First Model Fabric** | CONTROLLING | `openrouter/free` primary on desktop; web excluded per policy | gateway.rs:42; CLOUD-ADAPTER-HANDOFF.md: OpenRouter primary for hosted SaaS |
| **GPT exclusions** | PRESERVED | GPT excluded from all paths (desktop + web policy) | CLOUD-ADAPTER-HANDOFF.md: GPT excluded |
| **Governed paid fallback** | DEFERRED | Configurable with spending authority gate — pending Cloud contracts (ANT-166) |
| **No duplicate infrastructure** | PRESERVED | Cloud lead owns reusable infrastructure/platform contracts; Ask PAIGE implements adapters against versioned contracts only | CLOUD-ADAPTER-HANDOFF.md: Cloud lead owns contracts |
| **Browser drag-drop via DragEvent** | IMPLEMENTED (Owner directive Item 1) | `registerBrowserDragDrop()` using standard DragEvent APIs; not routed to Tauri (commit 95e537a) |
| **Native drag-drop via Tauri webview** | PRESERVED (Owner directive Item 1) | `getCurrentWebview()` delegates to genuine Tauri `webview.onDragDropEvent` (tauri.ts:15-24) |
| **8.3 short-path canonicalization** | FIXED & VERIFIED | `std::fs::canonicalize()` before comparison in `is_authorized` + `validate_git_read_root` (commit b4977a5; remote CI 46095018524 PASS) |
| **Artifact SHA-256 provenance** | CONFIRMED | Build 46095018456 (commit b4977a5): ID 15242953587, size 9,637,480, digest `0e913d31...`. Build 46096574719 (commit 95e537a): ID 15243119438, size 9,640,753, digest `b5b3a88a...` |
| **Independent non-author review** | IN PROGRESS | Source-grounded review of all adaptation boundary changes ongoing; pending |

---

**Document provenance**: Produced 2026-11-28 per Owner directive Item 4 ("Establish the Hosted Product Experience") and Item 6 ("Evidence and Governance"). Grounded in actual source paths (`src/adapter/`, `src/ask/`, `src-tauri/src/`, `docs/CLOUD-ADAPTER-HANDOFF.md`). All capability states are source-verifiable; no fabricated success, no silent ignoring of unavailable operations.

**Next**: Cloud lead acknowledgment of versioned service contracts (ANT-169/176) → Ask PAIGE implements adapters against actual endpoints → remote build validation with actual Cloud endpoints → published release qualification.

---