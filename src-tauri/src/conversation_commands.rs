//! Route standalone requests to existing read commands. The owner's original
//! message stays in the conversation; only command dispatch uses this text.

use crate::linear_lookup::issue_identifier;

fn strip_prefix<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    text.get(..prefix.len())?.eq_ignore_ascii_case(prefix).then(|| &text[prefix.len()..])
}

fn strip_suffix<'a>(text: &'a str, suffix: &str) -> Option<&'a str> {
    let index = text.len().checked_sub(suffix.len())?;
    text.get(index..)?.eq_ignore_ascii_case(suffix).then(|| &text[..index])
}

fn after_any<'a>(text: &'a str, prefixes: &[&str]) -> Option<&'a str> {
    prefixes.iter().find_map(|prefix| strip_prefix(text, prefix))
}

fn positive_issue_number(text: &str) -> Option<u64> {
    let text = text.strip_prefix('#').unwrap_or(text);
    if !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse::<u64>().ok().filter(|&number| number > 0)
}

pub(crate) fn github_issue_command(message: &str) -> Result<Option<u64>, String> {
    let mut words = message.split_whitespace();
    if !words.next().is_some_and(|word| word.eq_ignore_ascii_case("/github")) {
        return Ok(None);
    }
    words.next().and_then(positive_issue_number).filter(|_| words.next().is_none())
        .map(Some)
        .ok_or_else(|| "Enter one GitHub issue number, for example /github #42.".into())
}

fn unquote(text: &str) -> Option<&str> {
    let (inner, closing) = text.strip_prefix('"').map(|inner| (inner, '"'))
        .or_else(|| text.strip_prefix('“').map(|inner| (inner, '”')))?;
    let inner = inner.strip_suffix(closing)?;
    (!inner.contains(closing)).then_some(inner)
}

