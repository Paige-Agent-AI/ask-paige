use serde::Serialize;
use serde_json::Value;
use std::time::Instant;

#[derive(Default, Serialize)]
pub(crate) struct RoundEvidence {
    pub request_bytes: usize,
    pub elapsed_ms: u128,
    pub first_output_ms: Option<u128>,
    pub http_status: Option<u16>,
    pub request_id: Option<String>,
    pub provider: Option<String>,
    pub reported_model: Option<String>,
    pub strategy: Option<String>,
    pub gateway_latency_ms: Option<u64>,
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
    pub cached_prompt_tokens: Option<u64>,
    pub cost_usd: Option<f64>,
    pub finish_reason: Option<String>,
    pub returned_tool_calls: Option<usize>,
}

fn label(value: Option<&str>) -> Option<String> {
    value.filter(|value| !value.is_empty() && value.len() <= 128
        && value.bytes().all(|byte| byte.is_ascii_alphanumeric() || b" ._:/-".contains(&byte)))
        .map(str::to_owned)
}

impl RoundEvidence {
    pub(crate) fn read_headers(&mut self, response: &ureq::Response, omniroute: bool) {
        self.http_status = Some(response.status());
        if !omniroute { return; }
        self.request_id = label(response.header("X-OmniRoute-Request-Id"));
        self.reported_model = label(response.header("X-OmniRoute-Model"));
        self.provider = label(response.header("X-OmniRoute-Provider"));
        self.decision(response.header("X-OmniRoute-Decision"));
    }

    fn decision(&mut self, value: Option<&str>) {
        for field in value.unwrap_or_default().split(';') {
            let Some((key, value)) = field.trim().split_once('=') else { continue };
            let value = value.trim();
            match key {
                "strategy" => self.strategy = label(Some(value)),
                "provider" if self.provider.is_none() => self.provider = label(Some(value)),
                "latency_ms" => self.gateway_latency_ms = value.parse().ok(),
                _ => {}
            }
        }
    }

    pub(crate) fn observe(&mut self, chunk: &Value, started: Instant) {
        if self.request_id.is_none() { self.request_id = label(chunk["id"].as_str()); }
        if self.reported_model.is_none() { self.reported_model = label(chunk["model"].as_str()); }
        if self.provider.is_none() { self.provider = label(chunk["provider"].as_str()); }
        if let Some(usage) = chunk.get("usage").filter(|usage| usage.is_object()) {
            self.prompt_tokens = usage["prompt_tokens"].as_u64().or(self.prompt_tokens);
            self.completion_tokens = usage["completion_tokens"].as_u64().or(self.completion_tokens);
            self.cached_prompt_tokens = usage["prompt_tokens_details"]["cached_tokens"].as_u64().or(self.cached_prompt_tokens);
            self.cost_usd = usage["cost"].as_f64().filter(|cost| cost.is_finite() && *cost >= 0.0).or(self.cost_usd);
        }
        if self.first_output_ms.is_none() && chunk["choices"].as_array().is_some_and(|choices|
            choices.iter().any(|choice| {
                let delta = &choice["delta"];
                delta["content"].as_str().is_some_and(|text| !text.is_empty())
                    || delta["tool_calls"].as_array().is_some_and(|calls| calls.iter().any(|call|
                        ["name", "arguments"].iter().any(|field|
                            call["function"][*field].as_str().is_some_and(|value| !value.is_empty()))))
            })) {
            self.first_output_ms = Some(started.elapsed().as_millis());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn records_actual_output_and_ignores_unknown_header_fields() {
        let mut round = RoundEvidence::default();
        round.decision(Some("strategy=priority; provider=cloudflare-playground; latency_ms=834; token=should-not-be-saved"));
        assert_eq!(round.provider.as_deref(), Some("cloudflare-playground"));
        assert_eq!(round.strategy.as_deref(), Some("priority"));
        assert_eq!(round.gateway_latency_ms, Some(834));
        let clock = Instant::now();
        round.observe(&json!({"choices":[{"delta":{"role":"assistant"}}]}), clock);
        assert!(round.first_output_ms.is_none());
        round.observe(&json!({"model":"a/actual-model","choices":[{"delta":{"tool_calls":[{"function":{"name":"read_file"}}]}}]}), clock);
        round.observe(&json!({"usage":{"prompt_tokens":200,"completion_tokens":30,"prompt_tokens_details":{"cached_tokens":100},"cost":0.01}}), clock);
        assert!(round.first_output_ms.is_some());
        assert_eq!(round.reported_model.as_deref(), Some("a/actual-model"));
        assert_eq!(round.cached_prompt_tokens, Some(100));
        assert!(!serde_json::to_string(&round).unwrap().contains("should-not-be-saved"));
    }
}
