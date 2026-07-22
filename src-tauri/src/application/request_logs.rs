use std::sync::{Arc, Mutex, PoisonError};

use crate::{
    application::{error::ApplicationError, pagination::PageLimit},
    models::proxy::RequestLog,
    persistence::{
        runtime::{PersistenceHandle, PersistenceVersion},
        stores::request_log_store::RequestLogStore,
    },
};

#[derive(Clone)]
pub(crate) struct RequestLogService {
    runtime: PersistenceHandle,
    store: RequestLogStore,
    recent_cache: Arc<Mutex<RecentRequestLogCache>>,
}

#[derive(Default)]
struct RecentRequestLogCache {
    entry: Option<RecentRequestLogCacheEntry>,
    #[cfg(test)]
    database_reads: u64,
}

struct RecentRequestLogCacheEntry {
    limit: u32,
    revision: u64,
    rows: Vec<RequestLog>,
}

impl RequestLogService {
    pub(crate) fn new(runtime: PersistenceHandle) -> Self {
        Self {
            runtime,
            store: RequestLogStore,
            recent_cache: Arc::new(Mutex::new(RecentRequestLogCache::default())),
        }
    }

    pub(crate) async fn list_recent(
        &self,
        limit: PageLimit,
    ) -> Result<Vec<RequestLog>, ApplicationError> {
        let limit = limit.get();
        let version_before_lookup = self.runtime.persistence_version();
        if version_before_lookup.is_quiescent() {
            let cached_rows = self.with_cache(|cache| cache.get(limit, version_before_lookup));
            let version_after_lookup = self.runtime.persistence_version();
            if version_before_lookup == version_after_lookup {
                if let Some(rows) = cached_rows {
                    return Ok(rows);
                }
            }
        }

        let version_before_query = self.runtime.persistence_version();
        let mut read = self.runtime.begin_read().await?;
        let rows = self
            .store
            .list_recent(&mut read, limit)
            .await
            .map_err(ApplicationError::from)?;
        drop(read);
        let version_after_query = self.runtime.persistence_version();

        self.with_cache(|cache| {
            #[cfg(test)]
            {
                cache.database_reads += 1;
            }
            cache.publish(limit, version_before_query, version_after_query, &rows);
        });
        Ok(rows)
    }

    pub(crate) async fn clear(&self) -> Result<(), ApplicationError> {
        let store = self.store;
        self.runtime
            .write(|write| Box::pin(async move { store.clear(write).await }))
            .await
            .map_err(Into::into)
    }

    fn with_cache<T>(&self, operation: impl FnOnce(&mut RecentRequestLogCache) -> T) -> T {
        let mut cache = self
            .recent_cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        operation(&mut cache)
    }

    #[cfg(test)]
    fn database_reads(&self) -> u64 {
        self.with_cache(|cache| cache.database_reads)
    }
}

impl RecentRequestLogCache {
    fn get(&self, limit: u32, version: PersistenceVersion) -> Option<Vec<RequestLog>> {
        if !version.is_quiescent() {
            return None;
        }
        self.entry
            .as_ref()
            .filter(|entry| entry.limit == limit && entry.revision == version.data_revision())
            .map(|entry| entry.rows.clone())
    }

    fn publish(
        &mut self,
        limit: u32,
        version_before_query: PersistenceVersion,
        version_after_query: PersistenceVersion,
        rows: &[RequestLog],
    ) {
        if version_before_query != version_after_query || !version_after_query.is_quiescent() {
            return;
        }
        if self
            .entry
            .as_ref()
            .is_some_and(|entry| entry.revision > version_after_query.data_revision())
        {
            return;
        }
        self.entry = Some(RecentRequestLogCacheEntry {
            limit,
            revision: version_after_query.data_revision(),
            rows: rows.to_vec(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::runtime::PersistenceRuntime;

    #[tokio::test]
    async fn cache_hits_same_revision_and_commit_invalidates_but_rollback_does_not() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("request-log-cache.sqlite3");
        let runtime = PersistenceRuntime::initialize_new(&path)
            .await
            .expect("runtime");
        let service = RequestLogService::new(runtime.handle());
        let limit = PageLimit::new(500).expect("bounded limit");

        assert!(service
            .list_recent(limit)
            .await
            .expect("initial read")
            .is_empty());
        assert!(service
            .list_recent(limit)
            .await
            .expect("cached read")
            .is_empty());
        assert_eq!(service.database_reads(), 1);

        insert_request_log(&runtime, "committed-1", true).await;
        assert_eq!(
            service
                .list_recent(limit)
                .await
                .expect("after commit")
                .len(),
            1
        );
        assert_eq!(service.database_reads(), 2);
        assert_eq!(
            service.list_recent(limit).await.expect("cached row").len(),
            1
        );
        assert_eq!(service.database_reads(), 2);

        insert_request_log(&runtime, "rolled-back", false).await;
        assert_eq!(
            service
                .list_recent(limit)
                .await
                .expect("after rollback")
                .len(),
            1
        );
        assert_eq!(service.database_reads(), 2);

        let mut unrelated = runtime.begin_write().await.expect("unrelated write");
        sqlx::query(
            "UPDATE persistence_schema_compatibility SET updated_at = updated_at WHERE singleton_key = 1",
        )
        .execute(unrelated.connection())
        .await
        .expect("unrelated update");
        unrelated.commit().await.expect("unrelated commit");
        assert_eq!(
            service
                .list_recent(limit)
                .await
                .expect("after unrelated commit")
                .len(),
            1
        );
        assert_eq!(service.database_reads(), 3);
    }

    #[test]
    fn stale_or_overlapping_query_cannot_replace_a_newer_cache_revision() {
        let revision_one = PersistenceVersion::for_test(1, 0);
        let revision_two = PersistenceVersion::for_test(2, 0);
        let active_revision_two = PersistenceVersion::for_test(2, 1);
        let mut cache = RecentRequestLogCache::default();

        cache.publish(500, revision_one, revision_one, &[]);
        assert_eq!(cache.entry.as_ref().map(|entry| entry.revision), Some(1));
        cache.publish(500, revision_two, revision_two, &[]);
        assert_eq!(cache.entry.as_ref().map(|entry| entry.revision), Some(2));

        cache.publish(500, revision_one, revision_one, &[]);
        cache.publish(500, revision_two, active_revision_two, &[]);
        cache.publish(500, revision_one, revision_two, &[]);
        assert_eq!(cache.entry.as_ref().map(|entry| entry.revision), Some(2));
    }

    async fn insert_request_log(runtime: &PersistenceRuntime, id: &str, commit: bool) {
        let mut write = runtime.begin_write().await.expect("write");
        sqlx::query(
            "INSERT INTO request_logs (id, request_id, started_at, method, path, endpoint, status, created_at) VALUES (?1, ?1, '1', 'POST', '/v1/chat/completions', 'chat_completions', 'started', ?1)",
        )
        .bind(id)
        .execute(write.connection())
        .await
        .expect("insert request log");
        if commit {
            write.commit().await.expect("commit");
        }
    }
}
