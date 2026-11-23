use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io::Read, path::Path, time::Duration};

pub(crate) const SUBMIT: &str = "submit_workbench_plan";
pub(crate) const RESEARCH: &str = "read_official_source";

const SOURCES: &[(&str, &str, &str, &str)] = &[
    ("rust-tests", "The Rust Book · Writing Tests",
        "https://doc.rust-lang.org/book/ch11-01-writing-tests.html",
        "https://raw.githubusercontent.com/rust-lang/book/main/src/ch11-01-writing-tests.md"),
    ("tauri-v2-commands", "Tauri 2 · Calling Rust from the Frontend",
        "https://v2.tauri.app/develop/calling-rust/",
        "https://raw.githubusercontent.com/tauri-apps/tauri-docs/v2/src/content/docs/develop/calling-rust.mdx"),
    ("react-state", "React · useState",
        "https://react.dev/reference/react/useState",
        "https://raw.githubusercontent.com/reactjs/react.dev/main/src/content/reference/react/useState.md"),
    ("node-24-tests", "Node.js 24 · Test runner",
        "https://nodejs.org/docs/latest-v24.x/api/test.html",
        "https://raw.githubusercontent.com/nodejs/node/v24.x/doc/api/test.md"),
];

pub(crate) const GUIDANCE: &str = "For project edits, first inspect the relevant files with native read_file. If the approach depends on an unfamiliar external API or version, use read_official_source before planning and explain what the source established. Use project evidence alone when it is sufficient. Call submit_workbench_plan alone in its round with the exact file paths, approach, and verification command and expected result. The plan appears in Workbench before you can propose an edit. A successful plan is not owner approval. For simple read-only requests, answer directly.";

