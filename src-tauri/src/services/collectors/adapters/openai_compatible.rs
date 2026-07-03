use serde_json::{json, Value};

use crate::services::{
    collectors::{
        adapters::{AdapterOutput, CollectorTask},
        facts::{CollectedModelFact, CollectorFacts},
        url::{collector_base_urls, join_url},
    },
    database::AppDatabase,
};

fn parse_openai_models(station_id: &str, payload: &Value) -> Vec<CollectedModelFact> {
    payload
        .get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| item.get("id").and_then(Value::as_str))
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(|model| CollectedModelFact {
            station_id: station_id.to_string(),
            model: model.to_string(),
            available: true,
            source: "openai_models".to_string(),
            confidence: 0.9,
        })
        .collect()
}

pub fn collect(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station_id: &str,
    task: CollectorTask,
) -> Result<AdapterOutput, String> {
    match task {
        CollectorTask::Detect | CollectorTask::Models | CollectorTask::Full => {
            collect_models(database, data_key, station_id, task)
        }
        CollectorTask::Balance => manual_required_output(
            task,
            "unsupported_task",
            "OpenAI-compatible 站点不支持余额采集。",
        ),
        CollectorTask::Groups => manual_required_output(
            task,
            "unsupported_task",
            "OpenAI-compatible 站点不支持分组倍率采集。",
        ),
    }
}

pub fn collect_models(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station_id: &str,
    task: CollectorTask,
) -> Result<AdapterOutput, String> {
    let station = database.station_for_collector(station_id)?;
    let keys = database.list_station_keys(station_id.to_string())?;
    let Some(key) = keys
        .into_iter()
        .find(|key| key.enabled && key.api_key_present)
    else {
        return manual_required_output(task, "api_key_required", "模型采集需要可用 API Key。");
    };
    let api_key = match database.resolve_station_key_secret_with_data_key(data_key, &key.id) {
        Ok(api_key) => api_key,
        Err(error) => {
            return manual_required_output(
                task,
                "api_key_required",
                &format!(
                    "API Key 不可解密：{}",
                    crate::services::secrets::mask::redact_text(&error)
                ),
            );
        }
    };

    let urls = collector_base_urls(&station.base_url);
    let url = join_url(&urls.upstream_api_base_url, "/models");
    let started = std::time::Instant::now();
    let response = match ureq::get(&url)
        .set("Authorization", &format!("Bearer {api_key}"))
        .call()
    {
        Ok(response) => response,
        Err(ureq::Error::Status(_, response)) => response,
        Err(error) => {
            return failed_output(
                task,
                json!({
                    "url": url,
                    "status": null,
                    "ok": false,
                    "durationMs": started.elapsed().as_millis() as i64,
                }),
                "network_error",
                &crate::services::secrets::mask::redact_text(&error.to_string()),
                None,
            );
        }
    };

    let status_code = response.status();
    let text = response.into_string().unwrap_or_default();
    let payload = serde_json::from_str::<Value>(&text).unwrap_or(Value::Null);
    let endpoint_result = json!({
        "url": url,
        "status": status_code,
        "ok": (200..400).contains(&status_code),
        "durationMs": started.elapsed().as_millis() as i64,
    });

    if !(200..400).contains(&status_code) {
        return failed_output(
            task,
            endpoint_result,
            "http_error",
            &crate::services::secrets::mask::redact_text(&text),
            Some(crate::services::secrets::mask::redact_value(&payload)),
        );
    }

    let models = parse_openai_models(station_id, &payload);
    let model_names = models
        .iter()
        .map(|model| model.model.clone())
        .collect::<Vec<_>>();
    let mut facts = CollectorFacts::default();
    facts.models = models;

    Ok(AdapterOutput {
        adapter: "openai-compatible".to_string(),
        task,
        status: if model_names.is_empty() {
            "partial"
        } else {
            "success"
        }
        .to_string(),
        facts,
        summary_json: json!({
            "adapter": "openai-compatible",
            "task": task.as_str(),
            "endpointResults": [endpoint_result],
            "modelCount": model_names.len(),
        }),
        normalized_json: json!({ "models": model_names }),
        raw_json_redacted: Some(crate::services::secrets::mask::redact_value(&payload)),
        error_code: None,
        error_message: None,
    })
}

fn manual_required_output(
    task: CollectorTask,
    code: &str,
    message: &str,
) -> Result<AdapterOutput, String> {
    Ok(AdapterOutput {
        adapter: "openai-compatible".to_string(),
        task,
        status: "manual_required".to_string(),
        facts: CollectorFacts::default(),
        summary_json: json!({
            "adapter": "openai-compatible",
            "task": task.as_str(),
            "message": message,
        }),
        normalized_json: json!({ "models": [] }),
        raw_json_redacted: None,
        error_code: Some(code.to_string()),
        error_message: Some(message.to_string()),
    })
}

fn failed_output(
    task: CollectorTask,
    endpoint_result: Value,
    code: &str,
    message: &str,
    raw_json_redacted: Option<Value>,
) -> Result<AdapterOutput, String> {
    Ok(AdapterOutput {
        adapter: "openai-compatible".to_string(),
        task,
        status: "failed".to_string(),
        facts: CollectorFacts::default(),
        summary_json: json!({
            "adapter": "openai-compatible",
            "task": task.as_str(),
            "endpointResults": [endpoint_result],
        }),
        normalized_json: json!({ "models": [] }),
        raw_json_redacted,
        error_code: Some(code.to_string()),
        error_message: Some(message.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn openai_models_parser_reads_data_ids() {
        let models = parse_openai_models(
            "station-1",
            &json!({
                "data": [
                    { "id": "gpt-4o-mini" },
                    { "id": "text-embedding-3-small" },
                    { "id": "" },
                    { "object": "model" }
                ]
            }),
        );

        assert_eq!(models.len(), 2);
        assert!(models.iter().any(|model| model.model == "gpt-4o-mini"));
        assert!(models
            .iter()
            .all(|model| model.station_id == "station-1" && model.available));
    }
}
