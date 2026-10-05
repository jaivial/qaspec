//! Minimal OpenAI-compatible chat client with tool calls.

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};
use std::time::Duration;

pub struct Llm {
    base_url: String,
    api_key: Option<String>,
    agent: ureq::Agent,
    pub calls: u64,
    pub tokens: u64,
}

pub struct Reply {
    pub message: Value,
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
}

#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub args: Value,
}

impl Llm {
    pub fn new(base_url: &str, api_key: Option<String>) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(180))
            .build();
        Llm {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            agent,
            calls: 0,
            tokens: 0,
        }
    }

    pub fn chat(
        &mut self,
        model: &str,
        messages: &[Value],
        tools: Option<&Value>,
    ) -> Result<Reply> {
        let mut body =
            json!({ "model": model, "messages": messages, "temperature": 0, "max_tokens": 1500 });
        if let Some(t) = tools {
            body["tools"] = t.clone();
            body["tool_choice"] = json!("required");
        }
        let url = format!("{}/chat/completions", self.base_url);
        let mut last_err = None;
        for attempt in 0..3 {
            if attempt > 0 {
                std::thread::sleep(Duration::from_millis(800 * attempt));
            }
            let mut req = self
                .agent
                .post(&url)
                .set("content-type", "application/json");
            if let Some(k) = &self.api_key {
                req = req.set("authorization", &format!("Bearer {k}"));
            }
            self.calls += 1;
            match req.send_json(body.clone()) {
                Ok(resp) => {
                    let v: Value = resp.into_json()?;
                    self.tokens += v["usage"]["total_tokens"].as_u64().unwrap_or(0);
                    return parse_reply(&v);
                }
                Err(ureq::Error::Status(code, resp)) => {
                    let text = resp.into_string().unwrap_or_default();
                    if code == 429 || code >= 500 {
                        last_err = Some(anyhow!(
                            "LLM HTTP {code}: {}",
                            crate::browser::truncate(&text, 300)
                        ));
                        continue;
                    }
                    bail!("LLM HTTP {code}: {}", crate::browser::truncate(&text, 300));
                }
                Err(e) => {
                    last_err = Some(anyhow!("LLM request failed: {e}"));
                }
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow!("LLM request failed")))
    }
}

pub fn parse_reply(v: &Value) -> Result<Reply> {
    let msg = v["choices"][0]["message"].clone();
    if msg.is_null() {
        bail!(
            "LLM response has no message: {}",
            crate::browser::truncate(&v.to_string(), 300)
        );
    }
    let content = msg["content"].as_str().unwrap_or_default().to_string();
    let mut tool_calls = Vec::new();
    for tc in msg["tool_calls"].as_array().into_iter().flatten() {
        let raw = &tc["function"]["arguments"];
        let args = match raw {
            Value::String(s) if s.trim().is_empty() => json!({}),
            Value::String(s) => {
                serde_json::from_str(s).unwrap_or_else(|_| json!({ "_invalid": s }))
            }
            other => other.clone(),
        };
        tool_calls.push(ToolCall {
            id: tc["id"].as_str().unwrap_or("call").to_string(),
            name: tc["function"]["name"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            args,
        });
    }
    // Keep only what the API accepts back in the history.
    let mut message = json!({ "role": "assistant", "content": if content.is_empty() { Value::Null } else { json!(content) } });
    if !tool_calls.is_empty() {
        message["tool_calls"] = msg["tool_calls"].clone();
    }
    Ok(Reply {
        message,
        content,
        tool_calls,
    })
}

/// Extracts the first JSON object from a model answer (handles ```json fences and prose around it).
pub fn extract_json(text: &str) -> Option<Value> {
    let start = text.find('{')?;
    let mut depth = 0i32;
    let mut in_str = false;
    let mut esc = false;
    for (i, c) in text[start..].char_indices() {
        if in_str {
            if esc {
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return serde_json::from_str(&text[start..start + i + 1]).ok();
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tool_calls() {
        let v = json!({"choices":[{"message":{"role":"assistant","content":null,"tool_calls":[
            {"id":"c1","type":"function","function":{"name":"click","arguments":"{\"ref\":\"e2\"}"}}]}}]});
        let r = parse_reply(&v).unwrap();
        assert_eq!(r.tool_calls[0].name, "click");
        assert_eq!(r.tool_calls[0].args["ref"], "e2");
    }

    #[test]
    fn json_extraction() {
        assert_eq!(
            extract_json("sure:\n```json\n{\"verdict\":\"passed\",\"e\":\"a}b\"}\n```").unwrap()
                ["verdict"],
            "passed"
        );
        assert_eq!(extract_json("x {\"a\":{\"b\":1}} y").unwrap()["a"]["b"], 1);
        assert!(extract_json("nothing").is_none());
    }
}
