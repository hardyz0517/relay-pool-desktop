use serde_json::json;

use crate::models::change_events::UpsertChangeEventInput;

pub const SEVERITY_CRITICAL: &str = "critical";
pub const SEVERITY_WARNING: &str = "warning";
pub const SEVERITY_INFO: &str = "info";

pub const STATUS_UNREAD: &str = "unread";
pub const STATUS_READ: &str = "read";
pub const STATUS_DISMISSED: &str = "dismissed";
pub const STATUS_RESOLVED: &str = "resolved";

pub fn balance_dedupe_key(station_id: &str, status: &str) -> String {
    format!("balance:{status}:station:{station_id}")
}

pub fn station_key_dedupe_key(station_key_id: &str, event_type: &str) -> String {
    format!("{event_type}:station_key:{station_key_id}")
}

pub fn collector_dedupe_key(station_id: &str, event_type: &str) -> String {
    format!("{event_type}:collector:{station_id}")
}

pub fn pricing_dedupe_key(station_id: &str, group_name: Option<&str>, model: &str) -> String {
    format!(
        "price_changed:station:{station_id}:group:{}:model:{model}",
        group_name.unwrap_or("-")
    )
}

pub fn rate_dedupe_key(station_id: &str, group_name: &str) -> String {
    format!("rate_changed:station:{station_id}:group:{group_name}")
}

pub fn model_dedupe_key(station_id: &str, event_type: &str, model: &str) -> String {
    format!("{event_type}:station:{station_id}:model:{model}")
}

pub fn station_balance_event(
    station_id: &str,
    status: &str,
    value: Option<f64>,
    threshold: Option<f64>,
) -> Option<UpsertChangeEventInput> {
    match status {
        "depleted" => Some(UpsertChangeEventInput {
            severity: SEVERITY_CRITICAL.to_string(),
            event_type: "balance_depleted".to_string(),
            title: "余额耗尽".to_string(),
            message: format!(
                "站点余额已耗尽，当前余额 {}",
                value
                    .map(|item| item.to_string())
                    .unwrap_or_else(|| "未知".to_string())
            ),
            object_type: "station".to_string(),
            object_id: Some(station_id.to_string()),
            station_id: Some(station_id.to_string()),
            station_key_id: None,
            pricing_rule_id: None,
            request_log_id: None,
            old_value_json: None,
            new_value_json: Some(json!({ "value": value, "threshold": threshold }).to_string()),
            impact_json: Some(json!({ "routingRisk": "deprioritize_or_block" }).to_string()),
            dedupe_key: balance_dedupe_key(station_id, "depleted"),
            source: "balance".to_string(),
        }),
        "low" => Some(UpsertChangeEventInput {
            severity: SEVERITY_WARNING.to_string(),
            event_type: "balance_low".to_string(),
            title: "余额偏低".to_string(),
            message: format!(
                "站点余额低于阈值，当前余额 {}",
                value
                    .map(|item| item.to_string())
                    .unwrap_or_else(|| "未知".to_string())
            ),
            object_type: "station".to_string(),
            object_id: Some(station_id.to_string()),
            station_id: Some(station_id.to_string()),
            station_key_id: None,
            pricing_rule_id: None,
            request_log_id: None,
            old_value_json: None,
            new_value_json: Some(json!({ "value": value, "threshold": threshold }).to_string()),
            impact_json: Some(json!({ "routingRisk": "deprioritize" }).to_string()),
            dedupe_key: balance_dedupe_key(station_id, "low"),
            source: "balance".to_string(),
        }),
        _ => None,
    }
}

pub fn key_health_event(
    station_key_id: &str,
    station_id: &str,
    consecutive_failures: i64,
    last_error: Option<&str>,
    cooldown_until: Option<&str>,
) -> Option<UpsertChangeEventInput> {
    if consecutive_failures <= 0 && cooldown_until.is_none() {
        return None;
    }

    Some(UpsertChangeEventInput {
        severity: if consecutive_failures >= 3 {
            SEVERITY_CRITICAL
        } else {
            SEVERITY_WARNING
        }
        .to_string(),
        event_type: "key_invalid".to_string(),
        title: "Key 健康异常".to_string(),
        message: format!(
            "Key 连续失败 {consecutive_failures} 次{}",
            last_error
                .map(|value| format!("：{value}"))
                .unwrap_or_default()
        ),
        object_type: "station_key".to_string(),
        object_id: Some(station_key_id.to_string()),
        station_id: Some(station_id.to_string()),
        station_key_id: Some(station_key_id.to_string()),
        pricing_rule_id: None,
        request_log_id: None,
        old_value_json: None,
        new_value_json: Some(
            json!({
                "consecutiveFailures": consecutive_failures,
                "cooldownUntil": cooldown_until
            })
            .to_string(),
        ),
        impact_json: Some(json!({ "routingRisk": "candidate_filtered_or_deprioritized" }).to_string()),
        dedupe_key: station_key_dedupe_key(station_key_id, "key_invalid"),
        source: "health".to_string(),
    })
}

