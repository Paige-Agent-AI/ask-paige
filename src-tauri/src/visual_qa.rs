//! One owner-selected PNG and one bounded CSS correction per Visual QA run.
use crate::broker::{self, BrokerState};
use crate::chats::{ChatsStore, ExecutionEvent};
use crate::workflows::WorkflowRequest;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs, io::{Cursor, Read, Write}, path::{Path, PathBuf}, sync::Mutex, time::Duration};
use tauri::{AppHandle, Manager};

pub(crate) const BEFORE: &str = "visual_before";
pub(crate) const INSPECT: &str = "visual_inspection";
pub(crate) const COMPARE: &str = "visual_comparison";
pub(crate) const MAX_PNG_BYTES: usize = 2 * 1024 * 1024;
pub(crate) const MAX_SAVED_PNG_BYTES: usize = 5 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Scope {
    pub before: String,
    pub css: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub capture_window: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub proposal_only: bool,
}

impl Scope {
    pub(crate) fn validate(&mut self) -> Result<(), String> {
        if self.capture_window {
            if !self.before.is_empty() {
                return Err("Choose a project PNG or capture the PAIGE window.".into());
            }
        } else {
            self.before = screenshot_path(&self.before)?;
        }
        self.css = crate::workbench_plan::relative_key(&self.css)?;
        if !has_extension(&self.css, "css") {
            return Err("Choose one project-relative CSS file to correct.".into());
        }
        Ok(())
    }
}

fn free_image_route(item: &serde_json::Value) -> bool {
    let supports_images = item["architecture"]["input_modalities"].as_array()
        .is_some_and(|modes| modes.iter().any(|mode| mode == "image"));
    let zero = |value: &serde_json::Value| {
        value.as_f64().or_else(|| value.as_str()?.parse::<f64>().ok()) == Some(0.0)
    };
    let pricing = &item["pricing"];
    // OpenRouter's documented free router omits separate image and request
    // fees from its live entry. If the gateway adds either fee, require zero.
    supports_images && zero(&pricing["prompt"]) && zero(&pricing["completion"])
        && ["image", "request"].into_iter().all(|key| pricing.get(key).is_none_or(zero))
}

pub(crate) fn verify_image_route(base_url: &str, id: &str) -> Result<(), String> {
    const MAX_CATALOG_BYTES: u64 = 8 * 1024 * 1024;
    let reply = ureq::AgentBuilder::new().redirects(0).timeout(Duration::from_secs(15)).build()
        .get(&format!("{base_url}/models")).call().map_err(|_| "Could not verify OpenRouter image availability.")?;
    let mut bytes = Vec::new();
    reply.into_reader().take(MAX_CATALOG_BYTES + 1).read_to_end(&mut bytes)
        .map_err(|_| "Could not read the OpenRouter model catalog.")?;
    if bytes.len() as u64 > MAX_CATALOG_BYTES { return Err("The model catalog exceeded the verification limit.".into()); }
    let catalog: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| "The OpenRouter model catalog is invalid.")?;
    if !catalog["data"].as_array().is_some_and(|models|
        models.iter().any(|item| item["id"].as_str() == Some(id) && free_image_route(item))) {
        return Err("OpenRouter Free did not advertise free image input. The screenshot was not sent.".into());
    }
    Ok(())
}

fn has_extension(path: &str, extension: &str) -> bool {
    Path::new(path).extension().is_some_and(|value| value.eq_ignore_ascii_case(extension))
}

