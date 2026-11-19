// PAIGE Desktop - Ask PAIGE engineering inference session layer (Increment 1E).
//
// One conversational engineering surface over the existing 1B file broker and
// 1C terminal engine. Gateway routing is a fixed table (OmniRoute preferred,
// direct OpenRouter alternate, plus an explicitly test-only local harness);
// this is NOT a new provider registry or orchestration engine.
//
// The model-selection policy (gateway.rs) decides which qualified model
// handles each task; manual pin always wins when valid. Tool calls from the
// model dispatch to the EXISTING broker (read/write with owner approval) and
// the EXISTING terminal engine (allowlisted commands, risk-based authority).
// Every step emits verifiable receipts.

use crate::broker::BrokerState;
use crate::gateway::{
    select_model, AskTask, GatewayHealth, GatewayId, ModelCandidate, ModelSelection,
    OMNIROUTE_KNOWN_MODELS, OPENROUTER_KNOWN_MODELS, PRODUCTION_GATEWAYS,
    TEST_HARNESS_KNOWN_MODELS,
};
use crate::secrets::GatewayConfig;
use crate::terminal::{self, TerminalState};
use serde::Serialize;
use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};

const MAX_TOOL_ROUNDS: usize = 8;
const MAX_MESSAGE_CHARS: usize = 24_000;

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

pub struct AskState {
    next_turn: u64,
    cancel_flags: Mutex<std::collections::HashMap<String, Arc<AtomicBool>>>,
    receipts: Mutex<Vec<AskReceipt>>,
}

impl AskState {
    pub fn new() -> Self {
        Self {
            next_turn: 1,
            cancel_flags: Mutex::new(std::collections::HashMap::new()),
            receipts: Mutex::new(Vec::new()),
        }
    }

    fn register_turn(&mut self, id: String) -> Arc<AtomicBool> {
        let flag = Arc::new(AtomicBool::new(false));
        self.cancel_flags
            .lock()
            .unwrap()
            .insert(id, flag.clone());
        flag
    }

    fn unregister_turn(&self, id: &str) {
        self.cancel_flags.lock().unwrap().remove(id);
    }
}

#[derive(Clone, Serialize)]
pub struct AskReceipt {
    pub timestamp: u64,
    pub turn_id: String,
    pub kind: String, // "model_turn" | "tool"
    pub tool: String,
    pub gateway: String,
    pub model: String,
    pub summary: String,
    pub outcome: String,
    pub details: String, // compact JSON
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Verify an OpenRouter key against the authenticated credits endpoint
/// (/v1/models is PUBLIC and cannot validate a key).
fn verify_openrouter_key(key: &str) -> Result<bool, String> {
    let url = "https://openrouter.ai/api/v1/credits";
    let resp = match ureq::get(url)
        .set("Authorization", &format!("Bearer {key}"))
        .timeout(Duration::from_secs(30))
        .call()
    {
        Ok(r) => r,
        Err(ureq::Error::Status(code, _)) => {
            if code == 401 || code == 403 {
                return Ok(false);
            }
            return Err(format!("openrouter returned HTTP {code}"));
        }
        Err(e) => return Err(format!("network: {e}")),
    };
    if resp.status() == 200 {
        Ok(true)
    } else {
        Ok(false)
    }
}

// ---------------------------------------------------------------------------
// Gateway probing + catalog
// ---------------------------------------------------------------------------

fn known_catalog(gateway: &GatewayId) -> Vec<ModelCandidate> {
    let table: &[(&str, u8, bool, u64)] = match gateway {
        GatewayId::Omniroute => OMNIROUTE_KNOWN_MODELS,
        GatewayId::OpenrouterDirect => OPENROUTER_KNOWN_MODELS,
        GatewayId::TestHarness => TEST_HARNESS_KNOWN_MODELS,
    };
    table
        .iter()
        .map(|(id, tier, tools, ctx)| ModelCandidate {
            id: id.to_string(),
            gateway: gateway.key().to_string(),
            coding_tier: *tier,
            tools: *tools,
            context_bytes: *ctx,
        })
        .collect()
}

/// Fetch the /models catalog from a gateway. Returns (ids, reachable, auth-gated).
/// Note: ureq returns non-2xx as Err(Status), so classify 401/403 here.
fn fetch_catalog(gateway: &GatewayId, api_key: Option<&str>) -> Result<(Vec<String>, bool), String> {
    let url = format!("{}/models", gateway.base_url());
    let mut req = ureq::get(&url)
        .timeout(Duration::from_secs(30));
    if let Some(k) = api_key {
        req = req.set("Authorization", &format!("Bearer {k}"));
    }
    let resp = match req.call() {
        Ok(r) => r,
        Err(ureq::Error::Status(code, _)) => {
            if code == 401 || code == 403 {
                // Gateway is REACHABLE but its model catalog requires auth.
                return Ok((Vec::new(), false));
            }
            return Err(format!("gateway returned HTTP {code}"));
        }
        Err(e) => return Err(format!("network: {e}")),
    };
    let status = resp.status();
    if !(200..300).contains(&status) {
        return Err(format!("gateway returned HTTP {status}"));
    }
    let body = resp
        .into_string()
        .map_err(|e| format!("read body: {e}"))?;
    let v: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("parse catalog: {e}"))?;
    let mut ids = Vec::new();
    if let Some(data) = v.get("data").and_then(|d| d.as_array()) {
        for item in data {
            if let Some(id) = item.get("id").and_then(|i| i.as_str()) {
                ids.push(id.to_string());
            }
        }
    }
    Ok((ids, true))
}

