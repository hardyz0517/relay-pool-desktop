use base64::{engine::general_purpose, Engine as _};
use serde_json::Value;
use tauri::State;

use crate::{
    application::command_facades::LocalProxyCommandFacade,
    commands::error,
    ipc::dto::{settings::CcswitchImportResultDto, EmptyInputDto},
    models::proxy::ProxyStatus,
    observability::correlation,
};

#[tauri::command]
pub async fn import_relay_pool_to_ccswitch(
    facade: State<'_, LocalProxyCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<CcswitchImportResultDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "import_relay_pool_to_ccswitch",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            EmptyInputDto::parse(input)?;
            let target = facade
                .import_relay_pool_to_ccswitch()
                .await
                .map_err(super::public_local_proxy_error)?;
            let (result, deeplink) =
                prepare_ccswitch_import(&target.local_access_key, &target.proxy_status);

            super::open_url_with_system(&deeplink)?;

            Ok(result)
        },
    )
    .await
}

pub(crate) fn prepare_ccswitch_import(
    local_access_key: &str,
    status: &ProxyStatus,
) -> (CcswitchImportResultDto, String) {
    let endpoint = format!("http://{}:{}/v1", status.bind_addr, status.port);
    let homepage = format!("http://{}:{}", status.bind_addr, status.port);
    let provider_name = "Relay Pool Desktop".to_string();
    let deeplink = build_ccswitch_provider_deeplink(
        "codex",
        &provider_name,
        &homepage,
        &endpoint,
        local_access_key,
    );
    (
        CcswitchImportResultDto {
            app: "codex".to_string(),
            provider_name,
            endpoint,
        },
        deeplink,
    )
}

pub(crate) fn build_ccswitch_provider_deeplink(
    app: &str,
    provider_name: &str,
    homepage: &str,
    endpoint: &str,
    api_key: &str,
) -> String {
    let usage_script = general_purpose::STANDARD.encode(build_ccswitch_usage_script());
    let mut entries = vec![
        ("resource", "provider".to_string()),
        ("app", app.to_string()),
        ("name", provider_name.to_string()),
        ("homepage", homepage.to_string()),
        ("endpoint", endpoint.to_string()),
        ("apiKey", api_key.to_string()),
        ("configFormat", "json".to_string()),
        ("usageEnabled", "true".to_string()),
        ("usageScript", usage_script),
        ("usageAutoInterval", "30".to_string()),
        ("enabled", "true".to_string()),
    ];
    if app == "codex" {
        entries.insert(2, ("model", "gpt-5.4".to_string()));
    }

    let query = entries
        .into_iter()
        .map(|(key, value)| format!("{}={}", encode_query_param(key), encode_query_param(&value)))
        .collect::<Vec<_>>()
        .join("&");
    format!("ccswitch://v1/import?{query}")
}

fn build_ccswitch_usage_script() -> &'static str {
    r#"({
    request: {
      url: "{{baseUrl}}/usage",
      method: "GET",
      headers: { "Authorization": "Bearer {{apiKey}}" }
    },
    extractor: function(response) {
      const remaining = response?.remaining ?? response?.quota?.remaining ?? response?.balance;
      const unit = response?.unit ?? response?.quota?.unit ?? "USD";
      return {
        isValid: response?.is_active ?? response?.isValid ?? true,
        remaining,
        unit
      };
    }
  })"#
}

pub(crate) fn encode_query_param(value: &str) -> String {
    let mut output = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                output.push(byte as char);
            }
            b' ' => output.push('+'),
            _ => output.push_str(&format!("%{byte:02X}")),
        }
    }
    output
}
