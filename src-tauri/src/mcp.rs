// PAIGE Desktop - Native MCP client + connection manager (Increment 1D).
//
// This module makes PAIGE Desktop a full MCP client supporting multiple
// simultaneous independent servers:
//   - Local `stdio` subprocess servers (reusing the existing process engine
//     from terminal.rs for spawn/kill).
//   - Remote Streamable HTTP over HTTPS servers.
//
// It does NOT create a second orchestration layer or provider registry. It is a
// client layer that discovers capabilities (tools/resources/prompts), executes
// tools through PAIGE's existing approved authority semantics (receipts,
// approval boundaries where appropriate), and manages connection lifecycle.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};

// Reuse terminal engine helpers for PATH resolution and root validation.
use crate::terminal::{classify_command, TerminalRisk};
use crate::secrets::{base64_decode, base64_encode, dpapi_protect, dpapi_unprotect, config_dir};

const MCP_PROTOCOL_VERSION: &str = "2024-11-05";
const PING_INTERVAL_SECS: u64 = 30;
const RECONNECT_BACKOFF_SECS: &[u64] = &[2, 5, 10, 30, 60];
const STDIO_KILL_TIMEOUT_MS: u64 = 5000;
const MAX_RECEIPTS: usize = 200;

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum McpTransport {
    Stdio { command: String, args: Vec<String>, env: HashMap<String, String> },
    Remote { url: String, auth_header: Option<String>, auth_token: Option<String> },
}

impl McpTransport {
    pub fn kind(&self) -> &'static str {
        match self {
            McpTransport::Stdio { .. } => "stdio",
            McpTransport::Remote { .. } => "remote",
        }
    }

    pub fn display(&self) -> String {
        match self {
            McpTransport::Stdio { command, args, .. } => {
                let mut s = command.clone();
                for a in args {
                    s.push(' ');
                    s.push_str(a);
                }
                s
            }
            McpTransport::Remote { url, .. } => url.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub id: String,
    pub name: String,
    pub transport: McpTransport,
    #[serde(default)]
    pub auto_connect: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct McpResource {
    pub uri: String,
    pub name: Option<String>,
    pub mime_type: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct McpPrompt {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct McpCapabilities {
    pub tools: Vec<McpTool>,
    pub resources: Vec<McpResource>,
    pub prompts: Vec<McpPrompt>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct McpServerStatus {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub display: String,
    pub state: String, // offline | connecting | auth_needed | ready | error | unsupported
    pub detail: String,
    pub capabilities: McpCapabilities,
    pub last_seen_at: Option<u64>,
    pub server_info: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Serialize)]
pub struct McpReceipt {
    pub timestamp: u64,
    pub connection_id: String,
    pub tool: String,
    pub outcome: String,
    pub summary: String,
    pub details: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct McpCallResult {
    pub connection_id: String,
    pub tool: String,
    pub content: Vec<serde_json::Value>,
    pub is_error: bool,
}

// ---------------------------------------------------------------------------
// Runtime connection state
// ---------------------------------------------------------------------------

enum TransportHandle {
    Stdio {
        #[allow(dead_code)]
        pid: u32,
        stdin: std::process::ChildStdin,
        #[allow(dead_code)]
        stdout_reader: Option<thread::JoinHandle<()>>,
        #[allow(dead_code)]
        stderr_reader: Option<thread::JoinHandle<()>>,
        kill_tx: std::sync::mpsc::Sender<()>,
    },
    Remote {
        url: String,
        auth_header: Option<String>,
    },
}

struct PendingResponse {
    tx: std::sync::mpsc::Sender<Result<serde_json::Value, String>>,
}

struct McpConnection {
    config: McpServerConfig,
    handle: Option<TransportHandle>,
    state: String,
    detail: String,
    capabilities: McpCapabilities,
    server_info: Option<serde_json::Value>,
    last_seen_at: Option<u64>,
    next_id: i64,
    pending: HashMap<i64, PendingResponse>,
    receipts: Vec<McpReceipt>,
    cancel_health: Arc<AtomicBool>,
    #[allow(dead_code)]
    health_handle: Option<thread::JoinHandle<()>>,
    reconnect_attempt: usize,
}

pub struct McpState {
    connections: HashMap<String, McpConnection>,
    global_receipts: Vec<McpReceipt>,
}

impl McpState {
    pub fn new() -> Self {
        Self {
            connections: HashMap::new(),
            global_receipts: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Configuration persistence (DPAPI-encrypted, outside the repo)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct McpConfig {
    pub servers: Vec<McpServerConfig>,
}

impl McpConfig {
    fn path() -> Result<std::path::PathBuf, String> {
        Ok(config_dir()?.join("mcp.json"))
    }

    pub fn load() -> McpConfig {
        Self::path()
            .and_then(|p| std::fs::read_to_string(p).map_err(|e| e.to_string()))
            .and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string()))
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        let p = Self::path()?;
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&p, json).map_err(|e| format!("cannot write mcp config: {e}"))
    }
}

// Encrypt sensitive fields in a server config before saving.
fn protect_server_config(cfg: &mut McpServerConfig) -> Result<(), String> {
    match &mut cfg.transport {
        McpTransport::Remote { auth_token, auth_header, .. } => {
            if let Some(token) = auth_token.take() {
                let cipher = dpapi_protect(token.as_bytes())?;
                *auth_token = Some(format!("dpapi:{}", base64_encode(&cipher)));
            }
            if let Some(header) = auth_header.take() {
                let cipher = dpapi_protect(header.as_bytes())?;
                *auth_header = Some(format!("dpapi:{}", base64_encode(&cipher)));
            }
        }
        McpTransport::Stdio { env, .. } => {
            let keys: Vec<String> = env.keys().cloned().collect();
            for k in keys {
                // Heuristic: any env value that looks like a secret/token gets encrypted.
                let v = env.get(&k).cloned().unwrap_or_default();
                if looks_sensitive(&k, &v) && !v.starts_with("dpapi:") {
                    let cipher = dpapi_protect(v.as_bytes())?;
                    env.insert(k, format!("dpapi:{}", base64_encode(&cipher)));
                }
            }
        }
    }
    Ok(())
}

fn looks_sensitive(key: &str, value: &str) -> bool {
    let lower = key.to_lowercase();
    (lower.contains("key") || lower.contains("token") || lower.contains("secret") || lower.contains("password") || lower.contains("auth"))
        && !value.is_empty()
}

// Decrypt sensitive fields after loading.
fn unprotect_server_config(cfg: &mut McpServerConfig) -> Result<(), String> {
    match &mut cfg.transport {
        McpTransport::Remote { auth_token, auth_header, .. } => {
            if let Some(v) = auth_token {
                *v = unprotect_value(v)?;
            }
            if let Some(v) = auth_header {
                *v = unprotect_value(v)?;
            }
        }
        McpTransport::Stdio { env, .. } => {
            let keys: Vec<String> = env.keys().cloned().collect();
            for k in keys {
                let v = env.get(&k).cloned().unwrap_or_default();
                if v.starts_with("dpapi:") {
                    env.insert(k, unprotect_value(&v)?);
                }
            }
        }
    }
    Ok(())
}

fn unprotect_value(v: &str) -> Result<String, String> {
    if let Some(rest) = v.strip_prefix("dpapi:") {
        let cipher = base64_decode(rest)?;
        let plain = dpapi_unprotect(&cipher)?;
        String::from_utf8(plain).map_err(|_| "decrypted value is not valid UTF-8".to_string())
    } else {
        Ok(v.to_string())
    }
}

// ---------------------------------------------------------------------------
// JSON-RPC helpers
// ---------------------------------------------------------------------------

fn jsonrpc_request(id: i64, method: &str, params: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    })
}

fn jsonrpc_notification(method: &str, params: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
    })
}

fn parse_jsonrpc_response(value: serde_json::Value) -> Result<serde_json::Value, String> {
    if let Some(err) = value.get("error") {
        let msg = err
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("MCP error");
        let code = err.get("code").and_then(|c| c.as_i64()).unwrap_or(0);
        return Err(format!("MCP error {code}: {msg}"));
    }
    value
        .get("result")
        .cloned()
        .ok_or_else(|| "MCP response missing result".to_string())
}

// ---------------------------------------------------------------------------
// HTTP transport
// ---------------------------------------------------------------------------

#[cfg(feature = "insecure-test-bypass")]
#[derive(Debug)]
struct TestRemoteVerifier;

#[cfg(feature = "insecure-test-bypass")]
impl rustls::client::danger::ServerCertVerifier for TestRemoteVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::RSA_PSS_SHA256,
        ]
    }
}

