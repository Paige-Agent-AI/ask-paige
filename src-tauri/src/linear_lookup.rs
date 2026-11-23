//! The first Desktop Conversational Loop action. A typed, owner-requested
//! read becomes a final answer only after a matching Linear result is recorded.

use serde::Serialize;
use serde_json::Value;

const LOOKUP_INSTRUCTIONS: &[&str] = &[
    "use the connected linear tool to look up ",
    "look up linear issue ",
    "read linear issue ",
    "check linear issue ",
    "look up ",
];

pub(crate) fn trim_outer_quotes(message: &str) -> &str {
    message.trim_matches(|c: char| c.is_whitespace() || matches!(c, '"' | '“' | '”'))
}

pub(crate) fn requested_issue(message: &str) -> Result<Option<String>, String> {
    let normalized = trim_outer_quotes(message).to_ascii_lowercase().replace('’', "'");
    if normalized == "/linear" || normalized.starts_with("/linear ") {
        let input = normalized.strip_prefix("/linear").unwrap_or_default().trim_end_matches(['.', '?', '!']).trim();
        return issue_identifier(input)
            .map(Some)
            .ok_or_else(|| "Enter a single Linear issue identifier, for example /linear ANT-172.".into());
    }
    let mut sentences = normalized.split(['.', '?', '!']).map(trim_outer_quotes).filter(|text| !text.is_empty());
    let Some(first) = sentences.next() else { return Ok(None) };
    let first = [
        " and show me the real tool receipt before you say you're done",
        " and show me the real tool receipt",
        " and show me the tool receipt",
        " and return the title and status",
    ].iter().find_map(|&suffix| first.strip_suffix(suffix)).unwrap_or(first);
    let issue = LOOKUP_INSTRUCTIONS.iter().find_map(|&prefix| {
        let target = first.strip_prefix(prefix)?;
        let target = if prefix == "look up " {
            target.strip_suffix(" in linear")?
        } else {
            target.strip_suffix(" in linear").unwrap_or(target)
        };
        issue_identifier(target)
    });
    let has_only_read_followup = sentences.all(|text| matches!(text,
        "return the issue title and current status" | "return the title and status" |
        "this is a read-only verification" | "show me the real tool receipt before you say you're done"
    ));
    Ok(issue.filter(|_| has_only_read_followup))
}

