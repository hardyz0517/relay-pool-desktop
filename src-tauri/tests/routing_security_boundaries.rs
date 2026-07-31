use std::{fs, path::PathBuf};

fn repo_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repo root")
        .join(relative)
}

fn read_repo_file(relative: &str) -> String {
    fs::read_to_string(repo_path(relative)).unwrap_or_else(|error| panic!("read {relative}: {error}"))
}

#[test]
fn request_log_writes_and_dtos_never_rehydrate_full_upstream_url() {
    let finalization = read_repo_file("src-tauri/src/application/request_finalization/mod.rs");
    assert!(finalization.contains("upstream_base_url: None"));
    assert!(finalization
        .contains("request_terminal_mapping_preserves_safe_annotations_and_redacts_upstream_base_url"));
    assert!(!finalization.contains("upstream_base_url: annotations.upstream_base_url"));

    let store = read_repo_file("src-tauri/src/persistence/stores/request_log_store.rs");
    assert!(store.contains("Option::<&str>::None"));
    assert!(store.contains("upstream_base_url = ?"));
    assert!(!store.contains(".bind(record.annotations.upstream_base_url.as_deref())"));

    let logs_page = read_repo_file("src/features/logs/LogsPage.tsx");
    assert!(logs_page.contains("selected.upstreamBaseUrl ??"));
    assert!(!logs_page.contains("station.apiBaseUrl"));
    assert!(!logs_page.contains("websiteUrl"));
}

#[test]
fn sanitizer_lifecycle_is_owned_by_persistence_upgrade_and_runtime_ready_gate() {
    let sanitizer = read_repo_file("src-tauri/src/persistence/maintenance/request_log_url_sanitizer.rs");
    assert!(sanitizer.contains("CAST(upstream_base_url AS BLOB)"));
    assert!(sanitizer.contains("std::str::from_utf8(input)"));
    assert!(sanitizer.contains("url.set_query(None)"));
    assert!(sanitizer.contains("url.set_fragment(None)"));
    assert!(sanitizer.contains("url.set_path(\"\")"));
    assert!(sanitizer.contains("SET upstream_base_url = NULL"));
    assert!(sanitizer.contains("PRAGMA wal_checkpoint(TRUNCATE)"));
    assert!(sanitizer.contains("VACUUM"));
    assert!(!sanitizer.contains("SET upstream_base_url = ?"));

    let migrations = read_repo_file("src-tauri/src/persistence/migrations.rs");
    assert!(migrations.contains("sanitize_request_log_upstream_urls_before_schema18"));
    assert!(migrations.contains("if (5..18).contains(&schema_version)"));
    assert!(
        migrations.contains("sanitize_request_log_upstream_urls(&pool, RequestLogUrlSanitizerOptions::default()).await?")
    );

    let runtime = read_repo_file("src-tauri/src/persistence/runtime.rs");
    assert!(runtime.contains("if sqlx_version >= 18"));
    assert!(runtime.contains("assert_request_log_url_sanitizer_complete_on_connection"));
}
