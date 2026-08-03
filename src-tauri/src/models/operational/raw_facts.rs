#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OperationalFactReadOptions {
    candidate_limit: usize,
    requested_model: Option<String>,
    include_model_catalog: bool,
}

impl OperationalFactReadOptions {
    #[cfg(test)]
    pub(crate) fn for_request_model(model: impl Into<String>) -> Self {
        Self {
            candidate_limit: MAX_OPERATIONAL_CANDIDATES,
            requested_model: Some(model.into()),
            include_model_catalog: false,
        }
    }

    #[cfg(test)]
    pub(crate) fn for_model_catalog() -> Self {
        Self {
            candidate_limit: MAX_OPERATIONAL_CANDIDATES,
            requested_model: None,
            include_model_catalog: true,
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
}

#[cfg(test)]
pub(crate) const MAX_OPERATIONAL_CANDIDATES: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RawOperationalCandidateRow {
    pub(crate) station_key_id: String,
    pub(crate) station_id: String,
    pub(crate) endpoint_revision: i64,
    pub(crate) api_base_url: String,
    pub(crate) credential_available: bool,
    pub(crate) key_record_revision: i64,
    pub(crate) station_record_revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RawOperationalSettingRow {
    pub(crate) key: String,
    pub(crate) value: String,
    pub(crate) record_revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RawOperationalModelAliasRow {
    pub(crate) client_model: String,
    pub(crate) upstream_model: String,
    pub(crate) record_revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RawOperationalFactRows {
    pub(crate) candidates: Vec<RawOperationalCandidateRow>,
    pub(crate) settings: Vec<RawOperationalSettingRow>,
    pub(crate) model_aliases: Vec<RawOperationalModelAliasRow>,
    pub(crate) query_count: usize,
    pub(crate) loaded_full_model_catalog: bool,
}
