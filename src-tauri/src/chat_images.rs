//! Verified PNG results for ordinary conversations.
use crate::ask::{AskReceipt, default_actor, now_ms, publish_receipt};
use crate::chats::{ChatMessage, ChatsStore};
use crate::secrets::GatewayConfig;
use crate::visual_qa::{self, Screenshot};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::AppHandle;

pub(crate) const ROUTE: &str = "chat-images";
pub(crate) const RECEIPT: &str = "chat_image";
const MODEL: &str = "openai/gpt-image-2";
const RESPONSE_LIMIT: usize = 8 * 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Request {
    Open(Option<String>),
    Capture,
    Generate(String),
}

impl Request {
    pub(crate) fn status(&self) -> &'static str {
        match self {
            Self::Open(_) => "Opening a project PNG…",
            Self::Capture => "Waiting for window capture approval…",
            Self::Generate(_) => "Preparing an image generation request…",
        }
    }
}

fn command_tail<'a>(text: &'a str, lower: &str, prefix: &str) -> Option<&'a str> {
    let tail = lower.strip_prefix(prefix)?;
    (tail.is_empty() || tail.starts_with(char::is_whitespace))
        .then(|| text[prefix.len()..].trim())
}

fn local_path(path: &str) -> Result<String, String> {
    visual_qa::screenshot_path(path.trim().trim_matches(['"', '\'', '`', '“', '”']))
}

pub(crate) fn recognize(text: &str) -> Result<Option<Request>, String> {
    let mut text = text.trim();
    for group in [["paige, ", "hey paige, ", "paige: "].as_slice(),
        ["could you ", "can you ", "would you "].as_slice(), ["please "].as_slice()] {
        let lower = text.to_ascii_lowercase();
        if let Some(prefix) = group.iter().find(|prefix| lower.starts_with(**prefix)) {
            text = text[prefix.len()..].trim_start();
        }
    }
    let lower = text.to_ascii_lowercase();
    if let Some(path) = command_tail(text, &lower, "/show-image") {
        return Ok(Some(Request::Open(if path.is_empty() { None } else { Some(local_path(path)?) })));
    }
    if let Some(prompt) = command_tail(text, &lower, "/create-image") {
        return prompt_request(prompt).map(Some);
    }
    if lower == "/screenshot" { return Ok(Some(Request::Capture)); }

    let sentence = text.trim_end_matches(['.', '?', '!']).trim_end();
    let lower = sentence.to_ascii_lowercase();
    if matches!(lower.as_str(), "take a screenshot" | "take a screenshot of paige"
        | "take a screenshot of this window" | "take a screenshot of your window"
        | "take a screenshot of paige desktop" | "capture this paige desktop window"
        | "capture this window" | "capture your window" | "capture your screen"
        | "show me a screenshot of this window" | "show me a screenshot of paige desktop") {
        return Ok(Some(Request::Capture));
    }
    for prefix in ["create an image of ", "generate an image of ", "make an image of ",
        "create a png of ", "generate a png of ", "create a visual mockup of ",
        "generate a visual mockup of ", "create an image: ", "generate an image: "] {
        if lower.starts_with(prefix) { return prompt_request(sentence[prefix.len()..].trim()).map(Some); }
    }
    for prefix in ["show me ", "show ", "display ", "preview ", "open ", "attach "] {
        if !lower.starts_with(prefix) { continue; }
        let target = sentence[prefix.len()..].trim();
        let lower = target.to_ascii_lowercase();
        if matches!(lower.as_str(), "a png" | "a png file" | "the png" | "the png file"
            | "the png you found" | "the png file you found" | "the png file here in the chat") {
            return Ok(Some(Request::Open(None)));
        }
        let path = lower.strip_prefix("the image ").or_else(|| lower.strip_prefix("the file "))
            .map(|tail| &target[target.len() - tail.len()..]).unwrap_or(target);
        let path = path.trim_matches(['"', '\'', '`', '“', '”']);
        if path.to_ascii_lowercase().ends_with(".png") && !path.contains(['\n', '\r']) {
            return Ok(Some(Request::Open(Some(local_path(path)?))));
        }
    }
    Ok(None)
}

fn prompt_request(prompt: &str) -> Result<Request, String> {
    if prompt.is_empty() || prompt.len() > 4_000 || prompt.chars().any(|c| c.is_control() && c != '\n' && c != '\r') {
        return Err("Describe the image in up to 4,000 bytes.".into());
    }
    Ok(Request::Generate(prompt.into()))
}

