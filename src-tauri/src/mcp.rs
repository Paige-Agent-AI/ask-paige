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
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};

// Reuse terminal engine helpers for PATH resolution and root validation.
use crate::terminal::{classify_command, TerminalRisk};
use crate::secrets::{base64_decode, base64_encode, dpapi_protect, dpapi_unprotect, config_dir};
use crate::connected_tools::{self, Service};
use crate::provider_oauth::{self, Grant, Provider, SignIn};

const MCP_PROTOCOL_VERSION: &str = "2024-11-05";
const MCP_HTTP_PROTOCOL_VERSION: &str = "2025-06-18";
const MCP_SESSION_EXPIRED: &str = "MCP session expired (HTTP 404)";
const MCP_AUTH_RENEWED: &str = "The provider renewed its access token. The MCP session must reconnect.";
const PING_INTERVAL_SECS: u64 = 30;
const RECONNECT_BACKOFF_SECS: &[u64] = &[2, 5, 10, 30, 60];
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
    Stdio {
        command: String,
        args: Vec<String>,
        env: HashMap<String, String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        github_repository: Option<String>,
    },
    Remote {
        url: String,
        auth_header: Option<String>,
        auth_token: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        linear_oauth: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        supabase_oauth: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        notion_oauth: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        paige_oauth: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        notion_page: Option<String>,
    },
}

impl McpTransport {
    fn service(&self) -> Option<Service> {
        match self {
            Self::Remote { url, linear_oauth: Some(_), supabase_oauth: None, notion_oauth: None, paige_oauth: None, .. }
                if url == provider_oauth::LINEAR_RESOURCE => Some(Service::Linear),
            Self::Remote { url, linear_oauth: None, supabase_oauth: Some(_), notion_oauth: None, paige_oauth: None, .. }
                if connected_tools::supabase_project(url).is_some() => Some(Service::Supabase),
            Self::Remote { url, linear_oauth: None, supabase_oauth: None, notion_oauth: Some(_), paige_oauth: None,
                notion_page: Some(page), auth_header: None, auth_token: None }
                if url == provider_oauth::NOTION_RESOURCE
                    && page.len() == 32 && crate::notion_lookup::canonical_page_id(page).as_ref() == Some(page)
                    => Some(Service::Notion),
            Self::Remote { url, linear_oauth: None, supabase_oauth: None, notion_oauth: None, paige_oauth: Some(_),
                notion_page: None, auth_header: None, auth_token: None }
                if url == provider_oauth::PAIGE_RESOURCE => Some(Service::Paige),
            Self::Stdio { github_repository: Some(repository), .. }
                if connected_tools::github_repository(repository).is_some() => Some(Service::GitHub),
            _ => None,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            McpTransport::Stdio { .. } => "stdio",
            McpTransport::Remote { .. } => "remote",
        }
    }

