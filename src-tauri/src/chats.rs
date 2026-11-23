//! Durable Direct Chat history with owner-visible execution evidence.
//!
//! Chats and their messages/execution events are persisted to
//! `chats.json` under the app config directory (see `secrets::config_dir`),
//! so conversations survive closing and reopening PAIGE Desktop. Nothing
//! here spawns turns or mutates source files; it is display-oriented record
//! keeping. Commands are thin, typed wrappers over [`ChatsStore`].

use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::Path;

use crate::ask::AskReceipt;
use crate::broker::{self, BrokerState};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};
use crate::continuation::Checkpoint;
use crate::secrets::config_dir;
use crate::team::{Member, Team};
use crate::workflows::{research_artifact, ResearchArtifact, WorkflowRequest, WorkflowRun, WorkflowTemplate};
use std::collections::HashMap;

pub const STORE_VERSION: u32 = 1;

fn can_retry_openrouter_free(chat: &Chat, saved: &Checkpoint) -> bool {
    if chat.archived || saved.blocked.is_some() || saved.root.is_none() || !saved.workspace_access.0
        || saved.model.gateway != "omniroute" || saved.model.id != "auto/best-free"
        || saved.rounds != 0 || !saved.seen_calls.is_empty() || !saved.pending.is_empty()
        || saved.in_flight.is_some() || !saved.verified_edits.is_empty() || !saved.verified_effects.is_empty() {
        return false;
    }

    let events = || chat.execution.iter().filter(|event| event.turn_id.as_deref() == Some(&saved.turn_id));
    events().all(|event| event.kind == "model_turn"
        || (event.kind == "context" && event.tool == "project_memory" && event.outcome == "applied"))
        && events().next_back().is_some_and(|event|
            event.tool == "task_paused" && event.outcome == "paused"
                && event.gateway == saved.model.gateway && event.model == saved.model.id
                && event.summary == crate::ask::GATEWAY_STREAM_ERROR)
        && events().any(|event|
            event.tool == "select_model" && event.outcome == "selected"
                && event.gateway == saved.model.gateway && event.model == saved.model.id
                && serde_json::from_str::<serde_json::Value>(&event.details).ok().is_some_and(|details|
                    details["manual_pin_honored"].as_bool() == Some(false)))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn id_number(id: &str) -> Option<u64> {
    id.rsplit('-').next().and_then(|s| s.parse().ok())
}

// ---------------------------------------------------------------------------
// Persisted types
// ---------------------------------------------------------------------------

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub struct ChatToolEvent {
    pub tool: String,
    pub ok: bool,
    pub summary: String,
    pub timestamp: u64,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub struct ChatMessage {
    pub role: String, // "user" | "assistant"
    pub text: String,
    #[serde(default)]
    pub tool_events: Vec<ChatToolEvent>,
    #[serde(default)]
    pub model_label: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub gateway: Option<String>,
    #[serde(default)]
    pub turn_id: Option<String>,
}

/// One durable execution event ("What PAIGE did") for a turn/tool step.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub struct ExecutionEvent {
    pub timestamp: u64,
    pub kind: String, // "model_turn" | "tool"
    pub tool: String,
    pub gateway: String,
    pub model: String,
    pub summary: String,
    pub outcome: String,
    pub details: String, // compact JSON with rich detail (paths, diffs, bytes, …)
    pub actor: String,   // "human" | "agent" | "unknown"
    #[serde(default)]
    pub turn_id: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Chat {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    automatic_title: Option<bool>,
    pub created_at: u64,
    pub updated_at: u64,
    pub archived: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub team: Option<Team>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub specialists: Vec<Member>,
    #[serde(default)]
    pub project_roots: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    project_bound_from: Option<usize>,
    #[serde(default)]
    pub no_project: bool,
    #[serde(default)]
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub execution: Vec<ExecutionEvent>,
    #[serde(default)]
    pub recorded_turn_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub workflow_runs: Vec<WorkflowRun>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume: Option<ResumeStatus>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ResumeStatus {
    pub turn_id: String,
    pub rounds: usize,
    pub root: Option<String>,
    pub blocked: Option<String>,
    #[serde(default)]
    pub waiting_for_workspace: bool,
    #[serde(default)]
    pub restart_ready: bool,
    #[serde(default)]
    pub restart_waiting: bool,
    #[serde(default)]
    pub can_retry_openrouter_free: bool,
    pub read: bool,
    pub write: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ChatsFile {
    pub version: u32,
    pub chats: Vec<Chat>,
    #[serde(default)]
    pub active_id: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    checkpoints: HashMap<String, Checkpoint>,
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

pub struct ChatsStore {
    pub chats: Vec<Chat>,
    pub active_id: Option<String>,
    next_id: u64,
    checkpoints: HashMap<String, Checkpoint>,
}

#[derive(Serialize)]
pub(crate) struct ProjectMemoryEntry {
    pub(crate) chat_id: String,
    pub(crate) title: String,
    pub(crate) turn_id: String,
    request: String,
    reported_result: Option<String>,
    verified_steps: Vec<String>,
}

impl ChatsStore {
    pub fn lock_turn() -> Result<fs::File, String> {
        let lock = OpenOptions::new().create(true).read(true).write(true).truncate(false)
            .open(config_dir()?.join("chats-turn.lock"))
            .map_err(|e| format!("Could not open the active turn lock: {e}"))?;
        lock.try_lock().map_err(|e| format!("Could not acquire the active turn lock: {e}"))?;
        Ok(lock)
    }

    fn load(path: &Path) -> Result<Self, String> {
        let mut store = ChatsStore { chats: Vec::new(), active_id: None, next_id: 1, checkpoints: HashMap::new() };
        match fs::read(path) {
            Ok(bytes) => {
                let file: ChatsFile = serde_json::from_slice(&bytes)
                    .map_err(|_| "Chat history cannot be read. The original chats.json was preserved for recovery.".to_string())?;
                if file.version != STORE_VERSION {
                    return Err("Chat history uses an unsupported version. The original chats.json was preserved.".into());
                }
                store.chats = file.chats;
                store.active_id = file.active_id;
                store.checkpoints = file.checkpoints;
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(format!("Could not open chat history: {error}")),
        }
        if !store.chats.iter().any(|c| Some(c.id.as_str()) == store.active_id.as_deref()) {
            store.active_id = None;
        }
        store.next_id = store.chats.iter().filter_map(|c| id_number(&c.id)).max().unwrap_or(0) + 1;
        Ok(store)
    }

    fn save(&self, path: &Path) -> Result<(), String> {
        let body = ChatsFile {
            version: STORE_VERSION,
            chats: self.chats.clone(),
            active_id: self.active_id.clone(),
            checkpoints: self.checkpoints.clone(),
        };
        let json = serde_json::to_vec(&body)
            .map_err(|e| format!("serialize chats: {e}"))?;
        let tmp = path.with_extension("pending");
        (|| -> std::io::Result<()> {
            let mut file = fs::File::create(&tmp)?;
            file.write_all(&json)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&tmp, path)
        })().map_err(|e| format!("Could not save chat history: {e}"))
    }

    fn access<R>(write: bool, f: impl FnOnce(&mut Self) -> Result<R, String>) -> Result<R, String> {
        let dir = config_dir()?;
        let lock = OpenOptions::new().create(true).read(true).write(true).truncate(false)
            .open(dir.join("chats.lock")).map_err(|e| format!("Could not open chat history lock: {e}"))?;
        lock.lock().map_err(|e| format!("Could not lock chat history: {e}"))?;
        let path = dir.join("chats.json");
        let mut store = Self::load(&path)?;
        let result = f(&mut store)?;
        if write {
            store.save(&path)?;
        }
        Ok(result)
    }

    pub fn find(&self, id: &str) -> Option<&Chat> {
        self.chats.iter().find(|c| c.id == id)
    }

    pub fn find_mut(&mut self, id: &str) -> Option<&mut Chat> {
        self.chats.iter_mut().find(|c| c.id == id)
    }

    #[cfg(test)]
    fn touch(&mut self, id: &str, at: u64) {
        if let Some(c) = self.find_mut(id) {
            c.updated_at = at;
        }
    }

    pub fn new_chat(&mut self, project_roots: Vec<String>, at: u64) -> Chat {
        let id = format!("chat-{at:x}-{}", self.next_id);
        self.next_id += 1;
        let chat = Chat {
            id: id.clone(),
            title: "New chat".to_string(),
            automatic_title: Some(true),
            created_at: at,
            updated_at: at,
            archived: false,
            team: None,
            specialists: Vec::new(),
            project_bound_from: (!project_roots.is_empty()).then_some(0),
            project_roots,
            no_project: false,
            messages: Vec::new(),
            execution: Vec::new(),
            recorded_turn_ids: Vec::new(),
            workflow_runs: Vec::new(),
            resume: None,
        };
        self.chats.insert(0, chat.clone());
        self.active_id = Some(id.clone());
        chat
    }

    pub fn rename(&mut self, id: &str, title: &str, at: u64) -> Result<(), String> {
        let title = title.trim();
        if title.is_empty() {
            return Err("chat title must not be empty".to_string());
        }
        let c = self.find_mut(id).ok_or_else(|| format!("chat not found: {id}"))?;
        c.title = title.to_string();
        c.automatic_title = Some(false);
        c.updated_at = at;
        Ok(())
    }

    pub fn set_archived(&mut self, id: &str, archived: bool, at: u64) -> Result<(), String> {
        let c = self.find_mut(id).ok_or_else(|| format!("chat not found: {id}"))?;
        c.archived = archived;
        c.updated_at = at;
        Ok(())
    }

    pub fn delete(&mut self, id: &str) -> Result<(), String> {
        let pos = self.chats.iter().position(|c| c.id == id)
            .ok_or_else(|| format!("chat not found: {id}"))?;
        self.chats.remove(pos);
        self.checkpoints.remove(id);
        if self.active_id.as_deref() == Some(id) {
            self.active_id = self.chats.first().map(|c| c.id.clone());
        }
        Ok(())
    }

    pub fn list_sorted(&self) -> Vec<Chat> {
        let mut chats = self.chats.clone();
        for chat in &mut chats {
            for message in &mut chat.messages {
                if message.role == "assistant" && let Some(single) = crate::ask::repeated_answer(&message.text) {
                    message.text = single.to_owned();
                }
            }
            show_saved_linear_results(chat);
            chat.resume = self.checkpoints.get(&chat.id).map(|checkpoint| ResumeStatus {
                turn_id: checkpoint.turn_id.clone(), rounds: checkpoint.rounds, root: checkpoint.root.clone(),
                blocked: checkpoint.blocked.clone().or_else(|| checkpoint.restart_test.as_ref()
                    .filter(|test| test.started)
                    .map(|_| "Restart verification was already started. Check its receipts before running any new test.".into())),
                waiting_for_workspace: checkpoint.needs_workspace(),
                restart_ready: checkpoint.restart_test.as_ref().is_some_and(|test| test.ready() && checkpoint.blocked.is_none()),
                restart_waiting: checkpoint.restart_test.as_ref().is_some_and(|test| !test.started),
                can_retry_openrouter_free: can_retry_openrouter_free(chat, checkpoint),
                read: checkpoint.workspace_access.0, write: checkpoint.workspace_access.1,
            });
        }
        chats.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then_with(|| a.id.cmp(&b.id)));
        chats
    }

    /// Client-visible search: matches title and message text.
    pub fn search(&self, query: &str) -> Vec<Chat> {
        let q = query.trim().to_lowercase();
        self.list_sorted().into_iter().filter(|c| {
            q.is_empty()
                || c.title.to_lowercase().contains(&q)
                || c.team.as_ref().is_some_and(|team| team.members.iter().any(|member|
                    member.name.to_lowercase().contains(&q) || member.slug.to_lowercase().contains(&q)))
                || c.specialists.iter().any(|m| m.name.to_lowercase().contains(&q) || m.slug.contains(&q))
                || c.messages.iter().any(|m| m.text.to_lowercase().contains(&q))
        }).collect()
    }

    #[cfg(test)]
    fn append_messages(&mut self, id: &str, messages: Vec<ChatMessage>, at: u64) -> Result<(), String> {
        let c = self.find_mut(id).ok_or_else(|| format!("chat not found: {id}"))?;
        c.messages.extend(messages);
        c.updated_at = at;
        Ok(())
    }

    /// Only native Ask code can add work evidence. The renderer can read it.
    pub fn record(
        id: &str,
        receipt: &AskReceipt,
        message: Option<ChatMessage>,
    ) -> Result<(), String> {
        Self::record_with_checkpoint(id, receipt, message, None)
    }

    pub(crate) fn record_with_checkpoint(
        id: &str,
        receipt: &AskReceipt,
        message: Option<ChatMessage>,
        checkpoint: Option<&Checkpoint>,
    ) -> Result<(), String> {
        Self::access(true, |store| {
            {
                let chat = store.find_mut(id).ok_or("Conversation was not found.")?;
                if chat.recorded_turn_ids.contains(&receipt.turn_id) {
                    return Ok(());
                }
            }
            if let Some(checkpoint) = checkpoint {
                if checkpoint.turn_id != receipt.turn_id {
                    return Err("The receipt does not belong to the saved task.".into());
                }
                store.put_checkpoint(id, checkpoint)?;
            }
            let chat = store.find_mut(id).ok_or("Conversation was not found.")?;
            let r = receipt;
            chat.update_linear_title(r);
            chat.execution.push(ExecutionEvent {
                timestamp: r.timestamp,
                kind: r.kind.clone(),
                tool: r.tool.clone(),
                gateway: r.gateway.clone(),
                model: r.model.clone(),
                summary: r.summary.clone(),
                outcome: r.outcome.clone(),
                details: r.details.clone(),
                actor: r.actor.clone(),
                turn_id: Some(r.turn_id.clone()),
            });
            let terminal = message.is_some() || matches!(r.outcome.as_str(), "cancelled" | "failed") && r.tool == "turn_ended";
            if let Some(message) = message {
                chat.messages.push(message);
                chat.recorded_turn_ids.push(r.turn_id.clone());
            } else if matches!(r.outcome.as_str(), "cancelled" | "failed") && r.tool == "turn_ended" {
                chat.recorded_turn_ids.push(r.turn_id.clone());
            }
            chat.updated_at = now_ms();
            if terminal && store.checkpoints.get(id).is_some_and(|saved| saved.turn_id == r.turn_id) {
                store.checkpoints.remove(id);
            }
            Ok(())
        })
    }

    fn put_checkpoint(&mut self, id: &str, checkpoint: &Checkpoint) -> Result<(), String> {
        let chat = self.find(id).ok_or("Conversation was not found.")?;
        if chat.recorded_turn_ids.contains(&checkpoint.turn_id)
            || !chat.messages.iter().any(|message| message.role == "user" && message.turn_id.as_deref() == Some(&checkpoint.turn_id))
            || self.checkpoints.get(id).is_some_and(|saved| saved.turn_id != checkpoint.turn_id) {
            return Err("The saved coding task no longer belongs to this conversation.".into());
        }
        self.checkpoints.insert(id.into(), checkpoint.clone());
        Ok(())
    }

    pub(crate) fn save_checkpoint(id: &str, checkpoint: &Checkpoint) -> Result<(), String> {
        Self::access(true, |store| store.put_checkpoint(id, checkpoint))
    }

    pub(crate) fn checkpoint(id: &str, require_open: bool) -> Result<Option<Checkpoint>, String> {
        Self::access(false, |store| {
            let chat = store.find(id).ok_or("Conversation was not found.")?;
            if require_open && chat.archived {
                return Err("Unarchive the conversation before resuming the task.".into());
            }
            Ok(store.checkpoints.get(id).cloned())
        })
    }

    pub(crate) fn is_active(id: &str) -> Result<bool, String> {
        Self::access(false, |store| Ok(store.active_id.as_deref() == Some(id)))
    }

    pub(crate) fn can_retry_openrouter_free(id: &str, checkpoint: &Checkpoint) -> Result<bool, String> {
        Self::access(false, |store| {
            let chat = store.find(id).ok_or("Conversation was not found.")?;
            Ok(can_retry_openrouter_free(chat, checkpoint))
        })
    }

    pub fn team(id: &str) -> Result<Option<Team>, String> {
        Self::access(false, |store| {
            Ok(store.find(id).ok_or("Conversation was not found.")?.team.clone())
        })
    }

    pub(crate) fn specialists(id: &str) -> Result<Vec<Member>, String> {
        Self::access(false, |store| {
            Ok(store.find(id).ok_or("Conversation was not found.")?.specialists.clone())
        })
    }

    pub(crate) fn add_specialist(id: &str, member: Member) -> Result<Chat, String> {
        let _turn = Self::lock_turn()?;
        Self::access(true, |store| {
            let chat = store.find_mut(id).ok_or("Conversation was not found.")?;
            if chat.archived || chat.no_project || chat.project_roots.is_empty() {
                return Err("Open a chat with a pinned project before adding a specialist.".into());
            }
            if chat.specialists.iter().chain(chat.team.iter().flat_map(|team| &team.members))
                .any(|saved| saved.slug.eq_ignore_ascii_case(&member.slug)) {
                return Err(format!("This chat already has a specialist with the handle @{}.", member.slug));
            }
            if chat.specialists.len() >= 6 {
                return Err("This chat already has six named specialists.".into());
            }
            chat.specialists.push(member);
            chat.updated_at = now_ms();
            Ok(chat.clone())
        })
    }

    pub fn save_team(chat_id: Option<&str>, team: Team) -> Result<Chat, String> {
        let _turn = Self::lock_turn()?;
        Self::access(true, |store| {
            let id = chat_id.map(str::to_owned).unwrap_or_else(|| store.new_chat(vec![], now_ms()).id);
            let chat = store.find_mut(&id).ok_or("Conversation was not found.")?;
            if chat.archived {
                return Err("Unarchive the chat before adding specialists.".into());
            }
            if team.members.iter().any(|member| chat.specialists.iter()
                .any(|saved| saved.slug.eq_ignore_ascii_case(&member.slug))) {
                return Err("One of these handles already belongs to a specialist in this chat.".into());
            }
            if chat_id.is_none() { chat.no_project = true; }
            chat.team = Some(team);
            Ok(chat.clone())
        })
    }

    pub(crate) fn workspace(id: &str) -> Result<(Option<String>, bool), String> {
        Self::access(false, |store| {
            let chat = store.find(id).ok_or("Conversation was not found.")?;
            Ok((chat.project_roots.first().cloned(), chat.no_project))
        })
    }

    pub(crate) fn image_receipt(id: &str, turn_id: &str) -> Result<crate::visual_qa::Image, String> {
        Self::access(false, |store| {
            let chat = store.find(id).ok_or("Conversation was not found.")?;
            let event = chat.execution.iter().rev().find(|event| event.kind == "tool"
                && event.gateway == crate::chat_images::ROUTE && event.tool == crate::chat_images::RECEIPT
                && event.outcome == "verified" && event.turn_id.as_deref() == Some(turn_id))
                .ok_or("No verified image was saved for this turn.")?;
            let details: serde_json::Value = serde_json::from_str(&event.details)
                .map_err(|_| "The image receipt could not be read.")?;
            serde_json::from_value(details["image"].clone()).map_err(|_| "The image receipt is invalid.".into())
        })
    }

    pub(crate) fn last_listed_png(id: &str) -> Result<Option<String>, String> {
        Self::access(false, |store| {
            let chat = store.find(id).ok_or("Conversation was not found.")?;
            let bound_messages = &chat.messages[chat.project_bound_from.unwrap_or(0).min(chat.messages.len())..];
            let Some(event) = chat.execution.iter().rev().find(|event| event.tool == "list_files"
                && event.kind == "tool" && event.outcome == "ok"
                && bound_messages.iter().any(|msg| msg.role == "user" && msg.turn_id.is_some() && msg.turn_id == event.turn_id))
                else { return Ok(None); };
            let details: serde_json::Value = serde_json::from_str(&event.details)
                .map_err(|_| "The previous file list could not be read.")?;
            let mut names = details["files"].as_array().into_iter().flatten()
                .filter_map(|entry| entry["rel_path"].as_str())
                .filter_map(|path| crate::visual_qa::screenshot_path(path).ok())
                .collect::<Vec<_>>();
            names.sort_unstable();
            names.dedup();
            if names.len() == 1 { return Ok(names.pop()); }
            Ok(None)
        })
    }

    pub(crate) fn workflow_request(id: &str, turn_id: &str) -> Result<Option<WorkflowRequest>, String> {
        Self::access(false, |store| {
            let chat = store.find(id).ok_or("Conversation was not found.")?;
            Ok(chat.workflow_runs.iter().find(|run| run.turn_id == turn_id)
                .map(|run| run.request.clone()))
        })
    }

    pub(crate) fn visual_run(id: &str, turn_id: &str) -> Result<(WorkflowRun, Vec<ExecutionEvent>), String> {
        Self::access(false, |store| {
            let chat = store.find(id).ok_or("Conversation was not found.")?;
            let run = chat.workflow_runs.iter().find(|run| run.turn_id == turn_id
                && run.request.template == WorkflowTemplate::VisualQaV1 && run.request.visual.is_some())
                .ok_or("The Visual QA run was not found.")?;
            let events = chat.execution.iter().filter(|event| event.turn_id.as_deref() == Some(turn_id))
                .cloned().collect();
            Ok((run.clone(), events))
        })
    }

    pub(crate) fn word_report_for_open(id: &str, turn_id: &str) -> Result<(WorkflowRun, ResearchArtifact), String> {
        Self::access(false, |store| {
            let chat = store.find(id).ok_or("Conversation was not found.")?;
            let run = chat.workflow_runs.iter().find(|run| run.turn_id == turn_id && run.request.word_report())
                .ok_or("The Word report run was not found.")?;
            let artifact = research_artifact(chat, turn_id).ok_or("The Word report is not verified.")?;
            Ok((run.clone(), artifact))
        })
    }

    pub(crate) fn pending_research_artifact(id: &str, turn_id: &str) -> Result<Option<(WorkflowRun, ResearchArtifact)>, String> {
        Self::access(false, |store| {
            let chat = store.find(id).ok_or("Conversation was not found.")?;
            if chat.archived {
                return Err("Unarchive this conversation before accepting its artifact.".into());
            }
            let run = chat.workflow_runs.iter().find(|run| run.turn_id == turn_id
                && run.request.template == WorkflowTemplate::ResearchV1)
                .ok_or("The research run was not found.")?;
            if chat.execution.iter().any(|event| event.turn_id.as_deref() == Some(turn_id)
                && event.tool == "workflow_acceptance" && event.outcome == "accepted") {
                return Ok(None);
            }
            if !chat.execution.iter().any(|event| event.turn_id.as_deref() == Some(turn_id)
                && event.tool == "final_answer" && event.outcome == "complete") {
                return Err("Wait for the workflow to finish before reviewing the artifact.".into());
            }
            let artifact = research_artifact(chat, turn_id).ok_or("This run has no verified artifact preview.")?;
            Ok(Some((run.clone(), artifact)))
        })
    }

    pub(crate) fn record_workflow_decision(id: &str, turn_id: &str, mut event: ExecutionEvent) -> Result<(), String> {
        Self::access(true, |store| {
            let chat = store.find_mut(id).ok_or("Conversation was not found.")?;
            if chat.archived || !chat.workflow_runs.iter().any(|run| run.turn_id == turn_id) {
                return Err("The workflow conversation changed during confirmation.".into());
            }
            event.timestamp = now_ms();
            chat.updated_at = event.timestamp;
            chat.execution.push(event);
            Ok(())
        })
    }

    pub fn begin_turn(
        id: Option<&str>, turn_id: &str, text: String, may_send_while_paused: bool,
        workflow: Option<WorkflowRequest>,
    ) -> Result<(String, Vec<serde_json::Value>), String> {
        Self::access(true, |store| {
            let at = now_ms();
            let id = match id {
                Some(id) => id.to_string(),
                None => store.new_chat(vec![], at).id,
            };
            if store.checkpoints.contains_key(&id) && !may_send_while_paused {
                return Err("This task is paused. You can check workspace access, look up a Linear or GitHub issue, or read the selected Notion page here. To start other work in this conversation, resume or end the saved task first.".into());
            }
            let chat = store.find_mut(&id).ok_or("Conversation was not found.")?;
            if chat.archived {
                return Err("Unarchive the conversation before sending a message.".into());
            }
            let failed: std::collections::HashSet<_> = chat.execution.iter()
                .filter(|event| event.tool == "turn_ended" && event.outcome == "failed")
                .filter_map(|event| event.turn_id.as_deref())
                .collect();
            let completed: std::collections::HashSet<_> = chat.messages.iter()
                .filter(|m| m.role == "assistant")
                .filter_map(|m| m.turn_id.as_deref())
                .filter(|turn| !failed.contains(turn))
                .collect();
            let local_or_handoff_turns: std::collections::HashSet<_> = chat.execution.iter()
                .filter(|event| matches!(event.gateway.as_str(), "notion-mcp" | "github-mcp" | crate::web_search::ROUTE) || event.kind == "team_turn"
                    || (event.kind == "handoff" && event.tool != crate::team::DESKTOP_DELEGATE))
                .filter_map(|event| event.turn_id.as_deref())
                .collect();
            let history = chat.messages.iter()
                .filter(|m| !m.turn_id.as_deref().is_some_and(|turn| local_or_handoff_turns.contains(turn)))
                .filter(|m| matches!(m.role.as_str(), "user" | "assistant"))
                .filter(|m| m.turn_id.as_deref().is_none_or(|turn| completed.contains(turn)))
                .map(|m| {
                    let text = if m.role == "assistant" {
                        crate::ask::repeated_answer(&m.text).unwrap_or(&m.text)
                    } else { &m.text };
                    serde_json::json!({ "role": m.role, "content": text })
                })
                .collect();
            chat.title_from_request(&text);
            if let Some(request) = workflow {
                chat.workflow_runs.push(WorkflowRun {
                    turn_id: turn_id.into(), requested_at: at, request,
                });
            }
            chat.messages.push(ChatMessage {
                role: "user".into(), text, tool_events: vec![], model_label: None,
                model: None, gateway: None, turn_id: Some(turn_id.into()),
            });
            chat.updated_at = at;
            store.active_id = Some(id.clone());
            Ok((id, history))
        })
    }
    // The caller checks the live read grant. Recheck the persisted chat
    // binding under the store lock before inspecting any conversation.
    pub(crate) fn project_memory(id: &str, root: &str) -> Result<Vec<ProjectMemoryEntry>, String> {
        Self::access(false, |store| Ok(store.recent_project_memory(id, root)))
    }

    fn recent_project_memory(&self, id: &str, root: &str) -> Vec<ProjectMemoryEntry> {
        let belongs = |chat: &Chat| {
            !chat.no_project && !chat.archived
                && matches!(chat.project_roots.as_slice(), [pinned] if pinned == root)
        };
        if !self.find(id).is_some_and(belongs) {
            return Vec::new();
        }
        let mut other_chats: Vec<_> = self.chats.iter()
            .filter(|chat| chat.id != id && belongs(chat)).collect();
        other_chats.sort_by_key(|chat| std::cmp::Reverse(chat.updated_at));
        other_chats.into_iter().filter_map(|chat| chat.latest_project_memory(root)).take(3).collect()
    }
}

fn show_saved_linear_results(chat: &mut Chat) {
    let existing_replies: std::collections::HashSet<_> = chat.messages.iter()
        .filter(|message| message.role == "assistant")
        .filter_map(|message| message.turn_id.as_deref())
        .collect();
    let readbacks: Vec<_> = chat.messages.iter().enumerate().filter_map(|(index, message)| {
        let turn = message.turn_id.as_deref()?;
        if message.role != "user" || existing_replies.contains(turn)
            || !chat.execution.iter().any(|event| event.turn_id.as_deref() == Some(turn)
                && event.tool == "turn_ended" && event.outcome == "failed") {
            return None;
        }
        let identifier = crate::linear_lookup::requested_issue(&message.text).ok().flatten()?;
        let issue = chat.execution.iter().rev()
            .filter(|event| event.turn_id.as_deref() == Some(turn) && event.kind == "tool"
                && event.tool == "linear_get_issue" && event.outcome == "ok")
            .find_map(|event| {
                let result = serde_json::from_str(&event.details).ok()?;
                crate::linear_lookup::Issue::from_tool_result(&identifier, &result).ok()
            })?;
        let text = format!("Linear previously returned {} as “{}”. The issue is titled “{}”.",
            issue.identifier, issue.status, issue.title);
        Some((index + 1, ChatMessage {
            role: "assistant".into(), text, tool_events: vec![],
            model_label: Some("From the saved Linear receipt".into()), model: None,
            gateway: Some("linear-mcp".into()), turn_id: Some(turn.into()),
        }))
    }).collect();
    for (index, message) in readbacks.into_iter().rev() {
        chat.messages.insert(index, message);
    }
}

fn bounded_text(title: &str, limit: usize) -> String {
    let mut chars = title.chars();
    let prefix: String = chars.by_ref().take(limit).collect();
    if chars.next().is_some() {
        format!("{}…", prefix.trim_end())
    } else {
        prefix
    }
}

fn initial_title(message: &str) -> Option<String> {
    if let Ok(Some(issue)) = crate::linear_lookup::requested_issue(message) {
        return Some(format!("Linear {issue}"));
    }
    let title = crate::ask::unfenced_lines(message).flat_map(|line| line.split(". ")).find_map(|line| {
        let line = line.trim().trim_start_matches('#').trim_start();
        let mut title = line.split_whitespace().collect::<Vec<_>>().join(" ");
        for prefixes in [
            &["hey paige,", "hi paige,", "okay paige,", "paige,", "paige:"][..],
            &["please ", "could you ", "can you ", "would you ", "go ahead and "][..],
            &["please "][..],
        ] {
            let lower = title.to_ascii_lowercase();
            if let Some(prefix) = prefixes.iter().copied().find(|prefix| lower.starts_with(*prefix)) {
                title = title[prefix.len()..].trim_start().to_owned();
            }
        }
        let subject = title.trim_end_matches(['.', '?', '!']);
        (!subject.is_empty() && !crate::ask::is_greeting(subject)).then(|| subject.to_owned())
    })?;
    let mut chars = title.chars();
    let first = chars.next()?;
    let title = if title.starts_with('/') { title }
        else { format!("{}{}", first.to_uppercase(), chars.as_str()) };
    Some(bounded_text(&title, 72))
}

impl Chat {
    fn bind_project(&mut self, root: String) -> Result<(), String> {
        if let Some(saved) = self.project_roots.first() {
            if saved != &root { return Err("This chat is already pinned to a project.".into()); }
        } else {
            self.project_bound_from = Some(self.messages.len());
            self.project_roots.push(root);
        }
        self.no_project = false;
        Ok(())
    }

    fn title_from_request(&mut self, text: &str) {
        if !self.automatic_title.unwrap_or(self.messages.is_empty())
            || !matches!(self.title.as_str(), "New chat" | "New team chat") {
            return;
        }
        self.automatic_title = Some(true);
        if let Some(title) = initial_title(text) {
            self.title = title;
        }
    }

    fn update_linear_title(&mut self, receipt: &AskReceipt) {
        if self.automatic_title != Some(true) || receipt.kind != "tool"
            || receipt.tool != "linear_get_issue" || receipt.gateway != "linear-mcp" || receipt.outcome != "ok" {
            return;
        }
        let Ok(details) = serde_json::from_str::<serde_json::Value>(&receipt.details) else { return };
        let Some(issue) = details.pointer("/result/identifier").and_then(serde_json::Value::as_str)
            .and_then(crate::linear_lookup::issue_identifier) else { return };
        if self.title != format!("Linear {issue}") {
            return;
        }
        if let Some(title) = details.pointer("/result/title").and_then(serde_json::Value::as_str) {
            let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
            if !title.is_empty() {
                self.title = bounded_text(&format!("{issue} · {title}"), 72);
            }
        }
    }

    fn latest_project_memory(&self, root: &str) -> Option<ProjectMemoryEntry> {
        let mut entries = self.messages.iter().skip(self.project_bound_from.unwrap_or(0))
            .rev().filter(|message| message.role == "user").filter_map(|user| {
            let turn = user.turn_id.as_deref()?;
            let request = memory_excerpt(&user.text, 420)?;
            let events = self.execution.iter().filter(|event| event.turn_id.as_deref() == Some(turn));
            // Old saved conversations do not say which messages preceded the
            // pin. Include only turns with durable evidence of this project.
            if self.project_bound_from.is_none()
                && !self.workflow_runs.iter().any(|run| run.turn_id == turn && run.request.workspace_root == root)
                && !events.clone().any(|event| event.kind == "tool" && event.tool == "run_command"
                    && serde_json::from_str::<serde_json::Value>(&event.details).ok().is_some_and(|details|
                        details["exit_code"].as_i64().is_some() && details["cwd"].as_str() == Some(root))) {
                return None;
            }
            let completed = events.clone().any(|event| event.tool == "final_answer" && event.outcome == "complete"
                && matches!(event.kind.as_str(), "model_turn" | "workflow"));
            let mut verified_steps: Vec<_> = events.rev().filter_map(memory_step).take(4).collect();
            if !completed && verified_steps.is_empty() {
                return None;
            }
            verified_steps.reverse();
            let reported_result = if completed {
                self.messages.iter().rev().find(|message|
                    message.role == "assistant" && message.turn_id.as_deref() == Some(turn)
                ).and_then(|message| memory_excerpt(&message.text, 620))
            } else {
                None
            };
            Some(ProjectMemoryEntry {
                chat_id: self.id.clone(), title: bounded_text(&self.title, 72), turn_id: turn.to_owned(),
                request, reported_result, verified_steps,
            })
        });
        let latest = entries.next()?;
        if !latest.verified_steps.is_empty() {
            return Some(latest);
        }
        // A later acknowledgment should not displace the work it followed.
        entries.find(|entry| !entry.verified_steps.is_empty()).or(Some(latest))
    }
}

fn memory_step(event: &ExecutionEvent) -> Option<String> {
    let label = match (event.kind.as_str(), event.tool.as_str(), event.outcome.as_str()) {
        ("plan", "submit_workbench_plan", "ok") => "Workbench plan saved",
        ("tool", "write_file", "committed_verified" | "target_verified") => "File change verified by readback",
        ("tool", "run_command", "ok" | "error") => {
            let details: serde_json::Value = serde_json::from_str(&event.details).ok()?;
            if details["timed_out"] == true { "Command timed out" }
            else if details["exit_code"].as_i64()? == 0 { "Command exited successfully" }
            else { "Command exited with an error" }
        }
        _ => return None,
    };
    Some(label.to_owned())
}

fn memory_excerpt(text: &str, limit: usize) -> Option<String> {
    if crate::ask::is_greeting(text) || text.trim().starts_with('/') { return None; }
    let mut prose = String::new();
    for line in crate::ask::unfenced_lines(text).map(str::trim) {
        // File bodies, command output and connector payloads stay in receipts.
        if line.is_empty() || line.starts_with(['>', '{', '[']) { continue; }
        if !prose.is_empty() { prose.push(' '); }
        prose.extend(line.chars().take(limit + 1));
        if prose.chars().count() > limit { break; }
    }
    (!prose.is_empty()).then(|| bounded_text(&prose, limit))
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct ChatListResult {
    pub chats: Vec<Chat>,
    pub active_id: Option<String>,
}

#[tauri::command]
pub(crate) fn chat_read_image(chat_id: String, turn_id: String) -> Result<tauri::ipc::Response, String> {
    let image = ChatsStore::image_receipt(&chat_id, &turn_id)?;
    let bytes = crate::visual_qa::saved_png(&chat_id, &turn_id, &image.sha256)?;
    Ok(tauri::ipc::Response::new(bytes))
}

#[tauri::command]
pub fn chat_list() -> Result<ChatListResult, String> {
    ChatsStore::access(false, |store| Ok(ChatListResult {
        chats: store.list_sorted(),
        active_id: store.active_id.clone(),
    }))
}

#[tauri::command]
pub fn chat_search(query: String) -> Result<Vec<Chat>, String> {
    ChatsStore::access(false, |store| Ok(store.search(&query)))
}

#[tauri::command]
pub fn chat_new(app: AppHandle, no_project: Option<bool>) -> Result<Chat, String> {
    let _turn = ChatsStore::lock_turn()?;
    let no_project = no_project.unwrap_or(false);
    if no_project {
        broker::suspend_session_project(app.state::<Mutex<BrokerState>>())?;
    }
    ChatsStore::access(true, |store| {
        let id = store.new_chat(vec![], now_ms()).id;
        let chat = store.find_mut(&id).ok_or("Conversation was not found.")?;
        chat.no_project = no_project;
        Ok(chat.clone())
    })
}

#[tauri::command]
pub fn chat_select(app: AppHandle, id: Option<String>) -> Result<(), String> {
    // A view may still change during a running turn. Restore the workspace only
    // while the turn lock is held and inside the same chat-store transaction.
    let turn = ChatsStore::lock_turn().ok();
    ChatsStore::access(id.is_some(), |store| {
        if let Some(id) = id {
            store.find(&id).ok_or("Conversation was not found.")?;
            store.active_id = Some(id);
        }
        if turn.is_some() {
            if let Some(chat) = store.active_id.as_deref().and_then(|id| store.find(id))
                .filter(|chat| !chat.archived) {
                let broker_state = app.state::<Mutex<BrokerState>>();
                if let Some(root) = chat.project_roots.first() {
                    let restored = broker::select_session_project(&app, broker_state, root, false)?;
                    // Re-register the pinned git root as a session-scoped
                    // workflow folder so read-only Git tools work on it after a
                    // restart — but only when the broker actually authorized
                    // the folder for this session. Mutating Git commands keep
                    // the stricter owner-granted boundary; this entry is never
                    // persisted and never un-revokes a revoked grant.
                    if restored && let Some(canon) = std::fs::canonicalize(root).ok().filter(|p| p.is_dir()) {
                        crate::project::register_open_folder(&app, &canon, root);
                    }
                } else if chat.no_project {
                    broker::suspend_session_project(broker_state)?;
                }
            }
        }
        Ok(())
    })
}

#[tauri::command]
pub fn chat_pin_workspace(app: AppHandle, id: Option<String>, root: String) -> Result<Chat, String> {
    let _turn = ChatsStore::lock_turn()?;
    let active = broker::broker_status(app.state::<Mutex<BrokerState>>());
    if !active.read || active.root.as_deref() != Some(root.as_str()) {
        return Err("Authorize the project before pinning it to this chat.".into());
    }
    ChatsStore::access(true, |store| {
        let id = id.unwrap_or_else(|| store.new_chat(vec![], now_ms()).id);
        if let Some(saved) = store.checkpoints.get(&id) {
            match saved.root.as_deref() {
                Some(workspace) if workspace != root =>
                    return Err("This chat has a saved task in a different project.".into()),
                None if !saved.needs_workspace() =>
                    return Err("Resume or end this paused task before pinning a project.".into()),
                _ => {}
            }
        }
        let chat = store.find_mut(&id).ok_or("Conversation was not found.")?;
        if chat.archived {
            return Err("Unarchive the chat before pinning a project.".into());
        }
        chat.bind_project(root)?;
        let chat = chat.clone();
        store.active_id = Some(id);
        Ok(chat)
    })
}

#[tauri::command]
pub fn chat_rename(id: String, title: String) -> Result<Chat, String> {
    ChatsStore::access(true, |s| {
        s.rename(&id, &title, now_ms())?;
        Ok(s.find(&id).cloned().ok_or_else(|| format!("chat not found: {id}"))?)
    })
}

#[tauri::command]
pub fn chat_archive(id: String, archived: bool) -> Result<Chat, String> {
    ChatsStore::access(true, |s| {
        s.set_archived(&id, archived, now_ms())?;
        Ok(s.find(&id).cloned().ok_or_else(|| format!("chat not found: {id}"))?)
    })
}

#[tauri::command]
pub fn chat_delete(id: String) -> Result<(), String> {
    let _turn_guard = ChatsStore::lock_turn()?;
    ChatsStore::access(true, |s| s.delete(&id))?;
    crate::visual_qa::delete_chat_captures(&id)
        .map_err(|error| format!("The chat was deleted. {error}"))
}

#[tauri::command]
pub fn chat_export(id: String) -> Result<String, String> {
    ChatsStore::access(false, |store| {
        let chat = store.find(&id).ok_or("Conversation was not found.")?;
        serde_json::to_string_pretty(chat).map_err(|e| e.to_string())
    })
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_use_the_task_and_truncate_unicode_safely() {
        assert_eq!(initial_title("Hey PAIGE, could you fix the Git preview?\nMore detail").as_deref(), Some("Fix the Git preview"));
        assert_eq!(initial_title("Hi PAIGE"), None);
        assert_eq!(initial_title("/linear ant-174").as_deref(), Some("Linear ANT-174"));
        assert_eq!(initial_title(&"日".repeat(80)).unwrap().chars().count(), 73);
    }

    #[test]
    fn verified_issue_title_never_overwrites_a_manual_title() {
        let mut store = fresh_store();
        let id = store.active_id.clone().unwrap();
        let receipt = AskReceipt {
            timestamp: 1, turn_id: "linear-turn".into(), kind: "tool".into(),
            tool: "linear_get_issue".into(), gateway: "linear-mcp".into(),
            model: String::new(), summary: "Linear returned ANT-174".into(), outcome: "ok".into(),
            details: r#"{"result":{"identifier":"ANT-174","title":"Git diff previews"}}"#.into(),
            actor: "agent".into(),
        };
        let chat = store.find_mut(&id).unwrap();
        chat.title_from_request("/linear ANT-174");
        chat.update_linear_title(&receipt);
        assert_eq!(chat.title, "ANT-174 · Git diff previews");
        store.rename(&id, "Linear ANT-174", 3100).unwrap();
        let chat = store.find_mut(&id).unwrap();
        chat.update_linear_title(&receipt);
        assert_eq!(chat.title, "Linear ANT-174");
        store.rename(&id, "New chat", 3200).unwrap();
        let chat = store.find_mut(&id).unwrap();
        chat.title_from_request("Change the default");
        assert_eq!(chat.title, "New chat");
    }

    fn fresh_store() -> ChatsStore {
        let mut s = ChatsStore { chats: Vec::new(), active_id: None, next_id: 1, checkpoints: HashMap::new() };
        let a = s.new_chat(vec!["C:\\repo".into()], 1_000);
        let b = s.new_chat(vec![], 2_000);
        assert_eq!(a.id != b.id, true);
        assert_eq!(s.active_id.as_deref(), Some(b.id.as_str()));
        s
    }

    #[test]
    fn project_memory_survives_reopen_and_only_reads_the_exact_active_project() {
        let mut store = fresh_store();
        let root = r"C:\Desktop";
        let source = store.new_chat(vec![root.into()], 3_000);
        let sibling = store.new_chat(vec![r"C:\Desktop-other".into()], 4_000);
        let archived = store.new_chat(vec![root.into()], 5_000);
        let destination = store.new_chat(vec![root.into()], 6_000);
        let unpinned = store.new_chat(vec![], 6_100);
        for (chat, text) in [(&source, "Keep the preview limit at 2,000 characters."),
            (&sibling, "Unrelated project content"), (&archived, "Archived discussion"),
            (&unpinned, "Conversation from before the project was selected")] {
            let chat = store.find_mut(&chat.id).unwrap();
            chat.messages.push(ChatMessage {
                role: "user".into(), text: text.into(), tool_events: vec![],
                model_label: None, model: None, gateway: None, turn_id: Some("task".into()),
            });
            chat.execution.push(ExecutionEvent {
                timestamp: 1, kind: "tool".into(), tool: "run_command".into(),
                gateway: "test".into(), model: String::new(), summary: "Test".into(),
                outcome: "ok".into(), details: serde_json::json!({
                    "exit_code":0, "stdout":"unshared command output", "cwd":root,
                }).to_string(),
                actor: "agent".into(), turn_id: Some("task".into()),
            });
        }
        store.find_mut(&unpinned.id).unwrap().bind_project(root.into()).unwrap();
        store.set_archived(&archived.id, true, 7_000).unwrap();
        let source_chat = store.find_mut(&source.id).unwrap();
        source_chat.messages.push(ChatMessage {
            role: "user".into(), text: "Thanks for the fix.".into(), tool_events: vec![],
            model_label: None, model: None, gateway: None, turn_id: Some("thanks".into()),
        });
        source_chat.execution.push(ExecutionEvent {
            timestamp: 2, kind: "model_turn".into(), tool: "final_answer".into(),
            gateway: "test".into(), model: String::new(), summary: "Done".into(),
            outcome: "complete".into(), details: "{}".into(), actor: "agent".into(), turn_id: Some("thanks".into()),
        });
        let dir = std::env::temp_dir().join(format!("paige-project-memory-{}-{}", std::process::id(), now_ms()));
        fs::create_dir(&dir).unwrap();
        let path = dir.join("chats.json");
        store.save(&path).unwrap();
        let mut reopened = ChatsStore::load(&path).unwrap();
        let memory = reopened.recent_project_memory(&destination.id, root);
        assert_eq!(memory.len(), 1);
        assert_eq!(memory[0].chat_id, source.id);
        assert_eq!(memory[0].request, "Keep the preview limit at 2,000 characters.");
        assert_eq!(memory[0].verified_steps, ["Command exited successfully"]);
        assert!(!serde_json::to_string(&memory).unwrap().contains("unshared command output"));
        assert!(reopened.recent_project_memory(&destination.id, r"C:\Desktop-other").is_empty());
        // A legacy chat can provide exact worktree evidence from a native
        // command receipt, but a legacy prose-only turn cannot prove scope.
        reopened.find_mut(&source.id).unwrap().project_bound_from = None;
        let legacy = reopened.recent_project_memory(&destination.id, root);
        assert_eq!(legacy.len(), 1);
        assert_eq!(legacy[0].turn_id, "task");
        assert_eq!(memory_excerpt("Discuss the plan.\n```\nquoted code\n```", 80).as_deref(), Some("Discuss the plan."));
        fs::remove_file(path).unwrap();
        fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn openrouter_retry_requires_a_failed_auto_attempt_without_tool_activity() {
        let mut store = fresh_store();
        let id = store.active_id.clone().unwrap();
        let chat = store.find_mut(&id).unwrap();
        let mut saved = Checkpoint::new("turn", crate::gateway::ModelCandidate {
            id: "auto/best-free".into(), gateway: "omniroute".into(), coding_tier: 4,
            tools: true, context_bytes: 128_000,
        }, Some("C:\\repo".into()), vec![]);
        saved.workspace_access = (true, true);
        let selected = ExecutionEvent {
            timestamp: 1, kind: "model_turn".into(), tool: "select_model".into(),
            gateway: saved.model.gateway.clone(), model: saved.model.id.clone(),
            outcome: "selected".into(), summary: "Selected".into(), actor: "agent".into(),
            details: r#"{"manual_pin_honored":false}"#.into(), turn_id: Some(saved.turn_id.clone()),
        };
        chat.execution.push(selected.clone());
        assert!(!can_retry_openrouter_free(chat, &saved));
        let paused = ExecutionEvent {
            tool: "task_paused".into(), outcome: "paused".into(), summary: crate::ask::GATEWAY_STREAM_ERROR.into(),
            ..selected.clone()
        };
        chat.execution.push(ExecutionEvent { turn_id: Some("other turn".into()), ..paused.clone() });
        assert!(!can_retry_openrouter_free(chat, &saved));
        chat.execution.push(paused.clone());
        assert!(can_retry_openrouter_free(chat, &saved));
        chat.execution.insert(1, ExecutionEvent {
            kind: "context".into(), tool: "project_memory".into(), outcome: "applied".into(),
            ..selected.clone()
        });
        assert!(can_retry_openrouter_free(chat, &saved));

        chat.execution[0].details = r#"{"manual_pin_honored":true}"#.into();
        assert!(!can_retry_openrouter_free(chat, &saved));
        chat.execution[0] = selected;
        saved.verified_effects.insert("verified".into());
        assert!(!can_retry_openrouter_free(chat, &saved));
        saved.verified_effects.clear();
        chat.execution.insert(1, ExecutionEvent { kind: "tool".into(), ..paused });
        assert!(!can_retry_openrouter_free(chat, &saved));
    }

    #[test]
    fn reads_back_a_failed_linear_turn_without_changing_its_saved_messages() {
        let mut store = fresh_store();
        let id = store.active_id.clone().unwrap();
        let chat = store.find_mut(&id).unwrap();
        chat.messages.push(ChatMessage {
            role: "user".into(), text: "Look up ANT-171 in Linear and show me the tool receipt.”".into(),
            tool_events: vec![], model_label: None, model: None, gateway: None, turn_id: Some("turn".into()),
        });
        let issue = serde_json::json!({ "id": "ANT-171", "title": "A verified issue", "status": "In Progress" });
        let result = serde_json::json!({ "content": [{ "type": "text", "text": issue.to_string() }] });
        let received = ExecutionEvent {
            timestamp: 1, kind: "tool".into(), tool: "linear_get_issue".into(), gateway: "openrouter-direct".into(),
            model: String::new(), summary: "Linear read".into(), outcome: "ok".into(),
            details: result.to_string(), actor: "agent".into(), turn_id: Some("turn".into()),
        };
        chat.execution.push(received.clone());
        assert_eq!(store.list_sorted()[0].messages.len(), 1);
        store.find_mut(&id).unwrap().execution.push(ExecutionEvent {
            tool: "turn_ended".into(), outcome: "failed".into(), kind: "model_turn".into(), ..received
        });
        let visible = store.list_sorted();
        assert_eq!(visible[0].messages[1].model_label.as_deref(), Some("From the saved Linear receipt"));
        assert!(visible[0].messages[1].text.contains("A verified issue"));
        assert_eq!(store.find(&id).unwrap().messages.len(), 1);
        store.find_mut(&id).unwrap().messages[0].text = "Look up ANT-172 in Linear".into();
        assert_eq!(store.list_sorted()[0].messages.len(), 1);
    }

    #[test]
    fn list_is_sorted_by_updated_at_desc() {
        let mut s = fresh_store();
        let ids: Vec<String> = s.list_sorted().iter().map(|c| c.id.clone()).collect();
        s.touch(&ids[1], 9_000);
        let a = s.list_sorted();
        assert_eq!(a[0].id, ids[1]);
    }

    #[test]
    fn search_matches_title_and_messages() {
        let mut s = fresh_store();
        let c = s.new_chat(vec![], 3_000);
        s.rename(&c.id, "Billing chat", 3_100).unwrap();
        s.append_messages(&c.id, vec![ChatMessage {
            role: "user".into(),
            text: "Fix the invoice processor".into(),
            tool_events: vec![],
            model_label: None, model: None, gateway: None, turn_id: Some("t1".into()),
        }], 3_200).unwrap();
        assert_eq!(s.search("billing").len(), 1);
        assert_eq!(s.search("INVOICE").len(), 1);
        assert_eq!(s.search("nope").len(), 0);
    }

    #[test]
    fn archive_and_delete_update_store() {
        let mut s = fresh_store();
        let id = s.list_sorted()[0].id.clone();
        s.set_archived(&id, true, 4_000).unwrap();
        assert!(s.find(&id).unwrap().archived);
        s.delete(&id).unwrap();
        assert!(s.find(&id).is_none());
    }

    #[test]
    fn save_replaces_the_store_and_corruption_is_reported_without_overwrite() {
        let dir = std::env::temp_dir().join(format!("paige-chats-test-{}-{}", std::process::id(), now_ms()));
        fs::create_dir(&dir).unwrap();
        let path = dir.join("chats.json");
        let mut store = fresh_store();
        store.save(&path).unwrap();
        let id = store.active_id.clone().unwrap();
        store.rename(&id, "Persisted", 3_000).unwrap();
        store.save(&path).unwrap();
        assert_eq!(ChatsStore::load(&path).unwrap().find(&id).unwrap().title, "Persisted");
        fs::write(&path, b"{invalid").unwrap();
        assert!(ChatsStore::load(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"{invalid");
        fs::remove_file(path).unwrap();
        fs::remove_dir(dir).unwrap();
    }
}