pub(crate) fn screenshot_path(path: &str) -> Result<String, String> {
    let path = crate::workbench_plan::relative_key(path)?;
    if !has_extension(&path, "png") { return Err("Use a project-relative .png screenshot.".into()); }
    Ok(path)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Image {
    pub(crate) path: String,
    pub(crate) sha256: String,
    pub(crate) bytes: usize,
    pub(crate) width: u32,
    pub(crate) height: u32,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(crate) captured_window: bool,
}

pub(crate) struct Screenshot {
    pub(crate) meta: Image,
    pub(crate) bytes: Vec<u8>,
    pixels: Vec<[u8; 4]>,
}

fn decode(bytes: &[u8]) -> Result<(u32, u32, Vec<[u8; 4]>), String> {
    let mut decoder = png::Decoder::new_with_limits(Cursor::new(bytes), png::Limits { bytes: 24 * 1024 * 1024 });
    decoder.set_ignore_text_chunk(true);
    decoder.set_ignore_iccp_chunk(true);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let info = decoder.read_header_info().map_err(|_| "The screenshot is not a valid PNG.")?;
    if info.width == 0 || info.height == 0 || info.width > 2560 || info.height > 1600 {
        return Err("Use a screenshot no larger than 2560 by 1600 pixels.".into());
    }
    let mut reader = decoder.read_info().map_err(|_| "The PNG header could not be read.")?;
    if reader.info().animation_control.is_some() {
        return Err("Visual QA accepts still PNG screenshots.".into());
    }
    let size = reader.output_buffer_size().ok_or("The PNG size is invalid.")?;
    if size > 2560 * 1600 * 4 { return Err("The PNG exceeds the decoded size limit.".into()); }
    let mut output = vec![0; size];
    let frame = reader.next_frame(&mut output).map_err(|_| "The PNG pixels could not be decoded.")?;
    let raw = &output[..frame.buffer_size()];
    let channels = frame.color_type.samples();
    if frame.bit_depth != png::BitDepth::Eight || frame.color_type == png::ColorType::Indexed {
        return Err("The PNG pixel format is unsupported.".into());
    }
    let pixels = raw.chunks_exact(channels).map(|pixel| match pixel {
        [grey] => [*grey, *grey, *grey, 255],
        [grey, alpha] => [*grey, *grey, *grey, *alpha],
        [red, green, blue] => [*red, *green, *blue, 255],
        [red, green, blue, alpha] => [*red, *green, *blue, *alpha],
        _ => unreachable!("validated PNG color types have at most four channels"),
    }).collect();
    Ok((frame.width, frame.height, pixels))
}

pub(crate) fn load(app: &AppHandle, workflow: &WorkflowRequest, path: &str) -> Result<Screenshot, String> {
    load_project_png(app, &workflow.workspace_root, path)
}

pub(crate) fn load_project_png(app: &AppHandle, root: &str, path: &str) -> Result<Screenshot, String> {
    let path = screenshot_path(path)?;
    let bytes = broker::read_workflow_image(app.state::<Mutex<BrokerState>>(), root, &path)?;
    image(path, bytes, false)
}

pub(crate) fn image(path: String, bytes: Vec<u8>, captured_window: bool) -> Result<Screenshot, String> {
    if bytes.len() > MAX_SAVED_PNG_BYTES { return Err("This PNG exceeds the saved image limit.".into()); }
    let (width, height, pixels) = decode(&bytes)?;
    let meta = Image { path, sha256: hex::encode(Sha256::digest(&bytes)),
        bytes: bytes.len(), width, height, captured_window };
    Ok(Screenshot { meta, bytes, pixels })
}

fn chat_capture_prefix(chat_id: &str) -> String {
    hex::encode(Sha256::digest(chat_id.as_bytes()))
}

fn capture_filename(chat_id: &str, turn_id: &str, sha256: &str) -> Result<String, String> {
    if sha256.len() != 64 || !sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("The window capture checksum is invalid.".into());
    }
    let chat_key = chat_capture_prefix(chat_id);
    let turn_key = hex::encode(Sha256::digest(turn_id.as_bytes()));
    Ok(format!("{chat_key}-{turn_key}-{sha256}.png"))
}

fn capture_dir() -> Result<PathBuf, String> {
    Ok(crate::secrets::config_dir()?.join("visual-captures"))
}

pub(crate) fn confirm_window_capture() -> bool {
    broker::native_confirm(
        "Capture the visible content of this PAIGE Desktop window once?\n\nThe PNG stays on this device. Sharing it with an image model requires a separate approval.",
        "PAIGE Desktop - Capture window",
    )
}

fn capture_pixels(app: &AppHandle) -> Result<Screenshot, String> {
    #[cfg(windows)]
    let bytes = crate::window_capture::capture(app)?;
    #[cfg(not(windows))]
    let bytes = { let _ = app; return Err("PAIGE window capture is available on Windows.".into()); };
    image("PAIGE Desktop window".into(), bytes, true)
}

pub(crate) fn capture_window(
    app: &AppHandle, chat_id: &str, turn_id: &str, still_allowed: impl FnOnce() -> bool,
) -> Result<Screenshot, String> {
    let screenshot = capture_pixels(app)?;
    if !still_allowed() { return Err("Window capture was cancelled.".into()); }
    save_capture(chat_id, turn_id, &screenshot)?;
    Ok(screenshot)
}

