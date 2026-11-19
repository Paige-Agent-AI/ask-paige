# PAIGE Desktop Development Roadmap

**Owner-accepted milestone:** Increment 1C — Scoped Terminal Execution (final reconciliation accepted 2026-11-19).

**Amended sequence (owner, 2026-11-19):** 1C → 1E → 1D → 1F → 1G → 1H.

This roadmap records owner-authorized and queued outcomes. No outcome is activated for implementation until the owner explicitly selects it.

---

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

## Active

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

## Queued (controlling order)

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
