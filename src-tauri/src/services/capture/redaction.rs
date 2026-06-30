use serde_json::Value;

const TEXT_PREVIEW_LIMIT: usize = 4_000;

pub fn redact_value(value: &Value) -> Value {
    crate::services::secrets::mask::redact_value(value)
}

pub fn redact_text_preview(text: &str) -> String {
    crate::services::secrets::mask::redact_text_preview(text, TEXT_PREVIEW_LIMIT)
}
