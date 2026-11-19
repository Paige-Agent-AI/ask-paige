# PAIGE Desktop Development Roadmap

**Owner-accepted milestone:** Increment 1B-UI — dark PAIGE Desktop workstation shell with official Command Mark branding and verified Windows desktop launcher.

**Date accepted:** 2026-11-18

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

---

## Active

### 1C — Scoped Terminal Execution

- Local allowlisted command execution with process isolation, output capture and kill switch.
- Risk-based authority: read/standard-write run automatically; privileged/destructive actions require explicit owner approval.
- No shell injection, no unrestricted access.
- Process-spawn engine designed for reuse by the future native MCP `stdio` client.
- **Status:** implementation complete; regression tests and independent review in progress.

---

## Queued (controlling order)

### 1E — Intelligent Ask PAIGE Chat / Engineering Inference UX

- Functional Ask PAIGE chat experience with real composer controls:
  - Project, folder and workspace selection
  - PLAN / BUILD operating modes
  - File attachments
  - Slash (/) commands
  - Context (@) references
  - Model selection: **Auto by default, Manual override available**
- Auto mode intelligently selects qualified models per task through the preferred OmniRoute engineering gateway, with direct OpenRouter as the alternate.
- Display actual selected model, provider and connection status.
- Implement and test the actual model-selection decision behavior, not a decorative placeholder.
- Preserve existing inference-gateway verification constraints (no unverified connectivity claims).
- **Linear:** ANT-171

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
- 2026-11-19: Owner amended the controlling development sequence to **1C → 1E → 1D → 1F → 1G → 1H** and documented the 1E Ask PAIGE chat requirements in ANT-171. 1C remains the only currently authorized implementation outcome.

---

## Activation rule

I will not begin source-code implementation, provider activation, or production integration for any queued outcome until the owner explicitly designates it as the active next milestone.
