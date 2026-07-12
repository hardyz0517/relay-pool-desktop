use serde_json::Value;

pub const GROUP_CATEGORY_GPT: &str = "gpt";
pub const GROUP_CATEGORY_CLAUDE: &str = "claude";
pub const GROUP_CATEGORY_GEMINI: &str = "gemini";
pub const GROUP_CATEGORY_GROK: &str = "grok";
pub const GROUP_CATEGORY_IMAGE_GENERATION: &str = "image_generation";
pub const GROUP_CATEGORY_EMBEDDING: &str = "embedding";
pub const GROUP_CATEGORY_RERANK: &str = "rerank";
pub const GROUP_CATEGORY_UNKNOWN: &str = "unknown";

pub fn normalize_group_category(value: Option<&str>) -> Option<String> {
    match value.map(str::trim).map(str::to_lowercase).as_deref() {
        Some(GROUP_CATEGORY_GPT) => Some(GROUP_CATEGORY_GPT.to_string()),
        Some(GROUP_CATEGORY_CLAUDE) => Some(GROUP_CATEGORY_CLAUDE.to_string()),
        Some(GROUP_CATEGORY_GEMINI) => Some(GROUP_CATEGORY_GEMINI.to_string()),
        Some(GROUP_CATEGORY_GROK) => Some(GROUP_CATEGORY_GROK.to_string()),
        Some(GROUP_CATEGORY_IMAGE_GENERATION) => Some(GROUP_CATEGORY_IMAGE_GENERATION.to_string()),
        Some(GROUP_CATEGORY_EMBEDDING) => Some(GROUP_CATEGORY_EMBEDDING.to_string()),
        Some(GROUP_CATEGORY_RERANK) => Some(GROUP_CATEGORY_RERANK.to_string()),
        Some(GROUP_CATEGORY_UNKNOWN) => Some(GROUP_CATEGORY_UNKNOWN.to_string()),
        _ => None,
    }
}

pub fn infer_group_category(group_name: &str, raw_json_redacted: Option<&Value>) -> String {
    if is_image_generation_group_name(group_name) {
        return GROUP_CATEGORY_IMAGE_GENERATION.to_string();
    }

    if let Some(category) = group_category_from_platform(platform_field(raw_json_redacted)) {
        return category.to_string();
    }

    group_category_from_text(&format!(
        "{} {}",
        group_name,
        searchable_json_text(raw_json_redacted)
    ))
    .unwrap_or(GROUP_CATEGORY_UNKNOWN)
    .to_string()
}

fn group_category_from_platform(value: Option<&str>) -> Option<&'static str> {
    match normalize_text(value.unwrap_or_default()).as_str() {
        "openai" | "gpt" => Some(GROUP_CATEGORY_GPT),
        "anthropic" | "claude" => Some(GROUP_CATEGORY_CLAUDE),
        "google" | "gemini" => Some(GROUP_CATEGORY_GEMINI),
        "grok" | "xai" | "x-ai" => Some(GROUP_CATEGORY_GROK),
        _ => None,
    }
}

fn group_category_from_text(value: &str) -> Option<&'static str> {
    if text_matches_any(value, &["claude", "anthropic", "sonnet", "opus", "haiku"]) {
        return Some(GROUP_CATEGORY_CLAUDE);
    }
    if text_matches_any(value, &["gemini", "google"]) {
        return Some(GROUP_CATEGORY_GEMINI);
    }
    if text_matches_any(value, &["grok", "xai", "x-ai"]) {
        return Some(GROUP_CATEGORY_GROK);
    }
    if text_matches_any(value, &["embedding", "embed", "向量"]) {
        return Some(GROUP_CATEGORY_EMBEDDING);
    }
    if text_matches_any(value, &["rerank", "重排"]) {
        return Some(GROUP_CATEGORY_RERANK);
    }
    if text_matches_any(value, &["openai", "gpt", "codex"]) {
        return Some(GROUP_CATEGORY_GPT);
    }
    None
}

fn is_image_generation_group_name(value: &str) -> bool {
    text_matches_any(
        value,
        &[
            "图",
            "生图",
            "绘图",
            "image",
            "images",
            "picture",
            "pictures",
            "dall-e",
            "dalle",
            "midjourney",
        ],
    )
}

fn platform_field(value: Option<&Value>) -> Option<&str> {
    let object = value?.as_object()?;
    for key in ["platform", "provider", "model_provider", "modelProvider"] {
        if let Some(value) = object.get(key).and_then(Value::as_str) {
            if !value.trim().is_empty() {
                return Some(value);
            }
        }
    }
    None
}

fn searchable_json_text(value: Option<&Value>) -> String {
    let Some(value) = value else {
        return String::new();
    };
    let mut parts = Vec::new();
    collect_json_text(value, &mut parts);
    parts.join(" ")
}

fn collect_json_text(value: &Value, parts: &mut Vec<String>) {
    match value {
        Value::Null => {}
        Value::Bool(value) => parts.push(value.to_string()),
        Value::Number(value) => parts.push(value.to_string()),
        Value::String(value) => parts.push(value.clone()),
        Value::Array(items) => {
            for item in items {
                collect_json_text(item, parts);
            }
        }
        Value::Object(map) => {
            for (key, value) in map {
                parts.push(key.clone());
                collect_json_text(value, parts);
            }
        }
    }
}

fn text_matches_any(value: &str, matchers: &[&str]) -> bool {
    let normalized_value = normalize_text(value);
    matchers
        .iter()
        .map(|matcher| normalize_text(matcher))
        .filter(|matcher| !matcher.is_empty())
        .any(|matcher| normalized_value.contains(&matcher))
}

fn normalize_text(value: &str) -> String {
    value.trim().to_lowercase().replace(['_', ' '], "-")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn structured_sub2api_platform_beats_raw_image_fields() {
        let category = infer_group_category(
            "Claude Kiro",
            Some(&json!({"platform": "anthropic", "image_ratio": 2})),
        );

        assert_eq!(category, GROUP_CATEGORY_CLAUDE);
    }

    #[test]
    fn image_group_name_beats_openai_platform() {
        let category = infer_group_category("GPT画图分组", Some(&json!({"platform": "openai"})));

        assert_eq!(category, GROUP_CATEGORY_IMAGE_GENERATION);
    }
}
