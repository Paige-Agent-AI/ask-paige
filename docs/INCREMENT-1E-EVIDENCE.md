# Increment 1E — Intelligent Ask PAIGE Chat & Engineering Inference

**Status:** implementation, verification and exact-head review complete; awaiting owner acceptance
**Date:** 2026-11-19
**Authorization:** owner authorization, Increment 1E (ANT-171, ANT-168)
**Scope:** a genuinely functional Ask PAIGE experience with project/folder
selection, PLAN/BUILD modes, file attachments, slash commands, @ context
references, Auto (default) model selection with manual override, OmniRoute as
the preferred engineering gateway with direct OpenRouter as the alternate, and
the complete instruction → model → file/terminal tools → test → verified
result + receipts workflow.

---

## A. Source and commit identity

- **HEAD:** `8ba9a609375abb797e38145eb0f832879ac398a8`
- **Branch:** `feat/paige-desktop-prototype`

Key files:

- `src-tauri/src/gateway.rs` — fixed gateway table (OmniRoute preferred, direct OpenRouter alternate, test-only harness), qualified model pools, and the pure, unit-tested auto model-selection policy.
- `src-tauri/src/ask.rs` — the Ask PAIGE session layer: gateway probing, catalog building, SSE streaming chat, tool-call loop, receipts, cancellation.
- `src-tauri/src/secrets.rs` — DPAPI-protected owner OpenRouter key storage outside the repository.
- `src-tauri/src/terminal.rs` — added `run_to_completion` (synchronous, same allowlist/classification/cwd validation engine; used by the Ask tool loop).
- `src/hooks/useAsk.ts`, `src/screens/AskPaige.tsx` (+ css) — composer + transcript UI.
- `src/screens/ModelsConnections.tsx` — truthful gateway health + owner key connect flow.
- `scripts/test-harness-gateway.mjs` — TEST-ONLY OpenAI-compatible local gateway (scripts the model behaviors; never claimed as a provider).
- `scripts/test-ask-runtime.mjs` — 23-check end-to-end runtime suite.

---

## B. What is functional

| Requirement | State |
|---|---|
| Project/folder/workspace selection | Project chip shows the authorized lab root and live broker grant state; clicking it grants access. Attachments/@refs come only from the authorized root. |
| PLAN / BUILD modes | Real tool policy: PLAN permits reads and read-risk commands only — write_file is refused by the policy engine and mutating commands are rejected; BUILD additionally permits broker writes (native owner approval) and standard-write commands. Verified by runtime checks 5a–5c. |
| File attachments | Broker-backed file picker with filter, preview chips, and removal; attachments are read through the broker into the conversation with sha256 receipts. |
| Slash (/) commands | Palette with inspect / test / build / review / explain / research quick actions. |
| Context (@) references | Inline searchable references to authorized files with stable paths and permission bounds; resolved through the broker. |
| Model selection: Auto default + manual override | Auto (default) runs the implemented selection policy; the manual dropdown pins an exact qualified model. Manual pin is honored when valid and falls back with an explicit truthful reason otherwise. |
| Selected model/provider visibility | The chat shows the selected model, gateway and rationale line; Models & Connections shows live gateway health, catalog, and receipts. |
| OmniRoute preferred / OpenRouter alternate | Fixed gateway table with owner preference order; OmniRoute is probed live at 127.0.0.1:20128. |
| Streaming + cancel | SSE deltas stream into the transcript; Cancel interrupts the turn (verified by runtime check 7). |
| Receipts | Every model turn, tool call, attachment read and test run produces a receipt with gateway, model, outcome and details. |

---

## C. Auto model-selection policy (implemented and tested)

`select_model(task, catalog, health, manual_pin, include_test_gateways)` in
`gateway.rs` is pure and unit-tested (11 policy tests). Criteria, in declared
order:

1. Manual pin wins when present, valid, and on a healthy gateway.
2. Only reachable AND authenticated gateways are considered.
3. Gateway preference rank (OmniRoute first, direct OpenRouter alternate).
4. BUILD mode with tool needs requires tool-capable models.
5. Context window must fit conversation + attachments (2× headroom), with a truthful warning when it cannot.
6. Highest coding tier wins; ties break on larger context.

The rationale is human-readable, shown in the UI and stored in the receipt.
No silent cheap-model downgrade exists anywhere in the policy.

---

## D. Real connectivity verification (truthful states)

Verified live during this increment:

- **OmniRoute** (preferred): installed (`omniroute@3.8.53`), startable, and
  reachable at `http://127.0.0.1:20128/v1`. Its `/v1/models` catalog requires a
  gateway API key (HTTP 401), and its stored upstream OpenRouter credential
  **cannot be decrypted** because the owner's `STORAGE_ENCRYPTION_KEY` is
  empty/missing on this machine (the `.env` at
  `C:\Users\tonig\AppData\Roaming\npm\node_modules\omniroute\.env` has
  `STORAGE_ENCRYPTION_KEY=` with no value, and `~\.omniroute\.env` does not
  exist). Inference attempts return a truthful upstream 401
  (`provider_not_configured` / `No cookie auth credentials found`). The app
  reports this state honestly and excludes the gateway from auto-selection
  until it is healthy.
- **OpenRouter** (alternate): `api.openrouter.ai` is reachable. `/v1/models` is
  PUBLIC and cannot validate a key, so key verification uses the authenticated
  `/v1/credits` endpoint. No owner key is currently connected; a bogus key is
  rejected and NOT stored (runtime checks 8a/8b). The owner can connect their
  real key in Models & Connections; it is stored DPAPI-encrypted in
  `%APPDATA%\paige-desktop\gateways.json`, outside the repository.
