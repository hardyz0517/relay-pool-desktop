#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OperationalFactReadOptions {
    /// Maximum number of statically eligible routing candidates. The planning
    /// builder applies this after model/capability evaluation; the fact source
    /// must remain complete so an early database row cannot starve a match.
    candidate_limit: usize,
    requested_model: Option<String>,
    include_model_catalog: bool,
    include_legacy_aliases: bool,
}

impl OperationalFactReadOptions {
    pub(crate) fn for_request_model(model: impl Into<String>) -> Self {
        Self {
            candidate_limit: MAX_OPERATIONAL_CANDIDATES,
            requested_model: Some(model.into()),
            include_model_catalog: false,
            include_legacy_aliases: true,
        }
    }

    pub(crate) fn for_model_catalog() -> Self {
        Self {
            candidate_limit: MAX_OPERATIONAL_CANDIDATES,
            requested_model: None,
            include_model_catalog: true,
            include_legacy_aliases: true,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_candidate_limit(mut self, candidate_limit: usize) -> Self {
        self.candidate_limit = candidate_limit;
        self
    }

    pub(crate) fn candidate_limit(&self) -> usize {
        self.candidate_limit
    }

    pub(crate) fn requested_model(&self) -> Option<&str> {
        self.requested_model.as_deref()
    }

    pub(crate) fn include_model_catalog(&self) -> bool {
        self.include_model_catalog
    }

    /// Keeps legacy alias rows available to migration/audit readers while
    /// allowing production planning to opt out of the retired table entirely.
    pub(crate) fn without_legacy_aliases(mut self) -> Self {
        self.include_legacy_aliases = false;
        self
    }

    pub(crate) fn includes_legacy_aliases(&self) -> bool {
        self.include_legacy_aliases
    }
}

pub(crate) const MAX_OPERATIONAL_CANDIDATES: usize = 1024;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RawOperationalCandidateRow {
    pub(crate) station_key_id: String,
    pub(crate) station_id: String,
    pub(crate) key_enabled: bool,
    pub(crate) station_enabled: bool,
    pub(crate) endpoint_revision: i64,
    pub(crate) api_base_url: String,
    pub(crate) credential_available: bool,
    pub(crate) key_record_revision: i64,
    pub(crate) station_record_revision: i64,
    pub(crate) account_record_revision: i64,
    pub(crate) schedulable: bool,
    pub(crate) priority: i64,
    pub(crate) backup_only: bool,
    pub(crate) group_binding_id: Option<String>,
    pub(crate) group_record_revision: Option<i64>,
    pub(crate) group_binding_status: Option<String>,
    pub(crate) station_native_multiplier: Option<f64>,
    pub(crate) credit_per_cny: Option<f64>,
    pub(crate) group_id_hash: Option<String>,
    pub(crate) group_category: Option<String>,
    pub(crate) supports_chat_completions: bool,
    pub(crate) supports_responses: bool,
    pub(crate) supports_stream: bool,
    pub(crate) supports_tools: bool,
    pub(crate) supports_vision: bool,
    pub(crate) supports_reasoning: bool,
    pub(crate) model_allowlist_json: String,
    pub(crate) model_blocklist_json: String,
    pub(crate) preferred_models_json: String,
    pub(crate) routing_tags_json: String,
    pub(crate) balance_status: Option<String>,
    pub(crate) balance_value: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RawOperationalSettingRow {
    pub(crate) key: String,
    pub(crate) value: String,
    pub(crate) record_revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RawOperationalModelAliasRow {
    pub(crate) alias_id: String,
    pub(crate) client_model: String,
    pub(crate) upstream_model: String,
    pub(crate) record_revision: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RawOperationalFactRows {
    pub(crate) candidates: Vec<RawOperationalCandidateRow>,
    pub(crate) settings: Vec<RawOperationalSettingRow>,
    pub(crate) model_aliases: Vec<RawOperationalModelAliasRow>,
    pub(crate) query_count: usize,
    pub(crate) loaded_full_model_catalog: bool,
}
