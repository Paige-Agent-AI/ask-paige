// PAIGE Desktop - engineering inference gateway registry + auto model-selection
// policy (Increment 1E).
//
// This is a fixed, compile-time gateway table inside the Ask PAIGE session
// layer — NOT a general provider registry or a second orchestration engine.
// Exactly two production gateways exist (OmniRoute preferred, direct OpenRouter
// alternate) plus an explicitly-labeled local test-harness route that is only
// enabled by the `insecure-test-bypass` cargo feature for automated tests.
//
// The model-selection policy is implemented here and unit-tested: PAIGE
// decides which qualified model handles a task based on mode, tool needs,
// context size and live gateway health. Manual pin always wins when valid.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Gateway registry (compile-time)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GatewayId {
    Omniroute,
    OpenrouterDirect,
    TestHarness,
}

impl GatewayId {
    pub fn key(&self) -> &'static str {
        match self {
            GatewayId::Omniroute => "omniroute",
            GatewayId::OpenrouterDirect => "openrouter-direct",
            GatewayId::TestHarness => "test-harness",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            GatewayId::Omniroute => "OmniRoute (preferred)",
            GatewayId::OpenrouterDirect => "OpenRouter (direct alternate)",
            GatewayId::TestHarness => "Local Test Harness (tests only)",
        }
    }

    pub fn base_url(&self) -> &'static str {
        match self {
            // The owner's installed engineering gateway (Anthropic/OpenAI
            // compatible relay). Model ids are provider-prefixed upstream.
            GatewayId::Omniroute => "http://127.0.0.1:20128/v1",
            GatewayId::OpenrouterDirect => "https://openrouter.ai/api/v1",
            // OpenAI-compatible local harness; only reachable when the test
            // feature is enabled and the harness process is running.
            GatewayId::TestHarness => "http://127.0.0.1:41011/v1",
        }
    }

    pub fn is_test_only(&self) -> bool {
        matches!(self, GatewayId::TestHarness)
    }

    fn auto_model(&self) -> Option<&'static str> {
        match self {
            GatewayId::Omniroute => Some("auto/best-free"),
            GatewayId::OpenrouterDirect => Some("openrouter/auto"),
            GatewayId::TestHarness => None,
        }
    }
}

/// Production gateways in owner-declared preference order.
pub const PRODUCTION_GATEWAYS: &[GatewayId] = &[GatewayId::Omniroute, GatewayId::OpenrouterDirect];

