use tauri::{AppHandle, Manager};

use crate::services::{database::AppDatabase, secrets::SecretManager};

use super::{runtime::ProxyRuntimeState, startup};

const ENV_START_ON_LAUNCH: &str = "RELAY_POOL_START_PROXY_ON_LAUNCH";
const ENV_DEV_AUTO_START_PROXY: &str = "RELAY_POOL_DEV_AUTO_START_PROXY";

fn enabled(value: Option<&str>) -> bool {
    value.map(str::trim) == Some("1")
}

fn env_start_requested() -> bool {
    enabled(std::env::var(ENV_START_ON_LAUNCH).ok().as_deref())
        || enabled(std::env::var(ENV_DEV_AUTO_START_PROXY).ok().as_deref())
}

pub fn schedule(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        if let Err(error) = start_managed_if_requested(&app).await {
            eprintln!("Relay Pool proxy start-on-launch failed: {error}");
        }
    });
}

async fn start_managed_if_requested(app: &AppHandle) -> Result<(), String> {
    let Some(database) = app.try_state::<AppDatabase>() else {
        return Ok(());
    };
    if !database.local_proxy_start_on_launch_enabled()? && !env_start_requested() {
        return Ok(());
    }

    let secrets = app.state::<SecretManager>();
    let proxy = app.state::<ProxyRuntimeState>();
    startup::start_from_persisted_settings(database.inner(), *secrets.data_key(), proxy.inner())
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_start_env_requires_exact_normalized_one() {
        assert!(enabled(Some("1")));
        assert!(enabled(Some(" 1 ")));
        assert!(!enabled(None));
        assert!(!enabled(Some("")));
        assert!(!enabled(Some("true")));
        assert!(!enabled(Some("01")));
    }
}
