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
| Workspace gateway connection | When no production gateway is authenticated, Ask PAIGE displays an inline OpenRouter key banner at the top of the workspace (password field + Connect button) with the same DPAPI/credits-endpoint security as Models & Connections. |
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
  reachable at `http://127.0.0.1:20128/v1` when running. Its stored upstream
  OpenRouter credential **cannot be decrypted** because the owner's
  `STORAGE_ENCRYPTION_KEY` is empty/missing on this machine (the `.env` at
  `C:\Users\tonig\AppData\Roaming\npm\node_modules\omniroute\.env` has
  `STORAGE_ENCRYPTION_KEY=` with no value, and `~\.omniroute\.env` does not
  exist). When the server is not running, the app reports it truthfully as
  Offline; when running, it reports "auth needed". OmniRoute remains the
  architecturally preferred gateway (rank 0).
- **OpenRouter** (alternate): `api.openrouter.ai` is reachable. The owner
  connected a real OpenRouter API key through the workspace banner; the key was
  verified live against the authenticated `/v1/credits` endpoint, stored
  DPAPI-encrypted in `%APPDATA%\paige-desktop\gateways.json` outside the
  repository, and is never exposed to the WebView. A bogus key is rejected and
  NOT stored.
- **No credential harvesting**: no token scraping from other apps, no borrowing
  of SOLO/Operator secrets.

**To activate OmniRoute in addition to OpenRouter, the owner can restore
`STORAGE_ENCRYPTION_KEY` in the OmniRoute `.env` (the value from the installation
that encrypted `~\.omniroute\storage.sqlite`) and restart the server. No code
change is required.**

---

## E. The complete workflow demonstration

Because both production gateways initially lacked usable upstream credentials,
the full instruction → model → tools → test → verified-result workflow was first
proven end-to-end against the **explicitly-labeled TEST-ONLY harness gateway**
(`PAIGE_TEST_GATEWAY=1`, compiled only into the target-test build). That
exercised every real subsystem — real HTTP/SSE streaming, the real selection
policy, the real broker, the real terminal engine, real node test execution,
real receipts — with a scripted model, and was never presented as a paid
provider. The runtime suite (23 checks) proved the machinery.

### E.2 Live-model acceptance (owner-connected OpenRouter key)

After the owner connected their OpenRouter API key through the workspace banner,
a second 15-check acceptance run used **direct OpenRouter with a real hosted
model** (`openai/gpt-5.4`):

- ✅ OpenRouter gateway authenticated: **owner key verified** against the
  authenticated credits endpoint; 444 live models in catalog; 445 qualified
  candidates.
- ✅ OmniRoute remains the **architecturally preferred** gateway (rank 0) even
  while direct OpenRouter is the currently active alternate.
- ✅ Auto selection chose **`openai/gpt-5.4`** on `openrouter-direct` with a
  recorded rationale.
- ✅ The model initiated `read_file` on `math.js` inside the authorized lab root.
- ✅ The model initiated `list_files`.
- ✅ The model initiated `run_command`: `node math.test.js` → **exit code 0**,
  output included `✔ add works` and pass count.
- ✅ Final answer reported the verified result.
- ✅ Every receipt identifies the model (`openai/gpt-5.4`) and gateway
  (`openrouter-direct`).
- ✅ Only the authorized file (`math.js`) was read; no unauthorized file access.

**Live-model acceptance result: 15/15 checks passed.**

Provider usage: one OpenRouter chat-completions stream to `openai/gpt-5.4`
(usage receipt includes completion_tokens and cost).

---

## F. Regression totals — 156/156

| Suite | Result |
|---|---|
| Rust unit tests (`cargo test --lib`) | **39/39 pass** (17 new: 11 policy + 3 DPAPI/secrets + 3 ask correction tests) |
| Static security (`npm run test:security`) | **41/41 pass** |
| CSP runtime (`npm run test:security:runtime`) | **10/10 pass** |
| Broker runtime (`npm run test:broker:runtime`) | **26/26 pass** |
| Terminal runtime (`npm run test:terminal:runtime`) | **16/16 pass** |
| Ask runtime (`npm run test:ask:runtime`) | **24/24 pass** |
| **Total** | **156/156** |

