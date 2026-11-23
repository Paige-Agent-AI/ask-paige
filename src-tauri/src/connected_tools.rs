//! The tools exposed in Ask PAIGE for browser-authorized services.

use crate::provider_oauth::{self, Provider};
use serde_json::Value;
use tauri::Url;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Service {
    Linear,
    Supabase,
    GitHub,
    Notion,
    Paige,
}

pub(crate) struct Tool {
    pub name: &'static str,
    pub description: &'static str,
    pub writes: bool,
}

impl Tool {
    const fn read(name: &'static str, description: &'static str) -> Self {
        Self {
            name,
            description,
            writes: false,
        }
    }

    const fn write(name: &'static str, description: &'static str) -> Self {
        Self {
            name,
            description,
            writes: true,
        }
    }
}

const LINEAR: &[Tool] = &[
    Tool::read("list_issues", "Find and list Linear issues."),
    Tool::read("get_issue", "Get a Linear issue by ID or identifier."),
    Tool::read("list_projects", "Find and list Linear projects."),
    Tool::read("get_project", "Get details for a Linear project."),
    Tool::read("list_comments", "Read the comments on a Linear issue."),
    Tool::write("save_issue", "Create or update a Linear issue."),
    Tool::write("create_comment", "Add a comment to a Linear issue."),
];
const SUPABASE: &[Tool] = &[
    Tool::read(
        "list_tables",
        "List database tables in the connected Supabase project.",
    ),
    Tool::read(
        "list_extensions",
        "List installed database extensions in the connected Supabase project.",
    ),
    Tool::read(
        "list_migrations",
        "Read migration history in the connected Supabase project.",
    ),
    Tool::read("search_docs", "Search Supabase documentation."),
    Tool::write(
        "execute_sql",
        "Run SQL against the connected Supabase database. All SQL requires confirmation.",
    ),
    Tool::write(
        "apply_migration",
        "Apply a migration to the connected Supabase project.",
    ),
];
const GITHUB: &[Tool] = &[
    Tool::read(
        "list_issues",
        "List GitHub issues in the connected repository.",
    ),
    Tool::read(
        "issue_read",
        "Read a GitHub issue in the connected repository.",
    ),
    Tool::write(
        "issue_write",
        "Create or update a GitHub issue in the connected repository.",
    ),
    Tool::write(
        "add_issue_comment",
        "Add a GitHub issue comment in the connected repository.",
    ),
];

const NOTION: &[Tool] = &[
    Tool::read("notion-fetch", "Read the Notion page selected by the owner."),
];

const PAIGE: &[Tool] = &[
    Tool::read("list_subagents", "List PAIGE specialists."),
    Tool::write("delegate_to_subagent", "Send an approved specialist handoff."),
];

impl Service {
    pub(crate) fn slug(self) -> &'static str {
        match self {
            Self::Linear => "linear",
            Self::Supabase => "supabase",
            Self::GitHub => "github",
            Self::Notion => "notion",
            Self::Paige => "paige",
        }
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Linear => "Linear",
            Self::Supabase => "Supabase",
            Self::GitHub => "GitHub",
            Self::Notion => "Notion",
            Self::Paige => "PAIGE Orchestrator",
        }
    }

    pub(crate) fn remote_oauth(self) -> Option<Provider> {
        match self {
            Self::Linear => Some(Provider::Linear),
            Self::Supabase => Some(Provider::Supabase),
            Self::GitHub => None,
            Self::Notion => Some(Provider::Notion),
            Self::Paige => Some(Provider::Paige),
        }
    }

    pub(crate) fn tools(self) -> &'static [Tool] {
        match self {
            Self::Linear => LINEAR,
            Self::Supabase => SUPABASE,
            Self::GitHub => GITHUB,
            Self::Notion => NOTION,
            Self::Paige => PAIGE,
        }
    }

    pub(crate) fn required_tools(self) -> (&'static str, Option<&'static str>) {
        match self {
            Self::Linear => ("get_issue", Some("save_issue")),
            Self::Supabase => ("list_tables", Some("execute_sql")),
            Self::GitHub => ("list_issues", Some("issue_write")),
            Self::Notion => ("notion-fetch", None),
            Self::Paige => ("list_subagents", Some("delegate_to_subagent")),
        }
    }
}

