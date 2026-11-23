//! Durable progress for the existing Ask PAIGE Build loop.

use crate::gateway::ModelCandidate;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub(crate) struct FileEdit {
    pub(crate) path: String,
    pub(crate) sha256: String,
}

impl FileEdit {
    pub(crate) fn key(&self) -> (String, String) {
        (self.path.to_lowercase(), self.sha256.clone())
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct InFlight {
    pub(crate) id: String,
    pub(crate) tool: String,
    pub(crate) relative_path: Option<String>,
    pub(crate) edit: Option<FileEdit>,
    pub(crate) approved: bool,
    #[serde(default)]
    pub(crate) apply_started: bool,
    #[serde(default)]
    pub(crate) effect: Option<String>,
}

pub(crate) fn process_boot() -> &'static str {
    static BOOT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    BOOT.get_or_init(|| {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default().as_nanos();
        format!("{}-{nanos}", std::process::id())
    })
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct RestartTest {
    pub(crate) boot: String,
    pub(crate) relative_path: String,
    pub(crate) edit: FileEdit,
    pub(crate) started: bool,
}

impl RestartTest {
    pub(crate) fn ready(&self) -> bool {
        !self.started && self.boot != process_boot()
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Checkpoint {
    pub(crate) turn_id: String,
    pub(crate) model: ModelCandidate,
    pub(crate) root: Option<String>,
    pub(crate) messages: Vec<Value>,
    pub(crate) rounds: usize,
    pub(crate) seen_calls: HashSet<String>,
    pub(crate) verified_edits: HashSet<(String, String)>,
    pub(crate) pending: Vec<String>,
    pub(crate) in_flight: Option<InFlight>,
    pub(crate) blocked: Option<String>,
    #[serde(default)]
    pub(crate) provider_registrations: HashMap<String, String>,
    #[serde(default)]
    pub(crate) workspace_access: (bool, bool),
    #[serde(default)]
    pub(crate) waiting_for_workspace: bool,
    #[serde(default)]
    pub(crate) verified_effects: HashSet<String>,
    #[serde(default)]
    pub(crate) specialist: Option<crate::team::DesktopHandoff>,
    #[serde(default)]
    pub(crate) workbench_plan: Option<crate::workbench_plan::PlanState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) restart_test: Option<RestartTest>,
}

impl Checkpoint {
    pub(crate) fn new(turn_id: &str, model: ModelCandidate, root: Option<String>, messages: Vec<Value>) -> Self {
        Self {
            turn_id: turn_id.into(), model, root, messages, rounds: 0, seen_calls: HashSet::new(),
            verified_edits: HashSet::new(), pending: Vec::new(), in_flight: None, blocked: None,
            provider_registrations: HashMap::new(), verified_effects: HashSet::new(), workspace_access: (false, false),
            waiting_for_workspace: false, specialist: None, workbench_plan: None, restart_test: None,
        }
    }

    pub(crate) fn begin_round(&mut self, assistant: Value, ids: &[String]) -> Result<(), String> {
        if !self.pending.is_empty() || self.in_flight.is_some() {
            return Err("The previous tool round is unresolved.".into());
        }
        // Validate all IDs before mutating state
        let mut round_set = HashSet::new();
        for id in ids {
            if self.seen_calls.contains(id) || !round_set.insert(id) {
                return Err("The gateway repeated a tool-call ID. The call was not rerun.".into());
            }
        }
        // All checks passed, now insert
        for id in ids {
            self.seen_calls.insert(id.clone());
        }
        self.pending = ids.to_vec();
        self.messages.push(assistant);
        self.rounds += 1;
        Ok(())
    }

    pub(crate) fn settle(&mut self, id: &str, output: &str) -> Result<(), String> {
        if self.pending.first().is_none_or(|next| next != id) {
            return Err("The result does not match the saved tool call.".into());
        }
        self.messages.push(tool_result(id, output));
        self.pending.remove(0);
        self.in_flight = None;
        Ok(())
    }

    pub(crate) fn skip_pending(&mut self) {
        for id in self.pending.drain(..) {
            self.messages.push(tool_result(&id, r#"{"error":"This queued action did not run before the pause. Use the verified results to decide what is still needed."}"#));
        }
    }

    pub(crate) fn needs_workspace(&self) -> bool {
        if self.root.is_some() || self.workspace_access != (false, false)
            || self.in_flight.is_some() || !self.pending.is_empty() || !self.verified_edits.is_empty() {
            return false;
        }
        if self.waiting_for_workspace && self.verified_effects.is_empty() {
            return true;
        }

        // Older builds only recorded the denial as a normal tool error.
        // Adopt a grant only if the legacy task has no successful or unknown result.
        let mut denied_for_workspace = false;
        for message in self.messages.iter().filter(|message| message["role"] == "tool") {
            let Some(error) = message["content"].as_str()
                .and_then(|text| serde_json::from_str::<Value>(text).ok())
                .and_then(|result| result["error"].as_str().map(str::to_owned)) else {
                return false;
            };
            let is_workspace_denial = error == "no active authorization; grant a root first";
            let rejected_before_dispatch = is_workspace_denial
                || matches!(error.as_str(), "privileged/destructive command requires explicit owner approval" | "project root is not authorized")
                || (error.starts_with("command '") && error.ends_with("' not found in PATH"));
            if !rejected_before_dispatch {
                return false;
            }
            denied_for_workspace |= is_workspace_denial;
        }
        denied_for_workspace && self.verified_effects.is_empty()
    }

    pub(crate) fn accept_workspace(&mut self, root: Option<String>, read: bool, write: bool) -> Result<(), String> {
        if !self.needs_workspace() {
            return Err("This saved task cannot change its workspace.".into());
        }
        let root = root.filter(|_| read).ok_or("Authorize a project folder in Files before continuing.")?;
        self.root = Some(root);
        self.workspace_access = (read, write);
        self.waiting_for_workspace = false;
        Ok(())
    }

    pub(crate) fn completed_edit(&self, edit: &FileEdit) -> bool {
        self.verified_edits.contains(&edit.key())
    }
}

pub(crate) fn tool_result(id: &str, output: &str) -> Value {
    serde_json::json!({ "role": "tool", "tool_call_id": id, "content": output })
}

pub(crate) fn safe_to_interrupt(tool: &str) -> bool {
    matches!(tool, "read_file" | "list_files" | "git_status" | "git_log" | "git_diff" |
        "git_branches" | "git_worktrees" | "linear_get_issue" | crate::workbench_plan::RESEARCH)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Checkpoint {
        Checkpoint::new("turn-1", ModelCandidate {
            id: "fixture".into(), gateway: "test-harness".into(), tools: true, context_bytes: 128_000, coding_tier: 1,
        }, Some("C:\\lab".into()), Vec::new())
    }

    #[test]
    fn a_waiting_task_accepts_its_first_authorized_workspace_once() {
        let mut task = fixture();
        assert!(task.accept_workspace(Some(r"C:\other".into()), true, true).is_err());
        task.root = None;
        task.waiting_for_workspace = true;
        assert!(task.accept_workspace(None, false, false).is_err());
        assert!(task.accept_workspace(Some(r"C:\project".into()), false, true).is_err());
        task.accept_workspace(Some(r"C:\project".into()), true, true).unwrap();
        assert_eq!(task.root.as_deref(), Some(r"C:\project"));
        assert_eq!(task.workspace_access, (true, true));
        assert!(task.accept_workspace(Some(r"C:\other".into()), true, true).is_err());
    }

    #[test]
    fn legacy_workspace_denial_can_recover_only_when_settled_without_effects() {
        let mut task = fixture();
        task.root = None;
        task.messages.push(tool_result("legacy", r#"{"error":"no active authorization; grant a root first"}"#));
        assert!(task.needs_workspace());
        task.pending.push("unsettled".into());
        assert!(!task.needs_workspace());
        task.pending.clear();
        task.messages.push(tool_result("old-command", r#"{"error":"failed to wait for process"}"#));
        assert!(!task.needs_workspace());
        task.messages.pop();
        task.messages.push(tool_result("old-command", r#"{"exit_code":0}"#));
        assert!(!task.needs_workspace());
    }

    #[test]
    fn saved_protocol_keeps_verified_results_and_skips_unsent_calls() {
        let mut task = fixture();
        for round in 0..11 {
            let call = round.to_string();
            task.begin_round(serde_json::json!({ "role": "assistant" }), &[call.clone()]).unwrap();
            task.settle(&call, "verified").unwrap();
        }
        task.begin_round(serde_json::json!({ "role": "assistant" }), &["queued".into()]).unwrap();
        let mut restored: Checkpoint = serde_json::from_slice(&serde_json::to_vec(&task).unwrap()).unwrap();
        restored.skip_pending();
        assert_eq!(restored.rounds, 12);
        assert_eq!(restored.messages.iter().filter(|m| m["content"] == "verified").count(), 11);
        assert_eq!(restored.messages.last().unwrap()["tool_call_id"], "queued");
        assert!(restored.begin_round(Value::Null, &["0".into()]).is_err());
    }

    #[test]
    fn verified_file_reference_survives_a_restart_and_windows_path_case() {
        let mut task = fixture();
        let edit = FileEdit { path: "C:\\lab\\example.js".into(), sha256: "approved-bytes".into() };
        task.verified_edits.insert(edit.key());
        let restored: Checkpoint = serde_json::from_slice(&serde_json::to_vec(&task).unwrap()).unwrap();
        assert!(restored.completed_edit(&FileEdit { path: "c:\\LAB\\EXAMPLE.JS".into(), ..edit.clone() }));
        assert!(!restored.completed_edit(&FileEdit { sha256: "different-bytes".into(), ..edit }));
        assert!(!safe_to_interrupt("run_command"));
    }

    #[test]
    fn verified_effect_refuses_workspace_adoption_even_when_waiting() {
        let mut task = fixture();
        task.root = None;
        task.waiting_for_workspace = true;
        task.verified_effects.insert("verified-effect-1".into());
        assert!(!task.needs_workspace());
        assert!(task.accept_workspace(Some(r"C:\project".into()), true, true).is_err());
    }

    #[test]
    fn begin_round_rejects_duplicate_ids_without_polluting_seen_calls() {
        let mut task = fixture();
        task.begin_round(serde_json::json!({ "role": "assistant" }), &["call-1".into()]).unwrap();
        task.settle("call-1", "ok").unwrap();
        let err = task.begin_round(serde_json::json!({ "role": "assistant" }), &["call-2".into(), "call-2".into()]).unwrap_err();
        assert_eq!(err, "The gateway repeated a tool-call ID. The call was not rerun.");
        assert_eq!(task.rounds, 1);
        assert!(task.pending.is_empty());
        assert_eq!(task.seen_calls.len(), 1);
        assert!(task.seen_calls.contains("call-1"));
        assert!(!task.seen_calls.contains("call-2"));
        task.begin_round(serde_json::json!({ "role": "assistant" }), &["call-2".into()]).unwrap();
        assert_eq!(task.rounds, 2);
        assert_eq!(task.pending, vec!["call-2"]);
    }
}