fn tier_from_id(id: &str) -> u8 {
    let lower = id.to_lowercase();
    // Conservative qualification tiers for coding work.
    if lower.contains("mini") || lower.contains("flash") || lower.contains("haiku") {
        3
    } else if lower.contains("nano") {
        2
    } else {
        4
    }
}

fn build_catalog(gateway: &GatewayId, catalog_ids: &[String], use_known: bool) -> Vec<ModelCandidate> {
    let mut out: Vec<ModelCandidate> = catalog_ids
        .iter()
        .map(|id| ModelCandidate {
            id: id.clone(),
            gateway: gateway.key().to_string(),
            coding_tier: tier_from_id(id),
            tools: true,
            context_bytes: 128_000,
        })
        .collect();
    if use_known {
        for c in known_catalog(gateway) {
            if !out.iter().any(|x| x.id == c.id) {
                out.push(c);
            } else {
                // Promote the known-model record (owner-qualified tier/context).
                if let Some(slot) = out.iter_mut().find(|x| x.id == c.id) {
                    slot.coding_tier = c.coding_tier;
                    slot.context_bytes = c.context_bytes;
                }
            }
        }
    }
    out
}

/// Probe the configured gateways and return live health + catalog.
pub fn probe_gateways() -> (Vec<GatewayHealth>, Vec<ModelCandidate>) {
    let cfg = GatewayConfig::load();
    let mut healths = Vec::new();
    let mut catalog = Vec::new();

    // Production gateways in owner preference order.
    for (rank, gw) in PRODUCTION_GATEWAYS.iter().enumerate() {
        let key = match gw {
            GatewayId::OpenrouterDirect => cfg.openrouter_key().ok().flatten(),
            _ => None,
        };
        // For the direct OpenRouter gateway, /v1/models is PUBLIC — a live
        // catalog proves reachability but NOT authentication. Only a key
        // verified against the authenticated credits endpoint counts as
        // "authenticated", and only then may its models be auto-selected.
        let key_verified = match (&key, gw) {
            (Some(k), GatewayId::OpenrouterDirect) => verify_openrouter_key(k).unwrap_or(false),
            _ => false,
        };
        match fetch_catalog(gw, key.as_deref()) {
            Ok((ids, catalog_auth)) => {
                let authenticated = match gw {
                    // OmniRoute inference falls through anonymously when
                    // REQUIRE_API_KEY=false; its catalog is key-gated, so a
                    // 200 catalog (or a 401 we classified) is honest:
                    // authenticated only when the catalog itself authorized.
                    GatewayId::Omniroute => catalog_auth && !ids.is_empty(),
                    GatewayId::OpenrouterDirect => key_verified,
                    _ => catalog_auth,
                };
                let detail = match gw {
                    GatewayId::Omniroute => {
                        if authenticated {
                            format!("{} models in live catalog", ids.len())
                        } else {
                            "server reachable; model catalog requires a gateway API key and upstream credentials are unavailable (restore STORAGE_ENCRYPTION_KEY)".to_string()
                        }
                    }
                    GatewayId::OpenrouterDirect => {
                        if authenticated {
                            format!("owner key verified; {} models in catalog", ids.len())
                        } else if ids.is_empty() {
                            "api reachable; connect an owner key to enable inference".to_string()
                        } else {
                            "public catalog reachable; connect an owner key to enable inference".to_string()
                        }
                    }
                    _ => format!("{} models in live catalog", ids.len()),
                };
                healths.push(GatewayHealth {
                    gateway: gw.key().to_string(),
                    label: gw.label().to_string(),
                    reachable: true,
                    authenticated,
                    catalog_count: ids.len(),
                    detail,
                    rank,
                });
                if authenticated {
                    catalog.extend(build_catalog(gw, &ids, true));
                } else {
                    // Still expose the owner-qualified known pool for manual
                    // selection, but auto will skip this gateway (unhealthy).
                    catalog.extend(known_catalog(gw));
                }
            }
            Err(e) => {
                healths.push(GatewayHealth {
                    gateway: gw.key().to_string(),
                    label: gw.label().to_string(),
                    reachable: false,
                    authenticated: false,
                    catalog_count: 0,
                    detail: format!("unreachable: {e}"),
                    rank,
                });
                catalog.extend(known_catalog(gw));
            }
        }
    }

    // Test-only harness (never in a release build without the feature).
    #[cfg(feature = "insecure-test-bypass")]
    {
        if std::env::var("PAIGE_TEST_GATEWAY").ok().as_deref() == Some("1") {
            let gw = GatewayId::TestHarness;
            match fetch_catalog(&gw, None) {
                Ok((ids, authenticated)) => {
                    healths.push(GatewayHealth {
                        gateway: gw.key().to_string(),
                        label: gw.label().to_string(),
                        reachable: true,
                        authenticated,
                        catalog_count: ids.len(),
                        detail: "TEST-ONLY local harness gateway".to_string(),
                        rank: 2,
                    });
                    if authenticated {
                        catalog.extend(build_catalog(&gw, &ids, true));
                    }
                }
                Err(e) => {
                    healths.push(GatewayHealth {
                        gateway: gw.key().to_string(),
                        label: gw.label().to_string(),
                        reachable: false,
                        authenticated: false,
                        catalog_count: 0,
                        detail: format!("test harness not running: {e}"),
                        rank: 2,
                    });
                }
            }
        }
    }

    (healths, catalog)
}