    pub fn display(&self) -> String {
        match self {
            McpTransport::Stdio { github_repository: Some(repository), .. } => format!("github.com/{repository}"),
            McpTransport::Stdio { command, args, .. } => {
                let mut s = command.clone();
                for a in args {
                    s.push(' ');
                    s.push_str(a);
                }
                s
            }
            McpTransport::Remote { notion_page: Some(page), .. } if self.service() == Some(Service::Notion) =>
                crate::notion_lookup::page_url(page),
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
    registration: String,
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
    pub oauth_provider: Option<&'static str>,
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
    #[serde(skip)]
    pub(crate) structured_content: Option<serde_json::Value>,
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
    Remote(RemoteTransport),
}

#[derive(Clone)]
struct RemoteTransport {
    url: String,
    auth_header: Option<String>,
    session_id: Option<String>,
    protocol_version: Option<String>,
    oauth_registration: Option<(Provider, String)>,
}

impl RemoteTransport {
    fn new(url: &str, auth_header: &Option<String>, oauth_registration: Option<(Provider, String)>) -> Self {
        Self {
            url: url.to_owned(),
            auth_header: auth_header.clone(),
            session_id: None,
            protocol_version: None,
            oauth_registration,
        }
    }
}

struct PendingResponse {
    tx: std::sync::mpsc::Sender<Result<serde_json::Value, String>>,
    github_result: Option<GitHubResult>,
}

struct McpConnection {
    config: McpServerConfig,
    handle: Option<TransportHandle>,
    state: String,
    detail: String,
    capabilities: McpCapabilities,
    server_info: Option<serde_json::Value>,
    last_seen_at: Option<u64>,
    pending: HashMap<i64, PendingResponse>,
    receipts: Vec<McpReceipt>,
    cancel_health: Arc<AtomicBool>,
    #[allow(dead_code)]
    health_handle: Option<thread::JoinHandle<()>>,
    reconnect_attempt: usize,
}

impl McpConnection {
    fn new(config: McpServerConfig, detail: &str) -> Self {
        Self {
            config,
            handle: None,
            state: "offline".into(),
            detail: detail.into(),
            capabilities: McpCapabilities { tools: Vec::new(), resources: Vec::new(), prompts: Vec::new() },
            server_info: None,
            last_seen_at: None,
            pending: HashMap::new(),
            receipts: Vec::new(),
            cancel_health: Arc::new(AtomicBool::new(false)),
            health_handle: None,
            reconnect_attempt: 0,
        }
    }
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

    fn lock() -> Result<std::fs::File, String> {
        let file = std::fs::OpenOptions::new().read(true).write(true).create(true)
            .truncate(false).open(Self::path()?.with_extension("lock"))
            .map_err(|_| "Could not open the MCP configuration lock.")?;
        file.lock().map_err(|_| "Could not lock the MCP configuration.")?;
        Ok(file)
    }

    fn load() -> Result<Self, String> {
        match std::fs::read_to_string(Self::path()?) {
            Ok(data) => serde_json::from_str(&data).map_err(|_| "The saved MCP configuration is invalid.".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(_) => Err("Could not read the saved MCP configuration.".into()),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let p = Self::path()?;
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let temporary = p.with_extension(format!("{}.tmp", new_registration()?));
        let result = (|| {
            let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&temporary)?;
            file.write_all(json.as_bytes())?;
            file.sync_all()?;
            drop(file);
            std::fs::rename(&temporary, &p)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        result.map_err(|_| "Could not save the MCP configuration.".into())
    }
}

fn oauth_header(provider: Provider, registration: &str, rejected: Option<&str>) -> Result<String, String> {
    // The disk lock serializes refresh and removal across desktop processes.
    let _lock = McpConfig::lock()?;
    let mut config = McpConfig::load()?;
    let server = config.servers.iter_mut()
        .find(|server| server.registration == registration
            && server.transport.service().and_then(Service::remote_oauth) == Some(provider))
        .ok_or("MCP authentication failed: the provider sign-in was removed.")?;
    let sealed = match &mut server.transport {
        McpTransport::Remote { linear_oauth: Some(sealed), .. } if provider == Provider::Linear => sealed,
        McpTransport::Remote { supabase_oauth: Some(sealed), .. } if provider == Provider::Supabase => sealed,
        McpTransport::Remote { notion_oauth: Some(sealed), .. } if provider == Provider::Notion => sealed,
        McpTransport::Remote { paige_oauth: Some(sealed), .. } if provider == Provider::Paige => sealed,
        _ => return Err("MCP authentication failed: sign in again.".into()),
    };
    let mut grant = Grant::unseal(sealed)
        .map_err(|_| "MCP authentication failed: sign in again.")?;
    if grant.expires_soon() || rejected.is_some_and(|header| grant.bearer() == header) {
        grant = grant.refresh(provider)
            .map_err(|error| format!("MCP authentication failed: {error}"))?;
        *sealed = grant.seal()?;
        // Persist the rotated credential before sending a request with it.
        config.save()?;
    }
    Ok(grant.bearer())
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
                .redirects(0)
                .build();
        }
    }
    ureq::AgentBuilder::new().redirects(0).build()
}

fn remote_post(
    remote: &RemoteTransport,
    body: serde_json::Value,
) -> Result<(serde_json::Value, Option<String>), String> {
    if !remote.url.starts_with("https://") {
        return Err("remote MCP servers must use HTTPS".to_string());
    }
    let agent = remote_agent();
    let mut req = agent
        .post(&remote.url)
        .timeout(Duration::from_secs(30))
        .set("Content-Type", "application/json")
        .set("Accept", "application/json, text/event-stream");
    let oauth = remote.oauth_registration.as_ref()
        .map(|(provider, registration)| oauth_header(*provider, registration, None)).transpose()?;
    if let Some(h) = oauth.as_ref().or(remote.auth_header.as_ref()) {
        req = req.set("Authorization", h);
    }
    if let Some(session) = &remote.session_id {
        req = req.set("Mcp-Session-Id", session);
    }
    if let Some(version) = &remote.protocol_version {
        req = req.set("MCP-Protocol-Version", version);
    }
    // A removed OAuth registration cannot admit a queued PAIGE handoff. Hold the
    // same disk lock as removal until the admitted HTTP submission returns.
    let dispatch_fence = if let Some((Provider::Paige, registration)) = &remote.oauth_registration
        && body["method"] == "tools/call" && body["params"]["name"] == "delegate_to_subagent"
    {
        let lock = McpConfig::lock()?;
        if !McpConfig::load()?.servers.iter().any(|server|
            &server.registration == registration && server.transport.service() == Some(Service::Paige))
        {
            return Err("The PAIGE connection was removed before dispatch.".into());
        }
        Some(lock)
    } else {
        None
    };
    let sent = req.send_string(&body.to_string());
    drop(dispatch_fence);
    let resp = match sent {
        Ok(response) if (200..300).contains(&response.status()) => response,
        Err(ureq::Error::Status(401, response))
            if response.header("WWW-Authenticate").is_some_and(|value| value.contains("invalid_token"))
                && remote.oauth_registration.is_some() =>
        {
            let (provider, registration) = remote.oauth_registration.as_ref().unwrap();
            oauth_header(*provider, registration, oauth.as_deref())?;
            return Err(MCP_AUTH_RENEWED.into());
        }
        Err(ureq::Error::Status(status @ (401 | 403), _)) => {
            return Err(format!("MCP authentication failed (HTTP {status})"));
        }
        Err(ureq::Error::Status(404, _)) if remote.session_id.is_some() => {
            return Err(MCP_SESSION_EXPIRED.to_string());
        }
        Ok(response) | Err(ureq::Error::Status(_, response)) => {
            return Err(format!("MCP server returned HTTP {}", response.status()));
        }
        Err(e) => return Err(format!("MCP HTTP request failed: {e}")),
    };
    let Some(id) = body.get("id").and_then(|id| id.as_i64()) else {
        return Ok((serde_json::Value::Null, None));
    };
    let session = resp.header("Mcp-Session-Id").map(str::to_owned);
    if session.as_ref().is_some_and(|value| {
        value.is_empty() || value.len() > 4096 || !value.bytes().all(|b| b.is_ascii_graphic())
    }) {
        return Err("Invalid MCP session identifier".to_string());
    }
    let streamable = resp.content_type() == "text/event-stream";
    let reader = resp.into_reader().take(16 * 1024 * 1024);
    let message = if streamable {
        read_sse_response(BufReader::new(reader), id)?
    } else {
        serde_json::from_reader(reader).map_err(|e| format!("parse MCP response: {e}"))?
    };
    if message.get("id").and_then(|id| id.as_i64()) != Some(id) {
        return Err("MCP response ID does not match request".to_string());
    }
    Ok((parse_jsonrpc_response(message)?, session))
}

fn read_sse_response(reader: impl BufRead, id: i64) -> Result<serde_json::Value, String> {
    let mut data = String::new();
    for line in reader.lines() {
        let line = line.map_err(|e| format!("read MCP stream: {e}"))?;
        if let Some(value) = line.strip_prefix("data:") {
            data.push_str(value.strip_prefix(' ').unwrap_or(value));
            data.push('\n');
        } else if line.is_empty() && !data.is_empty() {
            if data.trim().is_empty() {
                data.clear();
                continue;
            }
            let message: serde_json::Value =
                serde_json::from_str(&data).map_err(|e| format!("parse MCP stream event: {e}"))?;
            if message.get("id").and_then(|id| id.as_i64()) == Some(id)
                && (message.get("result").is_some() || message.get("error").is_some())
            {
                return Ok(message);
            }
            data.clear();
        }
    }
    Err("MCP stream ended before the matching response".to_string())
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
    github: bool,
) -> Result<TransportHandle, String> {
    let mut cmd = if github {
        crate::github_mcp::command(&app)?
    } else {
        // Reuse terminal authority for user-configured stdio commands.
        if classify_command(command, args) == TerminalRisk::Privileged {
            return Err("privileged/destructive commands cannot be used as MCP stdio servers".into());
        }
        let resolved = crate::terminal::resolve_executable(command)?;
        let mut cmd = Command::new(resolved);
        cmd.args(args).envs(env);
        cmd
    };
    cmd.stdout(Stdio::piped())
        .stderr(if github { Stdio::null() } else { Stdio::piped() })
        .stdin(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| format!("failed to spawn MCP server: {e}"))?;
    let pid = child.id();
    let stdin = child.stdin.take().ok_or("failed to capture stdin")?;
    let stdout = child.stdout.take().ok_or("failed to capture stdout")?;
    let stderr = child.stderr.take();

    let (kill_tx, kill_rx) = std::sync::mpsc::channel::<()>();
    let app_stdout = app.clone();
    let conn_id_out = conn_id.clone();
    let stdout_reader = thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            if let Ok(line) = line {
                if let Some(state) = app_stdout.try_state::<Mutex<McpState>>() {
                    if route_incoming_message(&state, &conn_id_out, pid, &line) && !github {
                        emit_mcp_message(&app_stdout, &conn_id_out, "stdout", &line);
                    }
                }
            }
        }
    });

