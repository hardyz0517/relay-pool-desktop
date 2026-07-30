use std::collections::BTreeMap;

use crate::{
    models::monitoring::{ClientProfileId, ProtocolKind},
    services::monitoring::profiles::{
        claude_code::claude_code_compat_v1, codex_cli::codex_cli_compat_v1,
        gemini_cli::gemini_cli_compat_v1, standard::standard_api_v1, ClientProfileDefinition,
        ClientProfileRequestShape,
    },
};

#[derive(Debug, Clone)]
pub struct BuiltinProfileRegistry {
    profiles: BTreeMap<ClientProfileId, ClientProfileDefinition>,
}

impl Default for BuiltinProfileRegistry {
    fn default() -> Self {
        let profiles = [
            standard_api_v1(),
            codex_cli_compat_v1(),
            claude_code_compat_v1(),
            gemini_cli_compat_v1(),
            grok_cli_compat_disabled_placeholder(),
        ]
        .into_iter()
        .map(|profile| {
            profile
                .validate_boundaries()
                .expect("builtin profile respects auth boundary");
            (profile.id, profile)
        })
        .collect();
        Self { profiles }
    }
}

impl BuiltinProfileRegistry {
    pub fn get(&self, id: ClientProfileId) -> Option<&ClientProfileDefinition> {
        self.profiles.get(&id)
    }

    pub fn list(&self) -> impl Iterator<Item = &ClientProfileDefinition> {
        self.profiles.values()
    }

    pub fn validate_execution_profile(
        &self,
        id: ClientProfileId,
        protocol: ProtocolKind,
    ) -> Result<(), String> {
        let profile = self
            .get(id)
            .ok_or_else(|| "client profile is not registered".to_string())?;
        profile
            .validate_boundaries()
            .map_err(|violation| format!("client profile violates auth boundary: {violation:?}"))?;
        if !profile.supports_protocol(protocol) {
            return Err("client profile does not support protocol".to_string());
        }
        Ok(())
    }
}

fn grok_cli_compat_disabled_placeholder() -> ClientProfileDefinition {
    ClientProfileDefinition {
        id: ClientProfileId::GrokCliCompat,
        version: 1,
        enabled: false,
        supported_protocols: Vec::new(),
        request: ClientProfileRequestShape {
            method: "POST".to_string(),
            path: "{disabled_until_verified}".to_string(),
            headers: Vec::new(),
            body_defaults: Vec::new(),
        },
    }
}