// ---------------------------------------------------------------------------
// SSE streaming chat (OpenAI chat-completions format)
// ---------------------------------------------------------------------------

struct StreamOutcome {
    text: String,
    tool_calls: Vec<ToolCall>,
    #[allow(dead_code)]
    finish_reason: String,
    usage: Option<serde_json::Value>,
}

#[derive(Clone)]
struct ToolCall {
    id: String,
    name: String,
    args_json: String,
}

#[allow(clippy::too_many_arguments)]
fn stream_chat(
    gateway: &GatewayId,
    api_key: Option<&str>,
    model: &str,
    messages: &[serde_json::Value],
    tools: &[serde_json::Value],
    cancel: &AtomicBool,
    on_delta: &mut dyn FnMut(&str),
) -> Result<StreamOutcome, String> {
    let url = format!("{}/chat/completions", gateway.base_url());
    let mut body = serde_json::json!({
        "model": model,
        "messages": messages,
        "stream": true,
    });
    if !tools.is_empty() {
        body["tools"] = serde_json::json!(tools);
    }
    let mut req = ureq::post(&url)
        .timeout(Duration::from_secs(600))
        .set("Content-Type", "application/json");
    if let Some(k) = api_key {
        req = req.set("Authorization", &format!("Bearer {k}"));
    }
    let resp = req
        .send_string(&body.to_string())
        .map_err(|e| format!("gateway request failed: {e}"))?;
    let status = resp.status();
    if !(200..300).contains(&status) {
        let err_body = resp.into_string().unwrap_or_default();
        return Err(format!("gateway returned HTTP {status}: {}", truncate(&err_body, 400)));
    }

    let reader = resp.into_reader();
    let mut buf = BufReader::new(reader);
    let mut line = String::new();
    let mut text = String::new();
    let mut tool_calls: Vec<ToolCall> = Vec::new();
    let mut finish_reason = String::new();
    let mut usage = None::<serde_json::Value>;

    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".to_string());
        }
        line.clear();
        let n = buf
            .read_line(&mut line)
            .map_err(|e| format!("stream read failed: {e}"))?;
        if n == 0 {
            break; // EOF - stream complete
        }
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with(':') {
            continue;
        }
        let Some(data) = trimmed.strip_prefix("data: ") else { continue };
        if data == "[DONE]" {
            break;
        }
        let Ok(chunk) = serde_json::from_str::<serde_json::Value>(data) else { continue };
        // Non-standard: some gateways inline usage on the last chunk.
        if let Some(u) = chunk.get("usage") {
            if u.is_object() {
                usage = Some(u.clone());
            }
        }
        let Some(choices) = chunk.get("choices").and_then(|c| c.as_array()) else { continue };
        for choice in choices {
            if let Some(fr) = choice.get("finish_reason").and_then(|f| f.as_str()) {
                finish_reason = fr.to_string();
            }
            let Some(delta) = choice.get("delta") else { continue };
            if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                if !content.is_empty() {
                    text.push_str(content);
                    on_delta(content);
                }
            }
            if let Some(tcs) = delta.get("tool_calls").and_then(|t| t.as_array()) {
                for tc in tcs {
                    let idx = tc.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                    while tool_calls.len() <= idx {
                        tool_calls.push(ToolCall {
                            id: String::new(),
                            name: String::new(),
                            args_json: String::new(),
                        });
                    }
                    if let Some(id) = tc.get("id").and_then(|i| i.as_str()) {
                        if !id.is_empty() {
                            tool_calls[idx].id = id.to_string();
                        }
                    }
                    let Some(f) = tc.get("function") else { continue };
                    if let Some(name) = f.get("name").and_then(|n| n.as_str()) {
                        if !name.is_empty() {
                            tool_calls[idx].name = name.to_string();
                        }
                    }
                    if let Some(args) = f.get("arguments").and_then(|a| a.as_str()) {
                        tool_calls[idx].args_json.push_str(args);
                    }
                }
            }
        }
    }

    tool_calls.retain(|tc| !tc.name.is_empty() || !tc.args_json.is_empty());
    Ok(StreamOutcome {
        text,
        tool_calls,
        finish_reason,
        usage,
    })
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        format!("{}…", &s[..n])
    }
}