pub(crate) fn save_capture(chat_id: &str, turn_id: &str, screenshot: &Screenshot) -> Result<(), String> {
    let directory = capture_dir()?;
    fs::create_dir_all(&directory).map_err(|_| "Could not create the local capture folder.")?;
    let path = directory.join(capture_filename(chat_id, turn_id, &screenshot.meta.sha256)?);
    match fs::OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(mut file) => {
            if file.write_all(&screenshot.bytes).and_then(|()| file.sync_all()).is_err() {
                drop(file);
                let _ = fs::remove_file(&path);
                return Err("Could not save the window capture.".into());
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(_) => return Err("Could not save the window capture.".into()),
    }
    read_capture(&path, &screenshot.meta.sha256)?;
    Ok(())
}

pub(crate) fn saved_png(chat_id: &str, turn_id: &str, sha256: &str) -> Result<Vec<u8>, String> {
    let path = capture_dir()?.join(capture_filename(chat_id, turn_id, sha256)?);
    read_capture(&path, sha256)
}

fn read_capture(path: &Path, expected_sha256: &str) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    fs::File::open(path).map_err(|_| "The saved window capture is unavailable.")?
        .take((MAX_SAVED_PNG_BYTES + 1) as u64).read_to_end(&mut bytes)
        .map_err(|_| "The saved window capture could not be read.")?;
    if bytes.len() > MAX_SAVED_PNG_BYTES || hex::encode(Sha256::digest(&bytes)) != expected_sha256 {
        return Err("The window capture differs from its recorded checksum.".into());
    }
    Ok(bytes)
}

pub(crate) fn delete_chat_captures(chat_id: &str) -> Result<(), String> {
    let directory = capture_dir()?;
    let files = match fs::read_dir(directory) {
        Ok(files) => files,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err("Could not open the local capture folder.".into()),
    };
    let prefix = format!("{}-", chat_capture_prefix(chat_id));
    for file in files {
        let file = file.map_err(|_| "Could not list the local captures.")?;
        let name = file.file_name();
        let candidate = name.to_str().and_then(|n| n.strip_prefix(&prefix))
            .and_then(|n| n.strip_suffix(".png")).and_then(|n| n.split_once('-'));
        if candidate.is_some_and(|(turn, sha)| [turn, sha].iter()
            .all(|value| value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit()))) {
            fs::remove_file(file.path()).map_err(|_| "Could not delete a local chat capture.")?;
        }
    }
    Ok(())
}

pub(crate) fn load_recorded(
    app: &AppHandle, workflow: &WorkflowRequest, chat_id: &str, turn_id: &str, recorded: &Image,
) -> Result<Screenshot, String> {
    let screenshot = if recorded.captured_window {
        let bytes = saved_png(chat_id, turn_id, &recorded.sha256)?;
        image("PAIGE Desktop window".into(), bytes, true)?
    } else {
        load(app, workflow, &recorded.path)?
    };
    if screenshot.meta.sha256 != recorded.sha256 { return Err("The screenshot differs from its recorded checksum.".into()); }
    Ok(screenshot)
}

impl Screenshot {
    pub(crate) fn attachment_thumbnail_png(&self) -> Result<Vec<u8>, String> {
        let scale = self.meta.width.max(self.meta.height).div_ceil(144).max(1);
        let width = self.meta.width.div_ceil(scale);
        let height = self.meta.height.div_ceil(scale);
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                pixels.extend_from_slice(&self.pixels[((y * self.meta.width + x) * scale) as usize]);
            }
        }
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.write_header().and_then(|mut writer| writer.write_image_data(&pixels))
                .map_err(|_| "The attachment thumbnail could not be encoded.")?;
        }
        Ok(bytes)
    }

    pub(crate) fn pixel_only_png(&self) -> Result<Vec<u8>, String> {
        let mut bytes = Vec::new();
        let pixels: Vec<_> = self.pixels.iter().flatten().copied().collect();
        {
            let mut encoder = png::Encoder::new(&mut bytes, self.meta.width, self.meta.height);
            encoder.set_color(png::ColorType::Rgba);
            let mut writer = encoder.write_header().map_err(|_| "Could not prepare the PNG pixels.")?;
            writer.write_image_data(&pixels).map_err(|_| "Could not prepare the PNG pixels.")?;
            writer.finish().map_err(|_| "Could not finish the PNG payload.")?;
        }
        if bytes.len() > MAX_PNG_BYTES { return Err("Use a smaller screenshot for inspection.".into()); }
        Ok(bytes)
    }
}

