# PAIGE Desktop Development Roadmap

**Owner-accepted milestone:** Increment 1B-UI — dark PAIGE Desktop workstation shell with official Command Mark branding and verified Windows desktop launcher.

**Date accepted:** 2026-11-18

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

## Queued / owner-selected next outcomes

The order below is a proposal. The owner will designate the active next milestone.

### 1C — Scoped Terminal Execution

- Local allowlisted command execution with process isolation, output capture and kill switch.
- Native per-command approval where required.
- No shell injection, no unrestricted access.
- Depends on owner authorization.

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

### 1E — Engineering Inference Gateway Integration

- Verify and configure OmniRoute as preferred coding gateway.
- Support separately authenticated direct OpenRouter connection.
- Configure/test OpenCode inference routing.
- Determine Codex compatibility without disrupting authentication.
- Preserve coding tools, model capability, conversation continuity, usage attribution and safe recovery.
- Show truthful gateway, model and connection status in Models & Connections.
- **Linear:** ANT-168 inference directive

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

---

## Activation rule

I will not begin source-code implementation, provider activation, or production integration for any queued outcome until the owner explicitly designates it as the active next milestone.