// ---------------------------------------------------------------------------
// Tool dispatch (existing broker + existing terminal engine)
// ---------------------------------------------------------------------------

#[derive(Clone, Serialize)]
pub struct ToolResultEvent {
    pub turn_id: String,
    pub tool: String,
    pub call_id: String,
    pub ok: bool,
    pub summary: String,
    pub output: String,
}

fn dispatch_tool(
    app: &AppHandle,
    mode: &str,
    call: &ToolCall,
) -> Result<serde_json::Value, String> {
    let args: serde_json::Value =
        serde_json::from_str(&call.args_json).unwrap_or(serde_json::json!({}));

    match call.name.as_str() {
        "read_file" => {
            let path = args
                .get("path")
                .and_then(|p| p.as_str())
                .ok_or("missing path")?
                .to_string();
            let res = crate::broker::broker_read_file(app.state::<Mutex<BrokerState>>(), path)?;
            Ok(serde_json::json!({
                "path": res.path,
                "sha256": res.sha256,
                "size": res.size,
                "content": truncate(&res.content, MAX_MESSAGE_CHARS),
            }))
        }
        "list_files" => {
            let list = crate::broker::broker_list_files(app.state::<Mutex<BrokerState>>())?;
            Ok(serde_json::json!({ "files": list }))
        }
        "write_file" => {
            if mode != "build" {
                return Err("PLAN mode does not permit file writes; propose a diff instead".to_string());
            }
            let path = args
                .get("path")
                .and_then(|p| p.as_str())
                .ok_or("missing path")?
                .to_string();
            let content = args
                .get("content")
                .and_then(|c| c.as_str())
                .ok_or("missing content")?
                .to_string();
            // Broker proposal (checks current checksum of the file).
            let existing = crate::broker::broker_read_file(
                app.state::<Mutex<BrokerState>>(),
                path.clone(),
            )?;
            let prop = crate::broker::broker_propose_write(
                app.state::<Mutex<BrokerState>>(),
                path.clone(),
                content.clone(),
                existing.sha256.clone(),
            )?;
            crate::broker::broker_approve(
                app.state::<Mutex<BrokerState>>(),
                prop.proposal_id.clone(),
                prop.approval_token.clone(),
            )?;
            let receipt = crate::broker::broker_apply(
                app.state::<Mutex<BrokerState>>(),
                prop.proposal_id.clone(),
            )?;
            Ok(serde_json::json!({
                "path": receipt.path,
                "before_sha256": receipt.before_sha256,
                "after_sha256": receipt.after_sha256,
                "bytes_written": receipt.bytes_written,
                "readback_verified": receipt.readback_verified,
                "outcome": receipt.outcome,
            }))
        }
        "run_command" => {
            let command = args
                .get("command")
                .and_then(|c| c.as_str())
                .ok_or("missing command")?
                .to_string();
            let cmd_args: Vec<String> = args
                .get("args")
                .and_then(|a| a.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str())
                        .map(|s| s.to_string())
                        .collect()
                })
                .unwrap_or_default();
            let cwd = args.get("cwd").and_then(|c| c.as_str()).map(|s| s.to_string());
            // PLAN mode: read-risk commands only.
            let risk = terminal::classify_command(&command, &cmd_args);
            if mode == "plan" && risk != terminal::TerminalRisk::Read {
                return Err(format!(
                    "PLAN mode permits read-only commands; '{command}' is classified {:?}",
                    risk
                ));
            }
            let term_state = app.state::<Mutex<TerminalState>>();
            let result = terminal::run_to_completion(
                term_state.inner(),
                &command,
                &cmd_args,
                cwd.as_deref(),
                false,
            )?;
            Ok(serde_json::json!({
                "command": result.command_line,
                "cwd": result.cwd,
                "exit_code": result.exit_code,
                "stdout": truncate(&result.stdout, MAX_MESSAGE_CHARS),
                "stderr": truncate(&result.stderr, MAX_MESSAGE_CHARS),
                "timed_out": result.timed_out,
            }))
        }
        other => Err(format!("unknown tool: {other}")),
    }
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[derive(Clone, Serialize)]
pub struct AskStatus {
    pub gateways: Vec<GatewayHealth>,
    pub catalog: Vec<ModelCandidate>,
    pub selection: String,
    pub manual_pin: Option<String>,
    pub openrouter_connected: bool,
}