fn compare(before: &Screenshot, after: &Screenshot) -> Result<usize, String> {
    if (before.meta.width, before.meta.height) != (after.meta.width, after.meta.height) {
        return Err("Use the same viewport and scale for both screenshots. The pixel sizes must match.".into());
    }
    let count = before.pixels.iter().zip(&after.pixels).filter(|(a, b)| a != b).count();
    if count == 0 { return Err("The screenshots contain identical pixels. Capture the rendered correction.".into()); }
    Ok(count)
}

pub(crate) fn image_from_event(events: &[ExecutionEvent], stage: &str) -> Result<Image, String> {
    let tool = match stage {
        "before" => BEFORE,
        "after" => COMPARE,
        _ => return Err("Choose the recorded Before or After screenshot.".into()),
    };
    let event = events.iter().rev().find(|event| event.tool == tool && event.outcome == "verified")
        .ok_or("This screenshot has not been recorded.")?;
    let details: serde_json::Value = serde_json::from_str(&event.details).map_err(|_| "The screenshot receipt is invalid.")?;
    serde_json::from_value(details[stage].clone()).map_err(|_| "The screenshot receipt is invalid.".into())
}

pub(crate) fn test_arguments(command: &serde_json::Value, root: &str) -> Result<serde_json::Value, String> {
    let command = command.as_str().ok_or("Name one exact Visual QA test command.")?;
    if !command.is_ascii() || command.contains(['"', '\'', '&', '|', ';', '>', '<', '\n', '\r']) {
        return Err("Use an exact test command with unquoted arguments.".into());
    }
    let mut words = command.split_ascii_whitespace();
    let runner = words.next().ok_or("Name one exact Visual QA test command.")?;
    let args: Vec<_> = words.collect();
    let native = serde_json::json!({"command":runner, "args":args, "cwd":root});
    crate::workflows::WorkflowTemplate::VisualQaV1.validate_call("run_command", &native, root)?;
    Ok(native)
}

fn verified_write(events: &[ExecutionEvent]) -> Option<(usize, serde_json::Value)> {
    let index = events.iter().rposition(|event| event.kind == "tool"
        && event.tool == "write_file" && matches!(event.outcome.as_str(), "committed_verified" | "target_verified"))?;
    let edited: serde_json::Value = serde_json::from_str(&events[index].details).ok()?;
    (edited["readback_verified"] == true).then_some((index, edited))
}

pub(crate) fn verified_css_hash(events: &[ExecutionEvent]) -> Option<String> {
    let (_, edited) = verified_write(events)?;
    edited["after_sha256"].as_str().map(str::to_owned)
}

pub(crate) fn recorded_spawn_failure(events: &[ExecutionEvent], expected_call: &str) -> bool {
    let dispatch = events.iter().rposition(|event| event.kind == "approval"
        && event.tool == "run_command" && event.outcome == "test_dispatch_pending");
    let Some(index) = dispatch else { return false; };
    let Ok(details) = serde_json::from_str::<serde_json::Value>(&events[index].details) else { return false; };
    if details["call_id"] != expected_call { return false; }
    events[index + 1..].iter().rev().find(|event| event.kind == "tool" && event.tool == "run_command")
        .filter(|event| event.outcome == "error")
        .and_then(|event| serde_json::from_str::<serde_json::Value>(&event.details).ok())
        .is_some_and(|result| result["error"].as_str().is_some_and(|error| error.starts_with(crate::terminal::SPAWN_FAILED)))
}

pub(crate) fn recorded_test(events: &[ExecutionEvent]) -> Option<serde_json::Value> {
    let (write, _) = verified_write(events)?;
    events[write + 1..].iter().rev().filter(|event| event.kind == "tool" && event.tool == "run_command")
        .filter_map(|event| serde_json::from_str::<serde_json::Value>(&event.details).ok())
        .find(|test| test["exit_code"].is_number() || test["timed_out"] == true)
}

