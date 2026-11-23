//! Specialists use the existing Ask turn and receipt lifecycle. Saved chat
//! specialists run in Desktop. Hosted members belong to the PAIGE MCP roster.
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tauri::{AppHandle, Manager};

use crate::{
    ask::{AskReceipt, publish_receipt},
    chats::ChatsStore,
    gateway::ModelCandidate,
    mcp::{
        McpCallResult, McpCallToolRequest, McpServerStatus, McpState, call_approved_team_tool,
        mcp_call_bound, mcp_status, team_registration,
    },
};

const ROSTER_TOOL: &str = "list_subagents";
const DELEGATE_TOOL: &str = "delegate_to_subagent";
const MAX_MEMBERS: usize = 6;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Member {
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Team {
    pub connection_id: String,
    registration: String,
    pub members: Vec<Member>,
}

pub(crate) const DESKTOP_DELEGATE: &str = "delegate_specialist";

pub(crate) fn announces_delegate(reply: &str) -> bool {
    let text = reply.trim_start().to_lowercase().replace('’', "'");
    ["i'll ", "i will ", "let me ", "i'm going to ", "i am going to ", "i'm ", "i am "].iter()
        .filter_map(|prefix| text.strip_prefix(prefix))
        .any(|action| ["delegate ", "delegating ", "call delegate_specialist", "ask a supporting specialist",
            "bring in a specialist", "bring in another specialist", "bring in a supporting specialist",
            "bring in another agent"].iter().any(|action_prefix| action.starts_with(action_prefix)))
}

// Only an imperative addressed to a known lead obliges a delegate call. A
// mention elsewhere in a question or a discussion of delegation does not.
pub(crate) fn requests_delegate(message: &str) -> bool {
    let Some((address, instruction)) = message.trim().split_once(char::is_whitespace) else { return false; };
    if mentioned_slugs(address).next().is_none() {
        return false;
    }
    let instruction = instruction.trim_start().to_lowercase();
    let instruction = instruction.strip_prefix("please ").unwrap_or(&instruction);
    (instruction.starts_with("delegate ") && [" to a supporting specialist", " to another specialist",
        " to a temporary specialist", " to another agent", " to a supporting agent"].iter().any(|target| instruction.contains(target)))
        || ["call delegate_specialist", "call the native delegate_specialist", "use delegate_specialist",
            "bring in a specialist", "bring in a supporting specialist", "bring in another specialist",
            "bring in another agent"].iter().any(|prefix| instruction.starts_with(prefix))
}

pub(crate) struct SupportingReadRequest {
    pub(crate) path: Option<String>,
}

pub(crate) fn requested_supporting_read(message: &str) -> Result<Option<SupportingReadRequest>, String> {
    let mut quoted = false;
    let clauses = message.split(|ch| {
        if ch == '"' {
            quoted = !quoted;
        }
        !quoted && matches!(ch, '.' | ';' | '\n')
    });
    for clause in clauses.map(str::trim_start) {
        let lower = clause.to_ascii_lowercase();
        let Some(prefix) = ["have them call ", "ask them to call ", "their task is to call "]
            .into_iter().find(|prefix| lower.starts_with(prefix)) else { continue; };
        let tool = &lower[prefix.len()..];
        let token = if tool.starts_with("`read_file`") { "`read_file`" } else { "read_file" };
        let Some(tail) = tool.strip_prefix(token) else { continue; };
        if !tail.is_empty() && !tail.starts_with(char::is_whitespace) {
            continue;
        }
        let tail = clause[prefix.len() + token.len()..].trim_start();
        let path = if tail.get(..9).is_some_and(|value| value.eq_ignore_ascii_case("with path")) {
            let path = tail[9..].trim_start().strip_prefix('"')
                .and_then(|rest| rest.split_once('"').map(|(path, _)| path))
                .filter(|path| !path.trim().is_empty())
                .ok_or("Use a quoted relative path after 'with path' for a supporting read.")?;
            Some(path.to_owned())
        } else {
            None
        };
        return Ok(Some(SupportingReadRequest { path }));
    }
    Ok(None)
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) enum SupportingRead { Required, RetrySent, Verified }

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct DesktopHandoff {
    pub(crate) lead: Member,
    pub(crate) assistant: Option<Member>,
    pub(crate) assistant_used: bool,
    #[serde(default)]
    pub(crate) handoff_retry_sent: bool,
    #[serde(default)]
    pub(crate) handoff_requested: bool,
    #[serde(default)]
    pub(crate) supporting_read: Option<SupportingRead>,
    #[serde(default)]
    pub(crate) supporting_read_path: Option<String>,
}

impl DesktopHandoff {
    pub(crate) fn new(lead: Member, handoff_requested: bool) -> Self {
        Self { lead, assistant: None, assistant_used: false, handoff_retry_sent: false,
            handoff_requested, supporting_read: None, supporting_read_path: None }
    }

    pub(crate) fn requires_supporting_read(&self) -> bool {
        self.assistant.is_some() && matches!(self.supporting_read,
            Some(SupportingRead::Required | SupportingRead::RetrySent))
    }

    pub(crate) fn matches_supporting_read_path(&self, workspace: Option<&str>, actual: &str) -> bool {
        let Some(path) = &self.supporting_read_path else { return true; };
        let relative = std::path::Path::new(path);
        let Some(root) = workspace else { return false; };
        if relative.as_os_str().is_empty() || relative.components()
            .any(|component| !matches!(component, std::path::Component::Normal(_))) {
            return false;
        }
        let expected = std::path::Path::new(root).join(relative);
        expected.to_string_lossy().replace('/', "\\") == actual.replace('/', "\\")
    }

    pub(crate) fn active(&self) -> &Member {
        self.assistant.as_ref().unwrap_or(&self.lead)
    }

    pub(crate) fn assign(&mut self, args: Value) -> Result<Member, String> {
        if self.assistant_used {
            return Err("This task has already used its one supporting specialist.".into());
        }
        let member = named_member(
            args["name"].as_str().unwrap_or_default(),
            args["assignment"].as_str().unwrap_or_default(),
        )?;
        if member.slug == self.lead.slug {
            return Err("Give the supporting specialist a different name.".into());
        }
        self.assistant_used = true;
        self.assistant = Some(member.clone());
        Ok(member)
    }
}

fn named_member(name: &str, description: &str) -> Result<Member, String> {
    let name = name.trim();
    let description = description.trim();
    let valid_text = |text: &str, max| {
        !text.is_empty() && text.chars().count() <= max
            && !text.chars().any(|ch| ch.is_control() || matches!(ch,
                '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'))
    };
    if !valid_text(name, 48) || !valid_text(description, 500) {
        return Err("Enter a name of up to 48 characters and a role of up to 500 characters.".into());
    }
    let slug = name.split(|ch: char| !ch.is_ascii_alphanumeric()).filter(|part| !part.is_empty())
        .collect::<Vec<_>>().join("-").to_ascii_lowercase();
    if slug.is_empty() || slug == "paige" {
        return Err("Choose a name with Latin letters or numbers that is different from PAIGE.".into());
    }
    Ok(Member { slug, name: name.into(), description: Some(description.into()) })
}

pub(crate) fn desktop_target(members: &[Member], message: &str) -> Result<Option<Member>, String> {
    let Some(first) = mentioned_slugs(message).find_map(|slug| members.iter()
        .find(|member| member.slug.eq_ignore_ascii_case(slug))) else { return Ok(None); };
    if mentioned_slugs(message).any(|slug| !first.slug.eq_ignore_ascii_case(slug)) {
        return Err("Choose one lead specialist in your message. They can bring in help during the task.".into());
    }
    Ok(Some(first.clone()))
}

#[tauri::command]
pub fn team_name_specialist(chat_id: String, name: String, role: String) -> Result<crate::chats::Chat, String> {
    ChatsStore::add_specialist(&chat_id, named_member(&name, &role)?)
}

impl Team {
    pub(crate) fn is_addressed(message: &str) -> bool {
        mentioned_slugs(message).next().is_some()
    }

    pub fn targets(&self, message: &str) -> Result<Vec<Member>, String> {
        if message.chars().count() > 2_000
            || message.chars().any(|c| {
                (c.is_control() && !matches!(c, '\n' | '\t' | '\r'))
                    || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
            })
        {
            return Err("Use up to 2,000 characters in a Team Chat message.".into());
        }
        let mut targets: Vec<Member> = Vec::new();
        for slug in mentioned_slugs(message) {
            let member = self
                .members
                .iter()
                .find(|member| member.slug.eq_ignore_ascii_case(slug))
                .ok_or_else(|| format!("@{slug} is not a member of this team."))?;
            if !targets.iter().any(|target| target.slug == member.slug) {
                targets.push(member.clone());
            }
        }
        if targets.is_empty() {
            return Err("Address a team member with @ to send a handoff.".into());
        }
        Ok(targets)
    }
}

fn mentioned_slugs(message: &str) -> impl Iterator<Item = &str> {
    message.split_whitespace().filter_map(|word| word.strip_prefix('@'))
        .map(|word| word.trim_end_matches(|c: char| c.is_ascii_punctuation() && c != '-' && c != '_'))
        .filter(|slug| !slug.is_empty() && slug.bytes().all(|ch|
            ch.is_ascii_alphanumeric() || matches!(ch, b'-' | b'_')))
}

pub(crate) fn accepts_model_route(schema: &Value) -> bool {
    let route = &schema["properties"]["model_route"];
    route["type"] == "object"
        && route["properties"]["gateway"]["type"] == "string"
        && route["properties"]["model"]["type"] == "string"
}

fn model_route(model: &ModelCandidate) -> Value {
    json!({ "gateway": model.gateway, "model": model.id })
}

fn confirms_model_route(reply: &Value, requested: &Value) -> bool {
    let route = &reply["model_route"];
    route["gateway"] == requested["gateway"] && route["model"] == requested["model"]
        && route["gateway"].is_string() && route["model"].is_string()
}

fn connection(app: &AppHandle, id: &str) -> Result<McpServerStatus, String> {
    mcp_status(app.state::<Mutex<McpState>>()).servers.into_iter().find(|server| {
        server.id == id && server.state == "ready"
            && server.capabilities.tools.iter().any(|tool| tool.name == ROSTER_TOOL)
    }).ok_or_else(|| "Reconnect the PAIGE orchestration connection in Models & Connections.".into())
}

fn routing_connection(app: &AppHandle, id: &str) -> Result<String, String> {
    let server = connection(app, id)?;
    let compatible = server.capabilities.tools.iter().any(|tool|
        tool.name == DELEGATE_TOOL && accepts_model_route(&tool.input_schema));
    if !compatible {
        return Err("This orchestration connection does not support the chat's model route.".into());
    }
    Ok(server.name.chars().filter(|c| !c.is_control()
        && !matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'))
        .take(120).collect())
}

fn call(
    app: &AppHandle,
    id: &str,
    registration: &str,
    tool: &str,
    arguments: Value,
) -> Result<McpCallResult, String> {
    mcp_call_bound(
        app.clone(),
        app.state::<Mutex<McpState>>(),
        McpCallToolRequest {
            connection_id: id.into(),
            tool: tool.into(),
            arguments,
        },
        Some(registration),
    )
}

fn body(result: &McpCallResult) -> Option<Value> {
    result
        .content
        .iter()
        .filter_map(|item| item.get("text").and_then(Value::as_str))
        .find_map(|text| {
            serde_json::from_str::<Value>(text)
                .ok()
                .filter(Value::is_object)
        })
        .and_then(|value| {
            if result.is_error {
                value["error"]
                    .as_str()
                    .and_then(|body| serde_json::from_str::<Value>(body).ok())
                    .or(Some(value))
            } else {
                Some(value)
            }
        })
}

fn handoff_reply(result: &McpCallResult) -> Option<Value> {
    if result.is_error {
        return body(result);
    }
    let structured = result.structured_content.iter().filter(|value| value.is_object()).cloned();
    let text = result.content.iter().filter(|item| item["type"] == "text")
        .filter_map(|item| item["text"].as_str())
        .filter_map(|text| serde_json::from_str::<Value>(text).ok())
        .filter(Value::is_object);
    let mut replies = structured.chain(text);
    let reply = replies.next()?;
    replies.all(|other| other == reply).then_some(reply)
}

fn roster(app: &AppHandle, id: &str, registration: &str) -> Result<Vec<Member>, String> {
    connection(app, id)?;
    let result = call(app, id, registration, ROSTER_TOOL, json!({}))?;
    let body = body(&result)
        .filter(|body| body["ok"] == true && !result.is_error)
        .ok_or("The connection did not return an available PAIGE roster.")?;
    let rows = body["matches"]
        .as_array()
        .ok_or("The PAIGE roster response was incomplete.")?;
    let mut members = Vec::new();
    for row in rows.iter().filter(|row| row["runtime"] == "soft") {
        let Ok(member) = serde_json::from_value::<Member>(row.clone()) else {
            continue;
        };
        if !member.slug.is_empty()
            && member.slug.len() <= 96
            && member
                .slug
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
            && !member.name.trim().is_empty()
            && member.name.chars().count() <= 120
            && !member.name.chars().any(char::is_control)
        {
            if !members
                .iter()
                .any(|existing: &Member| existing.slug.eq_ignore_ascii_case(&member.slug))
            {
                members.push(member);
            }
        }
    }
    Ok(members)
}

#[tauri::command]
pub async fn team_roster(app: AppHandle, connection_id: String) -> Result<Vec<Member>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let registration = team_registration(&app.state::<Mutex<McpState>>(), &connection_id)?;
        roster(&app, &connection_id, &registration)
    })
    .await
    .map_err(|_| "The roster request was interrupted.".to_string())?
}

