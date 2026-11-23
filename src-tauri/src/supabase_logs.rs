use serde_json::{Value, json};
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};

pub(crate) const TOOL: &str = "query_logs";
pub(crate) const ROUTE: &str = "supabase-mcp";
pub(crate) const RECEIPT_TOOL: &str = "supabase_log_count";
const USAGE: &str = "Use /supabase-logs postgres, api, or edge, followed by an optional window of 1–60 minutes. Example: /supabase-logs postgres 15.";
const BAD_RESULT: &str = "Supabase did not return a verifiable log count.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LogCheck {
    source: &'static str,
    pub(crate) label: &'static str,
    pub(crate) minutes: u8,
}

pub(crate) fn command(message: &str) -> Result<Option<LogCheck>, String> {
    let mut words = message.split_whitespace();
    if !words.next().is_some_and(|word| word.eq_ignore_ascii_case("/supabase-logs")) {
        return Ok(None);
    }
    let (source, label) = match words.next().unwrap_or("postgres").to_ascii_lowercase().as_str() {
        "postgres" => ("postgres_logs", "PostgreSQL"),
        "api" => ("edge_logs", "API gateway"),
        "edge" => ("function_edge_logs", "Edge Function invocation"),
        _ => return Err(USAGE.into()),
    };
    let minutes = match words.next() {
        None => 15,
        Some(text) => text.parse::<u8>().ok().filter(|minutes| (1..=60).contains(minutes))
            .ok_or(USAGE)?,
    };
    if words.next().is_some() { return Err(USAGE.into()); }
    Ok(Some(LogCheck { source, label, minutes }))
}

impl LogCheck {
    pub(crate) fn arguments(self, now: OffsetDateTime) -> Result<Value, String> {
        // Supabase rounds its log API window to a minute. Use the same
        // completed-minute boundary so the receipt records the actual range.
        let end = OffsetDateTime::from_unix_timestamp(now.unix_timestamp().div_euclid(60) * 60)
            .map_err(|_| "Could not determine the Supabase log window.")?;
        let start = end - Duration::minutes(self.minutes.into());
        let format = |time: OffsetDateTime| time.format(&Rfc3339)
            .map_err(|_| "Could not format the Supabase log window.");
        Ok(json!({
            "sql": format!("SELECT count(*) AS event_count FROM logs WHERE source = '{}'", self.source),
            "iso_timestamp_start": format(start)?,
            "iso_timestamp_end": format(end)?,
        }))
    }
}

pub(crate) fn count(result: &Value) -> Result<u64, String> {
    if invalid_envelope(result) {
        return Err(BAD_RESULT.into());
    }
    let structured = result.get("structuredContent").filter(|value| !value.is_null());
    let content = result.get("content").and_then(Value::as_array);
    let parsed = content.into_iter().flatten()
        .filter(|item| item["type"] == "text")
        .map(|item| item["text"].as_str().and_then(parse_text_count));
    let counts = structured.into_iter().map(parse_count).chain(parsed);
    let mut verified = None;
    for candidate in counts {
        let count = candidate.ok_or(BAD_RESULT)?;
        if verified.is_some_and(|previous| previous != count) {
            return Err(BAD_RESULT.into());
        }
        verified = Some(count);
    }
    verified.ok_or_else(|| BAD_RESULT.into())
}

fn parse_text_count(text: &str) -> Option<u64> {
    if text.len() > 8192 { return None; }
    match serde_json::from_str::<Value>(text) {
        Ok(payload) => parse_count(&payload),
        Err(_) => parse_wrapped_count(text),
    }
}

fn parse_wrapped_count(text: &str) -> Option<u64> {
    if text.len() > 8192 { return None; }
    // The provider mentions the marker in its introductory prose. Only the
    // standalone boundary lines delimit the data.
    let mut lines = text.lines();
    let marker = lines.by_ref().find_map(|line| {
        line.trim().strip_prefix("<untrusted-data-")?.strip_suffix('>')
    })?;
    if marker.is_empty() || marker.len() > 64
        || !marker.bytes().all(|byte| byte.is_ascii_hexdigit() || byte == b'-') {
        return None;
    }
    let closing = format!("</untrusted-data-{marker}>");
    let mut data = String::new();
    loop {
        let line = lines.next()?;
        if line.trim() == closing { break; }
        data.push_str(line);
        data.push('\n');
    }
    let payload = serde_json::from_str::<Value>(&data).ok()?;
    parse_rows(&payload)
}