#[tauri::command]
pub fn ask_status() -> AskStatus {
    let cfg = GatewayConfig::load();
    let (gateways, catalog) = probe_gateways();
    // "Connected" means the stored key VERIFIED live against OpenRouter's
    // authenticated endpoint during this probe — not merely that a key exists.
    let openrouter_verified = gateways
        .iter()
        .find(|g| g.gateway == "openrouter-direct")
        .map(|g| g.authenticated)
        .unwrap_or(false);
    AskStatus {
        gateways,
        catalog,
        selection: cfg.selection.clone(),
        manual_pin: cfg.manual_pin.clone(),
        openrouter_connected: openrouter_verified,
    }
}

#[tauri::command]
pub fn ask_probe() -> AskStatus {
    ask_status()
}

#[tauri::command]
pub fn ask_connect_openrouter(key: String) -> Result<AskStatus, String> {
    let key = key.trim().to_string();
    // Verify the key against the authenticated credits endpoint BEFORE
    // storing it (the public /v1/models catalog cannot validate a key).
    let verified = verify_openrouter_key(&key)?;
    if !verified {
        return Err("OpenRouter rejected the key; nothing was stored".to_string());
    }
    let mut cfg = GatewayConfig::load();
    cfg.set_openrouter_key(&key)?;
    Ok(ask_status())
}

#[tauri::command]
pub fn ask_disconnect_openrouter() -> Result<AskStatus, String> {
    let mut cfg = GatewayConfig::load();
    cfg.clear_openrouter_key()?;
    Ok(ask_status())
}

#[tauri::command]
pub fn ask_set_model_selection(
    selection: String,
    manual_pin: Option<String>,
) -> Result<(), String> {
    if selection != "auto" && selection != "manual" {
        return Err("selection must be auto or manual".to_string());
    }
    let mut cfg = GatewayConfig::load();
    if selection == "manual" && manual_pin.is_none() {
        return Err("manual selection requires a model pin".to_string());
    }
    cfg.selection = selection;
    cfg.manual_pin = manual_pin;
    cfg.save()
}

#[derive(Clone, Serialize)]
pub struct AskTurnStarted {
    pub turn_id: String,
    pub selection: ModelSelection,
}