fn remote_agent() -> ureq::Agent {
    // Only the deliberately-labeled insecure test path disables cert
    // verification, and it is used solely by the 1D local HTTPS test server.
    #[cfg(feature = "insecure-test-bypass")]
    {
        if std::env::var("PAIGE_MCP_TEST_INSECURE_REMOTE").ok().as_deref() == Some("1") {
            // rustls requires a process-level CryptoProvider before building a
            // ClientConfig; install it once (ignore races).
            let _ = rustls::crypto::ring::default_provider().install_default();
            let cfg = rustls::ClientConfig::builder_with_protocol_versions(rustls::ALL_VERSIONS)
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(TestRemoteVerifier))
                .with_no_client_auth();
            return ureq::AgentBuilder::new()
                .tls_config(Arc::new(cfg))
                .build();
        }
    }
    ureq::Agent::new()
}

fn remote_post(
    url: &str,
    auth_header: Option<&str>,
    body: serde_json::Value,
) -> Result<serde_json::Value, String> {
    if !url.starts_with("https://") {
        return Err("remote MCP servers must use HTTPS".to_string());
    }
    let agent = remote_agent();
    let mut req = agent
        .post(url)
        .timeout(Duration::from_secs(30))
        .set("Content-Type", "application/json")
        .set("Accept", "application/json");
    if let Some(h) = auth_header {
        req = req.set("Authorization", h);
    }
    let resp = req
        .send_string(&body.to_string())
        .map_err(|e| format!("MCP HTTP request failed: {e}"))?;
    let status = resp.status();
    if status == 401 || status == 403 {
        return Err(format!("MCP authentication failed (HTTP {status})"));
    }
    if !(200..300).contains(&status) {
        let body = resp.into_string().unwrap_or_default();
        return Err(format!("MCP HTTP {status}: {}", truncate(&body, 400)));
    }
    let text = resp.into_string().map_err(|e| format!("read response: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("parse MCP response: {e}"))
}

// ---------------------------------------------------------------------------
// stdio transport
// ---------------------------------------------------------------------------

fn spawn_stdio_server(
    app: AppHandle,
    conn_id: String,
    command: &str,
    args: &[String],
    env: &HashMap<String, String>,
) -> Result<TransportHandle, String> {
    // Reuse terminal authority: command must be in PATH, not a path, and not privileged.
    let risk = classify_command(command, args);
    if risk == TerminalRisk::Privileged {
        return Err("privileged/destructive commands cannot be used as MCP stdio servers".to_string());
    }
    let resolved = crate::terminal::resolve_executable(command)?;

    let mut cmd = Command::new(&resolved);
    cmd.args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::piped());
    if !env.is_empty() {
        cmd.envs(env);
    }

    let mut child = cmd.spawn().map_err(|e| format!("failed to spawn MCP server: {e}"))?;
    let pid = child.id();
    let stdin = child.stdin.take().ok_or("failed to capture stdin")?;
    let stdout = child.stdout.take().ok_or("failed to capture stdout")?;
    let stderr = child.stderr.take().ok_or("failed to capture stderr")?;

    let (kill_tx, kill_rx) = std::sync::mpsc::channel::<()>();
    let app_stdout = app.clone();
    let conn_id_out = conn_id.clone();
    let stdout_reader = thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            if let Ok(line) = line {
                emit_mcp_message(&app_stdout, &conn_id_out, "stdout", &line);
                if let Some(state) = app_stdout.try_state::<Mutex<McpState>>() {
                    route_incoming_message(&state, &conn_id_out, &line);
                }
            }
        }
    });

    let app_stderr = app.clone();
    let conn_id_err = conn_id.clone();
    let stderr_reader = thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            if let Ok(line) = line {
                emit_mcp_message(&app_stderr, &conn_id_err, "stderr", &line);
                if let Some(state) = app_stderr.try_state::<Mutex<McpState>>() {
                    let mut s = state.lock().unwrap();
                    if let Some(conn) = s.connections.get_mut(&conn_id_err) {
                        if conn.state == "connecting" {
                            conn.detail = truncate(&line, 200);
                        }
                    }
                }
            }
        }
    });

    // Process watcher: detect unexpected exit and attempt reconnect.
    let conn_id_watch = conn_id.clone();
    thread::spawn(move || {
        let _ = kill_rx.recv_timeout(Duration::from_millis(STDIO_KILL_TIMEOUT_MS));
        let _ = child.wait();
        // The thread cannot hold an AppHandle borrow across the wait, so we
        // emit a reconnect request via a fresh clone after the child exits.
        let app2 = app.clone();
        if let Some(state) = app2.try_state::<Mutex<McpState>>() {
            let mut s = state.lock().unwrap();
            if let Some(conn) = s.connections.get_mut(&conn_id_watch) {
                if conn.state == "ready" || conn.state == "connecting" {
                    conn.state = "offline".to_string();
                    conn.detail = "stdio server process exited".to_string();
                    conn.handle = None;
                    conn.last_seen_at = Some(now_secs());
                    drop(s);
                    schedule_reconnect(app2, conn_id_watch);
                }
            }
        }
    });

    Ok(TransportHandle::Stdio {
        pid,
        stdin,
        stdout_reader: Some(stdout_reader),
        stderr_reader: Some(stderr_reader),
        kill_tx,
    })
}

