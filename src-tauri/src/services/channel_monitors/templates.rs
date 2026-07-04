use std::collections::HashMap;

use crate::{
    models::channel_monitors::ChannelMonitorRequestTemplate,
    services::database::now_millis_for_services,
};
use serde_json::{Number, Value};

#[derive(Debug, Clone)]
pub struct MonitorTemplateContext {
    pub model: String,
    pub max_tokens: i64,
    pub stream: bool,
    pub challenge: String,
}

#[derive(Debug, Clone)]
pub struct RenderedMonitorRequest {
    pub method: String,
    pub path: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

pub fn render_monitor_request(
    template: &ChannelMonitorRequestTemplate,
    context: &MonitorTemplateContext,
) -> Result<RenderedMonitorRequest, String> {
    let template_value: Value = serde_json::from_str(&template.request_body_json)
        .map_err(|error| format!("Monitor request template must be valid JSON: {error}"))?;
    let body_value = render_json_value(&template_value, context);
    let body = serde_json::to_vec(&body_value)
        .map_err(|error| format!("Monitor request body could not be encoded as JSON: {error}"))?;
    let mut headers = HashMap::new();
    headers.insert("content-type".to_string(), "application/json".to_string());

    Ok(RenderedMonitorRequest {
        method: normalize_monitor_method(&template.method)?,
        path: template.path.clone(),
        headers,
        body,
    })
}

pub(crate) fn normalize_monitor_method(method: &str) -> Result<String, String> {
    if method.is_empty() || method != method.trim() || !method.chars().all(is_http_token_char) {
        return Err("Invalid monitor request method".to_string());
    }

    let method = method.to_ascii_uppercase();
    if !matches!(method.as_str(), "GET" | "POST" | "HEAD") {
        return Err(format!("Unsupported monitor request method: {method}"));
    }

    Ok(method)
}

fn is_http_token_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric()
        || matches!(
            ch,
            '!' | '#'
                | '$'
                | '%'
                | '&'
                | '\''
                | '*'
                | '+'
                | '-'
                | '.'
                | '^'
                | '_'
                | '`'
                | '|'
                | '~'
        )
}

fn render_json_value(value: &Value, context: &MonitorTemplateContext) -> Value {
    match value {
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| render_json_value(item, context))
                .collect(),
        ),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| (key.clone(), render_json_value(value, context)))
                .collect(),
        ),
        Value::String(text) => render_string_value(text, context),
        _ => value.clone(),
    }
}

fn render_string_value(value: &str, context: &MonitorTemplateContext) -> Value {
    match value {
        "{{model}}" => Value::String(context.model.clone()),
        "{{challenge}}" => Value::String(context.challenge.clone()),
        "{{max_tokens}}" => Value::Number(Number::from(context.max_tokens)),
        "{{stream}}" => Value::Bool(context.stream),
        "{{timestamp}}" => Value::Number(Number::from(timestamp_u64())),
        _ => Value::String(render_mixed_string(value, context)),
    }
}

fn render_mixed_string(value: &str, context: &MonitorTemplateContext) -> String {
    value
        .replace("{{model}}", &context.model)
        .replace("{{max_tokens}}", &context.max_tokens.to_string())
        .replace("{{stream}}", if context.stream { "true" } else { "false" })
        .replace("{{challenge}}", &context.challenge)
        .replace("{{timestamp}}", &timestamp_u64().to_string())
}

