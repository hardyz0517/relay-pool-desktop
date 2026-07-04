use std::collections::HashMap;

use crate::{
    models::channel_monitors::ChannelMonitorRequestTemplate,
    services::database::now_millis_for_services,
};

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
    let rendered_body = render_template_text(&template.request_body_json, context);
    let body_value: serde_json::Value = serde_json::from_str(&rendered_body)
        .map_err(|error| format!("Monitor request body must render to valid JSON: {error}"))?;
    let body = serde_json::to_vec(&body_value)
        .map_err(|error| format!("Monitor request body could not be encoded as JSON: {error}"))?;
    let mut headers = HashMap::new();
    headers.insert("content-type".to_string(), "application/json".to_string());

    Ok(RenderedMonitorRequest {
        method: template.method.to_ascii_uppercase(),
        path: template.path.clone(),
        headers,
        body,
    })
}

fn render_template_text(template: &str, context: &MonitorTemplateContext) -> String {
    template
        .replace("{{model}}", &json_string_fragment(&context.model))
        .replace("{{max_tokens}}", &context.max_tokens.to_string())
        .replace("{{stream}}", if context.stream { "true" } else { "false" })
        .replace("{{challenge}}", &json_string_fragment(&context.challenge))
        .replace("{{timestamp}}", &now_millis_for_services().to_string())
}

fn json_string_fragment(value: &str) -> String {
    let encoded = serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string());
    encoded
        .strip_prefix('"')
        .and_then(|item| item.strip_suffix('"'))
        .unwrap_or("")
        .to_string()
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
                "max_tokens": {{max_tokens}},
                "stream": {{stream}},
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