fn verified_fix(events: &[ExecutionEvent]) -> Option<serde_json::Value> {
    let (_, edited) = verified_write(events)?;
    let test = recorded_test(events)?;
    (test["exit_code"] == 0 && test["timed_out"] == false
        && test["visual_css_verified"] == true).then_some(edited)
}

#[tauri::command]
pub fn workflow_read_visual(
    app: AppHandle, chat_id: String, turn_id: String, stage: String,
) -> Result<tauri::ipc::Response, String> {
    let (run, events) = ChatsStore::visual_run(&chat_id, &turn_id)?;
    let image = image_from_event(&events, &stage)?;
    let screenshot = load_recorded(&app, &run.request, &chat_id, &turn_id, &image)?;
    Ok(tauri::ipc::Response::new(screenshot.bytes))
}

#[tauri::command]
pub async fn workflow_retry_visual_test(app: AppHandle, chat_id: String, turn_id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || retry_visual_test(&app, &chat_id, &turn_id))
        .await.map_err(|_| "Visual QA test verification was interrupted.".to_string())?
}

fn retry_visual_test(app: &AppHandle, chat_id: &str, turn_id: &str) -> Result<(), String> {
    use serde_json::json;
    let _guard = ChatsStore::lock_turn()?;
    if !ChatsStore::is_active(chat_id)? || ChatsStore::checkpoint(chat_id, true)?.is_some() {
        return Err("Open this Visual QA chat and finish any active task before retrying its saved test.".into());
    }
    let (run, events) = ChatsStore::visual_run(chat_id, turn_id)?;
    let (write_index, write) = verified_write(&events).ok_or("There is no verified CSS edit.")?;
    let dispatch = events[write_index + 1..].iter().rev().find(|event| event.tool == "run_command"
        && event.kind == "approval" && event.outcome == "test_dispatch_pending")
        .and_then(|event| serde_json::from_str::<serde_json::Value>(&event.details).ok())
        .ok_or("The saved test has no dispatch receipt.")?;
    let retry_of = dispatch["call_id"].as_str().filter(|id| recorded_spawn_failure(&events, id))
        .ok_or("Retry is available only after a verified process-launch refusal.")?;
    let plan = events[..write_index].iter().rev().find(|event| event.kind == "plan"
        && event.tool == crate::workbench_plan::SUBMIT && event.outcome == "ok")
        .and_then(|event| serde_json::from_str::<serde_json::Value>(&event.details).ok())
        .ok_or("The verified edit has no saved test plan.")?;
    let scope = run.request.visual.as_ref().ok_or("The Visual QA scope is missing.")?;
    let root = &run.request.workspace_root;
    if plan["project"] != *root || plan["plan"]["files"] != json!([scope.css]) {
        return Err("The saved plan differs from the authorized Visual QA worktree or CSS file.".into());
    }
    let arguments = test_arguments(&plan["plan"]["test_command"], root)?;
    let approved = events[write_index + 1..].iter().rev()
        .filter(|event| event.kind == "approval" && event.tool == "run_command"
            && matches!(event.outcome.as_str(), "awaiting_review" | "awaiting_confirmation"))
        .filter_map(|event| serde_json::from_str::<serde_json::Value>(&event.details).ok())
        .find(|approval| approval["call_id"] == retry_of && approval["arguments"].is_object())
        .ok_or("The failed launch has no matching command approval receipt.")?;
    if approved["arguments"] != arguments || dispatch["css_sha256"] != write["after_sha256"] {
        return Err("The failed launch did not match the saved test plan and verified CSS.".into());
    }
    let _root = crate::ask::require_workflow_binding(app, &run.request)?;
    let (path, hash) = broker::workflow_file_checksum(app.state::<Mutex<BrokerState>>(), root, &scope.css, false)?;
    if write["path"].as_str() != Some(&path) || write["after_sha256"].as_str() != Some(&hash) {
        return Err("The CSS differs from the verified write. Inspect the worktree before retrying the test.".into());
    }

    let attempt = events.iter().filter(|event| event.kind == "approval"
        && event.tool == "run_command" && event.outcome == "awaiting_confirmation").count();
    let call_id = format!("visual-test-{turn_id}-{}", attempt + 1);
    let record = |kind: &str, tool: &str, outcome: &str, summary: &str, details: serde_json::Value| {
        ChatsStore::record_workflow_decision(chat_id, turn_id, ExecutionEvent {
            timestamp: 0, kind: kind.into(), tool: tool.into(), outcome: outcome.into(), summary: summary.into(),
            details: details.to_string(), gateway: String::new(), model: String::new(),
            actor: "unknown".into(), turn_id: Some(turn_id.into()),
        })
    };
    let mut dispatch_saved = false;
    let result = crate::ask::dispatch_saved_visual_test(app, &run.request, arguments.clone(), call_id.clone(),
        |status, _, subject, summary| {
            if status == "start_visual_test" {
                if !ChatsStore::is_active(chat_id)? || subject != hash {
                    return Err("The verified CSS or selected conversation changed during approval.".into());
                }
                record("approval", "run_command", "test_dispatch_pending", summary,
                    json!({"call_id":call_id, "recovery_of":retry_of, "arguments":arguments, "css_sha256":hash}))?;
                dispatch_saved = true;
                return Ok(());
            }
            record("approval", "run_command", status, summary,
                json!({"call_id":call_id, "recovery_of":retry_of, "arguments":arguments}))
        });
    match result {
        Ok(test) => {
            let passed = test["exit_code"] == 0 && test["timed_out"] == false && test["visual_css_verified"] == true;
            record("tool", "run_command", if passed { "ok" } else { "error" },
                if passed { "The saved Visual QA test passed and the CSS checksum still matches." }
                else { "The saved Visual QA test did not verify the correction." }, test)
        }
        Err(error) if dispatch_saved => {
            record("tool", "run_command", "error", &error, json!({"error":error, "call_id":call_id}))?;
            Err(error)
        }
        Err(error) => Err(error),
    }
}

