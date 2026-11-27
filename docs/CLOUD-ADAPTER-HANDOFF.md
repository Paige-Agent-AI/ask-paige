# Ask PAIGE → PAIGE Cloud Adapter Handoff

**Status**: Prepared for coordination via ANT-169/176  
**Canonical Ask PAIGE repo**: `https://github.com/Paige-Agent-AI/ask-paige`  
**Canonical PAIGE Cloud repo**: `https://github.com/Paige-Agent-AI/paige-cloud`  
**Model Fabric source**: `https://github.com/Paige-Agent-AI/Paige-Agent-AI` (supabase/functions/_shared/)

---

## ANT-166 — Model Fabric (PO25) Handoff

### Ask PAIGE Consumption Requirements (Stage 2B+)

| Requirement | Current Implementation | Cloud Contract Needed |
|-------------|----------------------|----------------------|
| **Free-first routing** | `openrouter/free` enforced client-side in `src-tauri/src/gateway.rs:42` | Server-side allowlist & validation |
| **Governed fallback architecture** | OmniRoute desktop-only, OpenRouter primary, Anthropic fallback | Configurable paid fallback with spending authority gate |
| **Pre-transmission financial enforcement** | Client-side budget gate in `fabric-check.mjs` | Server-side budget gate on EVERY candidate |
| **Product-consumption interface** | `fabricCompletion` / `fabricChatStream` via HTTP to Supabase | Versioned API contract with receipts |

### Missing Interfaces (Smallest First)
1. **Free-model allowlist endpoint** — `GET /model-fabric/free-models` → returns `["openrouter/free", ...]`
2. **Budget gate response schema** — `{ allowed: boolean, hitRecorded: boolean, quotaExhausted: boolean, retryAfter?: number }`
3. **Receipt header format** — `X-PAIGE-Model-Receipt` with model, class, tokens, cost, timestamp
4. **Quota exhaustion signaling** — 429 + `Retry-After` + `X-PAIGE-Quota-Exhausted: true`

### Implementation Notes
- OpenRouter is **PRIMARY** for hosted SaaS (MF-OWNER-2026-11-27-B)
- GPT models **excluded** from use AND consideration (all paths)
- OmniRoute remains **desktop-local** pending hosted expansion qualification
- `openrouter/free` is the **default free route**; no paid fallback without explicit spending authority

---

## ANT-169/176 — PAIGE Cloud Infrastructure Handoff

### Ask PAIGE Cloud Adapter Consumption Requirements

| # | Requirement | Current Desktop Implementation | Cloud Interface Needed |
|---|-------------|-------------------------------|------------------------|
| 1 | **Application registration** | N/A (desktop auto-registers) | `POST /apps` + webhook for config |
| 2 | **Unified account auth** | Local DPAPI secrets (`src-tauri/src/secrets.rs`) | Cognito/OIDC token exchange + refresh |
| 3 | **Org/project authorization** | Local scope registry (`src-tauri/src/scope.rs`) | Project-scoped JWT claims + RBAC |
| 4 | **Conversation persistence** | Local SQLite (`src-tauri/src/chats.rs`) | Aurora-backed `/conversations` CRUD |
| 5 | **Artifact storage** | Local filesystem (`chat_images.rs`) | S3-compatible signed uploads + CDN |
| 6 | **Device registration** | N/A | `POST /devices` with attestation |
| 7 | **Selective sync** | N/A | WebSocket delta sync protocol |
| 8 | **Remote build/execution** | Local `tauri build` | CodeBuild/GitHub Actions trigger API |
| 9 | **Deployment manifests** | NSIS installer artifact | Release manifest ingestion + rollout |
| 10 | **Model Fabric consumption** | Direct Supabase calls | Authenticated fabric proxy endpoint |
| 11 | **Usage/cost attribution** | Local receipts (`route_evidence.rs`) | Cost ledger webhook + per-org quotas |
| 12 | **Monitoring/recovery** | Local logs | CloudWatch/CloudTrail integration |

### Existing Cloud Capabilities (Per ANT-169/176)
- Cognito identity + recovery
- Aurora persistent account/org records  
- Org/project authorization boundaries
- App/workload registry
- Private Mission Control
- Scoped workload config
- AWS-native deploy + recovery
- SES transactional delivery + receipts
- Protected public marketing hosting

### Missing Interfaces (Smallest First)
1. **App registration endpoint** — `POST /api/v1/apps` → returns `app_id`, `app_secret`
2. **Auth token exchange** — `POST /auth/token` (client_credentials → JWT with org/project claims)
3. **Conversation API** — `GET/POST /api/v1/conversations` with project scoping
4. **Artifact signed upload** — `POST /api/v1/artifacts/upload-url` → presigned S3 URL
5. **Device registration** — `POST /api/v1/devices` with platform attestation
6. **Sync delta endpoint** — `GET /api/v1/sync?since=<cursor>` → NDJSON deltas

---

## Coordination Mechanism
- **Primary**: Linear issues ANT-166, ANT-169, ANT-176
- **Secondary**: Direct API calls once endpoints exist
- **No programmatic Linear/MCP access** from this session — manual owner comment required

---

## Next Actions (Owner-Directed)
1. Owner posts Model Fabric free-only contract on ANT-166
2. Cloud lead (ANT-169/176) acknowledges consumption requirements
3. Ask PAIGE implements adapters against versioned contracts
4. Remote build validation with actual cloud endpoints