fn payload(prompt: &str) -> Value {
    json!({"model":MODEL, "prompt":prompt, "n":1, "aspect_ratio":"1:1",
        "quality":"medium","output_format":"png","stream":false,
        "provider":{"allow_fallbacks":false}})
}

fn decode_generated(reply: &Value) -> Result<Screenshot, String> {
    let item = reply["data"].as_array().filter(|items| items.len() == 1).and_then(|items| items.first())
        .ok_or("OpenRouter did not return exactly one image.")?;
    if item["media_type"].as_str().is_some_and(|mime| mime != "image/png") {
        return Err("OpenRouter returned an image format other than PNG.".into());
    }
    let encoded = item["b64_json"].as_str().ok_or("OpenRouter did not return PNG bytes.")?;
    if encoded.len() > visual_qa::MAX_SAVED_PNG_BYTES.div_ceil(3) * 4 {
        return Err("The generated PNG exceeded the attachment size limit.".into());
    }
    let bytes = STANDARD.decode(encoded).map_err(|_| "OpenRouter returned invalid image data.")?;
    visual_qa::image("Generated image.png".into(), bytes, false)
}

fn generate(prompt: &str, key: &str) -> Result<(Screenshot, Option<f64>), String> {
    let agent = ureq::AgentBuilder::new().redirects(0).timeout(Duration::from_secs(180)).build();
    let url = format!("{}/images", crate::gateway::GatewayId::OpenrouterDirect.base_url());
    let answer = match agent.post(&url).set("Authorization", &format!("Bearer {key}"))
        .set("Content-Type", "application/json").send_string(&payload(prompt).to_string()) {
        Ok(answer) if answer.status() == 200 => answer,
        Err(ureq::Error::Status(401 | 403, _)) => return Err("OpenRouter rejected the connection or model access. Check Models & Connections.".into()),
        Err(ureq::Error::Status(status, _)) => return Err(format!("OpenRouter returned HTTP {status} for the image request.")),
        _ => return Err("OpenRouter did not return a confirmed image. Check OpenRouter activity before sending another request.".into()),
    };
    let mut data = Vec::new();
    answer.into_reader().take(RESPONSE_LIMIT as u64 + 1).read_to_end(&mut data)
        .map_err(|_| "OpenRouter interrupted the image response.")?;
    if data.len() > RESPONSE_LIMIT { return Err("OpenRouter’s image response exceeded the attachment limit.".into()); }
    let reply: Value = serde_json::from_slice(&data).map_err(|_| "OpenRouter sent an unreadable image response.")?;
    let cost = reply["usage"]["cost"].as_f64().filter(|value| value.is_finite() && *value >= 0.0);
    Ok((decode_generated(&reply)?, cost))
}

