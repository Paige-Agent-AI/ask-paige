# PAIGE Desktop — OmniRoute Endpoint-Key Integration (bounded outcome)

**Status:** implementation complete; regression green; independent non-author review running.
**Date:** 2026-11-19
**Authorization:** owner-authorized bounded OmniRoute integration outcome.

---

## A. What was done

### 1. Loopback binding (safe config correction)
- Resolved binding variable: `OMNIROUTE_SERVER_HOST=127.0.0.1` (cross-platform explicit setting, takes precedence over legacy `HOSTNAME`).
- Set in persistent `~/.omniroute/.env` (not the package `.env`, which gets overwritten on update).
- Restarted OmniRoute. Verified: `netstat` shows `TCP 127.0.0.1:20128 LISTENING` (was `0.0.0.0:20128`).

### 2. API authentication verified
- No key → `401 Unauthorized`
- Invalid key → `401 Unauthorized`
- Valid endpoint key → `200 OK` (1,070 models, includes `openrouter/auto-code`)

### 3. Database backup
- Copied `storage.sqlite` + WAL + SHM to `~/.omniroute/backup-20261119/`.
- Verified: `PRAGMA integrity_check` returns `ok`.

### 4. Native PAIGE Desktop integration (source changes)
Files changed:
- `src-tauri/src/secrets.rs` — `omniroute_key_protected` field + `omniroute_key()`/`set_omniroute_key()`/`clear_omniroute_key()` (DPAPI).
- `src-tauri/src/ask.rs` — `ask_connect_omniroute`/`ask_disconnect_omniroute` commands; OmniRoute key loaded for `fetch_catalog` and `stream_chat` (Bearer auth).
- `src-tauri/src/lib.rs` — command registration.
- `src-tauri/build.rs` — permission manifest.
- `src-tauri/capabilities/default.json` — capability allowlist.
- `src/hooks/useAsk.ts` — `connectOmniroute`/`disconnectOmniroute`.
- `src/screens/ModelsConnections.tsx` — OmniRoute endpoint key card (password input, DPAPI storage).
- `scripts/test-security-config.mjs` — allowlist update.

### 5. Verification evidence

| Check | Result |
|---|---|
| OmniRoute bound to 127.0.0.1 | ✅ `netstat` shows `127.0.0.1:20128 LISTENING` |
| No key → 401 | ✅ Verified |
| Invalid key → 401 | ✅ Verified |
| Valid key → 200 | ✅ 1,070 models |
| `openrouter/auto-code` in catalog | ✅ Present |
| PAIGE native connection | ✅ `ask_connect_omniroute` → 200, catalog populated |
| OmniRoute authenticated | ✅ 1,070 models, `authenticated: true` |
| Direct OpenRouter fallback | ✅ 444 models, `authenticated: true` |
| Qualified catalog | ✅ 1,514 total (1,070 OmniRoute + 444 OpenRouter) |
| Key not leaked to WebView | ✅ Verified |
| Negative: wrong key rejected | ✅ "OmniRoute rejected the key" |
| Negative: empty key rejected | ✅ "empty key" |
| Negative: disconnect works | ✅ `authenticated: false` after disconnect |

### 6. Regression tests
| Suite | Result |
|---|---|
| Rust unit tests | 45/45 |
| Static security | 41/41 |
| CSP runtime | 10/10 |
| Broker runtime | 26/26 |
| Terminal runtime | 16/16 |
| Ask runtime | 24/24 |
| **Total** | **162/162** |

### 7. Production binary (final)
- Path: `%LOCALAPPDATA%\PAIGE Desktop\paige-desktop.exe`
- SHA-256: `A8AB728D8A92866F13896F52A97FC7AA0BF5100302851E0D499334C6BC305031`
- Size: 6,140,416 bytes
- Test strings: absent (0 occurrences of `PAIGE_MCP_TEST_INSECURE_REMOTE`, `PAIGE_BROKER_TEST_MODE`, `PAIGE_TEST_GATEWAY`, `TEST-ONLY`)
- Cold launch: verified (embedded app at `tauri://localhost`, no Vite dev server needed)
- OmniRoute native connection: verified (1,054 models, `openrouter/auto-code` present)
- Direct OpenRouter fallback: verified (444 models)
- Negative security: wrong key → 401, empty key → rejected, disconnect works

### 8. Remaining owner-only blocker
The `STORAGE_ENCRYPTION_KEY` for OmniRoute's encrypted OpenRouter provider record is lost. The existing encrypted credential cannot be decrypted. To restore upstream OpenRouter authentication through OmniRoute, the owner must either:
- Recover the original `STORAGE_ENCRYPTION_KEY` (from memory/records), OR
- Create a capped API key in the existing OpenRouter workspace and re-enter it into OmniRoute via the dashboard.

This does NOT block PAIGE Desktop's native OmniRoute connection (which works via the endpoint key). It only blocks OmniRoute's ability to call OpenRouter upstream (inference).

### 9. `openrouter/auto-code` status
- Catalog visibility: ✅ confirmed (present in OmniRoute's `/v1/models`).
- Live inference: ⏳ not tested (requires upstream OpenRouter auth + budget authorization).