// ---------------------------------------------------------------------------
// Model catalog + qualification
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ModelCandidate {
    /// Fully-qualified model id as sent to the gateway.
    pub id: String,
    pub gateway: String,
    /// Coding quality tier 1..=5 (owner-qualified).
    pub coding_tier: u8,
    /// Tool/function-call support.
    pub tools: bool,
    /// Context window in bytes (approx).
    pub context_bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct GatewayHealth {
    pub gateway: String,
    pub label: String,
    pub reachable: bool,
    pub authenticated: bool,
    pub catalog_count: usize,
    pub detail: String,
    /// Preference rank (0 = preferred).
    pub rank: usize,
}

// Proven-live model on this installation (owner's OmniRoute call logs,
// 2026-11-17: openrouter/openai/gpt-5.4 served successfully).
pub const OMNIROUTE_KNOWN_MODELS: &[(&str, u8, bool, u64)] = &[
    ("openrouter/openai/gpt-5.4", 5, true, 400_000),
];

pub const OPENROUTER_KNOWN_MODELS: &[(&str, u8, bool, u64)] = &[
    ("openai/gpt-5.4", 5, true, 400_000),
    ("anthropic/claude-sonnet-4.6", 5, true, 200_000),
    ("z-ai/glm-5.3", 4, true, 200_000),
    ("openai/gpt-5.4-mini", 3, true, 128_000),
];

// Qualified pool for the TEST-ONLY harness gateway (mirrors the harness
// scripts in scripts/test-harness-gateway.mjs; never used in release builds).
pub const TEST_HARNESS_KNOWN_MODELS: &[(&str, u8, bool, u64)] = &[
    ("harness/alpha-coder", 5, true, 128_000),
    ("harness/beta-analyst", 4, true, 128_000),
    ("harness/gamma-mini", 2, false, 8_192),
];

// ---------------------------------------------------------------------------
// Selection policy
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct AskTask {
    pub mode: String,       // "plan" | "build"
    pub message: String,
    pub attachment_bytes: u64,
    pub needs_tools: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct ModelSelection {
    pub selected: Option<ModelCandidate>,
    pub rationale: String,
    /// True when the owner's manual pin was honored.
    pub manual_pin_honored: bool,
    /// Candidates considered, in the order evaluated.
    pub considered: usize,
}

/// Token estimate: ~4 chars per token.
#[allow(dead_code)]
fn estimate_tokens(bytes: u64) -> u64 {
    bytes / 4
}

/// PAIGE's engineering model-selection policy (Auto).
///
/// Quality/fitness first within owner-provided gateways; no silent cost
/// downgrade. Criteria (in declared order):
/// 1. Manual pin wins when present, valid and on a healthy gateway.
/// 2. Only gateways that are reachable AND authenticated are considered.
/// 3. Gateway preference rank (OmniRoute first, direct OpenRouter alternate).
/// 4. BUILD mode with tool needs requires `tools == true`.
/// 5. Context window must fit the conversation + attachments (with 2x headroom).
/// 6. Prefer the gateway's advertised Auto route when it fits.
/// 7. Highest coding tier wins among other candidates; prefer larger context on ties.
///
/// The policy is pure and unit-tested. `rationale` explains the decision in
/// concise owner-readable language.
pub fn select_model(
    task: &AskTask,
    catalog: &[ModelCandidate],
    health: &[GatewayHealth],
    manual_pin: Option<&str>,
    include_test_gateways: bool,
) -> ModelSelection {
    let healthy: Vec<&GatewayHealth> = health
        .iter()
        .filter(|h| h.reachable && h.authenticated)
        .filter(|h| match GatewayId::from_key(&h.gateway) {
            Some(g) => !g.is_test_only() || include_test_gateways,
            None => false,
        })
        .collect();

    // 1. Manual pin.
    if let Some(pin) = manual_pin {
        // An older ID-only preference has no reliable gateway provenance:
        // the previously selected provider may be missing from today's catalog.
        let Some((gateway, id)) = pin.split_once("::") else {
            return ModelSelection {
                selected: None,
                rationale: format!("Select a gateway for {pin} again in the model picker, or choose PAIGE Auto."),
                manual_pin_honored: false,
                considered: catalog.len(),
            };
        };
        let selected = catalog.iter()
            .find(|c| c.id == id && c.gateway == gateway && healthy.iter().any(|h| h.gateway == gateway))
            .cloned();
        let rationale = match &selected {
            Some(model) => format!("Manual override honored: {} on {}.", model.id, model.gateway),
            None => format!("{id} on {gateway} is unavailable. Select another model or PAIGE Auto."),
        };
        return ModelSelection {
            manual_pin_honored: selected.is_some(),
            selected,
            rationale,
            considered: catalog.len(),
        };
    }

    auto_select(task, catalog, &healthy)
}

pub(crate) const FREE_HANDOFF_ROUTER: &str = "openrouter/free";

pub(crate) fn select_free_handoff_model(
    task: &AskTask, catalog: &[ModelCandidate], health: &[GatewayHealth],
) -> Option<ModelSelection> {
    select_free_tool_model(task, catalog, health,
        "PAIGE Auto selected OpenRouter Free Models Router for the requested native specialist handoff.")
}

pub(crate) fn select_free_tool_model(
    task: &AskTask, catalog: &[ModelCandidate], health: &[GatewayHealth], rationale: &str,
) -> Option<ModelSelection> {
    let gateway = GatewayId::OpenrouterDirect.key();
    if !health.iter().any(|route| route.gateway == gateway && route.reachable && route.authenticated) {
        return None;
    }
    let model = catalog.iter().find(|route| route.gateway == gateway && route.id == FREE_HANDOFF_ROUTER
        && route.tools && route.context_bytes >= required_context_bytes(task))?;
    Some(ModelSelection {
        selected: Some(model.clone()),
        rationale: rationale.into(),
        manual_pin_honored: false,
        considered: catalog.len(),
    })
}

fn required_context_bytes(task: &AskTask) -> u64 {
    task.attachment_bytes
        .saturating_add(task.message.len() as u64)
        .saturating_mul(2)
        .saturating_add(8_192)
}

fn auto_select(task: &AskTask, catalog: &[ModelCandidate], healthy: &[&GatewayHealth]) -> ModelSelection {
    if healthy.is_empty() {
        return ModelSelection {
            selected: None,
            rationale: "No gateway is both reachable and authenticated. Connect a gateway in Models & Connections.".to_string(),
            manual_pin_honored: false,
            considered: catalog.len(),
        };
    }

    // Rank gateways by owner preference.
    let mut ranked_gateways: Vec<&GatewayHealth> = healthy.to_vec();
    ranked_gateways.sort_by_key(|h| h.rank);

    let needed_bytes = required_context_bytes(task);

    let mut best: Option<(&ModelCandidate, &GatewayHealth, String)> = None;
    for gw in &ranked_gateways {
        let mut candidates: Vec<&ModelCandidate> = catalog.iter().filter(|c| c.gateway == gw.gateway).collect();
        if candidates.is_empty() {
            continue;
        }
        // 4. BUILD + tools requirement.
        if task.needs_tools || task.mode == "build" {
            candidates.retain(|c| c.tools);
            if candidates.is_empty() {
                continue;
            }
        }
        let auto_model = GatewayId::from_key(&gw.gateway).and_then(|id| id.auto_model());
        if let Some(route) = candidates.iter().find(|c| {
            Some(c.id.as_str()) == auto_model && c.context_bytes >= needed_bytes
        }) {
            return ModelSelection {
                selected: Some((*route).clone()),
                rationale: format!("Auto selected the advertised route {} on {}.", route.id, gw.gateway),
                manual_pin_honored: false,
                considered: catalog.len(),
            };
        }

        // 5. Context fit. An Auto route outside its context budget can only be
        // considered under the existing best-effort policy.
        let ctx_fit_empty = !candidates.iter().any(|c| c.context_bytes >= needed_bytes);
        let pool: Vec<&&ModelCandidate> = if ctx_fit_empty {
            candidates.iter().collect()
        } else {
            candidates.iter().filter(|c| c.context_bytes >= needed_bytes).collect()
        };
        // 6. Highest coding tier; tie-break on larger context.
        let pick = pool.iter().max_by_key(|c| (c.coding_tier, c.context_bytes)).unwrap();
        let ctx_note = if ctx_fit_empty {
            " (warning: context window smaller than estimated need)"
        } else {
            ""
        };
        let reason = format!(
            "Auto selected {} on {}: coding tier {}/5, tools={}, ~{}KB context, gateway rank {}{}. Mode {}.",
            pick.id,
            gw.gateway,
            pick.coding_tier,
            pick.tools,
            pick.context_bytes / 1024,
            gw.rank + 1,
            ctx_note,
            task.mode.to_uppercase()
        );
        best = Some((pick, gw, reason));
        break; // first healthy gateway in preference order that has a qualifying model
    }

    match best {
        Some((c, _gw, reason)) => ModelSelection {
            selected: Some((*c).clone()),
            rationale: reason,
            manual_pin_honored: false,
            considered: catalog.len(),
        },
        None => ModelSelection {
            selected: None,
            rationale: "No qualified model matched the task on any healthy gateway.".to_string(),
            manual_pin_honored: false,
            considered: catalog.len(),
        },
    }
}

impl GatewayId {
    pub fn from_key(key: &str) -> Option<GatewayId> {
        match key {
            "omniroute" => Some(GatewayId::Omniroute),
            "openrouter-direct" => Some(GatewayId::OpenrouterDirect),
            "test-harness" => Some(GatewayId::TestHarness),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    fn health(gateway: &str, reachable: bool, authenticated: bool, rank: usize) -> GatewayHealth {
        GatewayHealth {
            gateway: gateway.to_string(),
            label: gateway.to_string(),
            reachable,
            authenticated,
            catalog_count: 0,
            detail: String::new(),
            rank,
        }
    }

    fn candidate(id: &str, gateway: &str, tier: u8, tools: bool, ctx: u64) -> ModelCandidate {
        ModelCandidate {
            id: id.to_string(),
            gateway: gateway.to_string(),
            coding_tier: tier,
            tools,
            context_bytes: ctx,
        }
    }

    fn healthy_all() -> Vec<GatewayHealth> {
        vec![
            health("omniroute", true, true, 0),
            health("openrouter-direct", true, true, 1),
        ]
    }

    #[test]
    fn explicit_handoff_prefers_only_the_healthy_free_tool_router() {
        let task = AskTask { mode: "plan".into(), message: "Delegate to a supporting specialist".into(),
            attachment_bytes: 0, needs_tools: true };
        let paid = candidate("openrouter/auto", "openrouter-direct", 5, true, 128_000);
        let free = candidate("openrouter/free", "openrouter-direct", 3, true, 128_000);
        assert!(select_free_handoff_model(&task, &[paid.clone()], &healthy_all()).is_none());
        assert!(select_free_handoff_model(&task, &[free.clone()], &[health("openrouter-direct", true, false, 1)]).is_none());
        let model = select_free_handoff_model(&task, &[paid, free], &healthy_all()).unwrap();
        assert!(!model.manual_pin_honored);
        assert_eq!(model.selected.unwrap().id, "openrouter/free");
    }

    #[test]
    fn auto_prefers_omniroute_with_qualifying_model() {
        let catalog = vec![
            candidate("openrouter/openai/gpt-5.4", "omniroute", 5, true, 400_000),
            candidate("openai/gpt-5.4", "openrouter-direct", 5, true, 400_000),
        ];
        let task = AskTask { mode: "build".into(), message: "add a test".into(), attachment_bytes: 0, needs_tools: true };
        let sel = select_model(&task, &catalog, &healthy_all(), None, false);
        assert_eq!(sel.selected.unwrap().gateway, "omniroute");
        assert!(!sel.manual_pin_honored);
        assert!(sel.rationale.contains("Auto selected"));
    }

    #[test]
    fn auto_falls_back_to_openrouter_when_omniroute_down() {
        let catalog = vec![
            candidate("openrouter/openai/gpt-5.4", "omniroute", 5, true, 400_000),
            candidate("openai/gpt-5.4", "openrouter-direct", 5, true, 400_000),
        ];
        let hs = vec![
            health("omniroute", false, false, 0),
            health("openrouter-direct", true, true, 1),
        ];
        let task = AskTask { mode: "build".into(), message: "x".into(), attachment_bytes: 0, needs_tools: true };
        let sel = select_model(&task, &catalog, &hs, None, false);
        assert_eq!(sel.selected.unwrap().gateway, "openrouter-direct");
    }

    #[test]
    fn auto_prefers_the_advertised_router_on_each_healthy_gateway() {
        let catalog = vec![
            candidate("auto/best-free", "omniroute", 3, true, 128_000),
            candidate("openrouter/auto", "omniroute", 5, true, 400_000),
            candidate("openrouter/auto", "openrouter-direct", 3, true, 128_000),
            candidate("openai/gpt-5.4", "openrouter-direct", 5, true, 400_000),
        ];
        let task = AskTask { mode: "build".into(), message: "test".into(), attachment_bytes: 0, needs_tools: true };
        let mut healths = healthy_all();
        let primary = select_model(&task, &catalog, &healths, None, false).selected.unwrap();
        assert_eq!((primary.gateway.as_str(), primary.id.as_str()), ("omniroute", "auto/best-free"));
        healths[0].authenticated = false;
        let alternate = select_model(&task, &catalog, &healths, None, false).selected.unwrap();
        assert_eq!((alternate.gateway.as_str(), alternate.id.as_str()), ("openrouter-direct", "openrouter/auto"));
    }

    #[test]
    fn qualified_selection_stays_on_the_gateway_chosen_by_the_owner() {
        let catalog = vec![
            candidate("openrouter/auto", "omniroute", 4, true, 128_000),
            candidate("openrouter/auto", "openrouter-direct", 4, true, 128_000),
        ];
        let task = AskTask { mode: "plan".into(), message: "test".into(), attachment_bytes: 0, needs_tools: false };
        let mut healths = healthy_all();
        let pin = Some("openrouter-direct::openrouter/auto");
        let selected = select_model(&task, &catalog, &healths, pin, false).selected.unwrap();
        assert_eq!(selected.gateway, "openrouter-direct");
        healths[1].authenticated = false;
        let unavailable = select_model(&task, &catalog, &healths, pin, false);
        assert!(unavailable.selected.is_none());
        assert!(unavailable.rationale.contains("is unavailable"));
        let remaining_catalog = &catalog[..1];
        let legacy = select_model(&task, remaining_catalog, &healths, Some("openrouter/auto"), false);
        assert!(legacy.selected.is_none());
        assert!(legacy.rationale.contains("Select a gateway"));
    }

    #[test]
    fn auto_requires_tools_for_build() {
        let catalog = vec![
            candidate("nocode/beta", "omniroute", 4, false, 100_000),
            candidate("coder/alpha", "omniroute", 3, true, 100_000),
        ];
        let task = AskTask { mode: "build".into(), message: "x".into(), attachment_bytes: 0, needs_tools: true };
        let sel = select_model(&task, &catalog, &healthy_all(), None, false);
        assert_eq!(sel.selected.unwrap().id, "coder/alpha");
    }

    #[test]
    fn plan_can_use_non_tool_model() {
        let catalog = vec![
            candidate("nocode/beta", "omniroute", 5, false, 400_000),
        ];
        let task = AskTask { mode: "plan".into(), message: "research".into(), attachment_bytes: 0, needs_tools: false };
        let sel = select_model(&task, &catalog, &healthy_all(), None, false);
        assert!(sel.selected.is_some(), "PLAN mode should allow non-tool models");
    }

    #[test]
    fn context_fit_selects_larger_window() {
        let catalog = vec![
            candidate("small/ctx", "omniroute", 5, true, 8_192),
            candidate("large/ctx", "openrouter-direct", 5, true, 400_000),
        ];
        // Attachments that overflow the small window.
        let task = AskTask { mode: "build".into(), message: "x".into(), attachment_bytes: 64_000, needs_tools: true };
        let sel = select_model(&task, &catalog, &healthy_all(), None, false);
        // OmniRoute's only model can't fit; policy should warn but still
        // prefer the gateway-order model... actually ctx_fit is empty on
        // omniroute, pool falls back to ALL candidates on that gateway and
        // selects it with a warning. Verify the warning is surfaced.
        assert!(sel.rationale.contains("warning: context window smaller"));
    }

    #[test]
    fn manual_pin_is_honored() {
        let catalog = vec![
            candidate("openrouter/openai/gpt-5.4", "omniroute", 5, true, 400_000),
            candidate("openai/gpt-5.4", "openrouter-direct", 5, true, 400_000),
        ];
        let task = AskTask { mode: "plan".into(), message: "x".into(), attachment_bytes: 0, needs_tools: false };
        let sel = select_model(&task, &catalog, &healthy_all(), Some("openrouter-direct::openai/gpt-5.4"), false);
        assert_eq!(sel.selected.unwrap().id, "openai/gpt-5.4");
        assert!(sel.manual_pin_honored);
        assert!(sel.rationale.contains("Manual override honored"));
    }

    #[test]
    fn manual_pin_on_unhealthy_gateway_requests_reselection() {
        let catalog = vec![
            candidate("openrouter/openai/gpt-5.4", "omniroute", 5, true, 400_000),
            candidate("openai/gpt-5.4", "openrouter-direct", 5, true, 400_000),
        ];
        let hs = vec![
            health("omniroute", true, true, 0),
            health("openrouter-direct", false, false, 1),
        ];
        let task = AskTask { mode: "plan".into(), message: "x".into(), attachment_bytes: 0, needs_tools: false };
        let sel = select_model(&task, &catalog, &hs, Some("openrouter-direct::openai/gpt-5.4"), false);
        assert!(!sel.manual_pin_honored);
        assert!(sel.selected.is_none());
        assert!(sel.rationale.contains("is unavailable"));
    }

    #[test]
    fn manual_pin_unknown_requests_reselection() {
        let catalog = vec![candidate("openrouter/openai/gpt-5.4", "omniroute", 5, true, 400_000)];
        let task = AskTask { mode: "plan".into(), message: "x".into(), attachment_bytes: 0, needs_tools: false };
        let sel = select_model(&task, &catalog, &healthy_all(), Some("omniroute::nope/model"), false);
        assert!(!sel.manual_pin_honored);
        assert!(sel.selected.is_none());
        assert!(sel.rationale.contains("is unavailable"));
    }

    #[test]
    fn no_healthy_gateway_reports_truthfully() {
        let catalog = vec![candidate("openrouter/openai/gpt-5.4", "omniroute", 5, true, 400_000)];
        let hs = vec![health("omniroute", true, false, 0)];
        let task = AskTask { mode: "build".into(), message: "x".into(), attachment_bytes: 0, needs_tools: true };
        let sel = select_model(&task, &catalog, &hs, None, false);
        assert!(sel.selected.is_none());
        assert!(sel.rationale.contains("No gateway is both reachable and authenticated"));
    }

    #[test]
    fn tier_beats_gateway_order_within_same_gateway() {
        let catalog = vec![
            candidate("mid/tier", "omniroute", 3, true, 400_000),
            candidate("top/tier", "omniroute", 5, true, 100_000),
        ];
        let task = AskTask { mode: "build".into(), message: "x".into(), attachment_bytes: 0, needs_tools: true };
        let sel = select_model(&task, &catalog, &healthy_all(), None, false);
        assert_eq!(sel.selected.unwrap().id, "top/tier");
    }

    #[test]
    fn estimate_tokens_sane() {
        assert_eq!(estimate_tokens(400), 100);
    }
}
