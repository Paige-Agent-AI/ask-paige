use crate::ask::{AskReceipt, default_actor, now_ms, publish_receipt};
use crate::chats::ChatMessage;
use crate::gateway::GatewayId;
use crate::secrets::GatewayConfig;
use serde::Serialize;
use serde_json::{Value, json};
use std::{io::Read, sync::atomic::{AtomicBool, Ordering}, time::{Duration, Instant}};

pub(crate) const ROUTE: &str = "omniroute-web";
const FREE_PROVIDER: &str = "duckduckgo-free";
const MAX_RESULTS: usize = 5;

pub(crate) fn command(message: &str) -> Result<Option<&str>, String> {
    let message = message.trim();
    let end = message.find(char::is_whitespace).unwrap_or(message.len());
    if !message[..end].eq_ignore_ascii_case("/web") { return Ok(None); }
    let query = message[end..].trim();
    if query.is_empty() || query.chars().count() > 200 || query.chars().any(char::is_control) {
        return Err("Use /web followed by one public search query, up to 200 characters.".into());
    }
    Ok(Some(query))
}

fn request_body(query: &str) -> Value {
    json!({"provider":FREE_PROVIDER, "search_type":"web", "max_results":MAX_RESULTS, "query":query})
}

#[derive(Serialize)]
struct SearchResult {
    title: String,
    url: String,
    snippet: String,
}

fn display_text(text: &str, limit: usize) -> String {
    text.chars().filter(|ch| !ch.is_control()).take(limit).collect()
}

fn parse(value: Value) -> Result<Vec<SearchResult>, String> {
    if value.get("error").is_some_and(|error| !error.is_null())
        || value["provider"].as_str() != Some(FREE_PROVIDER)
        || value["usage"]["search_cost_usd"].as_f64() != Some(0.0) {
        return Err("OmniRoute did not return a verified free search result.".into());
    }
    let results = value["results"].as_array().ok_or("OmniRoute did not return a search result list.")?;
    let mut seen = std::collections::HashSet::new();
    Ok(results.iter().filter_map(|item| {
        let address = item["url"].as_str()?;
        if address.len() > 2_048 || address.chars().any(char::is_control) { return None; }
        let url = tauri::Url::parse(address).ok()?;
        let host = url.domain()?;
        if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some()
            || !host.contains('.') || host.ends_with(".local") || host.ends_with(".localhost")
            || !seen.insert(url.to_string()) { return None; }
        let title = display_text(item["title"].as_str()?, 200);
        if title.trim().is_empty() { return None; }
        Some(SearchResult { title, url: url.to_string(), snippet: display_text(item["snippet"].as_str().unwrap_or_default(), 500) })
    }).take(MAX_RESULTS).collect())
}

fn search(query: &str) -> Result<Vec<SearchResult>, String> {
    let key = GatewayConfig::load().omniroute_key()
        .map_err(|_| "PAIGE could not read its OmniRoute connection. Check Models & Connections.")?;
    let agent = ureq::AgentBuilder::new().redirects(0).timeout(Duration::from_secs(22)).build();
    let url = format!("{}/search", GatewayId::Omniroute.base_url());
    let mut req = agent.post(&url).set("Content-Type","application/json");
    if let Some(key) = &key { req = req.set("Authorization", &format!("Bearer {key}")); }
    let response = match req.send_string(&request_body(query).to_string()) {
        Ok(response) if response.status() == 200 => response,
        Err(ureq::Error::Status(401 | 403, _)) => return Err("OmniRoute refused web search for this connection. Check its key and search permissions in Models & Connections.".into()),
        Err(ureq::Error::Status(status, _)) => return Err(format!("OmniRoute’s free web search returned HTTP {status}.")),
        _ => return Err("PAIGE could not reach OmniRoute’s free web search.".into()),
    };
    let mut bytes = Vec::new();
    response.into_reader().take(128_001).read_to_end(&mut bytes)
        .map_err(|_| "PAIGE could not read the web search response.")?;
    if bytes.len() > 128_000 { return Err("The web search response exceeded the result limit.".into()); }
    parse(serde_json::from_slice(&bytes).map_err(|_| "OmniRoute returned invalid search data.")?)
}