> **Post-review corrections (2026-11-19):** the independent review's material
> finding #1 was fixed — `truncate()` is now char-boundary-safe; the
> omitted-`mode` IPC default was changed to the safer PLAN policy; the
> ModelsConnections wording now names the credits endpoint. Three new unit
> tests cover the fixes. The workspace connect banner was added (`faf1aa6`),
> and the ask runtime suite was made environment-aware so it passes whether
> or not an owner OpenRouter key is connected. The broker runtime suite
> contains **26 executed checks** (23 literal call sites, one looping over 5
> unauthorized paths).

---

## G. Production binary

| Property | Value |
|---|---|
| Path | `src-tauri/target/release/paige-desktop.exe` |
| Build | `npm run tauri build` (full bundle: MSI + NSIS) |
| Size | **5,945,344 bytes** |
| SHA-256 | `F41994A5A2CD3EABF2DE7C33AB852E6EDFF04C199B39EF9C8EE087F240CF4952` |
| `PAIGE_BROKER_TEST_MODE` string | Absent |
| Test-harness gateway probe | Compiled out of release builds (cfg-gated to `insecure-test-bypass`) |

---

## H. Screenshots (running application)

- `ask-composer-empty.png` / `ask-composer-release.png` — composer with project chip, PLAN/BUILD, Attach, model picker, gateway status dots.
- `ask-slash-palette.png` — slash command menu.
- `ask-at-menu.png` — @ context reference menu (real authorized files).
- `ask-attach-menu.png` — attach picker.
- `ask-chat-build-complete.png` — a full BUILD turn against the test harness: auto-selected model, tool events (list_files, node --test), verified "tests passed" answer.
- `ask-chat-plan-refusal.png` — PLAN mode refusing a write with an explanation.
- `ask-workspace-connect-banner.png` — inline OpenRouter key connect banner at the top of the Ask PAIGE workspace.
- `ask-live-after-turn.png` — Ask PAIGE after the live OpenRouter turn: route line shows `openai/gpt-5.4 · openrouter-direct`, OpenRouter status dot green.
- `models-connections-test.png` — Models & Connections with the harness gateway plus truthful production gateway states.
- `models-connections-release.png` — release binary before key connect: OmniRoute truthful, OpenRouter not connected.
- `models-connections-openrouter-connected.png` — release binary after owner connected OpenRouter: green "Connected", 444 live models, qualified catalog populated, OmniRoute still preferred but Offline.

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

Exact-head independent review performed against the final commit
`faf1aa655fac65c24d60fc99b360b1db91a74cc0` by a non-author reviewer —
verdict **ACCEPT WITH FINDINGS**. Full report: `docs/REVIEW-INCREMENT-1E.md`.

The reviewer verified the post-correction code (`b5c6f44`) and the workspace
connect banner addition (`faf1aa6`), confirmed the diff scope was limited to
frontend UI/UX (no Rust backend changes), verified the banner's security
(password input, backend-only storage, key cleared from frontend state), and
confirmed the production binary properties.

Material findings from the earlier `8ba9a60` review were corrected in
`b5c6f44`:
1. `truncate()` char-boundary safety (fixed + 3 new tests).
2. Omitted-`mode` IPC default changed to PLAN.
3. ModelsConnections wording names the credits endpoint.

The `faf1aa6` addition adds an inline workspace OpenRouter connect banner with
the same backend security (password input, DPAPI storage, credits-endpoint
verification) and no new backend code. The final corrected regression total is
**156/156**.

Remaining non-blocking findings are documented in the review and the evidence
(context-fit warning vs failover semantics; `needs_tools` always true from the
app path; cross-turn conversation history future work; weak runtime check 7b;
`set_var` test isolation fragility). The final corrected regression total is
**152/152**.

---

## L. Status

Increment 1E is implemented, verified and reviewed. Awaiting owner acceptance
before beginning Increment 1D (Native MCP Client) or any other outcome.