pub fn collector_failed_event(
    station_id: &str,
    error_message: Option<&str>,
) -> UpsertChangeEventInput {
    UpsertChangeEventInput {
        severity: SEVERITY_WARNING.to_string(),
        event_type: "collector_failed".to_string(),
        title: "站点采集失败".to_string(),
        message: error_message
            .unwrap_or("采集失败，未返回详细错误")
            .to_string(),
        object_type: "station".to_string(),
        object_id: Some(station_id.to_string()),
        station_id: Some(station_id.to_string()),
        station_key_id: None,
        pricing_rule_id: None,
        request_log_id: None,
        old_value_json: None,
        new_value_json: None,
        impact_json: Some(json!({ "staleDataRisk": true }).to_string()),
        dedupe_key: collector_dedupe_key(station_id, "collector_failed"),
        source: "collector".to_string(),
    }
}

pub fn price_changed_event(
    station_id: &str,
    pricing_rule_id: &str,
    model: &str,
    group_name: Option<&str>,
    old_output_price: Option<f64>,
    new_output_price: Option<f64>,
    currency: &str,
) -> Option<UpsertChangeEventInput> {
    if old_output_price == new_output_price {
        return None;
    }
    let increased = match (old_output_price, new_output_price) {
        (Some(old), Some(new)) => new > old,
        _ => false,
    };
    Some(UpsertChangeEventInput {
        severity: if increased {
            SEVERITY_WARNING
        } else {
            SEVERITY_INFO
        }
        .to_string(),
        event_type: "price_changed".to_string(),
        title: if increased { "价格变贵" } else { "价格变化" }.to_string(),
        message: format!("模型 {model} 输出价格发生变化"),
        object_type: "pricing_rule".to_string(),
        object_id: Some(pricing_rule_id.to_string()),
        station_id: Some(station_id.to_string()),
        station_key_id: None,
        pricing_rule_id: Some(pricing_rule_id.to_string()),
        request_log_id: None,
        old_value_json: Some(json!({ "outputPrice": old_output_price, "currency": currency }).to_string()),
        new_value_json: Some(json!({ "outputPrice": new_output_price, "currency": currency }).to_string()),
        impact_json: Some(json!({ "cheapFirstMayChange": true }).to_string()),
        dedupe_key: pricing_dedupe_key(station_id, group_name, model),
        source: "pricing".to_string(),
    })
}

pub fn rate_changed_event(
    station_id: &str,
    group_name: &str,
    old_multiplier: f64,
    new_multiplier: f64,
) -> Option<UpsertChangeEventInput> {
    if (old_multiplier - new_multiplier).abs() < f64::EPSILON {
        return None;
    }
    let increased = new_multiplier > old_multiplier;
    Some(UpsertChangeEventInput {
        severity: if increased {
            SEVERITY_WARNING
        } else {
            SEVERITY_INFO
        }
        .to_string(),
        event_type: "rate_changed".to_string(),
        title: if increased { "倍率上涨" } else { "倍率下降" }.to_string(),
        message: format!("分组 {group_name} 倍率发生变化"),
        object_type: "station".to_string(),
        object_id: Some(station_id.to_string()),
        station_id: Some(station_id.to_string()),
        station_key_id: None,
        pricing_rule_id: None,
        request_log_id: None,
        old_value_json: Some(json!({ "groupName": group_name, "multiplier": old_multiplier }).to_string()),
        new_value_json: Some(json!({ "groupName": group_name, "multiplier": new_multiplier }).to_string()),
        impact_json: Some(json!({ "cheapFirstMayChange": true }).to_string()),
        dedupe_key: rate_dedupe_key(station_id, group_name),
        source: "collector".to_string(),
    })
}
