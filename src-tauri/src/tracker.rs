use serde_json::Value;

#[derive(Debug, Clone, Default)]
pub struct UsageStats {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub model: Option<String>,
}

pub fn usage_from_json_body(body: &[u8]) -> Option<UsageStats> {
    let value: Value = serde_json::from_slice(body).ok()?;
    extract_usage(&value)
}

pub fn usage_from_sse_chunk(chunk: &str) -> Option<UsageStats> {
    let mut latest = None;
    for line in chunk.lines() {
        let line = line.trim();
        if !line.starts_with("data:") {
            continue;
        }

        let payload = line.trim_start_matches("data:").trim();
        if payload == "[DONE]" || payload.is_empty() {
            continue;
        }

        if let Ok(value) = serde_json::from_str::<Value>(payload) {
            if let Some(usage) = extract_usage(&value) {
                latest = Some(usage);
            }
        }
    }
    latest
}

fn extract_usage(value: &Value) -> Option<UsageStats> {
    let model = value
        .get("model")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| {
            value
                .get("message")
                .and_then(|m| m.get("model"))
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        });

    if let Some(usage_obj) = value.get("usage") {
        return from_usage_object(usage_obj, model);
    }

    if let Some(message) = value.get("message") {
        if let Some(usage_obj) = message.get("usage") {
            return from_usage_object(usage_obj, model);
        }
    }

    None
}

fn from_usage_object(usage_obj: &Value, model: Option<String>) -> Option<UsageStats> {
    let input = usage_obj
        .get("input_tokens")
        .and_then(Value::as_i64)
        .or_else(|| usage_obj.get("prompt_tokens").and_then(Value::as_i64))
        .unwrap_or(0);
    let output = usage_obj
        .get("output_tokens")
        .and_then(Value::as_i64)
        .or_else(|| usage_obj.get("completion_tokens").and_then(Value::as_i64))
        .unwrap_or(0);

    if input == 0 && output == 0 && model.is_none() {
        return None;
    }

    Some(UsageStats {
        input_tokens: input,
        output_tokens: output,
        model,
    })
}
