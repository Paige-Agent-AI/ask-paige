# PAIGE Desktop Development Roadmap

**Owner-accepted milestone:** Increment 1C — Scoped Terminal Execution (final reconciliation accepted 2026-11-19).

**Earlier increment sequence (owner, 2026-11-19):** 1C → 1E → 1D → 1F → 1G → 1H.

This roadmap records owner-authorized and queued outcomes. No outcome is activated for implementation until the owner explicitly selects it.

---

## Current Desktop acceptance sequence

The owner set this order for the private Windows Desktop. Each milestone needs evidence in the installed app and owner acceptance before the next one proceeds. The earlier increment entries below retain their historical decisions and evidence.

### 1. Dependable engineering and restart recovery

In the installed app with Vite stopped, give PAIGE Auto a plain-English request to fix one real issue in a pinned local Desktop worktree. PAIGE must read the relevant source, propose a targeted diff, wait for the owner's native approval, and report genuine test results from the local broker. Qualify the model route through actual tool execution. If PAIGE cannot proceed safely, she must explain what failed and ask for the specific decision she needs.

**Acceptance.** Interrupt the run after the approved write is verified and before testing. Reopen the same chat, recover from the saved receipt, and run the offline tests without repeating the write or asking the owner to select the same project again. The chat must retain the diff, approval, verified file hash, test command, exit code, and result. The owner should only need to act on approvals and genuine decisions. [ANT-172](https://linear.app/paigeagentai/issue/ANT-172)

### 2. Readable Workbench and chat history

Keep chats on the left, the active conversation and composer in the center, and the Workbench on the right. Show clear inline states for the proposed diff, owner approval, and test outcome. Let the owner open the full diff, files, command output, and receipts from the relevant turn. Keep workflows in the background.

**Acceptance.** Demonstrate two chats with switching, search, rename, archive, and history after a cold restart. Check the complete diff and readable test output at Windows 125% and 150% scaling in light and dark modes. Finish owner acceptance for ANT-172 before starting milestone 3. [ANT-172](https://linear.app/paigeagentai/issue/ANT-172)

### 3. Reusable Engineering workflow

Use the existing PAIGE orchestration, Harness, Trust, broker, and chat receipts for the first reusable workflow. Run one real change in an isolated authorized worktree. Keep the plan, approval, focused tests, independent non-author reviewer handoff, and final owner decision in the same chat.

**Acceptance.** Reopen the installed app and inspect the saved run, full diff, test receipts, and reviewer findings. The owner can decide whether to accept the completed change. [ANT-179](https://linear.app/paigeagentai/issue/ANT-179)

### 4. Visual QA and refinement

Start with an owner-selected screenshot and one bounded UI correction. Attach the source evidence to the chat, route the edit through the existing approval flow, and show a genuine before-and-after comparison from the installed app.

**Acceptance.** Verify the corrected visual behavior and relevant interaction checks before expanding to short video. Broader Windows operations follow a separate owner authorization. [ANT-175](https://linear.app/paigeagentai/issue/ANT-175) · [ANT-173](https://linear.app/paigeagentai/issue/ANT-173)

---

## Chat entry point for Desktop capabilities

An owner can ask for a workflow, enabled skill, or specialist in ordinary chat. Use the conversation intent router for new capabilities before calling the existing executor. Preserve the original request and pinned project, ask inline for a missing input or an ambiguous choice, and record the selected action with its outcome. Manual selection and explicit commands remain available. Choosing a route grants no new access and never substitutes for a native approval. Future automations use the same entry point with an explicit schedule, destination, and approval before activation.

## Completed

### 1B-UI — PAIGE Desktop Owner Workstation

- Dark, PAIGE-branded Windows workstation shell.
- Collapsible left rail with nine screens.
- Native file broker connected to Files, Approvals and Activity.
- Truthful disabled/placeholder states for unimplemented capabilities.
- Official Command Mark applied across navigation, window chrome, executable icon and installer.
- Verified clickable Windows desktop shortcut.
- **Evidence:** `docs/INCREMENT-1B-UI-EVIDENCE.md`, `docs/INCREMENT-1B-UI-BRANDING-EVIDENCE.md`
- **Review:** `docs/REVIEW-INCREMENT-1B-UI.md`, `docs/REVIEW-1B-UI-BRANDING.md`
- **Linear:** ANT-168

### 1C — Scoped Terminal Execution

- Allowlisted command execution confined to authorized project roots, with output streaming, verified kill and receipts.
- Risk-based authority: read/standard-write auto-run; privileged/destructive require explicit owner approval.
- Fail-closed missing-root handling; Windows quoted-argument parsing; accurate stdout/stderr receipts.
- Process engine designed for reuse by the future native MCP `stdio` client.
- **Owner-accepted:** 2026-11-19 (after the final reconciliation pass).
- **Evidence:** `docs/INCREMENT-1C-EVIDENCE.md`
- **Reviews:** `docs/REVIEW-INCREMENT-1C.md`, `docs/REVIEW-INCREMENT-1C-CORRECTION.md`
- **Linear:** ANT-168

---

## Earlier implementation checkpoints

### 1D — Native MCP Client + Connection Manager

- Make PAIGE Desktop a full MCP client (ANT-170).
- Support multiple simultaneous independent MCP servers:
  - Local `stdio` subprocess servers (reusing the 1C terminal engine for spawn/kill).
  - Remote Streamable HTTP over HTTPS servers (production builds: strict cert trust).
- Reuse PAIGE's existing MCP client, capability gateway and integration contracts where applicable.
- Do not create a competing orchestration layer or provider registry.
- Required surfaces:
  - Models & Connections MCP server management.
  - Add / connect / disconnect / reconnect / revoke.
  - Tool/resource/prompt discovery (`tools/list`, etc.).
  - Authenticated connections, permissions, execution, receipts, reconnection and revocation.
  - Truthful status: ready, auth-needed, offline, unsupported.
  - Per-project/account bindings and isolation.
- Potential servers (only when installed/authorized): GitHub, Linear, Supabase, Vercel, Cloudflare, AWS, Google Workspace, research/browser, local file/build tools.
- Future secondary capability: PAIGE Desktop as a controlled MCP server for other coding agents (separate gated increment).
- **Status:** implementation complete; MCP runtime suite passing (19/19); Rust/static/broker/terminal/ask/MCP all green; independent non-author review PASS WITH MINOR FINDINGS (F1/F2 addressed); awaiting owner acceptance. STOP before 1F after owner acceptance.
- **Evidence:** `docs/INCREMENT-1D-EVIDENCE.md`
- **Linear:** ANT-170

### 1E — Intelligent Ask PAIGE Chat / Engineering Inference UX

- Functional Ask PAIGE composer: project selection, PLAN/BUILD modes, file
  attachments, slash commands, @ context references, Auto (default) model
  selection with manual override.
- Fixed gateway table: OmniRoute preferred, direct OpenRouter alternate
  (owner key connect, DPAPI-protected), test-only local harness for automated
  tests.
- Implemented + unit-tested auto model-selection policy with rationale and
  receipts; streaming with cancellation.
- Complete workflow: instruction → model → broker/terminal tools → real test
  run → verified result + receipts.
- **Status:** implementation complete; 156/156 regression checks; independent
  non-author review ACCEPT (post-correction final code); awaiting owner
  acceptance. Live paid inference requires the owner to restore
  OmniRoute's STORAGE_ENCRYPTION_KEY or connect an OpenRouter key.
- **Evidence:** `docs/INCREMENT-1E-EVIDENCE.md`
- **Linear:** ANT-171, ANT-168

---

## Earlier increment queue

### 1D — Native MCP Client + Connection Manager

- Make PAIGE Desktop a full MCP client (ANT-170).
- Support multiple simultaneous independent MCP servers:
  - Local `stdio` subprocess servers.
  - Remote Streamable HTTP over HTTPS servers.
- Reuse PAIGE's existing MCP client, capability gateway and integration contracts where applicable.
- Do not create a competing orchestration layer or provider registry.
- Required surfaces:
  - Models & Connections / Settings MCP server management.
  - Add / connect / disconnect / reconnect / revoke.
  - Tool/resource/prompt discovery (`tools/list`, etc.).
  - Authenticated connections, permissions, execution, receipts, reconnection and revocation.
  - Truthful status: ready, auth-needed, offline, unsupported.
  - Per-project/account bindings and isolation.
- Potential servers (only when installed/authorized): GitHub, Linear, Supabase, Vercel, Cloudflare, AWS, Google Workspace, research/browser, local file/build tools.
- Future secondary capability: PAIGE Desktop as a controlled MCP server for other coding agents (separate gated increment).
- **Linear:** ANT-170

### 1F — Git / Worktree / PR Workflow

- Authorized Git operations (clone, branch, commit, PR) via MCP or native broker.
- Project discovery and workspace management.
- Depends on 1D MCP and/or 1C terminal authorization.

### 1G — Document Creation & Editing

- Rich document editor integrated with Files and approved cloud stores.
- Depends on MCP/documents server.

### 1H — Secure PAIGE Account / Device Integration

- Connect Desktop to owner PAIGE account and device authority.
- Preserve single original PAIGE orchestration architecture.

---

## Decision log

- 2026-11-18: Owner formally accepted 1B-UI and the Command Mark branding correction.
- 2026-11-18: Owner approved MCP connectivity as a mandatory core capability, documented in ANT-170. Implementation is queued and not yet activated.
- 2026-11-19: Owner amended the controlling development sequence to **1C → 1E → 1D → 1F → 1G → 1H** and documented the 1E Ask PAIGE chat requirements in ANT-171.
- 2026-11-19: Owner accepted Increment 1C after the final reconciliation and authorized Increment 1E.

---

## Activation rule

I will not begin source-code implementation, provider activation, or production integration for any queued outcome until the owner explicitly designates it as the active next milestone.
