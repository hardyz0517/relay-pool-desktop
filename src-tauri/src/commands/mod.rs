use std::process::Command;

pub(crate) mod alerting;
pub(crate) mod capture;
pub(crate) mod ccswitch_import;
pub(crate) mod channel_monitoring;
pub(crate) mod channel_status;
pub(crate) mod collector_metadata;
pub(crate) mod credentials;
pub(crate) mod dashboard;
pub(crate) mod data_directory;
pub(crate) mod data_migration;
pub(crate) mod data_recovery;
pub(crate) mod data_store_startup;
pub(crate) mod endpoint_ping;
pub(crate) mod error;
pub(crate) mod key_pool;
pub(crate) mod local_proxy;
pub(crate) mod model_aliases;
pub(crate) mod operations;
pub(crate) mod pricing;
pub(crate) mod pricing_workspace;
pub(crate) mod provider_drafts;
pub(crate) mod request_logs;
pub(crate) mod routing_health;
pub(crate) mod runtime;
pub(crate) mod settings;
pub(crate) mod station_collection;
pub(crate) mod station_key_connectivity;
pub(crate) mod stations;
pub(crate) mod updater;

use crate::{
    application::{
        command_facades::{
            DataDirectoryCommandError, EndpointPingCommandError, LocalProxyCommandError,
        },
        error::ApplicationError,
    },
    background_tasks::{BlockingExecutorError, OperationRegistryError},
    observability::correlation,
};

fn command_application_error(error: ApplicationError) -> error::CommandError {
    error::command_application_error(error)
}

fn public_command_application_error(error: ApplicationError) -> error::CommandError {
    command_application_error(error)
}

fn public_local_proxy_error(error: LocalProxyCommandError) -> error::CommandError {
    match error {
        LocalProxyCommandError::Application(error) => public_command_application_error(error),
        LocalProxyCommandError::Runtime => error::CommandError::internal(None),
    }
}

fn public_endpoint_ping_error(error: EndpointPingCommandError) -> error::CommandError {
    match error {
        EndpointPingCommandError::Application(error) => public_command_application_error(error),
        EndpointPingCommandError::ResultUnknown => {
            error::CommandError::from_work(error::WorkFailure::ResultUnknown)
        }
    }
}

fn public_operation_registry_error(error: OperationRegistryError) -> error::CommandError {
    match error {
        OperationRegistryError::Overloaded => {
            error::CommandError::from_work(error::WorkFailure::Overloaded)
        }
        OperationRegistryError::Conflict { .. } => error::CommandError::try_new(
            error::CommandErrorCode::Conflict,
            "An operation with the same concurrency key is already running.",
            false,
            None,
            None,
        )
        .expect("operation conflict error is a bounded public contract"),
        OperationRegistryError::NotFound => error::CommandError::try_new(
            error::CommandErrorCode::NotFound,
            "The operation was not found.",
            false,
            None,
            None,
        )
        .expect("operation not-found error is a bounded public contract"),
        OperationRegistryError::Expired => error::CommandError::try_new(
            error::CommandErrorCode::NotFound,
            "The operation result has expired.",
            false,
            None,
            None,
        )
        .expect("operation expired error is a bounded public contract"),
        OperationRegistryError::AdmissionClosed => error::CommandError::try_new(
            error::CommandErrorCode::RuntimeUnavailable,
            "The desktop runtime is preparing data maintenance and is not accepting new operations.",
            true,
            None,
            None,
        )
        .expect("operation admission-closed error is a bounded public contract"),
        OperationRegistryError::InvalidSpec
        | OperationRegistryError::ProgressTooLarge { .. }
        | OperationRegistryError::TerminalAlreadyRecorded => error::CommandError::internal(None),
    }
}

fn public_data_directory_error(error: DataDirectoryCommandError) -> error::CommandError {
    match error {
        DataDirectoryCommandError::Application(error) => public_command_application_error(error),
        DataDirectoryCommandError::Blocking(error) => public_blocking_executor_error(error),
    }
}