#[tauri::command]
pub fn ask_receipts(state: State<'_, Mutex<AskState>>) -> Vec<AskReceipt> {
    state.lock().unwrap().receipts.lock().unwrap().clone()
}

#[derive(serde::Deserialize, Clone)]
pub struct AskAttachment {
    pub path: String,
}

#[derive(serde::Deserialize, Clone)]
pub struct AskSendRequest {
    pub message: String,
    #[serde(default = "default_mode")]
    pub mode: String, // "plan" | "build"
    #[serde(default)]
    pub attachments: Vec<AskAttachment>,
    #[serde(default)]
    pub context_refs: Vec<String>,
    #[serde(default)]
    pub history: Vec<serde_json::Value>,
    #[serde(default)]
    pub model_choice: Option<String>, // None = use saved selection; Some("auto") or model id
}

fn default_mode() -> String {
    "build".to_string()
}

#[tauri::command]
pub fn ask_send(
    app: AppHandle,
    state: State<'_, Mutex<AskState>>,
    request: AskSendRequest,
) -> Result<AskTurnStarted, String> {
    let mode = match request.mode.as_str() {
        "plan" | "build" => request.mode.clone(),
        _ => return Err("mode must be plan or build".to_string()),
    };

    // Build the task + probe + select the model.
    let attachment_bytes = 0u64; // filled after reads; selection uses message size
    let task = AskTask {
        mode: mode.clone(),
        message: request.message.clone(),
        attachment_bytes,
        needs_tools: true,
    };
    let (healths, catalog) = probe_gateways();
    let cfg = GatewayConfig::load();
    let choice = request.model_choice.clone();
    let manual_pin: Option<String> = match choice {
        Some(c) if c != "auto" => Some(c),
        Some(_) => None,
        None => {
            if cfg.selection == "manual" {
                cfg.manual_pin.clone()
            } else {
                None
            }
        }
    };
    let include_test = cfg!(feature = "insecure-test-bypass")
        && std::env::var("PAIGE_TEST_GATEWAY").ok().as_deref() == Some("1");
    let selection =
        select_model(&task, &catalog, &healths, manual_pin.as_deref(), include_test);

    let turn_id = {
        let mut s = state.lock().unwrap();
        let id = format!("turn-{}", s.next_turn);
        s.next_turn += 1;
        id
    };
    let Some(model) = selection.selected.clone() else {
        let msg = format!(
            "No qualified model is currently available. {}. Connect or restore a gateway in Models & Connections.",
            selection.rationale
        );
        return Err(msg);
    };
    let cancel_flag = state.lock().unwrap().register_turn(turn_id.clone());

    let _ = app.emit("ask-model-selected", &serde_json::json!({
        "turn_id": turn_id,
        "selection": selection,
        "gateway_label": model.gateway,
    }));

    // Record the model_turn receipt.
    {
        let receipt = AskReceipt {
            timestamp: now_secs(),
            turn_id: turn_id.clone(),
            kind: "model_turn".to_string(),
            tool: "select_model".to_string(),
            gateway: model.gateway.clone(),
            model: model.id.clone(),
            summary: truncate(&selection.rationale, 300),
            outcome: "selected".to_string(),
            details: serde_json::json!({ "rationale": selection.rationale, "manual_pin_honored": selection.manual_pin_honored }).to_string(),
        };
        state.lock().unwrap().receipts.lock().unwrap().push(receipt);
    }

    // Run the conversation loop in a background thread.
    let app2 = app.clone();
    let turn_id2 = turn_id.clone();
    std::thread::spawn(move || {
        let ask_state = app2.state::<Mutex<AskState>>();
        let result = run_turn(&app2, mode, request, model, cancel_flag, turn_id2.clone());
        ask_state.lock().unwrap().unregister_turn(&turn_id2);
        match result {
            Ok(()) => {}
            Err(e) => {
                if e == "cancelled" {
                    let _ = app2.emit(
                        "ask-done",
                        &serde_json::json!({
                            "turn_id": turn_id2, "status": "cancelled", "text": "",
                        }),
                    );
                } else {
                    let _ = app2.emit(
                        "ask-error",
                        &serde_json::json!({ "turn_id": turn_id2, "message": e }),
                    );
                }
            }
        }
    });

    Ok(AskTurnStarted {
        turn_id,
        selection,
    })
}