fn timestamp_u64() -> u64 {
    now_millis_for_services().min(u64::MAX as u128) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn renders_request_body_variables_and_default_json_header() {
        let template = template(
            "post",
            "/v1/chat/completions",
            r#"{
                "model": "{{model}}",
                "max_tokens": "{{max_tokens}}",
                "stream": "{{stream}}",
                "messages": [
                    { "role": "user", "content": "{{challenge}} at {{timestamp}}" }
                ]
            }"#,
        );
        let context = MonitorTemplateContext {
            model: "gpt-4o-mini".to_string(),
            max_tokens: 7,
            stream: false,
            challenge: "ping-check".to_string(),
        };

        let rendered = render_monitor_request(&template, &context).expect("rendered");

        assert_eq!(rendered.method, "POST");
        assert_eq!(rendered.path, "/v1/chat/completions");
        assert_eq!(
            rendered.headers.get("content-type").map(String::as_str),
            Some("application/json")
        );
        let body: Value = serde_json::from_slice(&rendered.body).expect("valid json");
        assert_eq!(body["model"], "gpt-4o-mini");
        assert_eq!(body["max_tokens"], 7);
        assert_eq!(body["stream"], false);
        let content = body["messages"][0]["content"].as_str().unwrap();
        assert!(content.starts_with("ping-check at "));
        assert!(!content.contains("{{timestamp}}"));
        assert!(!body.to_string().contains("{{model}}"));
    }

    #[test]
    fn renders_whole_string_placeholders_to_json_types() {
        let template = template(
            "POST",
            "/v1/chat/completions",
            r#"{
                "model": "{{model}}",
                "max_tokens": "{{max_tokens}}",
                "stream": "{{stream}}",
                "created_at": "{{timestamp}}",
                "messages": [
                    { "role": "user", "content": "{{challenge}}" }
                ]
            }"#,
        );
        let context = MonitorTemplateContext {
            model: "gpt-4o-mini".to_string(),
            max_tokens: 13,
            stream: true,
            challenge: "ping".to_string(),
        };

        let rendered = render_monitor_request(&template, &context).expect("rendered");
        let body: Value = serde_json::from_slice(&rendered.body).expect("valid json");

        assert_eq!(body["model"], "gpt-4o-mini");
        assert_eq!(body["max_tokens"], 13);
        assert!(body["max_tokens"].is_number());
        assert_eq!(body["stream"], true);
        assert!(body["stream"].is_boolean());
        assert!(body["created_at"].is_number());
    }

    #[test]
    fn renders_mixed_placeholders_with_json_string_escaping() {
        let template = template(
            "POST",
            "/v1/chat/completions",
            r#"{
                "model": "{{model}}",
                "messages": [
                    { "role": "user", "content": "Model {{model}} says {{challenge}}" }
                ]
            }"#,
        );
        let context = MonitorTemplateContext {
            model: "gpt-4o\n\"mini\"".to_string(),
            max_tokens: 1,
            stream: false,
            challenge: "quote: \"hello\"\nnext line".to_string(),
        };

        let rendered = render_monitor_request(&template, &context).expect("rendered");
        let body: Value = serde_json::from_slice(&rendered.body).expect("valid json");

        assert_eq!(body["model"], "gpt-4o\n\"mini\"");
        assert_eq!(
            body["messages"][0]["content"],
            "Model gpt-4o\n\"mini\" says quote: \"hello\"\nnext line"
        );
    }

    #[test]
    fn rejects_rendered_body_that_is_not_json() {
        let template = template("POST", "/v1/chat/completions", r#"{ "model": "{{model}"#);
        let context = MonitorTemplateContext {
            model: "gpt-4o-mini".to_string(),
            max_tokens: 1,
            stream: false,
            challenge: "ping".to_string(),
        };

        let error = render_monitor_request(&template, &context).expect_err("invalid json rejected");

        assert!(error.contains("JSON"));
    }

    #[test]
    fn rejects_unsupported_or_invalid_request_methods() {
        let context = MonitorTemplateContext {
            model: "gpt-4o-mini".to_string(),
            max_tokens: 1,
            stream: false,
            challenge: "ping".to_string(),
        };

        for method in ["TRACE", "BAD METHOD", "POST\r\nX-Bad: yes"] {
            let template = template(
                method,
                "/v1/chat/completions",
                r#"{ "model": "{{model}}" }"#,
            );

            let error = render_monitor_request(&template, &context)
                .expect_err("invalid method should be rejected");

            assert!(error.contains("method"), "{error}");
        }
    }

    fn template(
        method: &str,
        path: &str,
        request_body_json: &str,
    ) -> ChannelMonitorRequestTemplate {
        ChannelMonitorRequestTemplate {
            id: "template-1".to_string(),
            name: "Health check".to_string(),
            endpoint_kind: "chat_completions".to_string(),
            method: method.to_string(),
            path: path.to_string(),
            request_body_json: request_body_json.to_string(),
            enabled: true,
            built_in: true,
            note: None,
            created_at: "1".to_string(),
            updated_at: "1".to_string(),
        }
    }
}
