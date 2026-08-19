//! SQL-only persistence boundary for the model-mapping aggregate.
//!
//! This module intentionally does not decode the mapping document, compile
//! rules, or publish revision events.  The application service owns those
//! concerns and uses this store for normalized rows and immutable history.

use sqlx::{Row, SqliteConnection};

use crate::persistence::error::PersistenceError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoredModelMappingPolicy {
    pub(crate) revision: i64,
    pub(crate) unmatched_model_behavior: String,
    pub(crate) updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoredModelMappingRule {
    pub(crate) id: String,
    pub(crate) priority: i64,
    pub(crate) enabled: bool,
    pub(crate) matcher_kind: String,
    pub(crate) matcher_value: Option<String>,
    pub(crate) endpoint_conditions_json: String,
    pub(crate) stream_condition: String,
    pub(crate) tools_condition: String,
    pub(crate) vision_condition: String,
    pub(crate) reasoning_condition: String,
    pub(crate) action_kind: String,
    pub(crate) fallback_trigger: Option<String>,
    pub(crate) rejection_kind: Option<String>,
    pub(crate) rejection_message: Option<String>,
    pub(crate) note: Option<String>,
    pub(crate) created_at_ms: i64,
    pub(crate) updated_at_ms: i64,
    pub(crate) revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoredModelMappingTarget {
    pub(crate) id: String,
    pub(crate) rule_id: String,
    pub(crate) position: i64,
    pub(crate) target_kind: String,
    pub(crate) literal_upstream_model: Option<String>,
    pub(crate) model_profile_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoredModelProfile {
    pub(crate) id: String,
    pub(crate) canonical_model: String,
    pub(crate) display_name: String,
    pub(crate) default_upstream_model: Option<String>,
    pub(crate) status: String,
    pub(crate) note: Option<String>,
    pub(crate) created_at_ms: i64,
    pub(crate) updated_at_ms: i64,
    pub(crate) revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoredModelOfferingBinding {
    pub(crate) id: String,
    pub(crate) model_profile_id: String,
    pub(crate) station_key_id: Option<String>,
    pub(crate) station_id: Option<String>,
    pub(crate) upstream_model: String,
    pub(crate) source: String,
    pub(crate) enabled: bool,
    pub(crate) note: Option<String>,
    pub(crate) created_at_ms: i64,
    pub(crate) updated_at_ms: i64,
    pub(crate) revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoredLegacyModelAliasReview {
    pub(crate) id: String,
    pub(crate) legacy_alias_id: Option<String>,
    pub(crate) requested_model: Option<String>,
    pub(crate) selected_target: Option<String>,
    pub(crate) discarded_target: Option<String>,
    pub(crate) migration_status: String,
    pub(crate) created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoredModelMappingHistory {
    pub(crate) revision: i64,
    pub(crate) document_json: String,
    pub(crate) source: String,
    pub(crate) created_at_ms: i64,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ModelMappingStore;

impl ModelMappingStore {
    pub(crate) async fn load_policy(
        &self,
        connection: &mut SqliteConnection,
    ) -> Result<StoredModelMappingPolicy, PersistenceError> {
        let row = sqlx::query(
            "SELECT revision, unmatched_model_behavior, updated_at_ms
             FROM model_mapping_policies WHERE singleton_key = 1",
        )
        .fetch_optional(&mut *connection)
        .await?
        .ok_or(PersistenceError::NotFound)?;
        let revision: i64 = row.get("revision");
        let updated_at_ms: i64 = row.get("updated_at_ms");
        if revision <= 0 || updated_at_ms < 0 {
            return Err(PersistenceError::InvariantViolation(
                "model mapping policy revision is invalid".into(),
            ));
        }
        Ok(StoredModelMappingPolicy {
            revision,
            unmatched_model_behavior: row.get("unmatched_model_behavior"),
            updated_at_ms,
        })
    }

    pub(crate) async fn list_rules(
        &self,
        connection: &mut SqliteConnection,
        enabled_only: bool,
    ) -> Result<Vec<StoredModelMappingRule>, PersistenceError> {
        let query = if enabled_only {
            "SELECT id, priority, enabled, matcher_kind, matcher_value,
                    endpoint_conditions_json, stream_condition, tools_condition,
                    vision_condition, reasoning_condition, action_kind,
                    fallback_trigger, rejection_kind, rejection_message, note,
                    created_at_ms, updated_at_ms, revision
             FROM model_mapping_rules
             WHERE enabled = 1
             ORDER BY priority DESC, id COLLATE BINARY ASC"
        } else {
            "SELECT id, priority, enabled, matcher_kind, matcher_value,
                    endpoint_conditions_json, stream_condition, tools_condition,
                    vision_condition, reasoning_condition, action_kind,
                    fallback_trigger, rejection_kind, rejection_message, note,
                    created_at_ms, updated_at_ms, revision
             FROM model_mapping_rules
             ORDER BY priority DESC, id COLLATE BINARY ASC"
        };
        let rows = sqlx::query(query).fetch_all(&mut *connection).await?;
        rows.into_iter().map(rule_from_row).collect()
    }

    pub(crate) async fn list_rule_targets(
        &self,
        connection: &mut SqliteConnection,
        rule_id: &str,
    ) -> Result<Vec<StoredModelMappingTarget>, PersistenceError> {
        if rule_id.is_empty() {
            return Err(PersistenceError::ConstraintViolation);
        }
        let rows = sqlx::query(
            "SELECT id, rule_id, position, target_kind,
                    literal_upstream_model, model_profile_id
             FROM model_mapping_rule_targets
             WHERE rule_id = ?1
             ORDER BY position ASC, id COLLATE BINARY ASC",
        )
        .bind(rule_id)
        .fetch_all(&mut *connection)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| StoredModelMappingTarget {
                id: row.get("id"),
                rule_id: row.get("rule_id"),
                position: row.get("position"),
                target_kind: row.get("target_kind"),
                literal_upstream_model: row.get("literal_upstream_model"),
                model_profile_id: row.get("model_profile_id"),
            })
            .collect())
    }

    pub(crate) async fn list_profiles(
        &self,
        connection: &mut SqliteConnection,
        active_only: bool,
    ) -> Result<Vec<StoredModelProfile>, PersistenceError> {
        let query = if active_only {
            "SELECT id, canonical_model, display_name, default_upstream_model,
                    status, note, created_at_ms, updated_at_ms, revision
             FROM model_profiles WHERE status = 'active'
             ORDER BY canonical_model COLLATE BINARY ASC, id COLLATE BINARY ASC"
        } else {
            "SELECT id, canonical_model, display_name, default_upstream_model,
                    status, note, created_at_ms, updated_at_ms, revision
             FROM model_profiles
             ORDER BY canonical_model COLLATE BINARY ASC, id COLLATE BINARY ASC"
        };
        let rows = sqlx::query(query).fetch_all(&mut *connection).await?;
        Ok(rows
            .into_iter()
            .map(|row| StoredModelProfile {
                id: row.get("id"),
                canonical_model: row.get("canonical_model"),
                display_name: row.get("display_name"),
                default_upstream_model: row.get("default_upstream_model"),
                status: row.get("status"),
                note: row.get("note"),
                created_at_ms: row.get("created_at_ms"),
                updated_at_ms: row.get("updated_at_ms"),
                revision: row.get("revision"),
            })
            .collect())
    }

    pub(crate) async fn list_bindings(
        &self,
        connection: &mut SqliteConnection,
        profile_id: Option<&str>,
        enabled_only: bool,
    ) -> Result<Vec<StoredModelOfferingBinding>, PersistenceError> {
        let rows = sqlx::query(
            "SELECT id, model_profile_id, station_key_id, station_id,
                    upstream_model, source, enabled, note,
                    created_at_ms, updated_at_ms, revision
             FROM model_offering_bindings
             WHERE (?1 IS NULL OR model_profile_id = ?1)
               AND (?2 = 0 OR enabled = 1)
             ORDER BY model_profile_id COLLATE BINARY ASC,
                      station_key_id COLLATE BINARY ASC,
                      station_id COLLATE BINARY ASC,
                      id COLLATE BINARY ASC",
        )
        .bind(profile_id)
        .bind(if enabled_only { 1_i64 } else { 0_i64 })
        .fetch_all(&mut *connection)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| StoredModelOfferingBinding {
                id: row.get("id"),
                model_profile_id: row.get("model_profile_id"),
                station_key_id: row.get("station_key_id"),
                station_id: row.get("station_id"),
                upstream_model: row.get("upstream_model"),
                source: row.get("source"),
                enabled: row.get::<i64, _>("enabled") != 0,
                note: row.get("note"),
                created_at_ms: row.get("created_at_ms"),
                updated_at_ms: row.get("updated_at_ms"),
                revision: row.get("revision"),
            })
            .collect())
    }

    pub(crate) async fn list_legacy_reviews(
        &self,
        connection: &mut SqliteConnection,
    ) -> Result<Vec<StoredLegacyModelAliasReview>, PersistenceError> {
        let rows = sqlx::query(
            "SELECT id, legacy_alias_id, requested_model, selected_target,
                    discarded_target, migration_status, created_at_ms
             FROM legacy_model_alias_migration_reviews
             ORDER BY migration_status COLLATE BINARY ASC,
                      requested_model COLLATE BINARY ASC, id COLLATE BINARY ASC",
        )
        .fetch_all(&mut *connection)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| StoredLegacyModelAliasReview {
                id: row.get("id"),
                legacy_alias_id: row.get("legacy_alias_id"),
                requested_model: row.get("requested_model"),
                selected_target: row.get("selected_target"),
                discarded_target: row.get("discarded_target"),
                migration_status: row.get("migration_status"),
                created_at_ms: row.get("created_at_ms"),
            })
            .collect())
    }

    #[cfg(test)]
    pub(crate) async fn list_history(
        &self,
        connection: &mut SqliteConnection,
    ) -> Result<Vec<StoredModelMappingHistory>, PersistenceError> {
        let rows = sqlx::query(
            "SELECT revision, document_json, source, created_at_ms
             FROM model_mapping_document_history ORDER BY revision ASC",
        )
        .fetch_all(&mut *connection)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| StoredModelMappingHistory {
                revision: row.get("revision"),
                document_json: row.get("document_json"),
                source: row.get("source"),
                created_at_ms: row.get("created_at_ms"),
            })
            .collect())
    }

    pub(crate) async fn load_history_revision(
        &self,
        connection: &mut SqliteConnection,
        revision: i64,
    ) -> Result<Option<StoredModelMappingHistory>, PersistenceError> {
        if revision <= 0 {
            return Err(PersistenceError::ConstraintViolation);
        }
        let row = sqlx::query(
            "SELECT revision, document_json, source, created_at_ms
             FROM model_mapping_document_history
             WHERE revision = ?1",
        )
        .bind(revision)
        .fetch_optional(&mut *connection)
        .await?;
        row.map(|row| {
            let revision: i64 = row.get("revision");
            let created_at_ms: i64 = row.get("created_at_ms");
            if revision <= 0 || created_at_ms < 0 {
                return Err(PersistenceError::InvariantViolation(
                    "model mapping history metadata is invalid".into(),
                ));
            }
            Ok(StoredModelMappingHistory {
                revision,
                document_json: row.get("document_json"),
                source: row.get("source"),
                created_at_ms,
            })
        })
        .transpose()
    }
}