fn emit_mcp_message(app: &AppHandle, connection_id: &str, stream: &str, line: &str) {
    let _ = app.emit(
        "mcp-message",
        &serde_json::json!({
            "connection_id": connection_id,
            "stream": stream,
            "line": line,
            "timestamp": now_millis(),
        }),
    );
}

fn route_incoming_message(state: &Mutex<McpState>, connection_id: &str, line: &str) {
    let value: serde_json::Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => return,
    };
    // Only handle responses with an id; notifications are ignored for now.
    let id = value.get("id").and_then(|i| i.as_i64());
    let mut s = state.lock().unwrap();
    let Some(conn) = s.connections.get_mut(connection_id) else {
        return;
    };
    if let Some(id) = id {
        if let Some(pending) = conn.pending.remove(&id) {
            let result = parse_jsonrpc_response(value);
            let _ = pending.tx.send(result);
        }
    }
}

fn write_stdio(stdin: &mut std::process::ChildStdin, value: serde_json::Value) -> Result<(), String> {
    let line = value.to_string();
    stdin
        .write_all(line.as_bytes())
        .and_then(|_| stdin.write_all(b"\n"))
        .and_then(|_| stdin.flush())
        .map_err(|e| format!("failed to write to MCP stdio server: {e}"))
}

fn kill_stdio(handle: &mut TransportHandle) -> Result<(), String> {
    if let TransportHandle::Stdio { kill_tx, .. } = handle {
        let _ = kill_tx.send(());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Generic request dispatch
// ---------------------------------------------------------------------------

fn next_request_id(state: &Mutex<McpState>, connection_id: &str) -> Result<i64, String> {
    let mut s = state.lock().unwrap();
    let conn = s
        .connections
        .get_mut(connection_id)
        .ok_or("unknown MCP connection")?;
    let id = conn.next_id;
    conn.next_id += 1;
    Ok(id)
}

fn send_notification(
    _app: &AppHandle,
    state: &Mutex<McpState>,
    connection_id: &str,
    method: &str,
    params: serde_json::Value,
) -> Result<(), String> {
    let notification = jsonrpc_notification(method, params);
    let mut s = state.lock().unwrap();
    let conn = s
        .connections
        .get_mut(connection_id)
        .ok_or("unknown MCP connection")?;
    match conn.handle.as_mut() {
        Some(TransportHandle::Stdio { stdin, .. }) => write_stdio(stdin, notification),
        Some(TransportHandle::Remote { url, auth_header }) => {
            let url = url.clone();
            let auth = auth_header.clone();
            thread::spawn(move || {
                let _ = remote_post(&url, auth.as_deref(), notification);
                // Notifications are fire-and-forget; ignore the response.
            });
            Ok(())
        }
        None => Err("MCP connection has no transport".to_string()),
    }
}

fn send_request(
    app: &AppHandle,
    state: &Mutex<McpState>,
    connection_id: &str,
    method: &str,
    params: serde_json::Value,
    timeout_secs: u64,
) -> Result<serde_json::Value, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    let id = next_request_id(state, connection_id)?;

    {
        let mut s = state.lock().unwrap();
        let conn = s
            .connections
            .get_mut(connection_id)
            .ok_or("unknown MCP connection")?;
        if conn.state != "ready" && conn.state != "connecting" && method != "initialize" {
            return Err(format!("MCP connection is {}", conn.state));
        }
        conn.pending.insert(id, PendingResponse { tx });
        let request = jsonrpc_request(id, method, params);

        match conn.handle.as_mut() {
            Some(TransportHandle::Stdio { stdin, .. }) => {
                write_stdio(stdin, request)?;
            }
            Some(TransportHandle::Remote { url, auth_header }) => {
                let url = url.clone();
                let auth = auth_header.clone();
                let conn_id2 = connection_id.to_string();
                let app2 = app.clone();
                thread::spawn(move || {
                    // Normalize: send the extracted JSON-RPC result (or error),
                    // matching the stdio path where route_incoming_message
                    // parses the envelope before sending.
                    let result = remote_post(&url, auth.as_deref(), request).and_then(parse_jsonrpc_response);
                    if let Some(ts) = app2.try_state::<Mutex<McpState>>() {
                        let mut s = ts.lock().unwrap();
                        if let Some(conn) = s.connections.get_mut(&conn_id2) {
                            if let Some(pending) = conn.pending.remove(&id) {
                                let _ = pending.tx.send(result);
                            }
                        }
                    }
                });
            }
            None => return Err("MCP connection has no transport".to_string()),
        }
    }

    match rx.recv_timeout(Duration::from_secs(timeout_secs)) {
        Ok(r) => r,
        Err(_) => {
            // Remove stale pending entry.
            let mut s = state.lock().unwrap();
            if let Some(conn) = s.connections.get_mut(connection_id) {
                conn.pending.remove(&id);
                if conn.state == "ready" {
                    conn.state = "offline".to_string();
                    conn.detail = "request timed out".to_string();
                }
            }
            Err(format!("MCP {method} timed out"))
        }
    }
}

// ---------------------------------------------------------------------------
// Capability discovery
// ---------------------------------------------------------------------------

fn discover_capabilities(
    app: &AppHandle,
    state: &Mutex<McpState>,
    connection_id: &str,
) -> Result<McpCapabilities, String> {
    let tools = send_request(app, state, connection_id, "tools/list", serde_json::json!({}), 30)?;
    let resources = send_request(app, state, connection_id, "resources/list", serde_json::json!({}), 30)
        .unwrap_or(serde_json::json!({ "resources": [] }));
    let prompts = send_request(app, state, connection_id, "prompts/list", serde_json::json!({}), 30)
        .unwrap_or(serde_json::json!({ "prompts": [] }));

    let tools_vec = tools
        .get("tools")
        .and_then(|t| t.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|t| {
                    Some(McpTool {
                        name: t.get("name")?.as_str()?.to_string(),
                        description: t.get("description").and_then(|d| d.as_str()).map(|s| s.to_string()),
                        input_schema: t.get("inputSchema").or_else(|| t.get("input_schema")).cloned().unwrap_or(serde_json::json!({})),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let resources_vec = resources
        .get("resources")
        .and_then(|r| r.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|r| {
                    Some(McpResource {
                        uri: r.get("uri")?.as_str()?.to_string(),
                        name: r.get("name").and_then(|n| n.as_str()).map(|s| s.to_string()),
                        mime_type: r.get("mimeType").and_then(|m| m.as_str()).map(|s| s.to_string()),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let prompts_vec = prompts
        .get("prompts")
        .and_then(|p| p.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|p| {
                    Some(McpPrompt {
                        name: p.get("name")?.as_str()?.to_string(),
                        description: p.get("description").and_then(|d| d.as_str()).map(|s| s.to_string()),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(McpCapabilities {
        tools: tools_vec,
        resources: resources_vec,
        prompts: prompts_vec,
    })
}

// ---------------------------------------------------------------------------
// Connection lifecycle
// ---------------------------------------------------------------------------

fn do_connect(
    app: AppHandle,
    state: &Mutex<McpState>,
    connection_id: &str,
) -> Result<McpServerStatus, String> {
    {
        let mut s = state.lock().unwrap();
        let conn = s
            .connections
            .get_mut(connection_id)
            .ok_or("unknown MCP connection")?;
        conn.state = "connecting".to_string();
        conn.detail = "starting handshake".to_string();
        conn.capabilities = McpCapabilities {
            tools: Vec::new(),
            resources: Vec::new(),
            prompts: Vec::new(),
        };
        conn.server_info = None;
        conn.pending.clear();
        conn.next_id = 1;
    }

    // Spawn transport if needed.
    {
        let mut s = state.lock().unwrap();
        let conn = s.connections.get_mut(connection_id).unwrap();
        if conn.handle.is_none() {
            let cfg = conn.config.clone();
            match &cfg.transport {
                McpTransport::Stdio { command, args, env } => {
                    let handle = spawn_stdio_server(app.clone(), connection_id.to_string(), command, args, env)?;
                    conn.handle = Some(handle);
                }
                McpTransport::Remote { .. } => {
                    // No process to spawn; HTTP is stateless per request.
                }
            }
        }
    }

    // Initialize handshake.
    let init_params = serde_json::json!({
        "protocolVersion": MCP_PROTOCOL_VERSION,
        "capabilities": {
            "tools": {},
            "resources": { "subscribe": false },
            "prompts": {},
            "logging": {},
        },
        "clientInfo": { "name": "paige-desktop", "version": env!("CARGO_PKG_VERSION") },
    });
    let init_result = send_request(&app, state, connection_id, "initialize", init_params, 30)?;

    let protocol_version = init_result
        .get("protocolVersion")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    if !protocol_version.starts_with("2024-11") && !protocol_version.starts_with("2025") {
        let mut s = state.lock().unwrap();
        if let Some(conn) = s.connections.get_mut(connection_id) {
            conn.state = "unsupported".to_string();
            conn.detail = format!("server protocol version {protocol_version} is not supported");
        }
        return Err(format!("unsupported MCP protocol version {protocol_version}"));
    }

    let server_info = init_result.get("serverInfo").cloned();

    // Send initialized notification (fire-and-forget).
    let _ = send_notification(
        &app,
        state,
        connection_id,
        "notifications/initialized",
        serde_json::json!({}),
    );

    // Discover capabilities.
    let capabilities = discover_capabilities(&app, state, connection_id).map_err(|e| {
        // Mark auth-needed if the discovery failed with an auth error.
        let mut s = state.lock().unwrap();
        if let Some(conn) = s.connections.get_mut(connection_id) {
            if e.contains("authentication failed") {
                conn.state = "auth_needed".to_string();
                conn.detail = "authentication failed".to_string();
            } else {
                conn.state = "error".to_string();
                conn.detail = e.clone();
            }
        }
        e
    })?;

    {
        let mut s = state.lock().unwrap();
        let conn = s.connections.get_mut(connection_id).unwrap();
        conn.state = "ready".to_string();
        conn.detail = format!("{} tools, {} resources, {} prompts", capabilities.tools.len(), capabilities.resources.len(), capabilities.prompts.len());
        conn.capabilities = capabilities.clone();
        conn.server_info = server_info;
        conn.last_seen_at = Some(now_secs());
        conn.reconnect_attempt = 0;
    }

    start_health_monitor(app.clone(), connection_id.to_string());

    build_status(state, connection_id)
}

fn do_disconnect(state: &Mutex<McpState>, connection_id: &str, keep_config: bool) -> Result<(), String> {
    let mut s = state.lock().unwrap();
    let Some(conn) = s.connections.get_mut(connection_id) else {
        return Err("unknown MCP connection".to_string());
    };
    conn.cancel_health.store(true, Ordering::Relaxed);
    if let Some(mut handle) = conn.handle.take() {
        let _ = kill_stdio(&mut handle);
    }
    conn.state = "offline".to_string();
    conn.detail = if keep_config {
        "disconnected by user".to_string()
    } else {
        "revoked".to_string()
    };
    conn.last_seen_at = Some(now_secs());
    conn.pending.clear();
    Ok(())
}

fn schedule_reconnect(app: AppHandle, connection_id: String) {
    thread::spawn(move || {
        let app_for_connect = app.clone();
        thread::sleep(Duration::from_secs(2));
        let (attempt, should_reconnect) = if let Some(state) = app.try_state::<Mutex<McpState>>() {
            let s = state.lock().unwrap();
            let attempt = s.connections.get(&connection_id).map(|c| c.reconnect_attempt).unwrap_or(0);
            let is_offline = s.connections.get(&connection_id).map(|c| c.state == "offline").unwrap_or(false);
            (attempt, is_offline)
        } else {
            (0, false)
        };
        if !should_reconnect {
            return;
        }
        let delay = RECONNECT_BACKOFF_SECS.get(attempt).copied().unwrap_or(60);
        thread::sleep(Duration::from_secs(delay));
        if let Some(state) = app.try_state::<Mutex<McpState>>() {
            let mut s = state.lock().unwrap();
            if let Some(conn) = s.connections.get_mut(&connection_id) {
                if conn.state == "offline" {
                    conn.reconnect_attempt = (attempt + 1).min(RECONNECT_BACKOFF_SECS.len() - 1);
                    conn.state = "connecting".to_string();
                    conn.detail = "automatic reconnection".to_string();
                } else {
                    return;
                }
            }
            drop(s);
            let _ = do_connect(app_for_connect, &*state, &connection_id);
        }
    });
}

fn start_health_monitor(app: AppHandle, connection_id: String) {
    let cancel = {
        if let Some(state) = app.try_state::<Mutex<McpState>>() {
            let s = state.lock().unwrap();
            s.connections
                .get(&connection_id)
                .map(|c| c.cancel_health.clone())
                .unwrap_or_else(|| Arc::new(AtomicBool::new(true)))
        } else {
            Arc::new(AtomicBool::new(true))
        }
    };

    thread::spawn(move || {
        loop {
            for _ in 0..PING_INTERVAL_SECS {
                if cancel.load(Ordering::Relaxed) {
                    return;
                }
                thread::sleep(Duration::from_secs(1));
            }
            if cancel.load(Ordering::Relaxed) {
                return;
            }
            let app2 = app.clone();
            let result = if let Some(state) = app.try_state::<Mutex<McpState>>() {
                send_request(&app, &state, &connection_id, "ping", serde_json::json!({}), 15)
            } else {
                Err("app state unavailable".to_string())
            };
            if let Some(state) = app2.try_state::<Mutex<McpState>>() {
                let mut s = state.lock().unwrap();
                if let Some(conn) = s.connections.get_mut(&connection_id) {
                    if cancel.load(Ordering::Relaxed) {
                        return;
                    }
                    match result {
                        Ok(_) => {
                            conn.last_seen_at = Some(now_secs());
                            if conn.state != "ready" {
                                conn.state = "ready".to_string();
                            }
                        }
                        Err(e) => {
                            conn.state = "offline".to_string();
                            conn.detail = format!("health ping failed: {e}");
                            conn.handle = None;
                            drop(s);
                            schedule_reconnect(app2, connection_id);
                            return;
                        }
                    }
                }
            }
        }
    });
}

fn build_status(state: &Mutex<McpState>, connection_id: &str) -> Result<McpServerStatus, String> {
    let s = state.lock().unwrap();
    let conn = s
        .connections
        .get(connection_id)
        .ok_or("unknown MCP connection")?;
    Ok(McpServerStatus {
        id: conn.config.id.clone(),
        name: conn.config.name.clone(),
        kind: conn.config.transport.kind().to_string(),
        display: conn.config.transport.display(),
        state: conn.state.clone(),
        detail: conn.detail.clone(),
        capabilities: conn.capabilities.clone(),
        last_seen_at: conn.last_seen_at,
        server_info: conn.server_info.clone(),
    })
}

fn build_all_status(state: &Mutex<McpState>) -> Vec<McpServerStatus> {
    let s = state.lock().unwrap();
    s.connections
        .values()
        .map(|conn| McpServerStatus {
            id: conn.config.id.clone(),
            name: conn.config.name.clone(),
            kind: conn.config.transport.kind().to_string(),
            display: conn.config.transport.display(),
            state: conn.state.clone(),
            detail: conn.detail.clone(),
            capabilities: conn.capabilities.clone(),
            last_seen_at: conn.last_seen_at,
            server_info: conn.server_info.clone(),
        })
        .collect()
}

fn add_receipt(state: &Mutex<McpState>, connection_id: &str, receipt: McpReceipt) {
    let mut s = state.lock().unwrap();
    if let Some(conn) = s.connections.get_mut(connection_id) {
        conn.receipts.push(receipt.clone());
        if conn.receipts.len() > MAX_RECEIPTS {
            conn.receipts.remove(0);
        }
    }
    s.global_receipts.push(receipt);
    if s.global_receipts.len() > MAX_RECEIPTS {
        s.global_receipts.remove(0);
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n {
        return s.to_string();
    }
    let mut end = n;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &s[..end])
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[derive(Clone, Serialize)]
pub struct McpStatus {
    pub servers: Vec<McpServerStatus>,
}

#[tauri::command]
pub fn mcp_status(state: State<'_, Mutex<McpState>>) -> McpStatus {
    McpStatus {
        servers: build_all_status(state.inner()),
    }
}

#[derive(Clone, Deserialize)]
pub struct McpAddStdioRequest {
    pub id: String,
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub auto_connect: bool,
}

#[tauri::command]
pub fn mcp_add_stdio(
    app: AppHandle,
    state: State<'_, Mutex<McpState>>,
    request: McpAddStdioRequest,
) -> Result<McpServerStatus, String> {
    if request.id.is_empty() || request.name.is_empty() || request.command.is_empty() {
        return Err("id, name and command are required".to_string());
    }
    let mut cfg = McpConfig::load();
    if cfg.servers.iter().any(|s| s.id == request.id) {
        return Err("an MCP server with this id already exists".to_string());
    }

    let mut server = McpServerConfig {
        id: request.id.clone(),
        name: request.name,
        transport: McpTransport::Stdio {
            command: request.command,
            args: request.args,
            env: request.env,
        },
        auto_connect: request.auto_connect,
    };
    protect_server_config(&mut server)?;
    cfg.servers.push(server.clone());
    cfg.save()?;
    unprotect_server_config(&mut server)?;

    let conn = McpConnection {
        config: server,
        handle: None,
        state: "offline".to_string(),
        detail: "not connected".to_string(),
        capabilities: McpCapabilities {
            tools: Vec::new(),
            resources: Vec::new(),
            prompts: Vec::new(),
        },
        server_info: None,
        last_seen_at: None,
        next_id: 1,
        pending: HashMap::new(),
        receipts: Vec::new(),
        cancel_health: Arc::new(AtomicBool::new(false)),
        health_handle: None,
        reconnect_attempt: 0,
    };
    let id = conn.config.id.clone();
    state.lock().unwrap().connections.insert(id.clone(), conn);
    if request.auto_connect {
        return do_connect(app, state.inner(), &id);
    }
    build_status(state.inner(), &id)
}

#[derive(Clone, Deserialize)]
pub struct McpAddRemoteRequest {
    pub id: String,
    pub name: String,
    pub url: String,
    pub auth_header: Option<String>,
    pub auth_token: Option<String>,
    #[serde(default)]
    pub auto_connect: bool,
}

#[tauri::command]
pub fn mcp_add_remote(
    app: AppHandle,
    state: State<'_, Mutex<McpState>>,
    request: McpAddRemoteRequest,
) -> Result<McpServerStatus, String> {
    if request.id.is_empty() || request.name.is_empty() || request.url.is_empty() {
        return Err("id, name and url are required".to_string());
    }
    if !request.url.starts_with("https://") {
        return Err("remote MCP servers must use HTTPS".to_string());
    }
    let mut cfg = McpConfig::load();
    if cfg.servers.iter().any(|s| s.id == request.id) {
        return Err("an MCP server with this id already exists".to_string());
    }

    let mut server = McpServerConfig {
        id: request.id.clone(),
        name: request.name,
        transport: McpTransport::Remote {
            url: request.url,
            auth_header: request.auth_header,
            auth_token: request.auth_token,
        },
        auto_connect: request.auto_connect,
    };
    protect_server_config(&mut server)?;
    cfg.servers.push(server.clone());
    cfg.save()?;
    unprotect_server_config(&mut server)?;

    let (url, auth_header) = match &server.transport {
        McpTransport::Remote { url, auth_header, .. } => (url.clone(), auth_header.clone()),
        _ => unreachable!(),
    };
    let conn = McpConnection {
        config: server,
        handle: Some(TransportHandle::Remote { url, auth_header }),
        state: "offline".to_string(),
        detail: "not connected".to_string(),
        capabilities: McpCapabilities {
            tools: Vec::new(),
            resources: Vec::new(),
            prompts: Vec::new(),
        },
        server_info: None,
        last_seen_at: None,
        next_id: 1,
        pending: HashMap::new(),
        receipts: Vec::new(),
        cancel_health: Arc::new(AtomicBool::new(false)),
        health_handle: None,
        reconnect_attempt: 0,
    };
    let id = conn.config.id.clone();
    state.lock().unwrap().connections.insert(id.clone(), conn);
    if request.auto_connect {
        return do_connect(app, state.inner(), &id);
    }
    build_status(state.inner(), &id)
}

#[tauri::command]
pub fn mcp_connect(
    app: AppHandle,
    state: State<'_, Mutex<McpState>>,
    id: String,
) -> Result<McpServerStatus, String> {
    do_connect(app, state.inner(), &id)
}

#[tauri::command]
pub fn mcp_disconnect(state: State<'_, Mutex<McpState>>, id: String) -> Result<McpStatus, String> {
    do_disconnect(state.inner(), &id, true)?;
    Ok(McpStatus {
        servers: build_all_status(state.inner()),
    })
}

#[tauri::command]
pub fn mcp_reconnect(
    app: AppHandle,
    state: State<'_, Mutex<McpState>>,
    id: String,
) -> Result<McpServerStatus, String> {
    let _ = do_disconnect(state.inner(), &id, true);
    do_connect(app, state.inner(), &id)
}

#[tauri::command]
pub fn mcp_revoke(state: State<'_, Mutex<McpState>>, id: String) -> Result<McpStatus, String> {
    do_disconnect(state.inner(), &id, false)?;
    {
        let mut s = state.lock().unwrap();
        s.connections.remove(&id);
    }
    let mut cfg = McpConfig::load();
    cfg.servers.retain(|s| s.id != id);
    cfg.save()?;
    Ok(McpStatus {
        servers: build_all_status(state.inner()),
    })
}

#[derive(Clone, Deserialize)]
pub struct McpCallToolRequest {
    pub connection_id: String,
    pub tool: String,
    pub arguments: serde_json::Value,
}

#[tauri::command]
pub fn mcp_call_tool(
    app: AppHandle,
    state: State<'_, Mutex<McpState>>,
    request: McpCallToolRequest,
) -> Result<McpCallResult, String> {
    let result = send_request(
        &app,
        state.inner(),
        &request.connection_id,
        "tools/call",
        serde_json::json!({
            "name": request.tool,
            "arguments": request.arguments,
        }),
        60,
    );

    let (outcome, summary, details, content, is_error) = match &result {
        Ok(value) => {
            let content = value
                .get("content")
                .and_then(|c| c.as_array())
                .cloned()
                .unwrap_or_default();
            let is_error = value.get("isError").and_then(|v| v.as_bool()).unwrap_or(false);
            let summary = if is_error {
                format!("{} returned error", request.tool)
            } else {
                format!("{} returned {} content items", request.tool, content.len())
            };
            let details = serde_json::to_string(value).unwrap_or_default();
            (
                if is_error { "error".to_string() } else { "ok".to_string() },
                summary,
                details,
                content,
                is_error,
            )
        }
        Err(e) => {
            let summary = format!("{} failed: {}", request.tool, truncate(e, 160));
            (
                "error".to_string(),
                summary.clone(),
                serde_json::json!({ "error": e }).to_string(),
                Vec::new(),
                true,
            )
        }
    };

    let receipt = McpReceipt {
        timestamp: now_secs(),
        connection_id: request.connection_id.clone(),
        tool: request.tool.clone(),
        outcome: outcome.clone(),
        summary: summary.clone(),
        details: truncate(&details, 2000),
    };
    add_receipt(state.inner(), &request.connection_id, receipt);

    let _ = app.emit(
        "mcp-tool-result",
        &serde_json::json!({
            "connection_id": request.connection_id,
            "tool": request.tool,
            "outcome": outcome,
            "summary": summary,
        }),
    );

    if result.is_err() {
        return Err(summary);
    }

    Ok(McpCallResult {
        connection_id: request.connection_id,
        tool: request.tool,
        content,
        is_error,
    })
}

#[tauri::command]
pub fn mcp_receipts(state: State<'_, Mutex<McpState>>) -> Vec<McpReceipt> {
    state.lock().unwrap().global_receipts.clone()
}

// ---------------------------------------------------------------------------
// Load persisted servers on startup
// ---------------------------------------------------------------------------

pub fn load_persisted_servers(state: &Mutex<McpState>) {
    let cfg = McpConfig::load();
    let mut s = state.lock().unwrap();
    for mut server in cfg.servers {
        let _ = unprotect_server_config(&mut server);
        let handle = match &server.transport {
            McpTransport::Remote { url, auth_header, .. } => {
                Some(TransportHandle::Remote { url: url.clone(), auth_header: auth_header.clone() })
            }
            McpTransport::Stdio { .. } => None,
        };
        let conn = McpConnection {
            config: server,
            handle,
            state: "offline".to_string(),
            detail: "loaded from config; not connected".to_string(),
            capabilities: McpCapabilities {
                tools: Vec::new(),
                resources: Vec::new(),
                prompts: Vec::new(),
            },
            server_info: None,
            last_seen_at: None,
            next_id: 1,
            pending: HashMap::new(),
            receipts: Vec::new(),
            cancel_health: Arc::new(AtomicBool::new(false)),
            health_handle: None,
            reconnect_attempt: 0,
        };
        s.connections.insert(conn.config.id.clone(), conn);
    }
}

// ---------------------------------------------------------------------------
// Unit tests for pure logic
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jsonrpc_request_has_required_fields() {
        let r = jsonrpc_request(7, "tools/list", serde_json::json!({"cursor": "x"}));
        assert_eq!(r["jsonrpc"], "2.0");
        assert_eq!(r["id"], 7);
        assert_eq!(r["method"], "tools/list");
        assert_eq!(r["params"]["cursor"], "x");
    }

    #[test]
    fn parse_jsonrpc_response_extracts_result() {
        let v = serde_json::json!({"jsonrpc":"2.0","id":1,"result":{"tools":[]}});
        let r = parse_jsonrpc_response(v).unwrap();
        assert_eq!(r["tools"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn parse_jsonrpc_response_returns_error() {
        let v = serde_json::json!({"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"Method not found"}});
        let r = parse_jsonrpc_response(v);
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("Method not found"));
    }

    #[test]
    fn looks_sensitive_detects_secret_keys() {
        assert!(looks_sensitive("API_KEY", "abc"));
        assert!(looks_sensitive("auth_token", "tok"));
        assert!(!looks_sensitive("PATH", "/bin"));
    }

    #[test]
    fn truncate_is_char_boundary_safe() {
        let s = "a😀b😀c";
        assert_eq!(truncate(s, 5), "a😀…");
    }

    #[test]
    fn mcp_config_round_trip() {
        unsafe {
            std::env::set_var(
                "PAIGE_TEST_CONFIG_DIR",
                std::env::temp_dir().join("paige-mcp-test").to_string_lossy().to_string(),
            );
        }
        let mut cfg = McpConfig::default();
        cfg.servers.push(McpServerConfig {
            id: "test".to_string(),
            name: "Test".to_string(),
            transport: McpTransport::Remote {
                url: "https://example.com/mcp".to_string(),
                auth_header: Some("Bearer secret".to_string()),
                auth_token: None,
            },
            auto_connect: false,
        });
        cfg.save().unwrap();
        let loaded = McpConfig::load();
        assert_eq!(loaded.servers.len(), 1);
        assert_eq!(loaded.servers[0].id, "test");
    }
}
