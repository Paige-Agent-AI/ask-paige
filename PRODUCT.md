<!-- impeccable:product-schema 1 -->

# Product

## Platform

web (delivered as a Windows Tauri 2 native desktop application with a React/WebView frontend)

## Users

**Primary user:** the owner/operator (Mr. Mogul Maker), a solo founder using PAIGE Desktop as their personal AI engineering workstation on Windows. They want to give PAIGE an engineering objective and have it research, build, connect, test, document and deliver results using authorized local and cloud tools.

**Future users:** approved PAIGE Platform Operator engineers and eventually eligible SOLO-account holders.

*Inferred from explicit owner directive; confirm before broadening audience.*

## Product Purpose

PAIGE Desktop is PAIGE's full local coding and engineering workstation — the Windows-native execution surface of the same PAIGE ecosystem. It connects the owner to governed local files, scoped terminal commands, isolated Git worktrees, agent handoffs, approvals, receipts and authorized cloud services (Linear, GitHub, Supabase, Vercel, AWS, Cloudflare, Google Workspace, etc.).

## Positioning

A dark, conversation-centered, native-feeling Windows workstation that makes PAIGE visibly recognizable as the product while preserving safe, incremental rollout of real operating capabilities. Truthful about what is implemented versus planned; no fake buttons or simulated connections.

## Operating Context

- Runs on Windows as a Tauri 2 + React application.
- Independent repo at `C:\Users\tonig\paige-desktop`; no production PAIGE repo changes.
- Authorized lab root at `C:\Users\tonig\paige-desktop-lab` for file-authority testing.
- Native owner-confirmation boundary via Windows `MessageBoxW`.
- Builds use Rust + MSVC; production binary is unsigned during prototype phase.
- CSP and Tauri capability model constrain frontend capabilities.

## Capabilities and Constraints

**Currently implemented:**
- Native file authority broker (grant/list/read/propose/approve/apply/deny/cancel/revoke/status/operations/reconcile).
- OS-backed 32-byte approval tokens and native owner dialog.
- Strict Windows path validation and bounded file reads.
- Explicit Tauri app-command ACL and restrictive CSP.

**Authorized for this increment (1B-UI):**
- Dark, modern, PAIGE-branded workstation shell.
- Screens: Ask PAIGE, Projects & Workspace, Files, Approvals, Activity & Receipts, Agent Team, Terminal, Settings.
- Existing broker surfaced through Files and Approvals screens.
- Honest disabled/placeholder states for unimplemented capabilities.

**Not authorized in 1B-UI:**
- Live terminal execution, Git mutations, cloud inference, provider connectors, document engines, browser/Research integration, skills installation, multi-agent orchestration, production integration.

## Brand Commitments

- Name: PAIGE Desktop.
- Visual direction: dark, Codex-inspired, PAIGE identity with restrained purple accents, clean typography, generous whitespace, native Windows desktop feel.
- Voice: capable, concise, trustworthy; truthful about capability state.
- No AI-generated concept art as evidence; screenshots must come from the running compiled application.

## Evidence on Hand

- Working native broker executable and test suite (88/88 pass).
- Existing Tauri 2 + React scaffold at `C:\Users\tonig\paige-desktop`.
- Owner-approved Desktop Charter and Linear records ANT-168, ANT-161, ANT-147.

## Product Principles

1. **Truthful capability state.** Every button, panel and route honestly reflects whether its backend is implemented.
2. **Safety before scope.** Least-privilege native authority and owner confirmation for consequential actions.
3. **Incremental owner visibility.** Each milestone delivers a real, running UI increment the owner can see and use.
4. **One PAIGE.** Reuse existing PAIGE orchestration, contracts and capability registries; never build a second orchestrator.
5. **Independent verification.** Every increment includes non-author review and regression evidence.
