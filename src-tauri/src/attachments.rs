//! Composer attachment inspection. No network, workspace selection, or scope changes.
use std::io::Read;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Serialize;
use sha2::{Digest, Sha256};

const MAX_ITEMS: usize = 16;
const MAX_BATCH_BYTES: usize = 8 * 1024 * 1024;
const MAX_TEXT_BYTES: usize = 128 * 1024;

#[derive(Serialize)]
pub(crate) struct Inspection {
    state: &'static str,
    kind: &'static str,
    bytes: usize,
    sha256: Option<String>,
    thumbnail: Option<String>,
    detail: Option<String>,
}

impl Inspection {
    fn status(state: &'static str, kind: &'static str, detail: Option<String>) -> Self {
        Self { state, kind, bytes: 0, sha256: None, thumbnail: None, detail }
    }
}

fn inspect(path: &str, project: Option<&str>, remaining: usize) -> Result<Inspection, String> {
    let (canonical, file, _parents) = crate::broker::open_attachment(path, project)?;
    let metadata = file.metadata().map_err(|_| "Unable to check this attachment.")?;
    if metadata.is_dir() {
        return Ok(Inspection::status("ready", "folder", Some("Folder reference only".into())));
    }
    let extension = canonical.extension().and_then(|ext| ext.to_str()).unwrap_or("").to_ascii_lowercase();
    let (kind, limit) = match extension.as_str() {
        "png" => ("image", crate::visual_qa::MAX_PNG_BYTES),
        "txt" | "md" | "csv" | "json" | "log" | "toml" | "yaml" | "yml" => ("text", MAX_TEXT_BYTES),
        _ => return Ok(Inspection::status("unsupported", "file", Some("Choose a PNG or a supported text file.".into()))),
    };
    let limit = limit.min(remaining);
    if metadata.len() > limit as u64 { return Err("This file exceeds the attachment size limit.".into()); }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1).read_to_end(&mut bytes).map_err(|_| "Unable to read this attachment.")?;
    if bytes.len() > limit || bytes.len() as u64 != metadata.len() { return Err("This file changed during inspection.".into()); }
    crate::scope::verify_attachment_scope(&canonical, project)?;
    let mut result = Inspection::status("ready", kind, None);
    result.bytes = bytes.len();
    result.sha256 = Some(hex::encode(Sha256::digest(&bytes)));
    if kind == "image" {
        let image = crate::visual_qa::image("Attachment.png".into(), bytes, false)?;
        let thumbnail = image.attachment_thumbnail_png()?;
        result.thumbnail = Some(format!("data:image/png;base64,{}", STANDARD.encode(thumbnail)));
    } else if std::str::from_utf8(&bytes).is_err() || bytes.contains(&0) {
        return Ok(Inspection::status("unsupported", "file", Some("This file is not UTF-8 text.".into())));
    }
    Ok(result)
}

fn inspect_batch(paths: &[String], project: Option<&str>, available_bytes: usize) -> Vec<Inspection> {
    let mut remaining = available_bytes.min(MAX_BATCH_BYTES);
    paths.iter().map(|path| match inspect(path, project, remaining) {
        Ok(result) => { remaining -= result.bytes; result }
        Err(error) => Inspection::status("failed", "file", Some(error)),
    }).collect()
}

#[tauri::command]
pub(crate) async fn attachment_pick(window: tauri::WebviewWindow, folder: bool) -> Result<Vec<String>, String> {
    #[cfg(windows)]
    {
        let owner = window.hwnd().map_err(|error| error.to_string())?.0 as usize;
        tauri::async_runtime::spawn_blocking(move || crate::project::pick_local_paths(owner, folder, true)).await
            .map_err(|_| "The picker was interrupted.".to_string())?
    }
    #[cfg(not(windows))]
    {
        let _ = (window, folder);
        Err("The native attachment picker is available on Windows.".into())
    }
}

#[tauri::command]
pub(crate) async fn attachment_inspect(paths: Vec<String>, chat_id: Option<String>, pending_project: Option<String>, available_bytes: usize) -> Result<Vec<Inspection>, String> {
    if paths.len() > MAX_ITEMS { return Err("Choose up to 16 attachments per draft.".into()); }
    tauri::async_runtime::spawn_blocking(move || {
        use crate::chats::ChatsStore;
        let current_project = || -> Result<Option<String>, String> {
            match chat_id.as_deref() {
                Some(id) if ChatsStore::is_active(id)? => ChatsStore::workspace(id).map(|(root, _)| root),
                Some(_) => Err("This conversation is no longer active.".into()),
                None => Ok(pending_project.clone()),
            }
        };
        let project = current_project()?;
        let result = inspect_batch(&paths, project.as_deref(), available_bytes);
        if current_project()? != project { return Err("The chat project changed during inspection.".into()); }
        Ok(result)
    }).await.map_err(|_| "Attachment inspection was interrupted.".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_paths_never_stage() {
        for path in ["../notes.txt", "", "C:\\..\\notes.txt", "\\\\server\\share\\notes.txt"] {
            let result = inspect_batch(&[path.into()], None, MAX_BATCH_BYTES);
            assert_eq!(result[0].state, "failed");
            assert!(result[0].thumbnail.is_none());
        }
    }
}
