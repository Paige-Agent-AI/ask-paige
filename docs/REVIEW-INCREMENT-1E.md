# Independent Review — Increment 1E (exact-head)

**Reviewer:** independent non-author subagent (fresh session)
**Repo:** `C:\Users\tonig\paige-desktop`
**Branch:** `feat/paige-desktop-prototype`
**Reviewed code commit (exact):** `8ba9a609375abb797e38145eb0f832879ac398a8`
**HEAD at review:** `2e38e536da3573219c91422dccedf1620dca3aef` — docs-only on top (evidence + roadmap); working tree clean.
**Reviewer authored any of this work:** No — read-only session.

---

## Verdict: ACCEPT WITH FINDINGS

All substantive claims verified by inspection and re-run.

---

## Verified claims

- **`gateway.rs`** — fixed compile-time table (exactly 2 production gateways + TEST-only harness), pure `select_model` with manual-pin precedence, reachable+authenticated gating, rank ordering, BUILD/tools requirement, 2× context headroom, tier tie-break. 11 unit tests confirmed.
- **`ask.rs`** — SSE streaming with delta events; tool loop dispatching to the **existing** broker and terminal engines; receipts for selection, every tool call, attachment reads, final answer; cancellation via AtomicBool. PLAN-mode write refusal and read-only command enforcement are real. OpenRouter key verified against the **authenticated `/api/v1/credits` endpoint**, not public `/v1/models`, and verified *before* storage. OmniRoute 401s classified reachable-but-auth-needed with a truthful `STORAGE_ENCRYPTION_KEY` detail. Broker write approval is the genuine native dialog (auto-bypass cfg-gated to test builds).
- **`secrets.rs`** — DPAPI at rest, config outside the repo, `PAIGE_TEST_CONFIG_DIR` test isolation, plaintext never in the stored field. 3 tests.
- **`terminal.rs`** — `run_to_completion` reuses the same allowlist/classification/cwd/fail-closed machinery; no duplicate engine.
- **Capabilities** — only the 8 `ask_*` commands added; no plugin permissions; zero `tauri-plugin-*` crates (only `ureq`).
- **Frontend** — composer controls functional (project chip performs a real grant; PLAN/BUILD feeds the policy; attach/@ pull real broker listings; model dropdown wires selection; status dots truthful).
- **Harness** — clearly TEST-ONLY, plain Node server in `scripts/`, never bundled; probe requires both `insecure-test-bypass` and `PAIGE_TEST_GATEWAY=1`, both absent from the release binary.
- **Runtime suite** — 23 real end-to-end checks: real CDP-driven `invoke()` calls, a real `node --test` subprocess through the real 1C engine, on-disk file verification after the PLAN refusal, a real cancelled slow stream, and a genuine live bogus-key rejection via the real credits endpoint.
- **Credential harvesting** — none; the only credential touched is the owner-supplied OpenRouter key, DPAPI-encrypted.
- **Truthfulness** — the "no live paid model call" limitation is honestly reported with exact credential blockers and restore steps; the app's own states are truthful (OmniRoute never falsely "ready").

## Re-verified regression totals — 152/152 at review time

36 Rust + 41 static + 10 CSP + 26 broker + 16 terminal + 23 ask = 152. (Post-review correction pass adds 3 tests → 155/155.)

## Production binary at review

5,939,200 bytes, SHA-256 `BF12F21CBA5EA3C6A4F1AB38456DF0B71B4293604F3436A38DD98B05CBD9B5A5`. `PAIGE_BROKER_TEST_MODE` and `PAIGE_TEST_GATEWAY` absent.

## Findings

**Material (fixed in the post-review correction pass):**

1. `truncate()` (ask.rs) byte-sliced `&s[..n]` — a multibyte char at a cut point in model output would panic, and with the release profile's `panic="abort"` this aborts the whole app. **Fixed:** char-boundary-safe truncation + 3 unit tests (ASCII, emoji, CJK, non-boundary walk-back).
4. Omitted-`mode` IPC default was `build` — the more privileged mode as fallback for hand-crafted requests. **Fixed:** default is now `plan` (safer); covered by a deserialization test.
5. ModelsConnections wording said the key is "verified live against the OpenRouter catalog"; the code correctly uses the credits endpoint. **Fixed:** wording names the authenticated credits endpoint.

**Non-blocking (documented, accepted for 1E):**

2. Context-fit never causes gateway failover — rank dominates; a too-small-context model on the preferred gateway is selected with a warning rather than falling through to the alternate. Documented in-code.
3. `ask_send` always sets `needs_tools: true`, so the PLAN/non-tool-model branch is unreachable from the app path.
6. Cross-turn conversation `history` is threaded through the request type but the UI passes `[]` — no cross-turn memory (documented limitation).
7. Runtime check 7b asserts non-throw only (weak); one dead conditional in AskPaige.tsx (`cmd.prompt` both branches).
8. `secrets.rs` test mutates process-global `PAIGE_TEST_CONFIG_DIR` via `set_var` — benign today, fragile under parallel test growth.
9. Screenshot review was metadata-only (reviewer model cannot render images); the same flows are functionally proven by the 23-check runtime suite.

## Post-review correction identity

- Correction commit: `fix(1E)` on top of `8ba9a60` (see git log).
- Corrected binary: 5,941,248 bytes, SHA-256 `0D2CC999D32103113B187E66A533597256160C4A6B0E8F126ADF442EC6442A3C`.
- Corrected totals: **155/155** (39 + 41 + 10 + 26 + 16 + 23).