fn tool_definitions() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "read_file",
                "description": "Read a text file inside the authorized project root. Returns content, sha256 and size.",
                "parameters": {
                    "type": "object",
                    "properties": { "path": { "type": "string", "description": "Path relative to the project root" } },
                    "required": ["path"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "list_files",
                "description": "List files in the authorized project root.",
                "parameters": { "type": "object", "properties": {} }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "write_file",
                "description": "Replace a text file's content inside the authorized project root (BUILD mode only; requires owner approval).",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "content": { "type": "string" }
                    },
                    "required": ["path", "content"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "run_command",
                "description": "Run an allowlisted development command (e.g. node, npm test, cargo build) inside the authorized project root. PLAN mode: read-only commands only.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "command": { "type": "string", "description": "Command name (no paths)" },
                        "args": { "type": "array", "items": { "type": "string" } },
                        "cwd": { "type": "string", "description": "Optional absolute working directory inside the root" }
                    },
                    "required": ["command"]
                }
            }
        }),
    ]
}

fn system_prompt(mode: &str) -> String {
    let mode_policy = if mode == "plan" {
        "PLAN MODE: You may read files and run read-only commands to inspect and reason, but you MUST NOT write files or run mutating commands. Produce analysis and proposed diffs only."
    } else {
        "BUILD MODE: You may read and write files inside the authorized project root (writes require the owner's native approval) and run allowlisted development commands, including tests and builds."
    };
    format!(
        "You are PAIGE, the engineering assistant inside PAIGE Desktop on the owner's Windows workstation.\n\
         {mode_policy}\n\
         Use the provided tools for real work: read files before editing, verify your changes by running the project's actual tests, and report exact results. Prefer minimal, targeted changes. When you finish, summarize what changed and the verified test outcome."
    )
}