#[tauri::command]
pub async fn team_create(
    app: AppHandle,
    connection_id: String,
    slugs: Vec<String>,
    chat_id: Option<String>,
) -> Result<crate::chats::Chat, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if !(2..=MAX_MEMBERS).contains(&slugs.len())
            || slugs.iter().collect::<std::collections::HashSet<_>>().len() != slugs.len()
        {
            return Err("Choose two to six different specialists.".into());
        }
        let registration = team_registration(&app.state::<Mutex<McpState>>(), &connection_id)?;
        let roster = roster(&app, &connection_id, &registration)?;
        let members = slugs
            .iter()
            .map(|slug| {
                roster
                    .iter()
                    .find(|member| &member.slug == slug)
                    .cloned()
                    .ok_or_else(|| format!("@{slug} is no longer available on this connection."))
            })
            .collect::<Result<Vec<_>, _>>()?;
        ChatsStore::save_team(chat_id.as_deref(), Team {
            connection_id,
            registration,
            members,
        })
    })
    .await
    .map_err(|_| "Team creation was interrupted.".to_string())?
}

pub(crate) fn validate_connection(app: &AppHandle, team: &Team) -> Result<(), String> {
    if team_registration(&app.state::<Mutex<McpState>>(), &team.connection_id)? != team.registration
    {
        return Err(
            "This team's connection was replaced. Create a team with the new connection.".into(),
        );
    }
    routing_connection(app, &team.connection_id).map(|_| ())
}