pub(crate) fn recognize(message: &str) -> Option<String> {
    let message = message.trim();
    let message = unquote(message).unwrap_or(message).trim();
    if message.starts_with('/') || message.chars().any(char::is_control) {
        return None;
    }
    let text = message.trim_end_matches(['.', '?', '!']);
    let text = after_any(text, &["okay paige,", "hey paige,", "paige,"]).unwrap_or(text).trim_start();
    let text = after_any(text, &["can you ", "could you ", "would you "]).unwrap_or(text);
    let text = strip_prefix(text, "please ").unwrap_or(text);
    let text = strip_suffix(text, ", please").unwrap_or(text);
    let folded = text.to_ascii_lowercase();
    match folded.as_str() {
        "read the selected notion page" | "read my notion page" | "open the selected notion page"
        | "show me the selected notion page" | "show me what's on the selected notion page"
        | "show me what’s on the selected notion page" => return Some("/notion".into()),
        "check my workspace access" | "show me which projects you can access"
        | "what projects can you access" => return Some("/workspace".into()),
        _ => {}
    }

    let lookup = after_any(text, &["look up ", "check ", "read ", "show me ", "find "]);
    let linear = lookup.and_then(|target| {
        strip_prefix(target, "linear issue ").or_else(|| strip_suffix(target, " in linear"))
    }).or_else(|| {
        after_any(text, &["what's the status of ", "what’s the status of ", "what is the status of "])
            .and_then(|target| strip_suffix(target, " in linear"))
    });
    if let Some(issue) = linear.and_then(issue_identifier) {
        return Some(format!("/linear {issue}"));
    }
    let github = lookup.and_then(|target| {
        strip_prefix(target, "github issue ").or_else(|| {
            let target = strip_suffix(target, " on github").or_else(|| strip_suffix(target, " in github"))?;
            Some(strip_prefix(target, "issue ").unwrap_or(target))
        })
    });
    if let Some(number) = github.and_then(positive_issue_number) {
        return Some(format!("/github #{number}"));
    }

    if let Some(target) = lookup.and_then(|target| strip_prefix(target, "supabase ")) {
        let target = target.to_ascii_lowercase();
        let (target, window) = target.split_once(" for the last ")
            .or_else(|| target.split_once(" from the last "))
            .unwrap_or((&target, "15 minutes"));
        let source = match target {
            "logs" | "postgres logs" | "postgresql logs" => Some("postgres"),
            "api logs" | "api gateway logs" => Some("api"),
            "edge logs" | "edge function logs" => Some("edge"),
            _ => None,
        };
        if let Some((source, minutes)) = source.zip(strip_suffix(window, " minutes"))
            && !minutes.is_empty() && minutes.bytes().all(|byte| byte.is_ascii_digit()) {
            return Some(format!("/supabase-logs {source} {minutes}"));
        }
    }

    let query = after_any(text, &["search the public web for ", "search the web for ", "search online for "])
        .or_else(|| after_any(text, &["find ", "look up ", "search for "]).and_then(|target| {
            strip_suffix(target, " on the web").or_else(|| strip_suffix(target, " online"))
        }))?.trim();
    if query.is_empty() || query.chars().count() > 200 {
        return None;
    }
    if let Some(quoted) = unquote(query) {
        return (!quoted.trim().is_empty()).then(|| format!("/web {query}"));
    }
    // A standalone search noun phrase can be forwarded. For ambiguous clauses
    // or punctuation, stay in normal chat unless the query was explicitly quoted.
    let lower = query.to_ascii_lowercase();
    let ambiguous = lower.chars().any(|ch| !ch.is_alphanumeric() && !ch.is_whitespace()
        && !matches!(ch, '\'' | '’' | '-' | '+' | '_' | '.' | ':' | '/' | '#'))
        || [". ", ": ", " - ", "://"].iter().any(|separator| lower.contains(separator))
        || lower.split_whitespace().any(|word| matches!(word,
            "and" | "or" | "but" | "then" | "after" | "before" | "also" | "while"
                | "if" | "unless" | "instead" | "although" | "though"
        ));
    (!ambiguous).then(|| format!("/web {query}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_standalone_requests_and_preserves_search_terms() {
        for (prompt, expected) in [
            ("Could you search the web for Rust Cargo test docs?", "/web Rust Cargo test docs"),
            ("Paige, please find React's useEffect docs online.", "/web React's useEffect docs"),
            (r#"Search for "Rust and Go threading" on the web."#, r#"/web "Rust and Go threading""#),
            (r#"Search the web for "Rust native dialogs"."#, r#"/web "Rust native dialogs""#),
            ("Check ANT-171 in Linear.", "/linear ANT-171"),
            ("What’s the status of ANT-172 in Linear?", "/linear ANT-172"),
            ("Can you look up GitHub issue #42?", "/github #42"),
            ("Please show me issue #42 on GitHub.", "/github #42"),
            ("Read the selected Notion page, please.", "/notion"),
            ("Show me which projects you can access.", "/workspace"),
            ("Check Supabase edge function logs for the last 10 minutes.", "/supabase-logs edge 10"),
            ("Show me Supabase Postgres logs.", "/supabase-logs postgres 15"),
            ("CHECK SUPABASE API LOGS FOR THE LAST 10 MINUTES.", "/supabase-logs api 10"),
        ] {
            assert_eq!(recognize(prompt).as_deref(), Some(expected), "{prompt}");
        }
    }

    #[test]
    fn leaves_compound_requests_and_explicit_commands_to_their_existing_paths() {
        for prompt in [
            "/web Rust",
            "How do I search the web for Rust?",
            "Don't search the web for Rust",
            "Search the web for Rust. Then install it",
            "Search the web for Rust and then install it",
            "Search the web for Rust and run the local tests",
            "Search the web for Rust and please run the local tests",
            "Search the web for Rust and email the results",
            "Search the web for Rust but use my private notes",
            "Search the web for Rust—check my private project files next",
            "Search the web for Rust - check the local notes",
            "Search the web for Rust (check the local notes too)",
            "Search the web for Rust…check the local notes too",
            "Search the web for Rust unless a project file already answers this",
            r#"Search the web for "Rust" and email the results"#,
            "Search the web for Rust, then run the local tests",
            "Search the web for Rust\nHere are my notes",
            "Look up AND-7 in Linear and close it",
            "Check GitHub issue #42 in another/repo",
            "Read the selected Notion page and edit it",
            "Check the desktop logs from this project",
            "Check Supabase logs from yesterday",
        ] {
            assert_eq!(recognize(prompt), None, "{prompt}");
        }
    }
}