pub(crate) fn tools() -> [Value; 2] {
    let ids: Vec<_> = SOURCES.iter().map(|entry| entry.0).collect();
    [
        json!({"type":"function", "function":{
            "name": RESEARCH,
            "description": "Read a public official reference. Pick Rust unit testing, Tauri 2 commands, React state, or Node 24 testing. No user text is sent to the source.",
            "parameters": {"type":"object", "additionalProperties":false, "required":["source_id"],
                "properties":{"source_id":{"type":"string","enum":ids}}}
        }}),
        json!({"type":"function", "function":{
            "name": SUBMIT,
            "description": "Save the owner-visible plan before proposing file edits. Call alone after real project reads. Re-submit the plan if the scope changes.",
            "parameters": {"type":"object","additionalProperties":false,
                "required":["goal","approach","files","test_command","expected_result","research_needed","research_note"],
                "properties":{
                    "goal":{"type":"string"},
                    "approach":{"type":"string"},
                    "files":{"type":"array","items":{"type":"string","description":"Exact project-relative path"}},
                    "test_command":{"type":"string","description":"Exact command you intend to run in the pinned project"},
                    "expected_result":{"type":"string"},
                    "research_needed":{"type":"boolean"},
                    "research_note":{"type":"string","description":"What the official source established, or why local evidence suffices"}
                }}
        }}),
    ]
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Source {
    pub(crate) title: String,
    pub(crate) url: String,
    pub(crate) sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Proposal {
    goal: String,
    approach: String,
    files: Vec<String>,
    test_command: String,
    expected_result: String,
    research_needed: bool,
    research_note: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct PlanState {
    inspected: Vec<(String, String)>,
    sources: BTreeMap<String, Source>,
    research_attempted: bool,
    accepted: Option<Proposal>,
}

pub(crate) fn relative_key(path: &str) -> Result<String, String> {
    let normalized = path.replace('\\', "/");
    if normalized.len() > 240 || normalized.split('/').any(|part|
        part.is_empty() || part == "." || part == ".." || part.ends_with([' ', '.'])
            || part.contains(':') || part.chars().any(char::is_control)) {
        return Err("Use an exact path relative to the pinned project.".into());
    }
    Ok(normalized)
}

impl PlanState {
    pub(crate) fn record_read(&mut self, root: &str, receipt: &Value) {
        let Some(path) = receipt["path"].as_str()
            .and_then(|path| Path::new(path).strip_prefix(root).ok()).and_then(Path::to_str) else { return };
        if let (Ok(key), Some(hash)) = (relative_key(path), receipt["sha256"].as_str()) {
            self.inspected.retain(|(path, _)| path != &key);
            if self.inspected.len() == 32 {
                self.inspected.remove(0);
            }
            self.inspected.push((key, hash.into()));
        }
    }

    pub(crate) fn research(&mut self, args: &Value) -> Result<Value, String> {
        let id = args["source_id"].as_str().ok_or("Choose one official source by ID.")?;
        let (_, title, url, download) = SOURCES.iter().find(|entry| entry.0 == id)
            .ok_or("That source is outside the official reference catalog.")?;
        self.research_attempted = true;
        // Both the destination and request path are fixed; model data is never
        // included in the outbound request and redirects are disabled.
        let response = ureq::AgentBuilder::new().redirects(0).timeout(Duration::from_secs(12)).build()
            .get(download).call().map_err(|_| "The official source could not be reached.")?;
        if response.status() != 200 || !response.content_type().starts_with("text/plain") {
            return Err("The official source did not return its expected text.".into());
        }
        let mut bytes = Vec::new();
        response.into_reader().take(12_001).read_to_end(&mut bytes)
            .map_err(|_| "The official source could not be read.")?;
        let truncated = bytes.len() > 12_000;
        bytes.truncate(12_000);
        if bytes.is_empty() { return Err("The official source returned no text.".into()); }
        let source = Source {
            title: (*title).into(),
            url: (*url).into(),
            sha256: hex::encode(Sha256::digest(&bytes)),
        };
        let receipt = json!({"source_id":id, "source":source, "excerpt":String::from_utf8_lossy(&bytes), "truncated":truncated});
        self.sources.insert(id.into(), source);
        Ok(receipt)
    }

    pub(crate) fn sources(&self) -> &BTreeMap<String, Source> {
        &self.sources
    }

    pub(crate) fn submit(&mut self, args: Value, root: &str) -> Result<Value, String> {
        let proposed: Proposal = serde_json::from_value(args)
            .map_err(|_| "Provide the plan fields, exact paths, and test command.")?;
        for text in [&proposed.goal, &proposed.approach, &proposed.test_command, &proposed.expected_result, &proposed.research_note] {
            if text.trim().is_empty() || text.len() > 1_000 {
                return Err("Each plan field must contain between 1 and 1,000 bytes.".into());
            }
        }
        if proposed.files.is_empty() || proposed.files.len() > 12 {
            return Err("Name between one and twelve exact files to change.".into());
        }
        let files = proposed.files.iter().map(|path| relative_key(path)).collect::<Result<Vec<_>, _>>()?;
        if self.inspected.is_empty() {
            return Err("Read a relevant file in the pinned project before submitting a plan.".into());
        }
        if (self.research_attempted || proposed.research_needed) && self.sources.is_empty() {
            return Err("This plan needs a successful official source read. Report the research blocker if it is unavailable.".into());
        }
        let inspected: Vec<_> = self.inspected.iter()
            .map(|(path, hash)| json!({"path":path,"sha256":hash})).collect();
        let sources: Vec<_> = self.sources.values().collect();
        let receipt = json!({"plan":proposed, "project":root, "inspected":inspected, "sources":sources});
        self.accepted = Some(Proposal {files, ..proposed});
        Ok(receipt)
    }

    pub(crate) fn has_inspection(&self) -> bool { !self.inspected.is_empty() }

    pub(crate) fn test_command(&self) -> Option<&str> {
        self.accepted.as_ref().map(|plan| plan.test_command.as_str())
    }

    pub(crate) fn require_edit(&self, path: &str) -> Result<(), String> {
        let key = relative_key(path)?;
        if self.accepted.as_ref().is_some_and(|plan| plan.files.contains(&key)) {
            return Ok(());
        }
        Err("Read the project and submit a Workbench plan covering this file before proposing an edit.".into())
    }

    pub(crate) fn require_current_read(&self, root: &str, target: &str, before_sha: &str) -> Result<(), String> {
        let path = Path::new(target).strip_prefix(root).ok().and_then(Path::to_str)
            .ok_or("The write target is outside the planned project.")?;
        self.require_edit(path)?;
        let key = relative_key(path)?;
        if before_sha.is_empty() || self.inspected.iter().any(|(path, hash)| path == &key && hash == before_sha) {
            return Ok(());
        }
        Err("Read the current target file with native read_file before proposing an edit.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proposal(needs_research: bool) -> Value {
        json!({"goal":"Fix the title", "approach":"Correct the title and cover it with a test",
            "files":["src/title.rs"], "test_command":"cargo test --offline title",
            "expected_result":"The title regression passes", "research_needed":needs_research,
            "research_note":"Use the documented title behavior"})
    }

    #[test]
    fn requires_real_project_read_and_a_plan_covering_the_edit() {
        let mut plan = PlanState::default();
        assert!(plan.require_edit("src/title.rs").is_err());
        assert!(plan.submit(proposal(false), "/repo").is_err());
        plan.record_read("/repo", &json!({"path":"/other/src/title.rs","sha256":"other"}));
        assert!(plan.submit(proposal(false), "/repo").is_err());
        plan.record_read("/repo", &json!({"path":"/repo/src/title.rs","sha256":"read"}));
        assert!(plan.submit(proposal(false), "/repo").is_ok());
        let saved: PlanState = serde_json::from_value(serde_json::to_value(plan).unwrap()).unwrap();
        assert!(saved.require_edit(r"src\title.rs").is_ok());
        assert!(saved.require_edit("src/another.rs").is_err());
        assert!(saved.require_edit("src/Title.rs").is_err());
        assert!(saved.require_current_read("/repo", "/repo/src/title.rs", "read").is_ok());
        assert!(saved.require_current_read("/repo", "/repo/src/Title.rs", "read").is_err());
        assert!(saved.require_current_read("/repo", "/repo/src/title.rs", "changed").is_err());
    }

    #[test]
    fn research_cannot_claim_an_unread_source_or_use_an_arbitrary_url() {
        let mut plan = PlanState::default();
        plan.record_read("/repo", &json!({"path":"/repo/src/title.rs","sha256":"read"}));
        assert!(plan.research(&json!({"source_id":"https://example.com"})).is_err());
        assert!(plan.submit(proposal(true), "/repo").is_err());
        plan.research_attempted = true;
        assert!(plan.submit(proposal(false), "/repo").is_err());
        assert!(plan.require_edit("src/title.rs").is_err());
    }
}