pub(crate) fn issue_identifier(value: &str) -> Option<String> {
    let (team, number) = value.split_once('-')?;
    (team.len() <= 16 && !team.is_empty() && team.as_bytes()[0].is_ascii_alphabetic()
        && team.bytes().all(|byte| byte.is_ascii_alphanumeric())
        && number.len() <= 12 && !number.is_empty()
        && number.bytes().all(|byte| byte.is_ascii_digit())
        && number.bytes().any(|byte| byte != b'0'))
        .then(|| value.to_ascii_uppercase())
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Issue {
    pub(crate) identifier: String,
    pub(crate) title: String,
    pub(crate) status: String,
}

impl Issue {
    pub(crate) fn from_tool_result(identifier: &str, result: &Value) -> Result<Self, String> {
        let text_objects = result.get("content").and_then(Value::as_array)
            .into_iter().flatten()
            .filter(|item| item.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|item| serde_json::from_str::<Value>(item.get("text")?.as_str()?).ok());
        let items = result.get("structuredContent").filter(|item| item.is_object()).cloned()
            .into_iter().chain(text_objects);
        let mut verified = None;
        for item in items {
            let Some(returned) = item.get("identifier").or_else(|| item.get("id")).and_then(Value::as_str) else {
                continue;
            };
            if !returned.eq_ignore_ascii_case(identifier) {
                return Err("Linear returned a different issue than requested.".into());
            }
            let issue = Self::from_issue(identifier, &item)?;
            if verified.as_ref().is_some_and(|previous| previous != &issue) {
                return Err("Linear returned conflicting records for the issue.".into());
            }
            verified = Some(issue);
        }
        verified.ok_or_else(|| "Linear did not return the requested issue.".into())
    }

    fn from_issue(identifier: &str, issue: &Value) -> Result<Self, String> {
        let field = |value: Option<&Value>| value.and_then(Value::as_str)
            .map(str::trim).filter(|value| !value.is_empty()).map(str::to_owned);
        let title = field(issue.get("title")).ok_or("Linear did not return the issue title.")?;
        let status = field(issue.get("status"))
            .or_else(|| field(issue.pointer("/status/name")))
            .or_else(|| field(issue.pointer("/state/name")))
            .ok_or("Linear did not return the issue status.")?;
        Ok(Self { identifier: identifier.to_owned(), title, status })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn admits_an_explicit_read_of_one_issue_only() {
        let prompt = "Use the connected Linear tool to look up ANT-172. Return the issue title and current status. This is a read-only verification.";
        assert_eq!(requested_issue(prompt).unwrap().as_deref(), Some("ANT-172"));
        assert_eq!(requested_issue("Look up EXP2-24 in Linear").unwrap().as_deref(), Some("EXP2-24"));
        assert_eq!(requested_issue("Look up ANT-172 in Linear and show me the tool receipt.").unwrap().as_deref(), Some("ANT-172"));
        assert_eq!(requested_issue("Look up ANT-172 in Linear and show me the real tool receipt before you say you’re done.").unwrap().as_deref(), Some("ANT-172"));
        assert_eq!(requested_issue("/linear ant-172").unwrap().as_deref(), Some("ANT-172"));
        assert!(requested_issue("/linear ANT-172 ANT-173").is_err());
        assert_eq!(requested_issue("Look up ANT-172 in Linear. Close it.").unwrap(), None);
        assert_eq!(requested_issue("Quote this: look up ANT-172 in Linear.").unwrap(), None);
        assert_eq!(requested_issue("Compare ANT-172 with ANT-173").unwrap(), None);
    }

    #[test]
    fn recognizes_copied_lookup_text_with_typographic_quotes() {
        assert_eq!(requested_issue("Look up ANT-171 in Linear and show me the tool receipt.”").unwrap().as_deref(), Some("ANT-171"));
        assert_eq!(requested_issue("“Look up ANT-171 in Linear and show me the tool receipt.”").unwrap().as_deref(), Some("ANT-171"));
        assert_eq!(requested_issue("Look up ANT-171 in Linear.” Close it.").unwrap(), None);
    }

    #[test]
    fn final_answer_requires_the_requested_identity_title_and_status() {
        let result = |value: Value| json!({"content": [{"type":"text","text":value.to_string()}]});
        let matching = result(json!({"id":"internal-id","identifier":"ANT-172","title":"Desktop loop","status":"In Progress"}));
        let issue = Issue::from_tool_result("ANT-172", &matching).unwrap();
        assert_eq!((issue.title.as_str(), issue.status.as_str()), ("Desktop loop", "In Progress"));
        let typed = json!({"structuredContent":{"identifier":"ANT-172","title":"Desktop loop","status":{"name":"In Progress"}}});
        assert_eq!(Issue::from_tool_result("ANT-172", &typed).unwrap().status, "In Progress");
        let mut conflict = matching.clone();
        conflict["structuredContent"] = json!({"identifier":"ANT-172","title":"Desktop loop","status":"Done"});
        assert!(Issue::from_tool_result("ANT-172", &conflict).is_err());
        conflict["structuredContent"]["identifier"] = json!("ANT-173");
        assert!(Issue::from_tool_result("ANT-172", &conflict).unwrap_err().contains("different issue"));
        assert!(Issue::from_tool_result("ANT-173", &matching).is_err());
        assert!(Issue::from_tool_result("ANT-172", &result(json!({"identifier":"ANT-172","title":"Desktop loop"}))).is_err());
        assert!(Issue::from_tool_result("ANT-172", &json!({"content":[{"type":"text","text":"get_issue(id=ANT-172)"}]})).is_err());
    }
}