pub(crate) fn run(
    app: &AppHandle, chat: &str, turn: &str, request: Request, root: Option<&str>, cancel: &AtomicBool,
) -> Result<Value, String> {
    let active = || !cancel.load(Ordering::Relaxed);
    let same_active_chat = || active() && ChatsStore::is_active(chat).unwrap_or(false);
    let mut generation_submitted = false;
    let record = |kind: &str, tool: &str, outcome: &str, text: &str, details: Value, message| {
        publish_receipt(app, Some(chat), AskReceipt {
            timestamp: now_ms(), turn_id: turn.into(), kind: kind.into(), tool: tool.into(), gateway: ROUTE.into(),
            model: String::new(), summary: text.into(), outcome: outcome.into(),
            details: details.to_string(), actor: default_actor(),
        }, message)
    };
    if !active() { return Err("cancelled".into()); }
    let result: Result<(Screenshot, &str, Value), String> = (|| match request {
        Request::Open(path) => {
            let root = root.ok_or("Pin and authorize the project containing the PNG first.")?;
            let path = match path {
                Some(path) => path,
                None => ChatsStore::last_listed_png(chat)?.ok_or("Give me the PNG’s project-relative path, such as assets/logo.png.")?,
            };
            Ok((visual_qa::load_project_png(app, root, &path)?, "project", json!({})))
        }
        Request::Capture => {
            if !same_active_chat() { return Err("Open this chat before capturing the PAIGE Desktop window.".into()); }
            record("tool", "capture_chat_window", "awaiting_confirmation", "Waiting for Windows capture approval.", json!({}), None)?;
            if !visual_qa::confirm_window_capture() { return Err("Window capture was declined.".into()); }
            if !same_active_chat() { return Err("Window capture was cancelled because this chat is no longer active.".into()); }
            Ok((visual_qa::capture_window(app, chat, turn, same_active_chat)?, "screenshot", json!({})))
        }
        Request::Generate(prompt) => {
            let key = GatewayConfig::load().openrouter_key()
                .map_err(|_| "Check the OpenRouter connection in Models & Connections.")?
                .ok_or("Connect OpenRouter in Models & Connections to create an image.")?;
            record("tool", "generate_chat_image", "awaiting_confirmation", "Waiting for Windows image generation approval.",
                json!({"provider":"OpenRouter","model":MODEL,"images":1}), None)?;
            let confirmation = format!("Create one image using {MODEL} through OpenRouter?\n\nThis sends the prompt below to OpenRouter and may use your credits. Once submitted, stopping the chat cannot cancel the provider request. Project files, screenshots and chat history are excluded.\n\n{prompt}");
            if !crate::broker::native_confirm(&confirmation, "PAIGE Desktop - Create image") {
                return Err("Image generation was declined.".into());
            }
            if !active() { return Err("cancelled".into()); }
            record("tool", "generate_chat_image", "dispatch_attempted", "Submitting the approved request to OpenRouter. It may finish even if you stop waiting.",
                json!({"provider":"OpenRouter","model":MODEL,"images":1}), None)?;
            generation_submitted = true;
            let (image, cost) = generate(&prompt, &key)?;
            Ok((image, "generated", json!({"provider":"OpenRouter","model":MODEL,"cost_usd":cost})))
        }
    })();
    if !active() && !generation_submitted {
        return Err("cancelled".into());
    }
    let result = result.and_then(|(image, source, extra)| {
        if source != "screenshot" { visual_qa::save_capture(chat, turn, &image)?; }
        Ok((image, source, extra))
    });
    let (text, outcome, details) = match result {
        Ok((image, source, extra)) => {
            let text = match source {
                "screenshot" => "I captured this screenshot of PAIGE Desktop.",
                "generated" => "I created this image from your prompt.",
                _ => "Here’s the PNG from this project.",
            };
            (text.to_owned(), "verified", json!({"image":image.meta,"source":source,"generation":extra}))
        }
        Err(error) => (error.clone(), "error", json!({"error":error})),
    };
    record("tool", RECEIPT, outcome, &text, details, Some(ChatMessage {
        role: "assistant".into(), text: text.clone(), tool_events: vec![], model_label: None, model: None,
        gateway: Some(ROUTE.into()), turn_id: Some(turn.into()),
    }))?;
    Ok(json!({"turn_id":turn,"status":"complete","text":text}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_distinct_intents_and_confines_local_reads() {
        assert_eq!(recognize("Can you show me `dist/assets/logo.png`?"), Ok(Some(Request::Open(Some("dist/assets/logo.png".into())))));
        assert_eq!(recognize("Show me the PNG you found."), Ok(Some(Request::Open(None))));
        assert_eq!(recognize("Take a screenshot of your window."), Ok(Some(Request::Capture)));
        assert_eq!(recognize("Generate an image of a storefront."), Ok(Some(Request::Generate("a storefront".into()))));
        assert_eq!(recognize("Inspect a screenshot and propose a fix."), Ok(None));
        assert_eq!(recognize("How do I generate an image of a storefront?"), Ok(None));
        assert!(recognize("/show-image ../some.png").is_err());
        assert!(recognize("/show-image https://example.com/file.png").is_err());
        assert!(recognize("/create-image").is_err());
    }

    #[test]
    fn generation_has_a_single_png_and_sends_only_the_prompt() {
        let wire = payload("test prompt");
        assert_eq!(wire["model"], MODEL);
        assert_eq!(wire["prompt"], "test prompt");
        assert_eq!(wire["n"], 1);
        assert_eq!(wire["aspect_ratio"], "1:1");
        assert_eq!(wire["output_format"], "png");
        assert!(!wire["provider"]["allow_fallbacks"].as_bool().unwrap());
        assert!(decode_generated(&json!({"data":[{"url":"https://example.com/file.png"}]})).is_err());
        assert!(decode_generated(&json!({"data":[{"b64_json":"bm90IGEgcG5n"}]})).is_err());
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
            encoder.set_color(png::ColorType::Rgb);
            encoder.write_header().unwrap().write_image_data(&[40, 50, 60]).unwrap();
        }
        let result = decode_generated(&json!({"data":[{"media_type":"image/png","b64_json":STANDARD.encode(&bytes)}]})).unwrap();
        assert_eq!(result.bytes, bytes);
        assert_eq!(result.meta.width, 1);
    }
}