fn rule_from_row(row: sqlx::sqlite::SqliteRow) -> Result<StoredModelMappingRule, PersistenceError> {
    let priority: i64 = row.get("priority");
    let created_at_ms: i64 = row.get("created_at_ms");
    let updated_at_ms: i64 = row.get("updated_at_ms");
    let revision: i64 = row.get("revision");
    if created_at_ms < 0 || updated_at_ms < 0 || revision <= 0 {
        return Err(PersistenceError::InvariantViolation(
            "model mapping rule metadata is invalid".into(),
        ));
    }
    Ok(StoredModelMappingRule {
        id: row.get("id"),
        priority,
        enabled: row.get::<i64, _>("enabled") != 0,
        matcher_kind: row.get("matcher_kind"),
        matcher_value: row.get("matcher_value"),
        endpoint_conditions_json: row.get("endpoint_conditions_json"),
        stream_condition: row.get("stream_condition"),
        tools_condition: row.get("tools_condition"),
        vision_condition: row.get("vision_condition"),
        reasoning_condition: row.get("reasoning_condition"),
        action_kind: row.get("action_kind"),
        fallback_trigger: row.get("fallback_trigger"),
        rejection_kind: row.get("rejection_kind"),
        rejection_message: row.get("rejection_message"),
        note: row.get("note"),
        created_at_ms,
        updated_at_ms,
        revision,
    })
}

