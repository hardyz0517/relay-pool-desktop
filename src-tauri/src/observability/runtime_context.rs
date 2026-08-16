use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::runtime::InteractionId;

const SESSION_PREFIX: &str = "ctx_";
const INTERACTION_PREFIX: &str = "int_";
const TOKEN_HEX_LEN: usize = 32;
pub(crate) const DEFAULT_INTERACTION_TTL: Duration = Duration::from_secs(10 * 60);
pub(crate) const DEFAULT_MAX_ACTIVE_INTERACTIONS: usize = 128;

/// Versioned metadata attached by the single frontend IPC adapter.
///
/// This value is deliberately separate from command input DTOs so business
/// payloads keep their deny-unknown-fields contract. It is validated at the
/// command boundary before being copied into task-local observability state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct IpcRuntimeContextV1 {
    pub context_session_id: String,
    #[serde(default)]
    pub interaction_id: Option<String>,
}

impl IpcRuntimeContextV1 {
    fn validate_shape(&self) -> Result<(), RuntimeContextValidationError> {
        validate_token(&self.context_session_id, SESSION_PREFIX)?;
        if let Some(interaction_id) = &self.interaction_id {
            validate_token(interaction_id, INTERACTION_PREFIX)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ValidatedRuntimeContext {
    pub(crate) interaction_id: Option<InteractionId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeContextValidationError {
    InvalidShape,
    WrongSession,
    Expired,
    Capacity,
}

/// Process-local capability and bounded interaction admission table.
///
/// The context session id is an in-memory capability. Interaction ids are
/// admitted for a monotonic TTL and bounded active cardinality; no rejected
/// value is retained for diagnostics.
pub(crate) struct RuntimeContextRegistry {
    context_session_id: String,
    active: Mutex<HashMap<String, Instant>>,
    ttl: Duration,
    max_active: usize,
}

impl RuntimeContextRegistry {
    pub(crate) fn new() -> Self {
        Self::with_limits(DEFAULT_INTERACTION_TTL, DEFAULT_MAX_ACTIVE_INTERACTIONS)
    }

    pub(crate) fn with_limits(ttl: Duration, max_active: usize) -> Self {
        Self {
            context_session_id: new_token(SESSION_PREFIX),
            active: Mutex::new(HashMap::new()),
            ttl,
            max_active: max_active.max(1),
        }
    }

    pub(crate) fn context_session_id(&self) -> &str {
        &self.context_session_id
    }

    pub(crate) fn validate(
        &self,
        context: Option<&IpcRuntimeContextV1>,
        now: Instant,
    ) -> Result<ValidatedRuntimeContext, RuntimeContextValidationError> {
        let Some(context) = context else {
            return Ok(ValidatedRuntimeContext {
                interaction_id: None,
            });
        };
        context.validate_shape()?;
        if context.context_session_id != self.context_session_id {
            return Err(RuntimeContextValidationError::WrongSession);
        }

        let Some(raw_interaction_id) = context.interaction_id.as_deref() else {
            return Ok(ValidatedRuntimeContext {
                interaction_id: None,
            });
        };

        let mut active = self
            .active
            .lock()
            .map_err(|_| RuntimeContextValidationError::Capacity)?;
        let previously_seen = active.get(raw_interaction_id).copied();
        active.retain(|_, first_seen| elapsed(now, *first_seen) <= self.ttl);

        if let Some(first_seen) = previously_seen {
            if elapsed(now, first_seen) > self.ttl {
                return Err(RuntimeContextValidationError::Expired);
            }
        }

        if let Some(first_seen) = active.get(raw_interaction_id) {
            if elapsed(now, *first_seen) <= self.ttl {
                return Ok(ValidatedRuntimeContext {
                    interaction_id: Some(
                        InteractionId::from_public(raw_interaction_id)
                            .map_err(|_| RuntimeContextValidationError::InvalidShape)?,
                    ),
                });
            }
            active.remove(raw_interaction_id);
            return Err(RuntimeContextValidationError::Expired);
        }

        if active.len() >= self.max_active {
            return Err(RuntimeContextValidationError::Capacity);
        }
        let interaction_id = InteractionId::from_public(raw_interaction_id)
            .map_err(|_| RuntimeContextValidationError::InvalidShape)?;
        active.insert(raw_interaction_id.to_owned(), now);
        Ok(ValidatedRuntimeContext {
            interaction_id: Some(interaction_id),
        })
    }
}

fn elapsed(now: Instant, first_seen: Instant) -> Duration {
    now.checked_duration_since(first_seen).unwrap_or_default()
}

fn new_token(prefix: &str) -> String {
    format!("{prefix}{}", Uuid::now_v7().simple())
}

fn validate_token(value: &str, prefix: &str) -> Result<(), RuntimeContextValidationError> {
    let expected_len = prefix.len() + TOKEN_HEX_LEN;
    if value.len() != expected_len
        || !value.is_ascii()
        || !value.starts_with(prefix)
        || !value[prefix.len()..]
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(RuntimeContextValidationError::InvalidShape);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(
        registry: &RuntimeContextRegistry,
        interaction_id: Option<String>,
    ) -> IpcRuntimeContextV1 {
        IpcRuntimeContextV1 {
            context_session_id: registry.context_session_id().to_owned(),
            interaction_id,
        }
    }

    #[test]
    fn generated_tokens_are_bounded_and_anonymous() {
        let registry = RuntimeContextRegistry::new();
        assert_eq!(
            registry.context_session_id().len(),
            SESSION_PREFIX.len() + TOKEN_HEX_LEN
        );
        assert!(registry.context_session_id().starts_with(SESSION_PREFIX));
        assert!(!registry.context_session_id().contains('-'));
    }

    #[test]
    fn validates_same_interaction_until_ttl_then_rejects() {
        let ttl = Duration::from_secs(10);
        let registry = RuntimeContextRegistry::with_limits(ttl, 2);
        let now = Instant::now();
        let interaction = new_token(INTERACTION_PREFIX);
        let input = context(&registry, Some(interaction.clone()));
        let first = registry.validate(Some(&input), now).expect("first use");
        let second = registry
            .validate(Some(&input), now + Duration::from_secs(9))
            .expect("same interaction within ttl");
        assert_eq!(first.interaction_id, second.interaction_id);
        assert_eq!(
            registry.validate(Some(&input), now + ttl + Duration::from_secs(1)),
            Err(RuntimeContextValidationError::Expired)
        );
    }

    #[test]
    fn rejects_cross_session_shape_and_capacity_without_retaining_values() {
        let registry = RuntimeContextRegistry::with_limits(Duration::from_secs(60), 1);
        let now = Instant::now();
        let invalid = IpcRuntimeContextV1 {
            context_session_id: "ctx_not-a-token".into(),
            interaction_id: None,
        };
        assert_eq!(
            registry.validate(Some(&invalid), now),
            Err(RuntimeContextValidationError::InvalidShape)
        );

        let wrong_session = IpcRuntimeContextV1 {
            context_session_id: new_token(SESSION_PREFIX),
            interaction_id: None,
        };
        assert_eq!(
            registry.validate(Some(&wrong_session), now),
            Err(RuntimeContextValidationError::WrongSession)
        );

        let first = context(&registry, Some(new_token(INTERACTION_PREFIX)));
        registry.validate(Some(&first), now).expect("first slot");
        let second = context(&registry, Some(new_token(INTERACTION_PREFIX)));
        assert_eq!(
            registry.validate(Some(&second), now),
            Err(RuntimeContextValidationError::Capacity)
        );
    }

    #[test]
    fn serializes_with_camel_case_and_optional_interaction() {
        let registry = RuntimeContextRegistry::new();
        let value = serde_json::to_value(context(&registry, None)).expect("serialize");
        assert!(value.get("contextSessionId").is_some());
        assert!(value.get("interactionId").is_some());
        assert!(value["interactionId"].is_null());
    }
}