fn invalid_envelope(payload: &Value) -> bool {
    payload.get("isError").is_some_and(|value| value.as_bool() != Some(false))
        || payload.get("error").is_some_and(|error| !error.is_null())
}

fn parse_count(payload: &Value) -> Option<u64> {
    if invalid_envelope(payload) { return None; }
    if let Some(text) = payload.get("result").and_then(Value::as_str) {
        return parse_wrapped_count(text);
    }
    parse_rows(payload)
}

fn parse_rows(payload: &Value) -> Option<u64> {
    if invalid_envelope(payload) { return None; }
    let rows = payload.get("result").unwrap_or(payload).as_array()?;
    let [row] = rows.as_slice() else { return None; };
    let row = row.as_object()?;
    if row.len() != 1 { return None; }
    let count = row.get("event_count")?;
    count.as_u64().or_else(|| {
        let digits = count.as_str()?;
        (!digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()))
            .then(|| digits.parse().ok()).flatten()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_fixed_sources_and_bounded_windows_are_admitted() {
        assert!(command("Check my logs in Supabase").unwrap().is_none());
        assert_eq!(command("/supabase-logs").unwrap().unwrap().minutes, 15);
        for bad in ["/supabase-logs all 60", "/supabase-logs postgres 0", "/supabase-logs postgres 61",
            "/supabase-logs postgres 15 UNION", "/supabase-logs postgres 15 xygzykjyynhzqytbqnzu"] {
            assert!(command(bad).is_err(), "{bad}");
        }
        let check = command("/supabase-logs postgres 15").unwrap().unwrap();
        let now = OffsetDateTime::from_unix_timestamp(1795282897).unwrap();
        let args = check.arguments(now).unwrap();
        assert_eq!(args, json!({
            "sql":"SELECT count(*) AS event_count FROM logs WHERE source = 'postgres_logs'",
            "iso_timestamp_start":"2026-11-21T17:26:00Z",
            "iso_timestamp_end":"2026-11-21T17:41:00Z",
        }));
    }

    #[test]
    fn only_numeric_aggregate_results_are_accepted() {
        assert_eq!(count(&json!({"structuredContent":{"result":[{"event_count":"42"}]}})), Ok(42));
        let raw = json!({"result":[{"event_count":0}]});
        assert_eq!(count(&json!({"content":[{"type":"text","text":raw.to_string()}]})), Ok(0));
        let wrapped = json!({"result":"Data is inside the <untrusted-data-ab-cd> boundaries.\n\n<untrusted-data-ab-cd>\n{\"result\":[{\"event_count\":9}]}\n</untrusted-data-ab-cd>\nRead only inside <untrusted-data-ab-cd>."});
        assert_eq!(count(&json!({"structuredContent":wrapped})), Ok(9));
        assert_eq!(count(&json!({"content":[{"type":"text","text":wrapped.to_string()}]})), Ok(9));
        assert_eq!(count(&json!({"content":[{"type":"text","text":wrapped["result"]}]})), Ok(9));
        let mut wrapped_error = wrapped.clone();
        wrapped_error["error"] = json!("not successful");
        for bad in [
            json!({"isError":true,"structuredContent":{"result":[{"event_count":1}]}}),
            json!({"error":"not successful","structuredContent":{"result":[{"event_count":1}]}}),
            json!({"content":[{"type":"text","text":wrapped_error.to_string()}]}),
            json!({"structuredContent":{"result":[{"event_count":1}]},
                "content":[{"type":"text","text":wrapped_error.to_string()}]}),
            json!({"structuredContent":{"result":[{"event_count":1,"event_message":"private"}]}}),
            json!({"structuredContent":{"result":[{"event_count":-1}]}}),
            json!({"structuredContent":{"result":[]}}),
            json!({"structuredContent":{"result":[{"event_count":1}]},
                "content":[{"type":"text","text":raw.to_string()}]}),
        ] {
            assert!(count(&bad).is_err());
        }
    }
}