pub(crate) fn run(
    app: &tauri::AppHandle, chat_id: Option<&str>, turn_id: &str, query: &str, cancel: &AtomicBool,
) -> Result<Value, String> {
    if cancel.load(Ordering::Relaxed) { return Err("cancelled".into()); }
    let clock = Instant::now();
    let result = search(query);
    if cancel.load(Ordering::Relaxed) { return Err("cancelled".into()); }
    let elapsed_ms = clock.elapsed().as_millis();
    let (text, details, outcome) = match result {
        Ok(results) => {
            let text = match results.len() {
                0 => "I searched the public web and didn’t find any matching results.".into(),
                1 => "I found one result for your web search.".into(),
                count => format!("I found {count} results for your web search."),
            };
            (text, json!({"provider":FREE_PROVIDER,"elapsed_ms":elapsed_ms,"results":results}), "ok")
        }
        Err(error) => (error.clone(), json!({"requested_provider":FREE_PROVIDER,"elapsed_ms":elapsed_ms,"error":error}), "error"),
    };
    publish_receipt(app, chat_id, AskReceipt {
        timestamp: now_ms(), turn_id: turn_id.into(), kind: "tool".into(), tool: "web_search".into(),
        gateway: ROUTE.into(), model: String::new(), summary: text.clone(),
        outcome: outcome.into(), details: details.to_string(), actor: default_actor(),
    }, None)?;
    publish_receipt(app, chat_id, AskReceipt {
        timestamp: now_ms(), turn_id: turn_id.into(), kind: "connector_turn".into(), tool: "final_answer".into(),
        gateway: ROUTE.into(), model: String::new(), summary: text.clone(), outcome: "complete".into(),
        details: String::new(), actor: default_actor(),
    }, Some(ChatMessage {
        role:"assistant".into(), text:text.clone(), tool_events:vec![], model_label:None, model:None,
        gateway:Some(ROUTE.into()), turn_id:Some(turn_id.into()),
    }))?;
    Ok(json!({"turn_id":turn_id,"status":"complete","text":text}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_explicit_single_line_search_uses_a_fixed_free_provider() {
        assert_eq!(command("look for /web example").unwrap(), None);
        assert_eq!(command("/web site:doc.rust-lang.org cargo test").unwrap(), Some("site:doc.rust-lang.org cargo test"));
        assert!(command("/web x\nprivate project context").is_err());
        assert!(command("/web").is_err());
        let wire = request_body("public query");
        assert_eq!(wire["provider"], FREE_PROVIDER);
        assert_eq!(wire["query"], "public query");
        assert_eq!(wire.as_object().unwrap().len(), 4);
    }

    #[test]
    fn search_result_urls_are_bounded_and_passive() {
        let out = parse(json!({"provider":FREE_PROVIDER,"usage":{"search_cost_usd":0},"results":[
            {"title":"Book","url":"https://doc.rust-lang.org/book/","snippet":"Plain text"},
            {"title":"Duplicate","url":"https://doc.rust-lang.org/book/"},
            {"title":"Local","url":"https://127.0.0.1/page"},
            {"title":"Script","url":"javascript:alert(1)"},
            {"title":"Userinfo","url":"https://test@doc.rust-lang.org/book/"}
        ]})).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].title, "Book");
        assert!(parse(json!({"provider":"paid","usage":{"search_cost_usd":0},"results":[]})).is_err());
        assert!(parse(json!({"usage":{"search_cost_usd":0},"results":[]})).is_err());
        assert!(parse(json!({"provider":FREE_PROVIDER,"usage":{"search_cost_usd":0.02},"results":[]})).is_err());
    }
}
