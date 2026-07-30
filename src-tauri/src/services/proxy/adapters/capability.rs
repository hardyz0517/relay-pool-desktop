#![allow(dead_code)]

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AdapterCapabilityProtocol {
    ChatCompletions,
    Responses,
    Embeddings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AdapterCapabilityFeature {
    Stream,
    Tools,
    Vision,
    Reasoning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AdapterCapabilitySubject {
    Protocol(AdapterCapabilityProtocol),
    Feature(AdapterCapabilityFeature),
    Model { model: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AdapterCapabilityVerdict {
    Supported,
    Unsupported,
    Uncertain,
    Neutral,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AdapterCapabilitySignalKind {
    Structural,
    Semantic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AdapterCapabilitySignal {
    pub(crate) subject: AdapterCapabilitySubject,
    pub(crate) verdict: AdapterCapabilityVerdict,
    pub(crate) kind: AdapterCapabilitySignalKind,
    pub(crate) reason: &'static str,
}

impl AdapterCapabilitySignal {
    pub(crate) fn structural(
        subject: AdapterCapabilitySubject,
        verdict: AdapterCapabilityVerdict,
        reason: &'static str,
    ) -> Self {
        Self {
            subject,
            verdict,
            kind: AdapterCapabilitySignalKind::Structural,
            reason,
        }
    }

    pub(crate) fn semantic(
        subject: AdapterCapabilitySubject,
        verdict: AdapterCapabilityVerdict,
        reason: &'static str,
    ) -> Self {
        Self {
            subject,
            verdict,
            kind: AdapterCapabilitySignalKind::Semantic,
            reason,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AdapterHttpCapabilityProfile {
    OpenAiKnown,
    GenericOpenAiCompatible,
}

pub(crate) fn model_signal_from_http_status(
    profile: AdapterHttpCapabilityProfile,
    model: &str,
    status: u16,
) -> AdapterCapabilitySignal {
    let subject = AdapterCapabilitySubject::Model {
        model: model.to_string(),
    };
    match (profile, status) {
        (_, 429) => AdapterCapabilitySignal::semantic(
            subject,
            AdapterCapabilityVerdict::Neutral,
            "rate_limited_not_capability_evidence",
        ),
        (_, 503) => AdapterCapabilitySignal::semantic(
            subject,
            AdapterCapabilityVerdict::Neutral,
            "overloaded_not_capability_evidence",
        ),
        (AdapterHttpCapabilityProfile::OpenAiKnown, 404) => AdapterCapabilitySignal::semantic(
            subject,
            AdapterCapabilityVerdict::Unsupported,
            "known_openai_model_not_found",
        ),
        (AdapterHttpCapabilityProfile::GenericOpenAiCompatible, 403 | 404) => {
            AdapterCapabilitySignal::semantic(
                subject,
                AdapterCapabilityVerdict::Uncertain,
                "generic_status_without_capability_semantics",
            )
        }
        _ => AdapterCapabilitySignal::semantic(
            subject,
            AdapterCapabilityVerdict::Neutral,
            "http_status_not_capability_evidence",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_403_and_404_are_uncertain_not_negative_capability_evidence() {
        for status in [403, 404] {
            let signal = model_signal_from_http_status(
                AdapterHttpCapabilityProfile::GenericOpenAiCompatible,
                "model",
                status,
            );
            assert_eq!(signal.verdict, AdapterCapabilityVerdict::Uncertain);
        }
    }

    #[test]
    fn rate_limit_and_overload_are_neutral_not_model_unsupported() {
        for status in [429, 503] {
            let signal = model_signal_from_http_status(
                AdapterHttpCapabilityProfile::OpenAiKnown,
                "model",
                status,
            );
            assert_eq!(signal.verdict, AdapterCapabilityVerdict::Neutral);
        }
    }
}
