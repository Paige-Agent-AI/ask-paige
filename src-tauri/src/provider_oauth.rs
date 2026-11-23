//! Desktop OAuth for pinned provider issuers.

use std::{
    io::{Read, Write},
    net::{Ipv4Addr, TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::Url;

use crate::{
    broker::os_random_bytes,
    secrets::{base64_decode, base64_encode, dpapi_protect, dpapi_unprotect},
};

pub(crate) const LINEAR_RESOURCE: &str = "https://mcp.linear.app/mcp";
pub(crate) const SUPABASE_RESOURCE: &str = "https://mcp.supabase.com/mcp";
pub(crate) const NOTION_RESOURCE: &str = "https://mcp.notion.com/mcp";
pub(crate) const PAIGE_RESOURCE: &str = "https://xygzykjyynhzqytbqnzu.supabase.co/functions/v1/paige-mcp";
const CALLBACK: &str = "/oauth/paige";
const MAX_BODY: u64 = 64 * 1024;
const TIMEOUT: Duration = Duration::from_secs(150);

static SIGN_IN: Mutex<Option<Arc<AtomicBool>>> = Mutex::new(None);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Provider {
    Linear,
    Supabase,
    Notion,
    Paige,
}

struct Profile {
    name: &'static str,
    issuer: &'static str,
    authorize: &'static str,
    token: &'static str,
    register: &'static str,
    scopes: &'static str,
    token_auth: &'static str,
    resource: &'static str,
}

impl Provider {
    fn profile(self) -> Profile {
        match self {
            Self::Paige => Profile {
                name: "PAIGE Orchestrator",
                issuer: "https://paigeagent.ai",
                authorize: "https://paigeagent.ai/mcp/authorize",
                token: "https://xygzykjyynhzqytbqnzu.supabase.co/functions/v1/paige-mcp/oauth/token",
                register: "https://xygzykjyynhzqytbqnzu.supabase.co/functions/v1/paige-mcp/oauth/register",
                scopes: "crm.read workflows.run",
                token_auth: "none",
                resource: PAIGE_RESOURCE,
            },
            Self::Notion => Profile {
                name: "Notion",
                issuer: "https://mcp.notion.com",
                authorize: "https://mcp.notion.com/authorize",
                token: "https://mcp.notion.com/token",
                register: "https://mcp.notion.com/register",
                scopes: "default",
                token_auth: "none",
                resource: NOTION_RESOURCE,
            },
            Self::Linear => Profile {
                name: "Linear",
                issuer: "https://mcp.linear.app",
                authorize: "https://mcp.linear.app/authorize",
                token: "https://mcp.linear.app/token",
                register: "https://mcp.linear.app/register",
                scopes: "read write",
                token_auth: "none",
                resource: LINEAR_RESOURCE,
            },
            Self::Supabase => Profile {
                name: "Supabase",
                issuer: "https://api.supabase.com",
                authorize: "https://api.supabase.com/v1/oauth/authorize",
                token: "https://api.supabase.com/v1/oauth/token",
                register: "https://api.supabase.com/platform/oauth/apps/register",
                scopes: SUPABASE_ANALYTICS_SCOPES,
                token_auth: "client_secret_post",
                resource: SUPABASE_RESOURCE,
            },
        }
    }
}

pub(crate) struct SignIn {
    cancelled: Arc<AtomicBool>,
}

impl SignIn {
    pub(crate) fn flag(&self) -> Arc<AtomicBool> {
        self.cancelled.clone()
    }
    pub(crate) fn begin() -> Result<Self, String> {
        let mut active = SIGN_IN.lock().unwrap();
        if active.is_some() {
            return Err("A provider sign-in is already open.".into());
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        *active = Some(cancelled.clone());
        Ok(Self { cancelled })
    }

    pub(crate) fn check(&self) -> Result<(), String> {
        if self.cancelled.load(Ordering::Relaxed) {
            return Err("Sign-in was cancelled.".into());
        }
        Ok(())
    }

    pub(crate) fn commit<T>(&self, save: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        let _active = SIGN_IN.lock().unwrap();
        self.check()?;
        save()
    }

    pub(crate) fn run(&self, provider: Provider) -> Result<Grant, String> {
        let profile = provider.profile();
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .map_err(|_| "Could not open the local sign-in callback.")?;
        listener
            .set_nonblocking(true)
            .map_err(|_| "Could not prepare the local sign-in callback.")?;
        let address = listener
            .local_addr()
            .map_err(|_| "Could not read the local sign-in address.")?
            .to_string();
        let redirect = format!("http://{address}{CALLBACK}");
        let registration = register(&profile, &redirect)?;
        let state = random_value()?;
        let verifier = random_value()?;
        let challenge = base64_url(&Sha256::digest(verifier.as_bytes()));
        let authorize = authorization_url(
            &profile,
            &registration.client_id,
            &redirect,
            &state,
            &challenge,
        );
        self.check()?;
        open_browser(authorize.as_str())?;

        let deadline = Instant::now() + TIMEOUT;
        while Instant::now() < deadline {
            self.check()?;
            match listener.accept() {
                Ok((mut stream, _)) => {
                    match read_callback(&mut stream, &address, &state, profile.issuer) {
                        Callback::Invalid => browser_reply(
                            &mut stream,
                            "400 Bad Request",
                            "This sign-in callback is invalid.",
                        ),
                        Callback::Denied => {
                            browser_reply(
                                &mut stream,
                                "200 OK",
                                "Sign-in was declined. Return to PAIGE Desktop.",
                            );
                            return Err(format!("{} sign-in was declined.", profile.name));
                        }
                        Callback::Code(code) => {
                            self.check()?;
                            // Answer the browser while the code exchange completes privately.
                            browser_reply(
                                &mut stream,
                                "200 OK",
                                "Return to PAIGE Desktop to check your connection.",
                            );
                            drop(stream);
                            drop(listener);
                            let response = token_request(
                                &profile,
                                registration.client_secret.as_deref(),
                                &[
                                    ("grant_type", "authorization_code"),
                                    ("client_id", &registration.client_id),
                                    ("code", &code),
                                    ("code_verifier", &verifier),
                                    ("redirect_uri", &redirect),
                                    ("resource", profile.resource),
                                ],
                            )?;
                            self.check()?;
                            return Grant::from_response(
                                &profile,
                                registration.client_id,
                                registration.client_secret,
                                response,
                                None,
                            );
                        }
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(80));
                }
                Err(_) => return Err("The local sign-in callback stopped.".into()),
            }
        }
        Err(format!(
            "{} sign-in timed out. Start sign-in again.",
            profile.name
        ))
    }
}

impl Drop for SignIn {
    fn drop(&mut self) {
        *SIGN_IN.lock().unwrap() = None;
    }
}

#[tauri::command]
pub(crate) fn mcp_oauth_cancel() {
    if let Some(cancelled) = SIGN_IN.lock().unwrap().as_ref() {
        cancelled.store(true, Ordering::Relaxed);
    }
}

#[derive(Deserialize)]
struct Registration {
    client_id: String,
    client_secret: Option<String>,
    token_endpoint_auth_method: Option<String>,
}

fn register(profile: &Profile, redirect: &str) -> Result<Registration, String> {
    let metadata = serde_json::json!({
        "client_name": "PAIGE Desktop",
        "redirect_uris": [redirect],
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"],
        "token_endpoint_auth_method": profile.token_auth,
        "scope": profile.scopes,
    });
    let response = agent()
        .post(profile.register)
        .set("Content-Type", "application/json")
        .send_string(&metadata.to_string());
    let registration: Registration = read_response(response, profile.name, "registration")?;
    if !valid_value(&registration.client_id)
        || registration
            .token_endpoint_auth_method
            .as_deref()
            .is_some_and(|method| method != profile.token_auth)
        || (profile.token_auth == "client_secret_post"
            && !registration
                .client_secret
                .as_deref()
                .is_some_and(valid_value))
    {
        return Err(format!(
            "{} returned an incompatible OAuth client registration.",
            profile.name
        ));
    }
    Ok(registration)
}

fn authorization_url(
    profile: &Profile,
    client: &str,
    redirect: &str,
    state: &str,
    challenge: &str,
) -> Url {
    let mut url = Url::parse(profile.authorize).expect("valid provider OAuth URL");
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", client)
        .append_pair("redirect_uri", redirect)
        .append_pair("state", state)
        .append_pair("code_challenge", challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("scope", profile.scopes)
        .append_pair("resource", profile.resource);
    url
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .redirects(0)
        .timeout(Duration::from_secs(12))
        .build()
}

fn read_response<T: serde::de::DeserializeOwned>(
    response: Result<ureq::Response, ureq::Error>,
    provider: &str,
    step: &str,
) -> Result<T, String> {
    let response = match response {
        Ok(response) if (200..300).contains(&response.status()) => response,
        Ok(response) | Err(ureq::Error::Status(_, response)) => {
            return Err(format!(
                "{provider} OAuth {step} failed (HTTP {}).",
                response.status()
            ));
        }
        Err(_) => return Err(format!("Could not reach {provider} for OAuth {step}.")),
    };
    serde_json::from_reader(response.into_reader().take(MAX_BODY))
        .map_err(|_| format!("{provider} returned an invalid OAuth {step} response."))
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    token_type: String,
    expires_in: u64,
    scope: Option<String>,
}

fn token_request(
    profile: &Profile,
    secret: Option<&str>,
    parameters: &[(&str, &str)],
) -> Result<TokenResponse, String> {
    let mut form = parameters.to_vec();
    if profile.token_auth == "client_secret_post" {
        form.push((
            "client_secret",
            secret.ok_or("The saved OAuth client registration is incomplete.")?,
        ));
    }
    read_response(
        agent().post(profile.token).send_form(&form),
        profile.name,
        "token exchange",
    )
}

#[derive(Serialize, Deserialize)]
pub(crate) struct Grant {
    client_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    client_secret: Option<String>,
    access_token: String,
    refresh_token: String,
    expires_at: u64,
    #[serde(default)]
    supabase_analytics: bool,
}

const SUPABASE_DATABASE_SCOPES: &str = "projects:read database:read database:write";
const SUPABASE_ANALYTICS_SCOPES: &str = "projects:read database:read database:write analytics:read";

impl Grant {
    fn from_response(
        profile: &Profile,
        client_id: String,
        client_secret: Option<String>,
        response: TokenResponse,
        previous_refresh: Option<&str>,
    ) -> Result<Self, String> {
        if response
            .scope
            .as_deref()
            .is_some_and(|scope| !has_scopes(scope, profile.scopes))
        {
            return Err(format!(
                "{} did not grant the requested access.",
                profile.name
            ));
        }
        let refresh_token = response
            .refresh_token
            .or_else(|| previous_refresh.map(str::to_owned));
        if !response.token_type.eq_ignore_ascii_case("Bearer")
            || !valid_value(&response.access_token)
            || response.expires_in == 0
            || !refresh_token.as_deref().is_some_and(valid_value)
        {
            return Err(format!(
                "{} returned an incomplete OAuth grant. Sign in again.",
                profile.name
            ));
        }
        Ok(Self {
            client_id,
            client_secret,
            access_token: response.access_token,
            refresh_token: refresh_token.unwrap(),
            expires_at: now().saturating_add(response.expires_in),
            supabase_analytics: profile.scopes == SUPABASE_ANALYTICS_SCOPES,
        })
    }

    pub(crate) fn expires_soon(&self) -> bool {
        now().saturating_add(60) >= self.expires_at
    }

    pub(crate) fn refresh(&self, provider: Provider) -> Result<Self, String> {
        let mut profile = provider.profile();
        if provider == Provider::Supabase && !self.supabase_analytics {
            profile.scopes = SUPABASE_DATABASE_SCOPES;
        }
        let response = token_request(
            &profile,
            self.client_secret.as_deref(),
            &[
                ("grant_type", "refresh_token"),
                ("client_id", &self.client_id),
                ("refresh_token", &self.refresh_token),
                ("resource", profile.resource),
            ],
        )?;
        Self::from_response(
            &profile,
            self.client_id.clone(),
            self.client_secret.clone(),
            response,
            Some(&self.refresh_token),
        )
    }

    pub(crate) fn bearer(&self) -> String {
        format!("Bearer {}", self.access_token)
    }

    pub(crate) fn seal(&self) -> Result<String, String> {
        let data = serde_json::to_vec(self).map_err(|_| "Could not encode the saved sign-in.")?;
        Ok(base64_encode(&dpapi_protect(&data)?))
    }

    pub(crate) fn unseal(data: &str) -> Result<Self, String> {
        let grant: Self = serde_json::from_slice(&dpapi_unprotect(&base64_decode(data)?)?)
            .map_err(|_| "Could not read the saved sign-in.")?;
        if !valid_value(&grant.access_token)
            || !valid_value(&grant.refresh_token)
            || !valid_value(&grant.client_id)
        {
            return Err("The saved sign-in is invalid.".into());
        }
        Ok(grant)
    }
}

fn has_scopes(scope: &str, required: &str) -> bool {
    let scopes: Vec<_> = scope
        .split(|c: char| c.is_whitespace() || c == ',')
        .collect();
    required
        .split_ascii_whitespace()
        .all(|scope| scopes.contains(&scope))
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn valid_value(value: &str) -> bool {
    !value.is_empty() && value.len() <= 8192 && value.bytes().all(|byte| byte.is_ascii_graphic())
}

fn base64_url(bytes: &[u8]) -> String {
    base64_encode(bytes)
        .trim_end_matches('=')
        .replace('+', "-")
        .replace('/', "_")
}

fn random_value() -> Result<String, String> {
    let mut bytes = [0; 32];
    os_random_bytes(&mut bytes)?;
    Ok(base64_url(&bytes))
}

enum Callback {
    Invalid,
    Denied,
    Code(String),
}

fn read_callback(stream: &mut TcpStream, address: &str, expected: &str, issuer: &str) -> Callback {
    let timeout = Some(Duration::from_secs(1));
    if stream.set_read_timeout(timeout).is_err() || stream.set_write_timeout(timeout).is_err() {
        return Callback::Invalid;
    }
    // Bound the total header read even if the peer sends many small lines.
    let Ok(Some(bytes)) = read_headers(stream) else {
        return Callback::Invalid;
    };
    let Ok(headers) = std::str::from_utf8(&bytes) else {
        return Callback::Invalid;
    };
    parse_callback(headers, address, expected, issuer)
}

fn read_headers(stream: &TcpStream) -> std::io::Result<Option<Vec<u8>>> {
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut reader = stream;
    let mut bytes = Vec::new();
    let mut chunk = [0; 1024];
    while Instant::now() < deadline && bytes.len() < 16 * 1024 {
        let size = reader.read(&mut chunk)?;
        if size == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..size]);
        if let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            bytes.truncate(end + 4);
            return Ok(Some(bytes));
        }
    }
    Ok(None)
}

fn parse_callback(headers: &str, address: &str, expected: &str, expected_issuer: &str) -> Callback {
    let mut lines = headers.lines();
    let mut request = lines.next().unwrap_or("").split_ascii_whitespace();
    let (Some("GET"), Some(target), Some("HTTP/1.1"), None) = (
        request.next(),
        request.next(),
        request.next(),
        request.next(),
    ) else {
        return Callback::Invalid;
    };
    let hosts: Vec<_> = lines
        .filter_map(|line| line.split_once(':'))
        .filter_map(|(name, value)| name.eq_ignore_ascii_case("host").then_some(value.trim()))
        .collect();
    if hosts != [address] || !target.starts_with(&format!("{CALLBACK}?")) {
        return Callback::Invalid;
    }
    let Ok(url) = Url::parse(&format!("http://{address}{target}")) else {
        return Callback::Invalid;
    };
    if url.path() != CALLBACK || url.fragment().is_some() {
        return Callback::Invalid;
    }
    let mut state = None;
    let mut code = None;
    let mut error = None;
    let mut issuer = None;
    for (key, value) in url.query_pairs() {
        let slot = match key.as_ref() {
            "state" => &mut state,
            "code" => &mut code,
            "error" => &mut error,
            "iss" => &mut issuer,
            _ => continue,
        };
        if slot.replace(value.into_owned()).is_some() {
            return Callback::Invalid;
        }
    }
    if state.as_deref() != Some(expected)
        || issuer
            .as_deref()
            .is_some_and(|value| value != expected_issuer)
    {
        return Callback::Invalid;
    }
    match (code, error) {
        (Some(code), None) if valid_value(&code) => Callback::Code(code),
        (None, Some(_)) => Callback::Denied,
        _ => Callback::Invalid,
    }
}

fn browser_reply(stream: &mut TcpStream, status: &str, body: &str) {
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nContent-Security-Policy: default-src 'none'\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}

#[link(name = "shell32")]
unsafe extern "system" {
    fn ShellExecuteW(
        window: *mut std::ffi::c_void,
        operation: *const u16,
        file: *const u16,
        parameters: *const u16,
        directory: *const u16,
        show: i32,
    ) -> isize;
}

#[tauri::command]
pub fn mcp_paige_operator_login() -> Result<(), String> {
    open_browser("https://paigeagent.ai/operator")
}

fn open_browser(url: &str) -> Result<(), String> {
    let target: Vec<_> = url.encode_utf16().chain([0]).collect();
    let open: Vec<_> = "open".encode_utf16().chain([0]).collect();
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            open.as_ptr(),
            target.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
        )
    };
    if result <= 32 {
        return Err("Windows could not open the default browser for sign-in.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_request_binds_resource_scope_and_pkce() {
        let url = authorization_url(
            &Provider::Linear.profile(),
            "client",
            "http://127.0.0.1:3000/oauth/paige",
            "state",
            "challenge",
        );
        let query: std::collections::HashMap<_, _> = url.query_pairs().collect();
        assert_eq!(query["scope"], "read write");
        assert_eq!(query["resource"], LINEAR_RESOURCE);
        assert_eq!(query["code_challenge_method"], "S256");
        assert_eq!(query["state"], "state");
        assert_eq!(base64_url(&[255, 255]), "__8");
    }

    #[test]
    fn only_matching_loopback_callback_completes_sign_in() {
        let host = "127.0.0.1:3000";
        let parse = |target, request_host| {
            parse_callback(
                &format!("GET {target} HTTP/1.1\r\nHost: {request_host}\r\n\r\n"),
                host,
                "secret",
                Provider::Linear.profile().issuer,
            )
        };
        assert!(
            matches!(parse("/oauth/paige?state=secret&code=one", host), Callback::Code(code) if code == "one")
        );
        assert!(matches!(
            parse("/oauth/paige?state=secret&error=denied", host),
            Callback::Denied
        ));
        for target in [
            "/oauth/paige?state=wrong&code=one",
            "/oauth/paige?state=secret&state=other&code=one",
            "/oauth/paige?state=secret&code=one&code=two",
            "/oauth/paige?state=secret&code=one&iss=https%3A%2F%2Fexample.com",
            "/wrong?state=secret&code=one",
        ] {
            assert!(matches!(parse(target, host), Callback::Invalid));
        }
        assert!(matches!(
            parse("/oauth/paige?state=secret&code=one", "localhost:3000"),
            Callback::Invalid
        ));
    }

    #[test]
    fn grant_requires_both_scopes_and_seals_credentials() {
        let profile = Provider::Linear.profile();
        assert!(!has_scopes("read openid", profile.scopes));
        assert!(has_scopes("read,write", profile.scopes));
        let response = |scope: &str| TokenResponse {
            access_token: "fixture-access".into(),
            refresh_token: Some("fixture-refresh".into()),
            token_type: "Bearer".into(),
            expires_in: 3600,
            scope: Some(scope.into()),
        };
        assert!(
            Grant::from_response(&profile, "client".into(), None, response("read"), None).is_err()
        );
        let grant = Grant::from_response(
            &profile,
            "client".into(),
            None,
            response(profile.scopes),
            None,
        )
        .unwrap();
        let sealed = grant.seal().unwrap();
        assert!(!sealed.contains("fixture"));
        assert_eq!(
            Grant::unseal(&sealed).unwrap().bearer(),
            "Bearer fixture-access"
        );
    }

    #[test]
    fn paige_connector_pins_issuer_and_requests_specialist_scopes() {
        let profile = Provider::Paige.profile();
        assert_eq!(profile.issuer, "https://paigeagent.ai");
        assert_eq!(profile.authorize, "https://paigeagent.ai/mcp/authorize");
        assert_eq!(profile.resource, PAIGE_RESOURCE);
        assert_eq!(profile.token, format!("{PAIGE_RESOURCE}/oauth/token"));
        assert_eq!(profile.register, format!("{PAIGE_RESOURCE}/oauth/register"));
        assert_eq!(profile.scopes, "crm.read workflows.run");
    }

    #[test]
    fn supabase_uses_database_scopes_and_pinned_issuer() {
        let profile = Provider::Supabase.profile();
        let url = authorization_url(
            &profile,
            "client",
            "http://127.0.0.1:3000/oauth/paige",
            "state",
            "challenge",
        );
        let query: std::collections::HashMap<_, _> = url.query_pairs().collect();
        assert_eq!(
            url.origin().ascii_serialization(),
            "https://api.supabase.com"
        );
        assert_eq!(query["resource"], SUPABASE_RESOURCE);
        assert_eq!(query["scope"], SUPABASE_ANALYTICS_SCOPES);
        assert!(!has_scopes(SUPABASE_DATABASE_SCOPES, profile.scopes));
        assert!(!has_scopes("projects:read database:read", profile.scopes));
    }
}
