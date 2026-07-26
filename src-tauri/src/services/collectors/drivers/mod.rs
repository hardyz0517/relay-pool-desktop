use crate::services::collectors::contract::{
    DriverCapabilities, ProviderDescriptor, ProviderEntry, ProviderKind,
};

pub fn stage19a_static_entries() -> Vec<ProviderEntry> {
    vec![
        ProviderEntry::unsupported(ProviderDescriptor {
            kind: ProviderKind::Sub2Api,
            display_name: "Sub2API",
            station_types: &["sub2api"],
            capabilities: DriverCapabilities::none(),
        }),
        ProviderEntry::unsupported(ProviderDescriptor {
            kind: ProviderKind::NewApi,
            display_name: "NewAPI",
            station_types: &["newapi"],
            capabilities: DriverCapabilities::none(),
        }),
        ProviderEntry::unsupported(ProviderDescriptor {
            kind: ProviderKind::OpenAiCompatible,
            display_name: "OpenAI-compatible",
            station_types: &["openai-compatible", "openai_compatible"],
            capabilities: DriverCapabilities::none(),
        }),
    ]
}

pub const REQUIRED_PROVIDER_KINDS: &[ProviderKind] = &[
    ProviderKind::Sub2Api,
    ProviderKind::NewApi,
    ProviderKind::OpenAiCompatible,
];
