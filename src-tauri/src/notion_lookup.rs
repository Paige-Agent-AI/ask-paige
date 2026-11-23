//! Page binding and response verification for the owner-selected Notion source.

use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tauri::Url;

pub(crate) fn canonical_page_id(input: &str) -> Option<String> {
    let input = input.trim();
    if input.len() == 32 && input.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Some(input.to_ascii_lowercase());
    }
    if input.len() == 36 && input.bytes().enumerate().all(|(index, byte)| {
        if [8, 13, 18, 23].contains(&index) { byte == b'-' } else { byte.is_ascii_hexdigit() }
    }) {
        return Some(input.replace('-', "").to_ascii_lowercase());
    }
    let url = Url::parse(input).ok()?;
    let host = url.host_str()?;
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some()
        || url.port().is_some() || !(host == "app.notion.com" || host == "notion.so" || host.ends_with(".notion.so")
            || host.ends_with(".notion.site")) {
        return None;
    }
    let segment = url.path_segments()?.next_back()?;
    let take = if segment.len() >= 36 && segment.as_bytes().get(segment.len() - 28) == Some(&b'-') {
        36
    } else { 32 };
    let tail = segment.get(segment.len().checked_sub(take)?..)?;
    // Recurse only over the ID suffix, never over another URL.
    if !tail.bytes().all(|byte| byte.is_ascii_hexdigit() || byte == b'-') {
        return None;
    }
    canonical_page_id(tail)
}

pub(crate) fn page_url(id: &str) -> String {
    format!("https://www.notion.so/{id}")
}

pub(crate) fn bind_page(args: &mut Value, page_id: &str) -> Result<(), String> {
    let object = args.as_object_mut().ok_or("Notion page arguments must be an object.")?;
    if object.keys().any(|key| key != "id") {
        return Err("This Notion read accepts only the selected page ID.".into());
    }
    if let Some(value) = object.get("id") {
        let requested = value.as_str().and_then(canonical_page_id);
        if requested.as_deref() != Some(page_id) {
            return Err("Choose that Notion page in Models & Connections before reading it.".into());
        }
    }
    object.insert("id".into(), Value::String(page_url(page_id)));
    Ok(())
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Page {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) url: String,
    pub(crate) content_bytes: usize,
    pub(crate) content_sha256: String,
    pub(crate) excerpt: String,
    pub(crate) provider_truncated: bool,
}

impl Page {
    pub(crate) fn from_result(id: &str, items: &[Value], structured: Option<&Value>) -> Result<Self, String> {
        let text_objects = items.iter().filter_map(|item| {
            if item.get("type")?.as_str()? != "text" { return None; }
            serde_json::from_str::<Value>(item.get("text")?.as_str()?).ok()
        });
        let mut verified = None;
        for row in structured.cloned().into_iter().chain(text_objects) {
            let Some(url) = row.get("url").and_then(Value::as_str) else { continue; };
            // Some Notion result URLs use HTTP in metadata. Record our canonical HTTPS URL.
            let returned_url = url.strip_prefix("http://").map(|value| format!("https://{value}"));
            if canonical_page_id(returned_url.as_deref().unwrap_or(url)).as_deref() != Some(id) {
                return Err("Notion returned a different item than the selected page.".into());
            }
            if row.pointer("/metadata/type").and_then(Value::as_str).is_some_and(|kind| kind != "page") {
                return Err("Notion returned a different item type. Choose a Notion page.".into());
            }
            let title = row.get("title").and_then(Value::as_str).map(str::trim)
                .filter(|value| !value.is_empty()).ok_or("Notion did not return a page title.")?;
            let content = row.get("text").and_then(Value::as_str)
                .or_else(|| row.get("content").and_then(Value::as_str))
                .ok_or("Notion did not return readable page content.")?;
            let body = content.split_once("<content>").and_then(|(_, rest)| rest.rsplit_once("</content>"))
                .map_or(content, |(body, _)| body);
            let page = Self {
                id: id.to_owned(),
                title: title.chars().take(300).collect(),
                url: page_url(id),
                content_bytes: body.len(),
                content_sha256: hex::encode(Sha256::digest(body.as_bytes())),
                excerpt: body.trim().chars().take(600).collect(),
                provider_truncated: row.get("truncated").or_else(|| row.pointer("/metadata/truncated"))
                    .and_then(Value::as_bool).unwrap_or(false),
            };
            if verified.as_ref().is_some_and(|previous| previous != &page) {
                return Err("Notion returned conflicting versions of the selected page.".into());
            }
            verified = Some(page);
        }
        verified.ok_or_else(|| "Notion did not return a verifiable result for the selected page.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const ID: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn accepts_share_links_and_rejects_foreign_hosts_and_targets() {
        assert_eq!(canonical_page_id(&format!("https://team.notion.site/Notes-{ID}?pvs=4")).as_deref(), Some(ID));
        assert_eq!(canonical_page_id(&format!("https://app.notion.com/p/Notes-{ID}")).as_deref(), Some(ID));
        assert!(canonical_page_id(&format!("https://app.notion.com.attacker.example/{ID}")).is_none());
        assert_eq!(canonical_page_id("01234567-89ab-cdef-0123-456789abcdef").as_deref(), Some(ID));
        assert!(canonical_page_id(&format!("https://notion.so.attacker.example/{ID}")).is_none());
        assert!(canonical_page_id(&format!("https://me@notion.so/{ID}")).is_none());
        assert!(canonical_page_id("memory").is_none());
        assert!(canonical_page_id("--------------------------------").is_none());
        let mut args = json!({});
        bind_page(&mut args, ID).unwrap();
        assert_eq!(args["id"], page_url(ID));
        assert!(bind_page(&mut json!({"id":"ffffffffffffffffffffffffffffffff"}), ID).is_err());
        assert!(bind_page(&mut json!({"include_file_urls":true}), ID).is_err());
    }

    #[test]
    fn accepts_only_matching_page_content_and_hashes_the_read() {
        let result = json!({"url":page_url(ID), "title":"Design notes",
            "metadata":{"type":"page"}, "text":"<content>Selected note</content>"});
        let item = json!({"type":"text","text":result.to_string()});
        let page = Page::from_result(ID, &[item], None).unwrap();
        assert_eq!(page.title, "Design notes");
        assert_eq!(page.excerpt, "Selected note");
        assert_eq!(page.content_sha256, hex::encode(Sha256::digest(b"Selected note")));
        assert!(Page::from_result("ffffffffffffffffffffffffffffffff", &[], Some(&result)).is_err());
        assert!(Page::from_result(ID, &[], Some(&json!({"url":page_url(ID), "title":"Unverified"}))).is_err());
    }
}