fn public_blocking_executor_error(error: BlockingExecutorError) -> error::CommandError {
    match error {
        BlockingExecutorError::QueueFull | BlockingExecutorError::QueueTimeout => {
            error::CommandError::from_work(error::WorkFailure::Overloaded)
        }
        BlockingExecutorError::ExecutionTimeout => {
            error::CommandError::from_work(error::WorkFailure::Timeout)
        }
        BlockingExecutorError::CancelledBeforeStart
        | BlockingExecutorError::CancelledLateResultDiscarded => {
            error::CommandError::from_work(error::WorkFailure::ResultUnknown)
        }
        BlockingExecutorError::Closed
        | BlockingExecutorError::Panicked
        | BlockingExecutorError::JobFailed { .. }
        | BlockingExecutorError::ShutdownTimeout { .. } => {
            error::CommandError::from_work(error::WorkFailure::Internal)
        }
    }
}

fn current_correlation_id() -> Option<String> {
    correlation::current().map(|id| id.as_str().to_string())
}

struct SystemUrlLauncher {
    program: &'static str,
    args: Vec<String>,
}

#[cfg(target_os = "windows")]
fn system_url_launcher(url: &str) -> SystemUrlLauncher {
    SystemUrlLauncher {
        program: "rundll32.exe",
        args: vec!["url.dll,FileProtocolHandler".to_string(), url.to_string()],
    }
}

