use crate::models::monitoring::{FailureKind, ProtocolKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtocolCapabilityFacts {
    pub provider_protocol: Option<ProtocolKind>,
    pub endpoint_protocol: Option<ProtocolKind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtocolAutoResolution {
    pub protocol_kind: Option<ProtocolKind>,
    pub failure_kind: Option<FailureKind>,
    pub network_call_count: u32,
}

pub fn resolve_protocol_auto(facts: ProtocolCapabilityFacts) -> ProtocolAutoResolution {
    let resolved = match (facts.provider_protocol, facts.endpoint_protocol) {
        (Some(provider), Some(endpoint)) if provider == endpoint => Some(provider),
        (Some(protocol), None) | (None, Some(protocol)) => Some(protocol),
        _ => None,
    };

    ProtocolAutoResolution {
        protocol_kind: resolved,
        failure_kind: if resolved.is_some() {
            None
        } else {
            Some(FailureKind::NeedsConfiguration)
        },
        network_call_count: 0,
    }
}
