use tauri::{AppHandle, Manager};

use crate::services::{database::AppDatabase, secrets::SecretManager};

use super::{runtime::ProxyRuntimeState, startup};

const ENV_NAME: &str = "RELAY_POOL_DEV_AUTO_START_PROXY";

fn enabled(value: Option<&str>) -> bool {
    value.map(str::trim) == Some("1")
}

pub fn schedule(app: AppHandle) {
    if !enabled(std::env::var(ENV_NAME).ok().as_deref()) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        if let Err(error) = start_managed(&app).await {
            eprintln!("Relay Pool debug proxy auto-start failed: {error}");
        }
    });
}

async fn start_managed(app: &AppHandle) -> Result<(), String> {
    let database = app
        .try_state::<AppDatabase>()
        .ok_or_else(|| "data store is unavailable".to_string())?;
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
    fn auto_start_requires_exact_normalized_one() {
        assert!(enabled(Some("1")));
        assert!(enabled(Some(" 1 ")));
        assert!(!enabled(None));
        assert!(!enabled(Some("")));
        assert!(!enabled(Some("true")));
        assert!(!enabled(Some("01")));
    }
}