#[cfg(target_os = "macos")]
fn system_url_launcher(url: &str) -> SystemUrlLauncher {
    SystemUrlLauncher {
        program: "open",
        args: vec![url.to_string()],
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn system_url_launcher(url: &str) -> SystemUrlLauncher {
    SystemUrlLauncher {
        program: "xdg-open",
        args: vec![url.to_string()],
    }
}

fn open_url_with_system(url: &str) -> Result<(), String> {
    let launcher = system_url_launcher(url);
    let result = Command::new(launcher.program).args(launcher.args).status();

    result
        .and_then(|status| {
            if status.success() {
                Ok(())
            } else {
                Err(std::io::Error::other(format!(
                    "launcher exited with status {status}"
                )))
            }
        })
        .map_err(|error| format!("无法打开外部链接: {error}"))
}

fn validate_external_http_url(url: &str) -> Result<&str, String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err("外部链接为空，无法打开。".to_string());
    }
    if trimmed.chars().any(char::is_control) {
        return Err("外部链接包含无效字符，无法打开。".to_string());
    }
    let lower = trimmed.to_ascii_lowercase();
    if !lower.starts_with("http://") && !lower.starts_with("https://") {
        return Err("只支持打开 HTTP 或 HTTPS 链接。".to_string());
    }
    Ok(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_monitor_runner_errors_are_result_unknown_and_redacted() {
        let error = channel_monitoring::public_channel_monitor_run_error(
            "provider failed with api_key=sk-secret at C:/private/data.db".into(),
        );

        assert_eq!(error.code, error::CommandErrorCode::Conflict);
        assert_eq!(
            error.message,
            "The operation outcome could not be confirmed."
        );
        assert!(!error.retryable);
        assert!(!error.message.contains("sk-secret"));
        assert!(!error.message.contains("data.db"));
    }

    #[test]
    fn blocking_executor_failures_keep_public_work_classification() {
        let overloaded = public_blocking_executor_error(BlockingExecutorError::QueueFull);
        assert_eq!(overloaded.code, error::CommandErrorCode::Overloaded);
        assert!(overloaded.retryable);

        let timeout = public_blocking_executor_error(BlockingExecutorError::ExecutionTimeout);
        assert_eq!(timeout.code, error::CommandErrorCode::Timeout);
        assert!(timeout.retryable);

        let cancelled =
            public_blocking_executor_error(BlockingExecutorError::CancelledLateResultDiscarded);
        assert_eq!(cancelled.code, error::CommandErrorCode::Conflict);
        assert!(!cancelled.retryable);

        let internal = public_blocking_executor_error(BlockingExecutorError::Panicked);
        assert_eq!(internal.code, error::CommandErrorCode::Internal);
        assert!(!internal.retryable);
    }

    #[test]
    fn remote_key_failures_keep_public_machine_classification() {
        let unsupported = key_pool::public_remote_key_error(
            crate::services::remote_keys::RemoteKeyOperationError::Unsupported,
        );
        assert_eq!(unsupported.code, error::CommandErrorCode::Unsupported);
        assert!(!unsupported.retryable);

        let external = key_pool::public_remote_key_error(
            crate::services::remote_keys::RemoteKeyOperationError::ExternalUnavailable,
        );
        assert_eq!(external.code, error::CommandErrorCode::ExternalUnavailable);
        assert!(external.retryable);

        let conflict = key_pool::public_remote_key_error(
            crate::services::remote_keys::RemoteKeyOperationError::Conflict,
        );
        assert_eq!(conflict.code, error::CommandErrorCode::Conflict);
        assert!(!conflict.retryable);

        let not_found = key_pool::public_remote_key_error(
            crate::services::remote_keys::RemoteKeyOperationError::Application(
                ApplicationError::NotFound,
            ),
        );
        assert_eq!(not_found.code, error::CommandErrorCode::NotFound);
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn ccswitch_protocol_urls_use_windows_file_protocol_handler() {
        let launcher = system_url_launcher("ccswitch://v1/import?resource=provider");

        assert_eq!(launcher.program, "rundll32.exe");
        assert_eq!(
            launcher.args,
            vec![
                "url.dll,FileProtocolHandler",
                "ccswitch://v1/import?resource=provider"
            ]
        );
    }

    #[test]
    fn ccswitch_deeplink_matches_sub2api_codex_import_shape() {
        let deeplink = ccswitch_import::build_ccswitch_provider_deeplink(
            "codex",
            "Relay Pool Desktop",
            "http://127.0.0.1:8787",
            "http://127.0.0.1:8787/v1",
            "sk test",
        );

        assert!(deeplink.starts_with("ccswitch://v1/import?"));
        assert!(deeplink.contains("resource=provider"));
        assert!(deeplink.contains("app=codex"));
        assert!(deeplink.contains("model=gpt-5.4"));
        assert!(deeplink.contains("name=Relay+Pool+Desktop"));
        assert!(deeplink.contains("homepage=http%3A%2F%2F127.0.0.1%3A8787"));
        assert!(deeplink.contains("endpoint=http%3A%2F%2F127.0.0.1%3A8787%2Fv1"));
        assert!(deeplink.contains("apiKey=sk+test"));
        assert!(deeplink.contains("configFormat=json"));
        assert!(deeplink.contains("usageEnabled=true"));
        assert!(deeplink.contains("usageAutoInterval=30"));
        assert!(deeplink.contains("usageScript="));
    }

    #[test]
    fn ccswitch_import_uses_v2_local_access_key_before_building_deeplink() {
        let status = crate::models::proxy::ProxyStatus {
            running: true,
            lifecycle: crate::models::proxy::ProxyLifecycle::Running,
            bind_addr: "127.0.0.1".to_string(),
            port: 8787,
            started_at: None,
            last_error: None,
            active_requests: 0,
            request_count: 0,
        };

        let local_access_key = "sk-v2-test";
        let (_, deeplink) = ccswitch_import::prepare_ccswitch_import(local_access_key, &status);

        assert!(deeplink.contains(&format!(
            "apiKey={}",
            ccswitch_import::encode_query_param(local_access_key)
        )));
    }

    #[test]
    fn external_url_validation_accepts_http_urls() {
        assert_eq!(
            validate_external_http_url(" https://api.example.test/v1 "),
            Ok("https://api.example.test/v1")
        );
        assert_eq!(
            validate_external_http_url("HTTP://api.example.test"),
            Ok("HTTP://api.example.test")
        );
    }

    #[test]
    fn external_url_validation_rejects_non_http_urls() {
        let error = validate_external_http_url("ccswitch://v1/import?resource=provider")
            .expect_err("custom schemes should not be accepted by the station URL opener");

        assert!(error.contains("HTTP"));
    }
}