#[cfg(test)]
mod tests {
    use sqlx::{Connection, Executor, SqliteConnection};

    use super::ModelMappingStore;
    use crate::persistence::migrations::{migrator, migrator_through};

    #[tokio::test]
    async fn foundation_store_reads_migrated_alias_and_review_rows() {
        let mut connection = SqliteConnection::connect("sqlite::memory:")
            .await
            .expect("open sqlite");
        migrator_through(42)
            .expect("schema 42 migrator")
            .run(&mut connection)
            .await
            .expect("migrate baseline");
        connection
            .execute(
                "INSERT INTO model_aliases
                    (id, client_model, upstream_model, enabled, created_at, updated_at)
                 VALUES ('mapping-test-alias', 'codex-test', 'native-test', 1, '1', '1')",
            )
            .await
            .expect("insert alias");
        sqlx::query(
            "INSERT INTO model_aliases
                (id, client_model, upstream_model, enabled, created_at, updated_at)
             VALUES (?1, ?2, 'native-long', 1, '2', '2')",
        )
        .bind("mapping-long-alias")
        .bind("x".repeat(512))
        .execute(&mut connection)
        .await
        .expect("insert long alias");
        migrator()
            .run(&mut connection)
            .await
            .expect("migrate mapping");

        let store = ModelMappingStore;
        let policy = store.load_policy(&mut connection).await.expect("policy");
        assert_eq!(policy.revision, 1);
        assert_eq!(policy.unmatched_model_behavior, "preserve");
        let rules = store
            .list_rules(&mut connection, false)
            .await
            .expect("rules");
        assert_eq!(rules.len(), 2);
        assert!(rules
            .iter()
            .all(|rule| rule.id.len() <= 192 && rule.matcher_value.is_some()));
        let codex_rule = rules
            .iter()
            .find(|rule| rule.matcher_value.as_deref() == Some("codex-test"))
            .expect("codex rule");
        let targets = store
            .list_rule_targets(&mut connection, &codex_rule.id)
            .await
            .expect("targets");
        assert_eq!(targets.len(), 1);
        assert_eq!(
            targets[0].literal_upstream_model.as_deref(),
            Some("native-test")
        );
        assert!(store
            .list_legacy_reviews(&mut connection)
            .await
            .expect("reviews")
            .is_empty());
        let history = store.list_history(&mut connection).await.expect("history");
        assert_eq!(history.len(), 1);
        assert!(history[0].document_json.contains("native-test"));
        let baseline = store
            .load_history_revision(&mut connection, 1)
            .await
            .expect("baseline history")
            .expect("baseline row");
        assert_eq!(baseline.revision, 1);
        assert_eq!(baseline.document_json, history[0].document_json);
        assert!(store
            .load_history_revision(&mut connection, 42)
            .await
            .expect("missing history")
            .is_none());
    }
}