    let app_stderr = app.clone();
    let conn_id_err = conn_id.clone();
    let stderr_reader = stderr.map(|stderr| thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            if let Ok(line) = line {
                emit_mcp_message(&app_stderr, &conn_id_err, "stderr", &line);
                if let Some(state) = app_stderr.try_state::<Mutex<McpState>>() {
                    let mut s = state.lock().unwrap();
                    if let Some(conn) = s.connections.get_mut(&conn_id_err) {
                        if conn.state == "connecting" && uses_stdio_process(conn, pid) {
                            conn.detail = truncate(&line, 200);
                        }
                    }
                }
            }
        }
    }));

    // Process watcher: detect unexpected exit and attempt reconnect.
    let conn_id_watch = conn_id.clone();
    thread::spawn(move || {
        loop {
            if !matches!(child.try_wait(), Ok(None)) {
                break;
            }
            if !matches!(kill_rx.recv_timeout(Duration::from_millis(100)), Err(std::sync::mpsc::RecvTimeoutError::Timeout)) {
                let _ = child.kill();
                let _ = child.wait();
                break;
            }
        }
        // The thread cannot hold an AppHandle borrow across the wait, so we
        // emit a reconnect request via a fresh clone after the child exits.
        let app2 = app.clone();
        if let Some(state) = app2.try_state::<Mutex<McpState>>() {
            let mut s = state.lock().unwrap();
            if let Some(conn) = s.connections.get_mut(&conn_id_watch) {
                if uses_stdio_process(conn, pid) && matches!(conn.state.as_str(), "ready" | "connecting") {
                    conn.state = "offline".to_string();
                    conn.detail = "stdio server process exited".to_string();
                    conn.handle = None;
                    conn.last_seen_at = Some(now_secs());
                    let cancel = conn.cancel_health.clone();
                    if github {
                        for (_, pending) in conn.pending.drain() {
                            let _ = pending.tx.send(Err("The GitHub MCP server exited. Start browser sign-in again.".into()));
                        }
                    }
                    drop(s);
                    if !github {
                        schedule_reconnect(app2, conn_id_watch, cancel);
                    }
                }
            }
        }
    });

    Ok(TransportHandle::Stdio {
        pid,
        stdin,
        stdout_reader: Some(stdout_reader),
        stderr_reader,
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

fn uses_stdio_process(connection: &McpConnection, child_pid: u32) -> bool {
    matches!(&connection.handle, Some(TransportHandle::Stdio { pid, .. }) if *pid == child_pid)
}

fn route_incoming_message(state: &Mutex<McpState>, connection_id: &str, pid: u32, line: &str) -> bool {
    let mut s = state.lock().unwrap();
    let Some(conn) = s.connections.get_mut(connection_id).filter(|conn| uses_stdio_process(conn, pid)) else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else { return true };
    // Only handle responses with an id; notifications are ignored for now.
    let id = value.get("id").and_then(|i| i.as_i64());
    if let Some(id) = id {
        if let Some(pending) = conn.pending.remove(&id) {
            let mut result = parse_jsonrpc_response(value);
            if let Some(expected) = pending.github_result
                && !result.as_ref().is_ok_and(|value| expected.verifies(value))
            {
                // Authorization instructions and device codes are never tool output.
                result = Err("GitHub did not return a verified tool result. Check the connection and repository permissions.".into());
            }
            let _ = pending.tx.send(result);
        }
    }
    true
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

static NEXT_MCP_REQUEST: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(1);

fn send_notification(
    _app: &AppHandle,
    state: &Mutex<McpState>,
    connection_id: &str,
    method: &str,
    params: serde_json::Value,
    attempt: &AtomicBool,
) -> Result<(), String> {
    let notification = jsonrpc_notification(method, params);
    let mut s = state.lock().unwrap();
    let conn = s
        .connections
        .get_mut(connection_id)
        .ok_or("unknown MCP connection")?;
    if attempt.load(Ordering::Relaxed) {
        return Err("MCP connection changed during handshake".to_string());
    }
    match conn.handle.as_mut() {
        Some(TransportHandle::Stdio { stdin, .. }) => write_stdio(stdin, notification),
        Some(TransportHandle::Remote(remote)) => {
            let remote = remote.clone();
            drop(s);
            remote_post(&remote, notification).map(|_| ())
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
    attempt: &AtomicBool,
) -> Result<serde_json::Value, String> {
    send_request_bound(app, state, connection_id, method, params, timeout_secs, RequestBinding {
        attempt: Some(attempt), ..Default::default()
    })
}

#[derive(Default)]
struct RequestBinding<'a> {
    registration: Option<&'a str>,
    attempt: Option<&'a AtomicBool>,
    provider_write_approved: bool,
    github_sign_in_probe: bool,
    supabase_log_read: bool,
}

fn send_request_bound(
    app: &AppHandle,
    state: &Mutex<McpState>,
    connection_id: &str,
    method: &str,
    mut params: serde_json::Value,
    timeout_secs: u64,
    binding: RequestBinding<'_>,
) -> Result<serde_json::Value, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    let id = NEXT_MCP_REQUEST.fetch_add(1, Ordering::Relaxed);

    {
        // Keep the disk registration stable until the local GitHub request is
        // sent. Another PAIGE process may have replaced or removed it.
        let github_registration = method == "tools/call" && state.lock().unwrap().connections
            .get(connection_id).is_some_and(|conn| conn.config.transport.service() == Some(Service::GitHub));
        let github_config = if github_registration {
            Some((McpConfig::lock()?, McpConfig::load()?))
        } else {
            None
        };
        let mut s = state.lock().unwrap();
        let conn = s
            .connections
            .get_mut(connection_id)
            .ok_or("unknown MCP connection")?;
        if binding.registration.is_some_and(|id| id.is_empty() || id != conn.config.registration || conn.state != "ready") {
            return Err("The selected MCP connection changed or is unavailable.".into());
        }
        if binding.attempt.is_some_and(|cancel| cancel.load(Ordering::Relaxed)) {
            return Err("MCP connection changed before request dispatch".to_string());
        }
        if method == "tools/call" && binding.provider_write_approved
            && params["name"] == "delegate_to_subagent"
            && !conn.capabilities.tools.iter().any(|tool|
                tool.name == "delegate_to_subagent" && crate::team::accepts_model_route(&tool.input_schema))
        {
            return Err("The orchestration connection no longer supports the chat’s model route.".into());
        }
        if method == "tools/call" && let Some(service) = conn.config.transport.service() {
            if conn.state != "ready" && !(binding.github_sign_in_probe
                && service == Service::GitHub && conn.state == "connecting") {
                return Err("The browser-authorized connection is not ready.".into());
            }
            let tool = params.get("name").and_then(|name| name.as_str()).unwrap_or("");
            if service == Service::Supabase && tool == crate::supabase_logs::TOOL {
                let candidate_url = match &conn.config.transport {
                    McpTransport::Remote { url, .. } => url,
                    _ => return Err("Supabase project logs require browser sign-in.".into()),
                };
                if !binding.supabase_log_read || binding.registration.is_none() {
                    return Err("Use /supabase-logs in Ask PAIGE to read a bounded log count.".into());
                }
                let project = connected_tools::supabase_project(candidate_url)
                    .ok_or("The Supabase project scope is invalid.")?;
                if connected_tools::supabase_resource(&project).as_ref() != Some(candidate_url)
                    || !conn.capabilities.tools.iter().any(|tool| tool.name == crate::supabase_logs::TOOL) {
                    return Err("Sign in again in Models & Connections to enable Supabase logs.".into());
                }
            } else {
                let policy = service.tools().iter().find(|policy| policy.name == tool)
                    .ok_or("This provider tool is not supported by PAIGE.")?;
                if policy.writes && !binding.provider_write_approved {
                    return Err(format!("Use Ask PAIGE in Build mode to approve {} changes.", service.name()));
                }
            }
            if let McpTransport::Stdio { github_repository: Some(repository), .. } = &conn.config.transport {
                if !github_config.as_ref().is_some_and(|(_, cfg)| cfg.servers.iter().any(|saved|
                    saved.id == connection_id && saved.registration == conn.config.registration
                    && saved.transport.service() == Some(Service::GitHub)
                )) {
                    return Err("The saved GitHub sign-in changed. Open Connections to sign in again.".into());
                }
                connected_tools::bind_github_repository(&mut params["arguments"], repository)?;
            }
            if service == Service::Notion {
                let McpTransport::Remote { notion_page: Some(page), .. } = &conn.config.transport else {
                    return Err("Select a page for this Notion connection.".into());
                };
                crate::notion_lookup::bind_page(&mut params["arguments"], page)?;
            }
        }
        if conn.state != "ready" && conn.state != "connecting" && method != "initialize" {
            return Err(format!("MCP connection is {}", conn.state));
        }
        let github_result = (method == "tools/call" && conn.config.transport.service() == Some(Service::GitHub))
            .then(|| GitHubResult::for_request(&params));
        conn.pending.insert(id, PendingResponse { tx, github_result });
        let request = jsonrpc_request(id, method, params);

        match conn.handle.as_mut() {
            Some(TransportHandle::Stdio { stdin, .. }) => {
                write_stdio(stdin, request)?;
            }
            Some(TransportHandle::Remote(remote)) => {
                let remote = remote.clone();
                let initializing = method == "initialize";
                let conn_id2 = connection_id.to_string();
                let app2 = app.clone();
                thread::spawn(move || {
                    let result = remote_post(&remote, request);
                    if let Some(ts) = app2.try_state::<Mutex<McpState>>() {
                        let mut reconnect = None;
                        let mut s = ts.lock().unwrap();
                        if let Some(conn) = s.connections.get_mut(&conn_id2) {
                            if let Some(pending) = conn.pending.remove(&id) {
                                if result.as_ref().is_err_and(|err| err == MCP_SESSION_EXPIRED || err == MCP_AUTH_RENEWED)
                                    && conn.state == "ready"
                                    && matches!(&conn.handle, Some(TransportHandle::Remote(current)) if current.session_id == remote.session_id)
                                {
                                    conn.state = "offline".to_string();
                                    conn.detail = result.as_ref().unwrap_err().clone();
                                    conn.handle = None;
                                    reconnect = Some(conn.cancel_health.clone());
                                }
                                let result = result.map(|(value, session)| {
                                    if initializing {
                                        if let Some(TransportHandle::Remote(remote)) = &mut conn.handle {
                                            remote.session_id = session;
                                            remote.protocol_version = value
                                                .get("protocolVersion")
                                                .and_then(|v| v.as_str())
                                                .filter(|v| v.len() == 10 && v.bytes().all(|b| b.is_ascii_digit() || b == b'-'))
                                                .map(str::to_owned);
                                        }
                                    }
                                    value
                                });
                                let _ = pending.tx.send(result);
                            }
                        }
                        drop(s);
                        if let Some(cancel) = reconnect {
                            schedule_reconnect(app2.clone(), conn_id2, cancel);
                        }
                    }
                });
            }
            None => return Err("MCP connection has no transport".to_string()),
        }
    }

    let response = if binding.github_sign_in_probe {
        let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs);
        loop {
            if binding.attempt.is_some_and(|flag| flag.load(Ordering::Relaxed)) || std::time::Instant::now() >= deadline {
                break Err(std::sync::mpsc::RecvTimeoutError::Timeout);
            }
            match rx.recv_timeout(Duration::from_millis(150)) {
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                response => break response,
            }
        }
    } else {
        rx.recv_timeout(Duration::from_secs(timeout_secs))
    };
    match response {
        Ok(r) => r,
        Err(_) => {
            // Remove stale pending entry.
            let mut s = state.lock().unwrap();
            if let Some(conn) = s.connections.get_mut(connection_id) {
                if conn.pending.remove(&id).is_some() && conn.state == "ready" {
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
    attempt: &AtomicBool,
) -> Result<McpCapabilities, String> {
    let tools = send_request(app, state, connection_id, "tools/list", serde_json::json!({}), 30, attempt)?;
    let optional_list = |method| {
        match send_request(app, state, connection_id, method, serde_json::json!({}), 30, attempt) {
            Err(error) if error == MCP_SESSION_EXPIRED || error == MCP_AUTH_RENEWED || error.contains("authentication failed") => Err(error),
            result => Ok(result.unwrap_or(serde_json::Value::Null)),
        }
    };
    let resources = optional_list("resources/list")?;
    let prompts = optional_list("prompts/list")?;

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
    scheduled_for: Option<&AtomicBool>,
) -> Result<McpServerStatus, String> {
    connect_with_sign_in(app, state, connection_id, scheduled_for, None)
}

fn connect_with_sign_in(
    app: AppHandle,
    state: &Mutex<McpState>,
    connection_id: &str,
    scheduled_for: Option<&AtomicBool>,
    sign_in: Option<&SignIn>,
) -> Result<McpServerStatus, String> {
    let (protocol_version, cancel, service) = {
        let mut s = state.lock().unwrap();
        let conn = s
            .connections
            .get_mut(connection_id)
            .ok_or("unknown MCP connection")?;
        if scheduled_for.is_some_and(|cancel| cancel.load(Ordering::Relaxed) || conn.state != "offline") {
            return Err("MCP connection changed before scheduled reconnect".to_string());
        }
        let service = conn.config.transport.service();
        if service == Some(Service::GitHub) && sign_in.is_none() {
            return Err("Use Sign in with GitHub to open the browser.".into());
        }
        if let Some(sign_in) = sign_in {
            sign_in.check()?;
        }
        conn.state = "connecting".to_string();
        conn.detail = "starting handshake".to_string();
        conn.cancel_health.store(true, Ordering::Relaxed);
        let cancel = sign_in.map(SignIn::flag).unwrap_or_else(|| Arc::new(AtomicBool::new(false)));
        conn.cancel_health = cancel.clone();
        conn.capabilities = McpCapabilities {
            tools: Vec::new(),
            resources: Vec::new(),
            prompts: Vec::new(),
        };
        conn.server_info = None;
        conn.pending.clear();
        let protocol_version = match &conn.config.transport {
            McpTransport::Stdio { command, args, env, github_repository } => {
                let github = service == Some(Service::GitHub);
                if github_repository.is_some() && !github {
                    return Err("The saved GitHub repository is invalid.".into());
                }
                if conn.handle.is_none() {
                    let handle = spawn_stdio_server(app.clone(), connection_id.to_string(), command, args, env, github)?;
                    conn.handle = Some(handle);
                }
                if github { MCP_HTTP_PROTOCOL_VERSION } else { MCP_PROTOCOL_VERSION }
            }
            McpTransport::Remote { url, auth_header, linear_oauth, supabase_oauth, notion_oauth, paige_oauth, .. } => {
                let provider = service.and_then(Service::remote_oauth);
                if (linear_oauth.is_some() || supabase_oauth.is_some() || notion_oauth.is_some() || paige_oauth.is_some()) && provider.is_none() {
                    return Err("The saved OAuth endpoint is invalid.".into());
                }
                let oauth = provider.map(|provider| (provider, conn.config.registration.clone()));
                conn.handle = Some(TransportHandle::Remote(RemoteTransport::new(url, auth_header, oauth)));
                MCP_HTTP_PROTOCOL_VERSION
            }
        };
        (protocol_version, cancel, service)
    };

    // Initialize handshake.
    let init_params = serde_json::json!({
        "protocolVersion": protocol_version,
        "capabilities": {
            "tools": {},
            "resources": { "subscribe": false },
            "prompts": {},
            "logging": {},
        },
        "clientInfo": { "name": "paige-desktop", "version": env!("CARGO_PKG_VERSION") },
    });
    let initialize = || send_request(&app, state, connection_id, "initialize", init_params.clone(), 30, &cancel);
    let init_result = match initialize() {
        Err(error) if error == MCP_AUTH_RENEWED => initialize(),
        result => result,
    }.inspect_err(|err| mark_handshake_failure(state, connection_id, &cancel, err))?;

    let protocol_version = init_result
        .get("protocolVersion")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    if !protocol_version.starts_with("2024-11") && !protocol_version.starts_with("2025") {
        let mut s = state.lock().unwrap();
        if let Some(conn) = s.connections.get_mut(connection_id).filter(|_| !cancel.load(Ordering::Relaxed)) {
            conn.state = "unsupported".to_string();
            conn.detail = format!("server protocol version {protocol_version} is not supported");
        }
        return Err(format!("unsupported MCP protocol version {protocol_version}"));
    }

    let server_info = init_result.get("serverInfo").cloned();

    // Finish initialization before requesting capabilities.
    send_notification(
        &app,
        state,
        connection_id,
        "notifications/initialized",
        serde_json::json!({}),
        &cancel,
    ).inspect_err(|err| mark_handshake_failure(state, connection_id, &cancel, err))?;

    // Discover capabilities.
    let capabilities = discover_capabilities(&app, state, connection_id, &cancel)
        .inspect_err(|err| mark_handshake_failure(state, connection_id, &cancel, err))?;

    if let Some(service) = service {
        if !provider_read_write_ready(service, &capabilities) {
            let error = if service == Service::Notion {
                "MCP authentication failed: Notion did not expose its page read tool.".into()
            } else {
                format!("MCP authentication failed: {} did not expose both read and write tools.", service.name())
            };
            mark_handshake_failure(state, connection_id, &cancel, &error);
            return Err(error);
        }
        if service == Service::GitHub {
            let verified = send_request_bound(&app, state, connection_id, "tools/call", serde_json::json!({
                "name": "list_issues", "arguments": {"perPage": 1, "fields": ["number"]},
            }), 180, RequestBinding {
                attempt: Some(&cancel), github_sign_in_probe: true, ..Default::default()
            });
            if cancel.load(Ordering::Relaxed) {
                return Err("GitHub sign-in was cancelled.".into());
            }
            match verified {
                Ok(result) if github_probe_succeeded(&result) => {}
                Err(error) if error == "MCP tools/call timed out" => {
                    return Err("GitHub sign-in timed out before the repository check completed. Start sign-in again and finish the browser prompt.".into());
                }
                Err(error) => return Err(error),
                Ok(_) => return Err("GitHub did not return a verified issue list for the selected repository.".into()),
            }
        }
    }

    let finish = || {
        let mut s = state.lock().unwrap();
        let conn = s.connections.get_mut(connection_id)
            .filter(|conn| !cancel.load(Ordering::Relaxed) && conn.state == "connecting")
            .ok_or("MCP connection changed during handshake")?;
        // The sign-in cancel button belongs to this attempt only.
        let health = if sign_in.is_some() { Arc::new(AtomicBool::new(false)) } else { cancel.clone() };
        conn.cancel_health = health.clone();
        conn.state = "ready".to_string();
        conn.detail = format!("{} tools, {} resources, {} prompts", capabilities.tools.len(), capabilities.resources.len(), capabilities.prompts.len());
        conn.capabilities = capabilities.clone();
        conn.server_info = server_info;
        conn.last_seen_at = Some(now_secs());
        conn.reconnect_attempt = 0;
        Ok(health)
    };
    let health = if let Some(sign_in) = sign_in { sign_in.commit(finish)? } else { finish()? };

    start_health_monitor(app, connection_id.to_string(), health);

    build_status(state, connection_id)
}

fn mark_handshake_failure(state: &Mutex<McpState>, connection_id: &str, cancel: &AtomicBool, error: &str) {
    let mut s = state.lock().unwrap();
    if let Some(conn) = s.connections.get_mut(connection_id) {
        if !cancel.load(Ordering::Relaxed) && conn.state == "connecting" {
            conn.state = if error.contains("authentication failed") { "auth_needed" } else { "error" }.to_string();
            conn.detail = error.to_owned();
        }
    }
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

fn schedule_reconnect(app: AppHandle, connection_id: String, cancel: Arc<AtomicBool>) {
    thread::spawn(move || {
        let app_for_connect = app.clone();
        thread::sleep(Duration::from_secs(2));
        let (attempt, should_reconnect) = if let Some(state) = app.try_state::<Mutex<McpState>>() {
            let s = state.lock().unwrap();
            let attempt = s.connections.get(&connection_id).map(|c| c.reconnect_attempt).unwrap_or(0);
            let is_offline = s.connections.get(&connection_id)
                .is_some_and(|conn| conn.state == "offline" && !cancel.load(Ordering::Relaxed));
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
                if conn.state == "offline" && !cancel.load(Ordering::Relaxed) {
                    conn.reconnect_attempt = (attempt + 1).min(RECONNECT_BACKOFF_SECS.len() - 1);
                } else {
                    return;
                }
            }
            drop(s);
            let _ = do_connect(app_for_connect, &*state, &connection_id, Some(&cancel));
        }
    });
}

fn start_health_monitor(app: AppHandle, connection_id: String, cancel: Arc<AtomicBool>) {
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
                send_request(&app, &state, &connection_id, "ping", serde_json::json!({}), 15, &cancel)
            } else {
                Err("app state unavailable".to_string())
            };
            if let Some(state) = app2.try_state::<Mutex<McpState>>() {
                let mut s = state.lock().unwrap();
                if let Some(conn) = s.connections.get_mut(&connection_id) {
                    if cancel.load(Ordering::Relaxed) || conn.handle.is_none()
                        || (result.is_ok() && conn.state != "ready")
                    {
                        return;
                    }
                    match result {
                        Ok(_) => {
                            conn.last_seen_at = Some(now_secs());
                        }
                        Err(e) => {
                            conn.state = "offline".to_string();
                            let github = conn.config.transport.service() == Some(Service::GitHub);
                            conn.detail = if github { "Sign in to GitHub again.".into() } else { format!("health ping failed: {e}") };
                            if let Some(mut handle) = conn.handle.take() {
                                let _ = kill_stdio(&mut handle);
                            }
                            drop(s);
                            if !github {
                                schedule_reconnect(app2, connection_id, cancel);
                            }
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
        oauth_provider: conn.config.transport.service().map(Service::slug),
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
            oauth_provider: conn.config.transport.service().map(Service::slug),
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
    let lock = McpConfig::lock()?;
    let mut cfg = McpConfig::load()?;
    if cfg.servers.iter().any(|s| s.id == request.id) {
        return Err("an MCP server with this id already exists".to_string());
    }

    let mut server = McpServerConfig {
        id: request.id.clone(),
        name: request.name,
        registration: new_registration()?,
        transport: McpTransport::Stdio {
            command: request.command,
            args: request.args,
            env: request.env,
            github_repository: None,
        },
        auto_connect: request.auto_connect,
    };
    protect_server_config(&mut server)?;
    cfg.servers.push(server.clone());
    cfg.save()?;
    unprotect_server_config(&mut server)?;

    let conn = McpConnection::new(server, "not connected");
    let id = conn.config.id.clone();
    state.lock().unwrap().connections.insert(id.clone(), conn);
    drop(lock);
    if request.auto_connect {
        return do_connect(app, state.inner(), &id, None);
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
    let lock = McpConfig::lock()?;
    let mut cfg = McpConfig::load()?;
    if cfg.servers.iter().any(|s| s.id == request.id) {
        return Err("an MCP server with this id already exists".to_string());
    }

    let mut server = McpServerConfig {
        id: request.id.clone(),
        name: request.name,
        registration: new_registration()?,
        transport: McpTransport::Remote {
            url: request.url,
            auth_header: request.auth_header,
            auth_token: request.auth_token,
            linear_oauth: None,
            supabase_oauth: None,
            notion_oauth: None,
            paige_oauth: None,
            notion_page: None,
        },
        auto_connect: request.auto_connect,
    };
    protect_server_config(&mut server)?;
    cfg.servers.push(server.clone());
    cfg.save()?;
    unprotect_server_config(&mut server)?;

    let conn = McpConnection::new(server, "not connected");
    let id = conn.config.id.clone();
    state.lock().unwrap().connections.insert(id.clone(), conn);
    drop(lock);
    if request.auto_connect {
        return do_connect(app, state.inner(), &id, None);
    }
    build_status(state.inner(), &id)
}

#[tauri::command]
pub(crate) async fn mcp_paige_oauth(app: AppHandle, id: Option<String>) -> Result<McpServerStatus, String> {
    sign_in_remote(app, Service::Paige, id, None).await
}

#[tauri::command]
pub(crate) async fn mcp_linear_oauth(app: AppHandle, id: Option<String>) -> Result<McpServerStatus, String> {
    sign_in_remote(app, Service::Linear, id, None).await
}

#[tauri::command]
pub(crate) async fn mcp_supabase_oauth(
    app: AppHandle,
    id: Option<String>,
    project_ref: Option<String>,
) -> Result<McpServerStatus, String> {
    sign_in_remote(app, Service::Supabase, id, project_ref).await
}

#[tauri::command]
pub(crate) async fn mcp_notion_oauth(
    app: AppHandle,
    id: Option<String>,
    page: Option<String>,
) -> Result<McpServerStatus, String> {
    sign_in_remote(app, Service::Notion, id, page).await
}

#[tauri::command]
pub(crate) async fn mcp_github_oauth(
    app: AppHandle,
    id: Option<String>,
    repository: Option<String>,
) -> Result<McpServerStatus, String> {
    let sign_in = SignIn::begin()?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Mutex<McpState>>();
        let (existing, saved_repository) = sign_in_target(state.inner(), Service::GitHub, id)?;
        let repository = saved_repository.or(repository)
            .filter(|repository| connected_tools::github_repository(repository).is_some())
            .ok_or("Enter a GitHub repository in owner/name format.")?;
        let transport = McpTransport::Stdio {
            command: "github-mcp-server".into(),
            args: vec!["stdio".into()],
            env: HashMap::new(),
            github_repository: Some(repository),
        };
        let id = sign_in.commit(|| save_managed_connection(state.inner(), Service::GitHub, existing, transport))?;
        let flag = sign_in.flag();
        let result = connect_with_sign_in(app.clone(), state.inner(), &id, None, Some(&sign_in));
        if let Err(error) = &result {
            let mut s = state.lock().unwrap();
            if let Some(connection) = s.connections.get_mut(&id).filter(|conn| Arc::ptr_eq(&conn.cancel_health, &flag)) {
                connection.cancel_health.store(true, Ordering::Relaxed);
                connection.pending.clear();
                if let Some(mut handle) = connection.handle.take() {
                    let _ = kill_stdio(&mut handle);
                }
                connection.state = "auth_needed".into();
                connection.detail = error.clone();
            }
        }
        result
    }).await.map_err(|_| "GitHub sign-in stopped unexpectedly.")?
}

async fn sign_in_remote(
    app: AppHandle,
    service: Service,
    id: Option<String>,
    target: Option<String>,
) -> Result<McpServerStatus, String> {
    let sign_in = SignIn::begin()?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Mutex<McpState>>();
        let (existing, saved_scope) = sign_in_target(state.inner(), service, id)?;
        let notion_page = if service == Service::Notion {
            Some(saved_scope.as_deref().or(target.as_deref()).and_then(crate::notion_lookup::canonical_page_id)
                .ok_or("Paste a Notion page link or page ID.")?)
        } else {
            None
        };
        let url = match service {
            Service::Linear => provider_oauth::LINEAR_RESOURCE.into(),
            Service::Notion => provider_oauth::NOTION_RESOURCE.into(),
            Service::Paige => provider_oauth::PAIGE_RESOURCE.into(),
            Service::Supabase => {
                let project = if let Some(saved) = saved_scope.as_deref() {
                    connected_tools::supabase_project(saved)
                } else {
                    target
                };
                project.as_deref().and_then(connected_tools::supabase_resource)
                    .ok_or("Enter a valid 20-character Supabase project reference.")?
            }
            Service::GitHub => return Err("GitHub uses the local MCP server.".into()),
        };
        let provider = service.remote_oauth().ok_or("Unknown remote OAuth provider.")?;
        let grant = sign_in.run(provider)?;
        let sealed = grant.seal()?;
        let (linear_oauth, supabase_oauth, notion_oauth, paige_oauth) = match provider {
            Provider::Linear => (Some(sealed), None, None, None),
            Provider::Supabase => (None, Some(sealed), None, None),
            Provider::Notion => (None, None, Some(sealed), None),
            Provider::Paige => (None, None, None, Some(sealed)),
        };
        let transport = McpTransport::Remote {
            url, auth_header: None, auth_token: None, linear_oauth, supabase_oauth, notion_oauth, paige_oauth, notion_page,
        };
        let id = sign_in.commit(|| save_managed_connection(state.inner(), service, existing, transport))?;
        do_connect(app.clone(), state.inner(), &id, None)
    }).await.map_err(|_| "Provider sign-in stopped unexpectedly.")?
}

type ExistingRegistration = (String, String);

fn sign_in_target(
    state: &Mutex<McpState>,
    service: Service,
    id: Option<String>,
) -> Result<(Option<ExistingRegistration>, Option<String>), String> {
    let Some(id) = id else { return Ok((None, None)); };
    let s = state.lock().unwrap();
    let connection = s.connections.get(&id)
        .filter(|connection| connection.config.transport.service() == Some(service))
        .ok_or("This browser sign-in connection was removed.")?;
    let scope = match &connection.config.transport {
        McpTransport::Remote { notion_page, .. } if service == Service::Notion => notion_page.clone(),
        McpTransport::Remote { url, .. } => Some(url.clone()),
        McpTransport::Stdio { github_repository, .. } => github_repository.clone(),
    };
    Ok((Some((id, connection.config.registration.clone())), scope))
}

fn save_managed_connection(
    state: &Mutex<McpState>,
    service: Service,
    existing: Option<ExistingRegistration>,
    transport: McpTransport,
) -> Result<String, String> {
    if transport.service() != Some(service) {
        return Err("The provider connection target is invalid.".into());
    }
    let _lock = McpConfig::lock()?;
    let mut config = McpConfig::load()?;
    let id = if let Some((id, registration)) = &existing {
        if !config.servers.iter().any(|server|
            server.id == *id && server.registration == *registration && server.transport.service() == Some(service)
        ) {
            return Err("The connection changed during sign-in. Start sign-in again.".into());
        }
        id.clone()
    } else {
        if config.servers.iter().any(|server| server.transport.service() == Some(service)) {
            return Err(format!("{} already has a browser sign-in. Open Manage to reconnect it.", service.name()));
        }
        let mut id = format!("{}-oauth", service.slug());
        if config.servers.iter().any(|server| server.id == id) {
            id = format!("{id}-{}", new_registration()?);
        }
        id
    };
    let server = McpServerConfig {
        id: id.clone(),
        name: service.name().into(),
        registration: new_registration()?,
        transport,
        auto_connect: service.remote_oauth().is_some(),
    };
    config.servers.retain(|entry| entry.id != id);
    config.servers.push(server.clone());
    config.save()?;

    let mut s = state.lock().unwrap();
    if let Some(connection) = s.connections.get_mut(&id) {
        connection.cancel_health.store(true, Ordering::Relaxed);
        connection.pending.clear();
        connection.config = server;
        if let Some(mut handle) = connection.handle.take() {
            let _ = kill_stdio(&mut handle);
        }
        connection.state = "offline".into();
        connection.detail = "ready to connect".into();
    } else {
        s.connections.insert(id.clone(), McpConnection::new(server, "ready to connect"));
    }
    Ok(id)
}

#[tauri::command]
pub fn mcp_connect(
    app: AppHandle,
    state: State<'_, Mutex<McpState>>,
    id: String,
) -> Result<McpServerStatus, String> {
    set_oauth_auto_connect(state.inner(), &id, true)?;
    do_connect(app, state.inner(), &id, None)
}

#[tauri::command]
pub fn mcp_disconnect(state: State<'_, Mutex<McpState>>, id: String) -> Result<McpStatus, String> {
    set_oauth_auto_connect(state.inner(), &id, false)?;
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
    set_oauth_auto_connect(state.inner(), &id, true)?;
    let _ = do_disconnect(state.inner(), &id, true);
    do_connect(app, state.inner(), &id, None)
}

fn set_oauth_auto_connect(state: &Mutex<McpState>, id: &str, auto_connect: bool) -> Result<(), String> {
    let registration = {
        let s = state.lock().unwrap();
        s.connections.get(id).filter(|conn| conn.config.transport.service().and_then(Service::remote_oauth).is_some())
            .map(|conn| conn.config.registration.clone())
    };
    let Some(registration) = registration else { return Ok(()); };
    let _lock = McpConfig::lock()?;
    let mut cfg = McpConfig::load()?;
    let connection = cfg.servers.iter_mut().find(|server|
        server.id == id && server.registration == registration && server.transport.service().and_then(Service::remote_oauth).is_some()
    ).ok_or("The saved connection changed. Reopen PAIGE to refresh your connections.")?;
    if connection.auto_connect != auto_connect {
        connection.auto_connect = auto_connect;
        cfg.save()?;
    }
    Ok(())
}

#[tauri::command]
pub fn mcp_revoke(state: State<'_, Mutex<McpState>>, id: String) -> Result<McpStatus, String> {
    let _lock = McpConfig::lock()?;
    if !state.lock().unwrap().connections.contains_key(&id) {
        return Err("unknown MCP connection".into());
    }
    let mut cfg = McpConfig::load()?;
    cfg.servers.retain(|s| s.id != id);
    cfg.save()?;
    do_disconnect(state.inner(), &id, false)?;
    {
        let mut s = state.lock().unwrap();
        s.connections.remove(&id);
    }
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

fn provider_read_write_ready(service: Service, capabilities: &McpCapabilities) -> bool {
    let (read, write) = service.required_tools();
    let exposes_read = capabilities.tools.iter().any(|tool|
        tool.name == read || (service == Service::Linear && tool.name == "list_issues"));
    exposes_read && write.is_none_or(|required| capabilities.tools.iter().any(|tool| tool.name == required))
}

enum GitHubResult {
    IssueList,
    Issue(Option<u64>),
    Comments,
    SubIssues,
    Parent,
    Labels,
    IssueWrite,
    CommentWrite,
    Unknown,
}

impl GitHubResult {
    fn for_request(params: &serde_json::Value) -> Self {
        let arguments = &params["arguments"];
        match (params["name"].as_str(), arguments["method"].as_str()) {
            (Some("list_issues"), _) => Self::IssueList,
            (Some("issue_read"), Some("get")) => Self::Issue(arguments["issue_number"].as_u64()),
            (Some("issue_read"), Some("get_comments")) => Self::Comments,
            (Some("issue_read"), Some("get_sub_issues")) => Self::SubIssues,
            (Some("issue_read"), Some("get_parent")) => Self::Parent,
            (Some("issue_read"), Some("get_labels")) => Self::Labels,
            (Some("issue_write"), _) => Self::IssueWrite,
            (Some("add_issue_comment"), _) => Self::CommentWrite,
            _ => Self::Unknown,
        }
    }

    fn verifies(self, response: &serde_json::Value) -> bool {
        if response.get("isError").is_some_and(|value| value.as_bool() != Some(false))
            || response.get("inputRequests").is_some()
        {
            return false;
        }
        if let Some(content) = response.get("structuredContent") {
            return match self {
                Self::IssueList => content["issues"].is_array(),
                Self::Issue(Some(number)) => github_issue_matches(&content["issue"], number),
                Self::Issue(None) | Self::Unknown => false,
                _ => content.is_object(),
            };
        }

        // The pinned GitHub helper returns legacy JSON in a single text item on
        // the negotiated protocol. Login instructions and arbitrary text are
        // never successful provider results.
        let Some([item]) = response["content"].as_array().map(Vec::as_slice) else {
            return false;
        };
        if item["type"].as_str() != Some("text") {
            return false;
        }
        let Some(content) = item["text"].as_str()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok()) else {
            return false;
        };
        match self {
            Self::IssueList => content["issues"].is_array(),
            Self::Issue(Some(number)) => github_issue_matches(&content, number),
            Self::Comments | Self::SubIssues => content.is_array(),
            Self::Parent => content.get("parent").is_some_and(|parent| parent.is_null() || parent.is_object()),
            Self::Labels => content["labels"].is_array(),
            Self::IssueWrite => github_legacy_write_receipt(&content),
            Self::CommentWrite => github_legacy_write_receipt(&content)
                || (github_legacy_write_receipt(&content["comment"])
                    && github_legacy_write_receipt(&content["reaction"])),
            Self::Issue(None) | Self::Unknown => false,
        }
    }
}

fn github_issue_matches(value: &serde_json::Value, number: u64) -> bool {
    number > 0 && value["number"].as_u64() == Some(number)
        && value["title"].as_str().is_some_and(|title| !title.trim().is_empty())
        && value["state"].as_str().is_some_and(|state|
            state.eq_ignore_ascii_case("open") || state.eq_ignore_ascii_case("closed"))
}

fn github_legacy_write_receipt(value: &serde_json::Value) -> bool {
    value["id"].as_str().and_then(|id| id.parse::<u64>().ok()).is_some_and(|id| id > 0)
        && value["url"].as_str().and_then(|url| tauri::Url::parse(url).ok()).is_some_and(|url|
            url.scheme() == "https"
                && matches!(url.host_str(), Some("github.com" | "api.github.com")))
}

pub(crate) fn github_issue_summary(result: McpCallResult) -> Result<(String, String, String), String> {
    let invalid = || "GitHub did not return identifiable issue details.".to_string();
    if result.is_error {
        return Err(invalid());
    }
    let issue = if let Some(data) = result.structured_content {
        data.get("issue").cloned().ok_or_else(invalid)?
    } else {
        let text = result.content.first().and_then(|item| item["text"].as_str()).ok_or_else(invalid)?;
        serde_json::from_str::<serde_json::Value>(text).map_err(|_| invalid())?
    };
    let title = issue["title"].as_str().ok_or_else(invalid)?;
    let state = issue["state"].as_str().ok_or_else(invalid)?;
    Ok((title.split_whitespace().collect::<Vec<_>>().join(" "), state.to_ascii_lowercase(), result.connection_id))
}

fn github_probe_succeeded(value: &serde_json::Value) -> bool {
    GitHubResult::IssueList.verifies(value)
}

pub(crate) struct ProviderRoute {
    id: String,
    registration: String,
    service: Service,
    pub(crate) scope: String,
    repository: Option<String>,
    pub(crate) notion_page: Option<String>,
    pub(crate) supabase_logs_enabled: bool,
    tools: Vec<McpTool>,
}

impl ProviderRoute {
    pub(crate) fn name(&self) -> &'static str { self.service.name() }
    pub(crate) fn registration(&self) -> &str { &self.registration }

    pub(crate) fn definitions(&self) -> impl Iterator<Item = serde_json::Value> + '_ {
        self.service.tools().iter().filter_map(|policy| {
            // The explicit /notion read renders the page locally with its receipt.
            if self.service == Service::Notion { return None; }
            let tool = self.tools.iter().find(|tool| tool.name == policy.name)?;
            let description = if policy.writes {
                format!("{} Requires Build mode and native confirmation.", policy.description)
            } else {
                policy.description.into()
            };
            Some(serde_json::json!({
                "type": "function",
                "function": {
                    "name": format!("{}_{}", self.service.slug(), policy.name),
                    "description": description,
                    "parameters": tool.input_schema,
                }
            }))
        })
    }

    pub(crate) fn requires_approval(&self, name: &str) -> Result<bool, String> {
        self.service.tools().iter()
            .find(|policy| policy.name == name && self.tools.iter().any(|live| live.name == name))
            .map(|policy| policy.writes)
            .ok_or_else(|| "This tool is unavailable for the current provider connection.".into())
    }

    pub(crate) fn bind_arguments(&self, args: &mut serde_json::Value) -> Result<(), String> {
        if let Some(repository) = &self.repository {
            connected_tools::bind_github_repository(args, repository)?;
        }
        Ok(())
    }
}

pub(crate) fn provider_routes(state: &Mutex<McpState>) -> HashMap<&'static str, ProviderRoute> {
    let state = state.lock().unwrap();
    [Service::Linear, Service::Supabase, Service::GitHub, Service::Notion].into_iter().filter_map(|service| {
        let mut connections = state.connections.values()
            .filter(|conn| conn.state == "ready" && conn.config.transport.service() == Some(service));
        let connection = connections.next()?;
        if connections.next().is_some() {
            return None;
        }
        let (scope, repository) = match &connection.config.transport {
            McpTransport::Remote { url, .. } if service == Service::Supabase =>
                (format!("Project {}", connected_tools::supabase_project(url)?), None),
            McpTransport::Stdio { github_repository: Some(repository), .. } =>
                (format!("github.com/{repository}"), Some(repository.clone())),
            McpTransport::Remote { notion_page: Some(page), .. } if service == Service::Notion =>
                (crate::notion_lookup::page_url(page), None),
            _ => ("Signed-in Linear workspace".into(), None),
        };
        let notion_page = match &connection.config.transport {
            McpTransport::Remote { notion_page, .. } if service == Service::Notion => notion_page.clone(),
            _ => None,
        };
        let supabase_logs_enabled = matches!(&connection.config.transport,
            McpTransport::Remote { url, .. } if service == Service::Supabase
            && connected_tools::supabase_project(url).as_deref().and_then(connected_tools::supabase_resource).as_ref() == Some(url))
            && connection.capabilities.tools.iter().any(|tool| tool.name == crate::supabase_logs::TOOL);
        Some((service.slug(), ProviderRoute {
            id: connection.config.id.clone(),
            registration: connection.config.registration.clone(),
            service, scope, repository, notion_page, supabase_logs_enabled,
            tools: connection.capabilities.tools.clone(),
        }))
    }).collect()
}

#[tauri::command]
pub fn mcp_call_tool(
    app: AppHandle,
    state: State<'_, Mutex<McpState>>,
    request: McpCallToolRequest,
) -> Result<McpCallResult, String> {
    mcp_call_bound(app, state, request, None)
}

pub(crate) fn mcp_call_bound(
    app: AppHandle,
    state: State<'_, Mutex<McpState>>,
    request: McpCallToolRequest,
    registration: Option<&str>,
) -> Result<McpCallResult, String> {
    call_tool(app, state, request, RequestBinding { registration, ..Default::default() })
}

// Callers in team.rs own the native approval before using a PAIGE write tool.
pub(crate) fn call_approved_team_tool(
    app: AppHandle,
    request: McpCallToolRequest,
    registration: &str,
) -> Result<McpCallResult, String> {
    if request.tool != "delegate_to_subagent" {
        return Err("This tool is not a specialist action.".into());
    }
    let state = app.state::<Mutex<McpState>>();
    call_tool(app.clone(), state, request, RequestBinding {
        registration: Some(registration), provider_write_approved: true, ..Default::default()
    })
}

pub(crate) fn call_supabase_log_count(
    app: &AppHandle,
    route: &ProviderRoute,
    arguments: serde_json::Value,
) -> Result<u64, String> {
    if !route.supabase_logs_enabled {
        return Err("Sign in again in Models & Connections to enable Supabase logs.".into());
    }
    // The caller supplies the fixed query from supabase_logs. Unlike generic
    // MCP tools, raw diagnostics responses are never emitted or stored.
    let result = send_request_bound(
        app, app.state::<Mutex<McpState>>().inner(), &route.id, "tools/call",
        serde_json::json!({ "name": crate::supabase_logs::TOOL, "arguments": arguments }),
        30,
        RequestBinding { registration: Some(&route.registration), supabase_log_read: true, ..Default::default() },
    ).map_err(|error| {
        if error == "Sign in again in Models & Connections to enable Supabase logs." {
            error
        } else {
            "The Supabase log query failed. Check the connection and try again.".into()
        }
    })?;
    crate::supabase_logs::count(&result)
}

pub(crate) fn call_provider_tool(
    app: AppHandle,
    route: &ProviderRoute,
    tool: &str,
    arguments: serde_json::Value,
    write_approved: bool,
) -> Result<McpCallResult, String> {
    let state = app.state::<Mutex<McpState>>();
    call_tool(app.clone(), state, McpCallToolRequest {
        connection_id: route.id.clone(), tool: tool.into(), arguments,
    }, RequestBinding {
        registration: Some(&route.registration), provider_write_approved: write_approved,
        ..Default::default()
    })
}

fn call_tool(
    app: AppHandle,
    state: State<'_, Mutex<McpState>>,
    request: McpCallToolRequest,
    binding: RequestBinding<'_>,
) -> Result<McpCallResult, String> {
    let result = send_request_bound(
        &app,
        state.inner(),
        &request.connection_id,
        "tools/call",
        serde_json::json!({
            "name": request.tool,
            "arguments": request.arguments,
        }),
        60,
        binding,
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
        structured_content: result.ok().and_then(|mut value| value.as_object_mut()?.remove("structuredContent")),
    })
}

#[tauri::command]
pub fn mcp_receipts(state: State<'_, Mutex<McpState>>) -> Vec<McpReceipt> {
    state.lock().unwrap().global_receipts.clone()
}

// ---------------------------------------------------------------------------
// Load persisted servers on startup
// ---------------------------------------------------------------------------

fn new_registration() -> Result<String, String> {
    let mut bytes = [0u8; 16];
    crate::broker::os_random_bytes(&mut bytes)?;
    Ok(hex::encode(bytes))
}

pub(crate) fn team_registration(state: &Mutex<McpState>, id: &str) -> Result<String, String> {
    let state = state.lock().map_err(|_| "The PAIGE connection is unavailable.")?;
    let connection = state.connections.get(id).ok_or("The PAIGE connection was removed.")?;
    if connection.config.registration.is_empty() {
        return Err("Remove and reconnect this PAIGE server before using Team Chat.".into());
    }
    Ok(connection.config.registration.clone())
}

pub fn load_persisted_servers(state: &Mutex<McpState>) {
    let Ok(_lock) = McpConfig::lock() else { return; };
    let Ok(mut cfg) = McpConfig::load() else { return; };
    if cfg.servers.iter().any(|server| server.registration.is_empty()) {
        let mut upgraded = McpConfig { servers: cfg.servers.clone() };
        for server in &mut upgraded.servers {
            if server.registration.is_empty() {
                server.registration = new_registration().unwrap_or_default();
            }
        }
        if upgraded.save().is_ok() {
            cfg = upgraded;
        }
    }
    let mut s = state.lock().unwrap();
    for mut server in cfg.servers {
        let _ = unprotect_server_config(&mut server);
        let conn = McpConnection::new(server, "loaded from config; not connected");
        s.connections.insert(conn.config.id.clone(), conn);
    }
}

pub(crate) fn reconnect_saved_oauth(app: AppHandle) {
    thread::spawn(move || {
        let state = app.state::<Mutex<McpState>>();
        let pending = {
            let s = state.lock().unwrap();
            s.connections.values()
                .filter(|conn| conn.config.auto_connect && conn.config.transport.service().and_then(Service::remote_oauth).is_some())
                .map(|conn| (conn.config.id.clone(), conn.cancel_health.clone()))
                .collect::<Vec<_>>()
        };
        for (id, cancel) in pending {
            let _ = do_connect(app.clone(), state.inner(), &id, Some(&cancel));
            let _ = app.emit("mcp-status-changed", ());
        }
    });
}

// ---------------------------------------------------------------------------
// Unit tests for pure logic
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_sign_in_requires_an_actual_issue_result() {
        assert!(github_probe_succeeded(&serde_json::json!({"structuredContent":{"issues":[]}})));
        let legacy = serde_json::json!({"content":[{"type":"text","text":"{\"issues\":[{\"number\":2361}]}"}]});
        assert!(github_probe_succeeded(&legacy));
        assert!(!github_probe_succeeded(&serde_json::json!({"content":[{"type":"text","text":"authorize GitHub"}]})));
        assert!(!github_probe_succeeded(&serde_json::json!({"isError":true,"structuredContent":{"issues":[]}})));
    }

    #[test]
    fn github_legacy_results_match_the_requested_tool() {
        let text_result = |text: &str| serde_json::json!({"content":[{"type":"text","text":text}]});
        let issue = text_result(r#"{"number":2361,"title":"The issue","state":"open"}"#);
        assert!(GitHubResult::Issue(Some(2361)).verifies(&issue));
        assert!(!GitHubResult::Issue(Some(2362)).verifies(&issue));
        assert!(!GitHubResult::IssueWrite.verifies(&issue));

        let typed_issue = serde_json::json!({"structuredContent":{
            "method":"get","issue":{"number":2361,"title":"The issue","state":"open"}
        }});
        assert!(GitHubResult::Issue(Some(2361)).verifies(&typed_issue));
        assert!(!GitHubResult::Issue(Some(2362)).verifies(&typed_issue));
        assert!(!GitHubResult::Unknown.verifies(&typed_issue));

        let login = text_result(r#"{"message":"Authorize GitHub to continue"}"#);
        assert!(!GitHubResult::IssueList.verifies(&login));
        assert!(!GitHubResult::Issue(Some(2361)).verifies(&login));
        assert!(!GitHubResult::CommentWrite.verifies(&login));
        let mut error = issue.clone();
        error["isError"] = serde_json::json!(true);
        assert!(!GitHubResult::Issue(Some(2361)).verifies(&error));
        let mut mixed = issue;
        mixed["content"].as_array_mut().unwrap().push(serde_json::json!({"type":"text","text":"Sign in"}));
        assert!(!GitHubResult::Issue(Some(2361)).verifies(&mixed));

        let write = text_result(r#"{"id":"123","url":"https://github.com/owner/repo/issues/1"}"#);
        assert!(GitHubResult::IssueWrite.verifies(&write));
        let unrelated = text_result(r#"{"id":"123","url":"https://github.com.example.com/owner/repo/issues/1"}"#);
        assert!(!GitHubResult::IssueWrite.verifies(&unrelated));
    }

    #[test]
    fn linear_chat_exposes_only_discovered_issue_tools_with_write_approval() {
        let tool = |name: &str| McpTool {
            name: name.into(),
            description: None,
            input_schema: serde_json::json!({"type": "object"}),
        };
        let mut capabilities = McpCapabilities {
            tools: vec![tool("get_issue")], resources: vec![], prompts: vec![],
        };
        assert!(!provider_read_write_ready(Service::Linear, &capabilities));
        capabilities.tools.extend([tool("save_issue"), tool("delete_issue")]);
        assert!(provider_read_write_ready(Service::Linear, &capabilities));
        let route = ProviderRoute {
            id: "linear-oauth".into(), registration: "fixture".into(), service: Service::Linear,
            scope: "Linear".into(), repository: None, notion_page: None,
            supabase_logs_enabled: false, tools: capabilities.tools,
        };
        assert_eq!(route.definitions().count(), 2);
        assert!(!route.requires_approval("get_issue").unwrap());
        assert!(route.requires_approval("save_issue").unwrap());
        assert!(route.requires_approval("delete_issue").is_err());
    }

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
        // Shared with secrets::config_key_store_round_trip: the env var is
        // process-global and cargo runs lib tests in parallel.
        let _guard = crate::secrets::TEST_CONFIG_LOCK.lock().unwrap();
        unsafe {
            std::env::set_var(
                "PAIGE_TEST_CONFIG_DIR",
                std::env::temp_dir().join("paige-mcp-test").to_string_lossy().to_string(),
            );
        }
        let mut cfg = McpConfig::default();
        cfg.servers.push(McpServerConfig {
            registration: String::new(),
            id: "test".to_string(),
            name: "Test".to_string(),
            transport: McpTransport::Remote {
                url: "https://example.com/mcp".to_string(),
                auth_header: Some("Bearer secret".to_string()),
                auth_token: None,
                linear_oauth: None,
                supabase_oauth: None,
                notion_oauth: None,
                paige_oauth: None,
                notion_page: None,
            },
            auto_connect: false,
        });
        cfg.save().unwrap();
        let loaded = McpConfig::load().unwrap();
        assert_eq!(loaded.servers.len(), 1);
        assert_eq!(loaded.servers[0].id, "test");
    }
}