pub(crate) fn supabase_resource(project: &str) -> Option<String> {
    supabase_resource_for(project, "database,docs,debugging")
}

fn supabase_resource_for(project: &str, features: &str) -> Option<String> {
    if project.len() != 20
        || !project
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
    {
        return None;
    }
    let mut url =
        Url::parse(provider_oauth::SUPABASE_RESOURCE).expect("valid Supabase resource URL");
    url.query_pairs_mut()
        .append_pair("project_ref", project)
        .append_pair("features", features);
    Some(url.into())
}

pub(crate) fn supabase_project(resource: &str) -> Option<String> {
    let url = Url::parse(resource).ok()?;
    let project = url
        .query_pairs()
        .find(|(key, _)| key == "project_ref")?
        .1
        .into_owned();
    let known_scope = supabase_resource(&project).as_deref() == Some(resource)
        || supabase_resource_for(&project, "database,docs").as_deref() == Some(resource);
    known_scope.then_some(project)
}

pub(crate) fn github_repository(repository: &str) -> Option<(&str, &str)> {
    let (owner, repo) = repository.split_once('/')?;
    if owner.is_empty()
        || owner.len() > 39
        || owner.starts_with('-')
        || owner.ends_with('-')
        || !owner
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        || repo.is_empty()
        || repo.len() > 100
        || matches!(repo, "." | "..")
        || !repo
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return None;
    }
    Some((owner, repo))
}

pub(crate) fn bind_github_repository(
    arguments: &mut Value,
    repository: &str,
) -> Result<(), String> {
    let (owner, repo) =
        github_repository(repository).ok_or("The saved GitHub repository is invalid.")?;
    let object = arguments
        .as_object_mut()
        .ok_or("GitHub tool arguments must be an object.")?;
    for (field, expected) in [("owner", owner), ("repo", repo)] {
        if object.get(field).is_some_and(|value| {
            !value
                .as_str()
                .is_some_and(|value| value.eq_ignore_ascii_case(expected))
        }) {
            return Err(
                "This GitHub connection is scoped to the repository selected in Connections."
                    .into(),
            );
        }
        object.insert(field.into(), Value::String(expected.into()));
    }
    for (field, expected) in [("parent_owner", owner), ("parent_repo", repo)] {
        if object.get(field).is_some_and(|value| {
            !value
                .as_str()
                .is_some_and(|value| value.eq_ignore_ascii_case(expected))
        }) {
            return Err("A GitHub parent issue must be in the selected repository.".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn supabase_resource_requires_one_project_and_known_tools() {
        let url = supabase_resource("abcdefghijklmnopqrst").unwrap();
        assert_eq!(
            supabase_project(&url).as_deref(),
            Some("abcdefghijklmnopqrst")
        );
        assert!(supabase_project(&(url + "&skip_elicitations=execute_sql")).is_none());
        assert!(supabase_resource("foreign/project").is_none());
        let legacy = supabase_resource_for("abcdefghijklmnopqrst", "database,docs").unwrap();
        assert_eq!(supabase_project(&legacy).as_deref(), Some("abcdefghijklmnopqrst"));
        assert_ne!(supabase_resource("abcdefghijklmnopqrst").as_deref(), Some(legacy.as_str()));
        assert!(
            Service::Supabase
                .tools()
                .iter()
                .find(|tool| tool.name == "execute_sql")
                .unwrap()
                .writes
        );
    }

    #[test]
    fn github_arguments_cannot_escape_the_selected_repository() {
        let mut args = json!({"method":"get", "issue_number": 1});
        bind_github_repository(&mut args, "owner/repo").unwrap();
        assert_eq!(args["repo"], "repo");
        assert!(bind_github_repository(&mut args, "other/repo").is_err());
        assert!(
            bind_github_repository(
                &mut json!({"parent_repo":"other","parent_issue_number":1}),
                "owner/repo"
            )
            .is_err()
        );
        assert!(
            bind_github_repository(
                &mut json!({"parent_owner":"other","parent_issue_number":1}),
                "owner/repo"
            )
            .is_err()
        );
        assert!(github_repository("owner/../repo").is_none());
    }
}