fn run_turn(
    app: &AppHandle,
    mode: String,
    request: AskSendRequest,
    model: ModelCandidate,
    cancel: Arc<AtomicBool>,
    turn_id: String,
) -> Result<(), String> {
    let gateway = GatewayId::from_key(&model.gateway).ok_or("unknown gateway")?;
    let api_key = match gateway {
        GatewayId::OpenrouterDirect => GatewayConfig::load().openrouter_key().ok().flatten(),
        _ => None,
    };

    // ---- Build the message list ----
    let mut messages: Vec<serde_json::Value> = vec![
        serde_json::json!({ "role": "system", "content": system_prompt(&mode) }),
    ];

    // Attachments + context refs: read through the broker now.
    let mut context_block = String::new();
    let mut read_paths: Vec<String> = Vec::new();
    for att in &request.attachments {
        read_paths.push(att.path.clone());
    }
    for cr in &request.context_refs {
        read_paths.push(cr.clone());
    }
    let state = app.state::<Mutex<AskState>>();
    for path in read_paths {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".to_string());
        }
        match crate::broker::broker_read_file(app.state::<Mutex<BrokerState>>(), path.clone()) {
            Ok(res) => {
                context_block.push_str(&format!(
                    "\n--- FILE: {} (sha256 {}, {} bytes) ---\n{}\n--- END FILE ---\n",
                    res.path,
                    res.sha256,
                    res.size,
                    truncate(&res.content, MAX_MESSAGE_CHARS)
                ));
                let receipt = AskReceipt {
                    timestamp: now_secs(),
                    turn_id: turn_id.clone(),
                    kind: "tool".to_string(),
                    tool: "read_file".to_string(),
                    gateway: model.gateway.clone(),
                    model: model.id.clone(),
                    summary: format!("read {} ({} bytes)", res.path, res.size),
                    outcome: "ok".to_string(),
                    details: serde_json::json!({ "path": res.path, "sha256": res.sha256 }).to_string(),
                };
                state.lock().unwrap().receipts.lock().unwrap().push(receipt);
            }
            Err(e) => {
                context_block.push_str(&format!(
                    "\n--- FILE: {} could not be read: {e} ---\n",
                    path
                ));
            }
        }
    }
    if !context_block.is_empty() {
        messages.push(serde_json::json!({
            "role": "user",
            "content": format!("Project context (read from the authorized root):{}", context_block),
        }));
    }

    // History (frontend-provided prior turns).
    for h in &request.history {
        messages.push(h.clone());
    }

    // The user's instruction.
    messages.push(serde_json::json!({
        "role": "user",
        "content": format!("MODE: {}.\n{}", mode.to_uppercase(), request.message),
    }));

    let tools = tool_definitions();

    // ---- Tool loop ----
    for round in 0..MAX_TOOL_ROUNDS {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".to_string());
        }
        let app_delta = app.clone();
        let turn_delta = turn_id.clone();
        let mut on_delta = move |s: &str| {
            let _ = app_delta.emit(
                "ask-delta",
                &serde_json::json!({ "turn_id": turn_delta, "text": s }),
            );
        };
        let outcome = stream_chat(
            &gateway,
            api_key.as_deref(),
            &model.id,
            &messages,
            &tools,
            &cancel,
            &mut on_delta,
        )?;

        if outcome.tool_calls.is_empty() {
            // Final answer.
            let _ = app.emit(
                "ask-done",
                &serde_json::json!({
                    "turn_id": turn_id,
                    "status": "complete",
                    "text": outcome.text,
                    "rounds": round,
                    "usage": outcome.usage,
                }),
            );
            let receipt = AskReceipt {
                timestamp: now_secs(),
                turn_id: turn_id.clone(),
                kind: "model_turn".to_string(),
                tool: "final_answer".to_string(),
                gateway: model.gateway.clone(),
                model: model.id.clone(),
                summary: truncate(&outcome.text, 200),
                outcome: "complete".to_string(),
                details: serde_json::json!({ "rounds": round, "usage": outcome.usage }).to_string(),
            };
            state.lock().unwrap().receipts.lock().unwrap().push(receipt);
            return Ok(());
        }

        // Execute each tool call.
        let mut tool_results: Vec<serde_json::Value> = Vec::new();
        for tc in &outcome.tool_calls {
            if cancel.load(Ordering::Relaxed) {
                return Err("cancelled".to_string());
            }
            let _ = app.emit(
                "ask-tool-start",
                &serde_json::json!({ "turn_id": turn_id, "tool": tc.name, "call_id": tc.id }),
            );
            let result = dispatch_tool(app, &mode, tc);
            let (ok, output) = match &result {
                Ok(v) => (true, v.to_string()),
                Err(e) => (false, serde_json::json!({ "error": e }).to_string()),
            };
            let summary = match &result {
                Ok(_) => format!("{} ok", tc.name),
                Err(e) => truncate(e, 160),
            };
            let _ = app.emit(
                "ask-tool-result",
                &ToolResultEvent {
                    turn_id: turn_id.clone(),
                    tool: tc.name.clone(),
                    call_id: tc.id.clone(),
                    ok,
                    summary: summary.clone(),
                    output: truncate(&output, 2000),
                },
            );
            let receipt = AskReceipt {
                timestamp: now_secs(),
                turn_id: turn_id.clone(),
                kind: "tool".to_string(),
                tool: tc.name.clone(),
                gateway: model.gateway.clone(),
                model: model.id.clone(),
                summary: format!("{}: {}", tc.name, summary),
                outcome: if ok { "ok".to_string() } else { "error".to_string() },
                details: truncate(&output, 2000),
            };
            state.lock().unwrap().receipts.lock().unwrap().push(receipt);

            tool_results.push(serde_json::json!({
                "role": "tool",
                "tool_call_id": tc.id,
                "content": truncate(&output, MAX_MESSAGE_CHARS),
            }));
        }

        // Append assistant message with tool_calls, then tool results.
        messages.push(serde_json::json!({
            "role": "assistant",
            "content": outcome.text,
            "tool_calls": outcome.tool_calls.iter().map(|tc| serde_json::json!({
                "id": tc.id,
                "type": "function",
                "function": { "name": tc.name, "arguments": tc.args_json },
            })).collect::<Vec<_>>(),
        }));
        for tr in tool_results {
            messages.push(tr);
        }
    }

    Err(format!("exceeded the maximum of {MAX_TOOL_ROUNDS} tool rounds without a final answer"))
}

#[tauri::command]
pub fn ask_cancel(state: State<'_, Mutex<AskState>>, turn_id: String) -> Result<(), String> {
    let s = state.lock().unwrap();
    if let Some(flag) = s.cancel_flags.lock().unwrap().get(&turn_id) {
        flag.store(true, Ordering::Relaxed);
        Ok(())
    } else {
        Ok(()) // unknown/finished turn: no-op
    }
}