pub(crate) fn run(
    app: &AppHandle,
    team: &Team,
    targets: Vec<Member>,
    chat_id: &str,
    turn_id: &str,
    message: &str,
    model: &ModelCandidate,
    cancel: &AtomicBool,
) -> Result<String, String> {
    let connection_name = routing_connection(app, &team.connection_id)?;
    if model.id.len() > 240 || model.id.chars().any(|ch| ch.is_control()
        || matches!(ch, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'))
    {
        return Err("The selected model cannot be displayed safely in the approval dialog.".into());
    }
    let requested_route = model_route(model);
    // Recheck eligibility. A roster entry may have been disabled or changed
    // runtime since the team was saved.
    let current = roster(app, &team.connection_id, &team.registration)?;
    let mut outcomes = Vec::new();
    for member in targets {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        let record = |kind: &str, outcome: &str, summary: &str, detail: Value, actor: &str| {
            publish_receipt(
                app,
                Some(chat_id),
                AskReceipt {
                    timestamp: crate::ask::now_ms(),
                    turn_id: turn_id.into(),
                    kind: kind.into(),
                    tool: DELEGATE_TOOL.into(),
                    gateway: connection_name.clone(),
                    model: if outcome == "completed" { model.id.clone() } else { String::new() },
                    summary: summary.into(),
                    outcome: outcome.into(),
                    details: detail.to_string(),
                    actor: actor.into(),
                },
                None,
            )
        };
        let base = json!({
            "slug": member.slug, "name": member.name, "connection_id": team.connection_id,
            "requested_model_route": requested_route,
        });
        if !current
            .iter()
            .any(|entry| entry.slug == member.slug && entry.name == member.name)
        {
            record(
                "handoff",
                "unavailable",
                "Specialist is unavailable on this connection.",
                base,
                "agent",
            )?;
            outcomes.push(format!("@{} unavailable", member.slug));
            continue;
        }
        record(
            "handoff",
            "requested",
            "PAIGE prepared a handoff.",
            base.clone(),
            "agent",
        )?;
        let request_id = crate::ask::next_review_id();
        let review = crate::ask::await_review(app, turn_id, &request_id, cancel, || record(
            "approval", "awaiting_review", "Review this specialist handoff.", json!({
                "slug": member.slug, "name": member.name, "review_id": request_id,
                "path": connection_name, "arguments": { "message": message, "model_route": requested_route },
            }), "unknown",
        ));
        if let Err(error) = review {
            record("approval", "review_closed", &error, base.clone(), "unknown")?;
            return Err(error);
        }
        record(
            "approval", "awaiting_confirmation", "Confirm the specialist handoff in Windows.",
            base.clone(), "unknown",
        )?;
        let approved = crate::broker::native_confirm(
            &format!(
                "Send this message to @{} through {}?\n\nModel: {} via {}\n\n{}",
                member.slug, connection_name, model.id, model.gateway, message
            ),
            "PAIGE Desktop - Approve team handoff",
        );
        if !approved || cancel.load(Ordering::Relaxed) {
            let outcome = if approved { "cancelled" } else { "declined" };
            record(
                "approval",
                outcome,
                "The handoff was not sent.",
                base,
                "unknown",
            )?;
            outcomes.push(format!("@{} {outcome}", member.slug));
            continue;
        }
        record(
            "approval",
            "confirmed",
            "Native confirmation returned Yes. Identity unverified.",
            base.clone(),
            "unknown",
        )?;
        let eligible = roster(app, &team.connection_id, &team.registration).is_ok_and(|roster| {
            roster
                .iter()
                .any(|entry| entry.slug == member.slug && entry.name == member.name)
        });
        if !eligible || cancel.load(Ordering::Relaxed) {
            record(
                "handoff",
                "unavailable",
                "The specialist could not be reverified. The handoff was not sent.",
                base,
                "agent",
            )?;
            outcomes.push(format!("@{} not sent", member.slug));
            continue;
        }
        record(
            "handoff",
            "dispatching",
            "PAIGE is sending the approved message.",
            base.clone(),
            "agent",
        )?;
        let response = call_approved_team_tool(
            app.clone(),
            McpCallToolRequest {
                connection_id: team.connection_id.clone(), tool: DELEGATE_TOOL.into(),
                arguments: json!({
                    "slug": member.slug, "input": { "message": message }, "model_route": requested_route,
                }),
            },
            &team.registration,
        );
        let reply = response.as_ref().ok().and_then(handoff_reply);
        let matched = reply
            .as_ref()
            .filter(|reply| reply["subagent"] == member.slug && reply["runtime"] == "soft");
        let completed = matched
            .filter(|reply| {
                reply["ok"] == true && response.as_ref().is_ok_and(|result| !result.is_error)
                    && confirms_model_route(reply, &requested_route)
            })
            .and_then(|reply| {
                let invocation = reply["invocation_id"]
                    .as_str()
                    .filter(|id| !id.is_empty())?;
                let answer = reply["result"]["answer"]
                    .as_str()
                    .filter(|answer| !answer.trim().is_empty())?;
                Some((invocation, answer))
            });
        if let Some((invocation, answer)) = completed {
            record(
                "handoff",
                "completed",
                "Specialist returned a reply.",
                json!({
                    "slug": member.slug, "name": member.name,
                    "connection_id": team.connection_id, "invocation_id": invocation, "answer": answer,
                    "model_route": requested_route,
                }),
                "agent",
            )?;
            outcomes.push(format!("@{} replied", member.slug));
        } else {
            let failed = matched.is_some_and(|reply| reply["ok"] == false);
            let outcome = if failed { "failed" } else { "unverified" };
            record(
                "handoff",
                outcome,
                if failed {
                    "The orchestrator reported a failure."
                } else {
                    "The orchestrator did not confirm the handoff and its model route. Check the invocation before trying again."
                },
                base,
                "agent",
            )?;
            outcomes.push(format!("@{} {outcome}", member.slug));
            if !failed {
                break;
            }
        }
    }
    Ok(outcomes.join(" · "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn promised_handoff_is_distinct_from_a_result() {
        assert!(announces_delegate("I’ll delegate the file check."));
        assert!(announces_delegate("I am delegating the task to the specialist."));
        assert!(requests_delegate("@reviewer Please delegate the test to a supporting specialist."));
        assert!(requests_delegate("@reviewer Use delegate_specialist to verify the file."));
        assert!(!requests_delegate("@reviewer Explain how to delegate to another specialist."));
        assert!(!requests_delegate("@reviewer Do not delegate this task."));
        assert!(!requests_delegate("@reviewer Delegate the agent deployment to me."));
        let mut run = DesktopHandoff::new(named_member("Reviewer", "verify").unwrap(), true);
        let read = requested_supporting_read("Delegate the check. Have them call `read_file` with path \".gitignore\".").unwrap().unwrap();
        assert_eq!(read.path.as_deref(), Some(".gitignore"));
        run.supporting_read_path = read.path;
        assert!(run.matches_supporting_read_path(Some(r"C:\work"), r"C:\work\.gitignore"));
        assert!(!run.matches_supporting_read_path(Some(r"C:\work"), r"C:\work\README.md"));
        assert!(!run.matches_supporting_read_path(Some(r"C:\work"), r"C:\else\.gitignore"));
        assert!(!run.matches_supporting_read_path(Some(r"C:\work"), r"C:\work\.GITIGNORE"));
        let dotted = requested_supporting_read("Have them call read_file with path \"notes. final.md\".").unwrap().unwrap();
        assert_eq!(dotted.path.as_deref(), Some("notes. final.md"));
        assert!(requested_supporting_read("Have them call read_file with path \"unfinished").is_err());
        assert!(requested_supporting_read("Do not have them call read_file.").unwrap().is_none());
        assert!(requested_supporting_read("Have them call read_file_metadata instead.").unwrap().is_none());
        assert!(announces_delegate("Let me bring in a specialist."));
        assert!(!announces_delegate("I delegated the file check and got the result."));
        assert!(!announces_delegate("I’ll explain how to delegate the file check."));
        assert!(!announces_delegate("I’ll bring in the latest test results."));
    }

    #[test]
    fn desktop_handoff_keeps_one_child_after_serializing() {
        let lead = named_member("Test Reviewer", "Review the pinned project").unwrap();
        let mut run = DesktopHandoff::new(lead, true);
        assert_eq!(run.active().slug, "test-reviewer");
        assert!(run.assign(json!({"name": "Test Reviewer", "assignment": "Recheck it"})).is_err());
        assert!(run.assign(json!({"name": "Verifier", "assignment": "  "})).is_err());
        assert!(run.assign(json!({"name": "Verifier", "assignment": "Read the fixture"})).is_ok());
        let mut saved: DesktopHandoff = serde_json::from_value(serde_json::to_value(run).unwrap()).unwrap();
        assert_eq!(saved.active().slug, "verifier");
        assert!(saved.assign(json!({"name": "Another", "assignment": "Run a check"})).is_err());
        saved.assistant.take();
        assert_eq!(saved.active().slug, "test-reviewer");
        assert!(saved.assign(json!({"name": "Another", "assignment": "Run a check"})).is_err());
    }

    #[test]
    fn desktop_mentions_use_chat_handles_and_one_lead() {
        let members = [named_member("Test Reviewer", "Review").unwrap(), named_member("Verifier", "Verify").unwrap()];
        assert!(desktop_target(&members, "Read @src/main.rs").unwrap().is_none());
        assert_eq!(desktop_target(&members, "@Test-Reviewer please check @test-reviewer.").unwrap().unwrap().slug,
            "test-reviewer");
        assert!(desktop_target(&members, "@test-reviewer @verifier check this").is_err());
        assert!(desktop_target(&members, "@test-reviewer @unknown check this").is_err());
        assert!(named_member("PAIGE", "Verify").is_err());
    }

    #[test]
    fn handoff_requires_a_declared_and_confirmed_route() {
        let legacy = json!({"properties": {"input": {"type": "object"}}});
        assert!(!accepts_model_route(&legacy));
        let capable = json!({"properties": {"model_route": {"type":"object","properties": {
            "gateway":{"type":"string"}, "model":{"type":"string"}
        }}}});
        assert!(accepts_model_route(&capable));

        let auto = json!({"gateway":"omniroute", "model":"auto/best-free"});
        let pinned = json!({"gateway":"openrouter-direct", "model":"openai/gpt-5.4"});
        for route in [&auto, &pinned] {
            assert!(confirms_model_route(&json!({"model_route": route}), route));
            assert!(!confirms_model_route(&json!({"ok":true}), route));
        }
        assert!(!confirms_model_route(&json!({"model_route": auto}), &pinned));

        let mut result = McpCallResult {
            connection_id: "fixture".into(), tool: DELEGATE_TOOL.into(), content: vec![],
            is_error: false, structured_content: Some(json!({"model_route": auto})),
        };
        assert!(confirms_model_route(&handoff_reply(&result).unwrap(), &auto));
        result.content.push(json!({"type":"text","text":json!({"model_route": pinned}).to_string()}));
        assert!(handoff_reply(&result).is_none());
    }

    #[test]
    fn mentions_only_select_known_members_once_in_message_order() {
        let team = Team {
            connection_id: "fixture".into(),
            registration: "fixture-registration".into(),
            members: ["research", "review"]
                .iter()
                .map(|slug| Member {
                    slug: (*slug).into(),
                    name: (*slug).into(),
                    description: None,
                })
                .collect(),
        };
        assert_eq!(
            team.targets("@Review, please ask @research. @review")
                .unwrap()
                .iter()
                .map(|member| member.slug.as_str())
                .collect::<Vec<_>>(),
            vec!["review", "research"]
        );
        assert!(team.targets("person@review.example").is_err());
        assert!(team.targets("@nobody").is_err());
    }
}
