pub mod adapters;
pub(crate) mod attempt;
pub(crate) mod diagnostic_memory;
pub mod endpoint_adapter;
pub mod error;
pub mod execution;
pub(crate) mod finalization;
// The raw TCP parser supports loopback fixtures; production ingress is Hyper-based.
#[cfg(test)]
pub mod http_request;
pub mod ingress;
pub(crate) mod lifecycle;
pub mod limits;
mod local_auth;
pub mod observability;
pub(crate) mod protocol;
pub mod request;
pub(crate) mod request_send;
pub mod response_body;
pub mod responses_chat_fallback;
pub mod responses_chat_stream;
pub mod routing_repository;
pub(crate) mod routing_runtime;
pub mod runtime;
pub mod server;
pub mod startup;
pub mod startup_auto_start;
pub mod upstream;

#[cfg(test)]
mod lifecycle_concurrency_tests;
#[cfg(test)]
mod lifecycle_fault_tests;
#[cfg(test)]
mod soak_tests;
#[cfg(test)]
mod test_support;

pub fn redact_error_message(message: &str) -> String {
    let mut output = crate::services::secrets::mask::redact_text(message);
    if output.len() > 160 {
        let boundary = output
            .char_indices()
            .map(|(index, _)| index)
            .take_while(|index| *index <= 160)
            .last()
            .unwrap_or(0);
        output.truncate(boundary);
        output.push_str("...");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_error_message_masks_key_like_content() {
        let message = "upstream rejected sk-real-secret-value";

        let redacted = redact_error_message(message);

        assert!(!redacted.contains("sk-real-secret-value"));
        assert!(redacted.contains("[REDACTED]"));
    }

    #[test]
    fn redact_error_message_truncates_utf8_without_panicking() {
        let message = "上游返回了很长的中文错误信息。".repeat(20);

        let redacted = redact_error_message(&message);

        assert!(redacted.ends_with("..."));
        assert!(redacted.len() <= 163);
    }
}
