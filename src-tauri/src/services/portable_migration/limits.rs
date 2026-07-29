use std::time::Duration;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PortableMigrationLimitsV1 {
    pub(crate) max_age_file_bytes: u64,
    pub(crate) max_sqlite_bytes: u64,
    pub(crate) max_manifest_bytes: usize,
    pub(crate) max_extensions_bytes: usize,
    pub(crate) max_passphrase_utf8_bytes: usize,
    pub(crate) max_required_features: usize,
    pub(crate) max_required_feature_bytes: usize,
    pub(crate) max_record_counts: usize,
    pub(crate) max_regular_field_bytes: usize,
    pub(crate) max_large_redacted_json_field_bytes: usize,
    pub(crate) max_json_depth: usize,
    pub(crate) max_rows_per_table: u64,
    pub(crate) max_total_user_table_rows: u64,
    pub(crate) export_deadline_secs: u64,
    pub(crate) inspection_deadline_secs: u64,
    pub(crate) prepare_deadline_secs: u64,
    pub(crate) drain_deadline_secs: u64,
}

impl PortableMigrationLimitsV1 {
    pub(crate) const CURRENT: Self = Self {
        max_age_file_bytes: 2_416_919_552,
        max_sqlite_bytes: 2_147_483_648,
        max_manifest_bytes: 262_144,
        max_extensions_bytes: 65_536,
        max_passphrase_utf8_bytes: 1_024,
        max_required_features: 64,
        max_required_feature_bytes: 128,
        max_record_counts: 128,
        max_regular_field_bytes: 1_048_576,
        max_large_redacted_json_field_bytes: 8_388_608,
        max_json_depth: 64,
        max_rows_per_table: 5_000_000,
        max_total_user_table_rows: 10_000_000,
        export_deadline_secs: 7_200,
        inspection_deadline_secs: 1_800,
        prepare_deadline_secs: 7_200,
        drain_deadline_secs: 30,
    };

    pub(crate) const fn export_deadline(self) -> Duration {
        Duration::from_secs(self.export_deadline_secs)
    }

    pub(crate) const fn inspection_deadline(self) -> Duration {
        Duration::from_secs(self.inspection_deadline_secs)
    }

    pub(crate) const fn prepare_deadline(self) -> Duration {
        Duration::from_secs(self.prepare_deadline_secs)
    }

    pub(crate) const fn drain_deadline(self) -> Duration {
        Duration::from_secs(self.drain_deadline_secs)
    }

    pub(crate) fn decrypted_payload_upper_bound(
        self,
        manifest_len: usize,
        sqlite_len: u64,
    ) -> Option<u64> {
        let manifest_len = u64::try_from(manifest_len).ok()?;
        8_u64
            .checked_add(4)?
            .checked_add(manifest_len)?
            .checked_add(32)?
            .checked_add(8)?
            .checked_add(sqlite_len)?
            .checked_add(32)
    }

    pub(crate) fn validate_passphrase(self, passphrase: &str) -> Result<(), LimitViolation> {
        if passphrase.len() > self.max_passphrase_utf8_bytes {
            return Err(LimitViolation::PassphraseTooLarge);
        }
        Ok(())
    }

    pub(crate) fn validate_regular_field_len(self, len: usize) -> Result<(), LimitViolation> {
        if len > self.max_regular_field_bytes {
            return Err(LimitViolation::RegularFieldTooLarge);
        }
        Ok(())
    }

    pub(crate) fn validate_large_redacted_json_field_len(
        self,
        len: usize,
    ) -> Result<(), LimitViolation> {
        if len > self.max_large_redacted_json_field_bytes {
            return Err(LimitViolation::LargeRedactedJsonFieldTooLarge);
        }
        Ok(())
    }
}

impl Default for PortableMigrationLimitsV1 {
    fn default() -> Self {
        Self::CURRENT
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum LimitViolation {
    #[error("portable migration passphrase exceeds v1 limit")]
    PassphraseTooLarge,
    #[error("portable migration regular field exceeds v1 limit")]
    RegularFieldTooLarge,
    #[error("portable migration large redacted JSON field exceeds v1 limit")]
    LargeRedactedJsonFieldTooLarge,
}

#[cfg(test)]
mod tests {
    use super::{LimitViolation, PortableMigrationLimitsV1};

    #[test]
    fn operation_deadlines_are_explicit_and_do_not_use_global_default() {
        let limits = PortableMigrationLimitsV1::CURRENT;

        assert_eq!(limits.export_deadline().as_secs(), 2 * 60 * 60);
        assert_eq!(limits.inspection_deadline().as_secs(), 30 * 60);
        assert_eq!(limits.prepare_deadline().as_secs(), 2 * 60 * 60);
        assert_eq!(limits.drain_deadline().as_secs(), 30);
    }

    #[test]
    fn numeric_limits_accept_limit_minus_one_and_limit_but_reject_limit_plus_one() {
        let limits = PortableMigrationLimitsV1::CURRENT;

        assert!(limits
            .validate_passphrase(&"a".repeat(limits.max_passphrase_utf8_bytes - 1))
            .is_ok());
        assert!(limits
            .validate_passphrase(&"a".repeat(limits.max_passphrase_utf8_bytes))
            .is_ok());
        assert_eq!(
            limits
                .validate_passphrase(&"a".repeat(limits.max_passphrase_utf8_bytes + 1))
                .unwrap_err(),
            LimitViolation::PassphraseTooLarge
        );

        assert!(limits
            .validate_regular_field_len(limits.max_regular_field_bytes - 1)
            .is_ok());
        assert!(limits
            .validate_regular_field_len(limits.max_regular_field_bytes)
            .is_ok());
        assert_eq!(
            limits
                .validate_regular_field_len(limits.max_regular_field_bytes + 1)
                .unwrap_err(),
            LimitViolation::RegularFieldTooLarge
        );
    }

    #[test]
    fn decrypted_payload_bound_uses_checked_arithmetic() {
        let limits = PortableMigrationLimitsV1::CURRENT;

        assert_eq!(
            limits
                .decrypted_payload_upper_bound(limits.max_manifest_bytes, limits.max_sqlite_bytes),
            Some(84 + limits.max_manifest_bytes as u64 + limits.max_sqlite_bytes)
        );
        assert_eq!(limits.decrypted_payload_upper_bound(1, u64::MAX), None);
    }
}
