use serde::{Deserialize, Serialize};

use super::subject::StableEventCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DataDisposition {
    NotCollected,
    Redacted,
    SafeEnum,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimeError {
    pub(crate) domain: StableEventCode,
    pub(crate) code: StableEventCode,
    pub(crate) retryable: bool,
    pub(crate) data_disposition: DataDisposition,
}

impl RuntimeError {
    pub(crate) fn new(
        domain: StableEventCode,
        code: StableEventCode,
        retryable: bool,
        data_disposition: DataDisposition,
    ) -> Self {
        Self {
            domain,
            code,
            retryable,
            data_disposition,
        }
    }
}