#[tauri::command]
pub async fn workflow_compare_visual(
    app: AppHandle, chat_id: String, turn_id: String, path: Option<String>, capture_window: Option<bool>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = ChatsStore::lock_turn()?;
        if !ChatsStore::is_active(&chat_id)? { return Err("Open this Visual QA chat before adding its After screenshot.".into()); }
        let (run, events) = ChatsStore::visual_run(&chat_id, &turn_id)?;
        if events.iter().any(|event| event.tool == COMPARE && event.outcome == "verified") {
            return Err("This run already has a verified comparison.".into());
        }
        let _root = crate::ask::require_workflow_binding(&app, &run.request)?;
        if !events.iter().any(|event| event.tool == INSPECT && event.outcome == "verified") {
            return Err("There is no verified screenshot inspection for this run.".into());
        }
        let edited = verified_fix(&events).ok_or("Finish the approved CSS edit and its passing test first.")?;
        let scope = run.request.visual.as_ref().ok_or("The Visual QA scope is missing.")?;
        let (target, hash) = broker::workflow_file_checksum(
            app.state::<Mutex<BrokerState>>(), &run.request.workspace_root, &scope.css, false,
        )?;
        if edited["path"].as_str() != Some(&target) || edited["after_sha256"].as_str() != Some(&hash) {
            return Err("The CSS has changed since the verified fix.".into());
        }
        let recorded = image_from_event(&events, "before")?;
        let before = load_recorded(&app, &run.request, &chat_id, &turn_id, &recorded)?;
        let after = match (path, capture_window.unwrap_or(false)) {
            (Some(path), false) => load(&app, &run.request, &path)?,
            (None, true) if before.meta.captured_window => {
                if !confirm_window_capture() { return Err("Window capture was declined.".into()); }
                if !ChatsStore::is_active(&chat_id)? { return Err("The active Visual QA chat changed.".into()); }
                let _root = crate::ask::require_workflow_binding(&app, &run.request)?;
                capture_pixels(&app)?
            }
            _ => return Err("Choose a project PNG or capture the PAIGE window after a window-based inspection.".into()),
        };
        if !ChatsStore::is_active(&chat_id)? { return Err("The active Visual QA chat changed.".into()); }
        let _root = crate::ask::require_workflow_binding(&app, &run.request)?;
        let (_, current_hash) = broker::workflow_file_checksum(
            app.state::<Mutex<BrokerState>>(), &run.request.workspace_root, &scope.css, false,
        )?;
        if current_hash != hash { return Err("The CSS changed during the comparison.".into()); }
        let changed_pixels = compare(&before, &after)?;
        if after.meta.captured_window { save_capture(&chat_id, &turn_id, &after)?; }
        ChatsStore::record_workflow_decision(&chat_id, &turn_id, ExecutionEvent {
            timestamp: 0, kind: "workflow".into(), tool: COMPARE.into(), outcome: "verified".into(),
            gateway: String::new(), model: String::new(),
            summary: "Recorded the Before and After screenshots and measured their pixel differences.".into(),
            details: serde_json::json!({"before":before.meta, "after":after.meta,
                "changed_pixels":changed_pixels, "css_sha256":hash}).to_string(),
            actor: "unknown".into(), turn_id: Some(turn_id.clone()),
        })
    }).await.map_err(|_| "The screenshot comparison was interrupted.".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screenshot(color: u8, width: u32) -> Screenshot {
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, width, 1);
        encoder.set_color(png::ColorType::Rgb);
        encoder.write_header().unwrap().write_image_data(&vec![color; width as usize * 3]).unwrap();
        let (width, height, pixels) = decode(&bytes).unwrap();
        Screenshot { meta: Image { path: "fixture.png".into(), sha256: hex::encode(Sha256::digest(&bytes)),
            bytes: bytes.len(), width, height, captured_window: false }, bytes, pixels }
    }

    #[test]
    fn capture_scope_is_exclusive_and_storage_keys_are_chat_and_turn_bound() {
        let mut scope = Scope { before: "before.png".into(), css: "styles.css".into(),
            proposal_only: true, capture_window: true };
        assert!(scope.validate().is_err());
        scope.before.clear();
        assert!(scope.validate().is_ok());
        scope.capture_window = false;
        assert!(scope.validate().is_err());

        let sha256 = "a".repeat(64);
        let key = capture_filename("../first", "../turn", &sha256).unwrap();
        assert!(key.starts_with(&chat_capture_prefix("../first")));
        assert!(!key.strip_suffix(".png").unwrap().contains(['.', '/', '\\']));
        assert_ne!(key, capture_filename("second", "../turn", &sha256).unwrap());
        assert_ne!(key, capture_filename("../first", "different", &sha256).unwrap());
        assert!(capture_filename("first", "turn", "../escape.png").is_err());
    }

    #[test]
    fn comparison_rejects_copies_and_mismatched_viewports() {
        let before = screenshot(0, 4);
        assert!(compare(&before, &screenshot(0, 4)).is_err());
        assert!(compare(&before, &screenshot(1, 5)).is_err());
        assert_eq!(compare(&before, &screenshot(255, 4)).unwrap(), 4);
        assert!(decode(b"not a PNG").is_err());
        assert!(screenshot_path("../private.png").is_err());
        let encoded = before.pixel_only_png().unwrap();
        assert_eq!(decode(&encoded).unwrap().2, before.pixels);
    }

    #[test]
    fn retry_requires_the_matching_dispatch_and_a_proven_spawn_failure() {
        use serde_json::json;
        let event = |kind: &str, outcome: &str, details: serde_json::Value| ExecutionEvent {
            timestamp: 0, kind: kind.into(), tool: "run_command".into(),
            gateway: String::new(), model: String::new(), summary: String::new(),
            outcome: outcome.into(), details: details.to_string(), actor: "unknown".into(), turn_id: None,
        };
        let mut events = vec![event("approval", "test_dispatch_pending", json!({"call_id":"call-1"}))];
        assert!(!recorded_spawn_failure(&events, "call-1"));
        events.push(event("tool", "error", json!({"error":"failed to capture stdout"})));
        assert!(!recorded_spawn_failure(&events, "call-1"));
        events.push(event("tool", "error", json!({"error":"failed to spawn process: invalid Windows executable"})));
        assert!(!recorded_spawn_failure(&events, "call-2"));
        assert!(recorded_spawn_failure(&events, "call-1"));
        events.push(event("tool", "ok", json!({"exit_code":0,"timed_out":false})));
        assert!(!recorded_spawn_failure(&events, "call-1"));
    }

    #[test]
    fn visual_route_checks_image_support_and_rejects_image_fees() {
        use serde_json::json;
        let mut item = json!({"architecture":{"input_modalities":["text","image"]},
            "pricing":{"prompt":"0","completion":"0"}});
        assert!(free_image_route(&item));
        item["pricing"]["image"] = json!("0.01");
        assert!(!free_image_route(&item));
        item["pricing"]["image"] = json!("0");
        assert!(free_image_route(&item));
        item["architecture"]["input_modalities"] = json!(["text"]);
        assert!(!free_image_route(&item));
    }
}