- **No credential harvesting**: no token scraping from other apps, no
  borrowing of SOLO/Operator secrets. The single credential blocker above is
  reported to the owner with exact restore steps.

**To activate live paid inference, the owner can either:**
1. Restore OmniRoute: set `STORAGE_ENCRYPTION_KEY` in the OmniRoute `.env` (the
   value from the installation that encrypted `~\.omniroute\storage.sqlite`),
   then restart the server; or
2. Connect the owner's OpenRouter key in Models & Connections.

---

## E. The complete workflow demonstration

Because both production gateways currently lack usable upstream credentials,
the full instruction → model → tools → test → verified-result workflow is
demonstrated end-to-end against the **explicitly-labeled TEST-ONLY harness
gateway** (`PAIGE_TEST_GATEWAY=1`, compiled only into the target-test build).
This exercises every real subsystem — real HTTP/SSE streaming, the real
selection policy, the real broker, the real terminal engine, real node test
execution, real receipts — with a scripted model, and is never presented as a
paid provider. The runtime suite (23 checks) proves:

- Auto-selection chooses `harness/alpha-coder` for a BUILD task with a
  recorded rationale.
- The model lists the authorized files and runs `node --test` through the 1C
  terminal engine inside the lab root: exit code 0, `pass 1`, captured in a
  tool receipt.
- The final answer reports the exact verified result; a `final_answer`
  receipt closes the turn.
- Manual pin (`harness/beta-analyst`) is honored for a PLAN question.
- A PLAN-mode write attempt is refused by the policy engine; the file is
  verified unmodified.
- An unknown pin falls back to Auto with a truthful reason.
- A slow stream is cancelled; no final answer is produced afterwards.
- A bogus OpenRouter key is rejected and not stored.
- The OmniRoute gateway state is truthful (never falsely "ready").

---

## F. Regression totals — 152/152

| Suite | Result |
|---|---|
| Rust unit tests (`cargo test --lib`) | **36/36 pass** (14 new: 11 policy + 3 secrets/DPAPI) |
| Static security (`npm run test:security`) | **41/41 pass** |
| CSP runtime (`npm run test:security:runtime`) | **10/10 pass** |
| Broker runtime (`npm run test:broker:runtime`) | **26/26 pass** |
| Terminal runtime (`npm run test:terminal:runtime`) | **16/16 pass** |
| Ask runtime (`npm run test:ask:runtime`) | **23/23 pass** |
| **Total** | **152/152** |

---

## G. Production binary

| Property | Value |
|---|---|
| Path | `src-tauri/target/release/paige-desktop.exe` |
| Build | `npm run tauri build` (full bundle: MSI + NSIS) |
| Size | **5,939,200 bytes** |
| SHA-256 | `BF12F21CBA5EA3C6A4F1AB38456DF0B71B4293604F3436A38DD98B05CBD9B5A5` |
| `PAIGE_BROKER_TEST_MODE` string | Absent |
| Test-harness gateway probe | Compiled out of release builds (cfg-gated to `insecure-test-bypass`) |

---

## H. Screenshots (running application)

- `ask-composer-empty.png` / `ask-composer-release.png` — composer with project chip, PLAN/BUILD, Attach, model picker, gateway status dots.
- `ask-slash-palette.png` — slash command menu.
- `ask-at-menu.png` — @ context reference menu (real authorized files).
- `ask-attach-menu.png` — attach picker.
- `ask-chat-build-complete.png` — a full BUILD turn: auto-selected model, tool events (list_files, node --test), verified "tests passed" answer.
- `ask-chat-plan-refusal.png` — PLAN mode refusing a write with an explanation.
- `models-connections-test.png` — Models & Connections with the harness gateway plus truthful production gateway states.
- `models-connections-release.png` — release binary: OmniRoute reachable/auth-needed, OpenRouter not connected, qualified pools, receipts.

---

## I. Security and architecture boundaries

- All inference HTTP happens in the Rust backend; the WebView CSP still
  forbids external network egress (`41/41` static + `10/10` runtime).
- The owner's OpenRouter key is DPAPI-encrypted at rest outside the repo and
  never enters the WebView.
- No new Tauri plugin permissions; the new `ask_*` commands are the only
  capability additions and are covered by the static allowlist test.
- PLAN/BUILD enforcement, broker approvals, terminal allowlist and fail-closed
  roots are all enforced by the existing 1B/1C engines — the Ask layer only
  dispatches.
- The gateway table is a fixed compile-time table inside the Ask session
  layer — not a general provider registry or a second orchestration engine.
- No MCP (1D), Git (1F), documents (1G) or account/device work (1H) was started.

---

## J. Genuine limitations

1. **No live paid model call was demonstrated** because both production
   gateways currently lack usable upstream credentials (see §D). The owner can
   activate OmniRoute (restore STORAGE_ENCRYPTION_KEY) or connect an OpenRouter
   key; no code change is required.
2. The qualified model pools for the production gateways are small curated
   lists (proven models + tier data); a live catalog replaces/augments them
   once a gateway authenticates.
3. The OmniRoute catalog endpoint requires a gateway API key the owner has
   not minted; OmniRoute therefore shows "auth needed" even if its upstream
   credential were restored, until auto-selection verifies via a live call.
4. Conversation history is currently per-composer (frontend state); durable
   cross-session conversation continuity is future work.
5. Attachments are read as text with a 24KB in-conversation cap; images/PDFs
   are not yet supported (truthfully absent from the UI).

---

## K. Independent review

Exact-head independent review performed against `8ba9a60` by a non-author
reviewer. Full report: `docs/REVIEW-INCREMENT-1E.md`.

---

## L. Status

Increment 1E is implemented, verified and reviewed. Awaiting owner acceptance
before beginning Increment 1D (Native MCP Client) or any other outcome.
