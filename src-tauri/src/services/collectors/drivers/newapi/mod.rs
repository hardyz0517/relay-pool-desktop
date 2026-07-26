pub(crate) mod parsers;

use futures_util::future::{BoxFuture, FutureExt};
use http::{header, HeaderName, HeaderValue, Method, StatusCode};
use serde_json::{json, Value};

use crate::{
    outbound::{
        OutboundFailureKind, OutboundHeaderPolicy, OutboundHeaders, OutboundRequest,
        SecretHeaderValue,
    },
    services::{
        collectors::{
            contract::{
                CollectorContext, CollectorDriver, CollectorTaskKind, CredentialSecretPurpose,
                DriverOutput, DriverOutputStatus, ProviderAuthContext, ProviderKind,
                RedactedDiagnostics,
            },
            evidence::{redact_text, redact_value, EndpointEvidence, EndpointRole, EvidenceSet},
            facts::CollectorFacts,
            failure::{
                AuthEffect, DriverFailure, DriverFailureKind, FailedEndpoint, RetryDisposition,
            },
        },
        station_endpoints::build_management_url,
    },
};

const NEW_API_USER_HEADER: HeaderName = HeaderName::from_static("new-api-user");
const NEWAPI_LOG_PAGE_SIZE: usize = 100;
const NEWAPI_LOG_MAX_PAGES: usize = 100;
const NEWAPI_LOG_TYPE_CONSUME: i64 = 2;
const NEWAPI_DASHBOARD_MAX_WINDOW_SECONDS: i64 = 30 * 24 * 60 * 60;
const NEWAPI_DASHBOARD_TOTAL_START_TIMESTAMP: i64 = 0;
const NEWAPI_DASHBOARD_TOTAL_MAX_WINDOWS: usize = 240;

pub const SUPPORTED_COLLECTOR_TASKS: &[CollectorTaskKind] = &[
    CollectorTaskKind::Detect,
    CollectorTaskKind::Balance,
    CollectorTaskKind::Groups,
    CollectorTaskKind::Models,
];

pub struct NewApiCollectorDriver;

impl CollectorDriver for NewApiCollectorDriver {
    fn kind(&self) -> ProviderKind {
        ProviderKind::NewApi
    }

    fn collect<'a>(
        &'a self,
        context: &'a CollectorContext<'a>,
        task: CollectorTaskKind,
    ) -> BoxFuture<'a, Result<DriverOutput, DriverFailure>> {
        async move {
            match task {
                CollectorTaskKind::Detect => Ok(detect_output()),
                CollectorTaskKind::Balance => collect_balance(context).await,
                CollectorTaskKind::Groups => collect_groups(context).await,
                CollectorTaskKind::Models => collect_models(context).await,
                CollectorTaskKind::Full => Err(DriverFailure::unsupported(
                    "NewAPI full collection is split by the collector parent task",
                )),
            }
        }
        .boxed()
    }
}

fn detect_output() -> DriverOutput {
    DriverOutput {
        facts: CollectorFacts::default(),
        evidence: Vec::new(),
        status: DriverOutputStatus::Success,
        diagnostics: RedactedDiagnostics {
            summary: Some(json!({"adapter": "newapi", "task": "detect"}).to_string()),
            raw_json_redacted: None,
        },
    }
}

async fn collect_balance(context: &CollectorContext<'_>) -> Result<DriverOutput, DriverFailure> {
    let website_url = website_url(context)?;
    let (status_payload, status_endpoint) = execute_json(
        context,
        EndpointRole::Website,
        &website_url,
        "/api/status",
        false,
    )
    .await?;
    let status_data = parsers::envelope_data(&status_payload).map_err(|error| {
        malformed(
            EndpointRole::Website,
            Some(status_endpoint.clone()),
            error.message,
        )
    })?;
    let status = parsers::parse_status(status_data);
    let (self_payload, self_endpoint) = execute_json(
        context,
        EndpointRole::Balance,
        &website_url,
        "/api/user/self",
        true,
    )
    .await?;
    let self_data = parsers::envelope_data(&self_payload).map_err(|error| {
        malformed(
            EndpointRole::Balance,
            Some(self_endpoint.clone()),
            error.message,
        )
    })?;
    let (usage_stats, mut usage_evidence) =
        collect_usage_stats(context, &website_url, self_data, status.quota_per_unit).await;
    let mut balance_data = self_data.clone();
    merge_optional_usage_stats_into_balance_data(&mut balance_data, usage_stats);
    let facts = CollectorFacts {
        balances: vec![parsers::parse_balance_fact(
            &context.station.station_id,
            &balance_data,
            status.quota_per_unit,
        )],
        ..CollectorFacts::default()
    };
    Ok(DriverOutput {
        facts,
        evidence: vec![status_endpoint],
        status: DriverOutputStatus::Success,
        diagnostics: RedactedDiagnostics {
            summary: Some(
                json!({
                    "quotaPerUnit": status.quota_per_unit,
                    "quotaPerUnitAvailable": status.quota_per_unit.is_some(),
                })
                .to_string(),
            ),
            raw_json_redacted: Some(redact_value(&json!({
                "status": status_payload,
                "self": balance_data,
            }))),
        },
    })
    .map(|mut output| {
        output.evidence.push(self_endpoint);
        output.evidence.append(&mut usage_evidence);
        output
    })
}

async fn collect_groups(context: &CollectorContext<'_>) -> Result<DriverOutput, DriverFailure> {
    let website_url = website_url(context)?;
    let (payload, endpoint) = execute_json(
        context,
        EndpointRole::Groups,
        &website_url,
        "/api/user/self/groups",
        true,
    )
    .await?;
    let data = parsers::envelope_data(&payload)
        .map_err(|error| malformed(EndpointRole::Groups, Some(endpoint.clone()), error.message))?;
    let facts = parsers::parse_group_facts(&context.station.station_id, data);
    let group_count = facts.groups.len();
    let rate_count = facts.rates.len();
    Ok(DriverOutput {
        facts,
        evidence: vec![endpoint],
        status: if group_count == 0 {
            DriverOutputStatus::Partial
        } else {
            DriverOutputStatus::Success
        },
        diagnostics: RedactedDiagnostics {
            summary: Some(json!({"groupCount": group_count, "rateCount": rate_count}).to_string()),
            raw_json_redacted: Some(redact_value(&payload)),
        },
    })
}

async fn collect_models(context: &CollectorContext<'_>) -> Result<DriverOutput, DriverFailure> {
    let website_url = website_url(context)?;
    let (payload, endpoint) = execute_json(
        context,
        EndpointRole::Models,
        &website_url,
        "/api/user/models",
        true,
    )
    .await?;
    let data = parsers::envelope_data(&payload)
        .map_err(|error| malformed(EndpointRole::Models, Some(endpoint.clone()), error.message))?;
    let models = parsers::parse_models(&context.station.station_id, data);
    let model_names = models
        .iter()
        .map(|model| model.model.clone())
        .collect::<Vec<_>>();
    Ok(DriverOutput {
        facts: CollectorFacts {
            models,
            ..CollectorFacts::default()
        },
        evidence: vec![endpoint],
        status: if model_names.is_empty() {
            DriverOutputStatus::Partial
        } else {
            DriverOutputStatus::Success
        },
        diagnostics: RedactedDiagnostics {
            summary: Some(
                json!({"modelCount": model_names.len(), "models": model_names}).to_string(),
            ),
            raw_json_redacted: Some(redact_value(&payload)),
        },
    })
}

#[derive(Debug, Clone, Default)]
struct NewApiUsageStats {
    today_request_count: Option<i64>,
    today_consumption: Option<f64>,
    today_base_consumption: Option<f64>,
    total_base_consumption: Option<f64>,
    today_token_count: Option<i64>,
    total_token_count: Option<i64>,
    today_input_token_count: Option<i64>,
    today_output_token_count: Option<i64>,
    total_input_token_count: Option<i64>,
    total_output_token_count: Option<i64>,
}

#[derive(Debug, Clone, Default)]
struct NewApiLogStatWindow {
    consumption: Option<f64>,
    base_consumption: Option<f64>,
}

#[derive(Debug, Clone, Default)]
struct NewApiLogWindow {
    request_count: Option<i64>,
    input_token_count: Option<i64>,
    output_token_count: Option<i64>,
}

#[derive(Debug, Clone, Default)]
struct NewApiDashboardUsageWindow {
    request_count: Option<i64>,
    token_count: Option<i64>,
    quota: Option<i64>,
    consumption: Option<f64>,
}

#[derive(Debug, Clone, Copy)]
struct NewApiDashboardTotalTarget {
    request_count: i64,
    quota: i64,
}

impl NewApiDashboardUsageWindow {
    fn add(&mut self, other: NewApiDashboardUsageWindow) {
        if !self.has_any() {
            self.request_count = other.request_count;
            self.token_count = other.token_count;
            self.quota = other.quota;
            self.consumption = other.consumption;
            return;
        }
        self.request_count = checked_sum_i64(self.request_count, other.request_count);
        self.token_count = checked_sum_i64(self.token_count, other.token_count);
        self.quota = checked_sum_i64(self.quota, other.quota);
        self.consumption = checked_sum_f64(self.consumption, other.consumption);
    }

    fn has_any(&self) -> bool {
        self.request_count.is_some()
            || self.token_count.is_some()
            || self.quota.is_some()
            || self.consumption.is_some()
    }
}

impl NewApiUsageStats {
    fn has_any(&self) -> bool {
        self.today_request_count.is_some()
            || self.today_consumption.is_some()
            || self.today_base_consumption.is_some()
            || self.total_base_consumption.is_some()
            || self.today_token_count.is_some()
            || self.total_token_count.is_some()
            || self.today_input_token_count.is_some()
            || self.today_output_token_count.is_some()
            || self.total_input_token_count.is_some()
            || self.total_output_token_count.is_some()
    }
}

async fn collect_usage_stats(
    context: &CollectorContext<'_>,
    website_url: &str,
    self_data: &Value,
    quota_per_unit: Option<f64>,
) -> (Option<NewApiUsageStats>, Vec<EndpointEvidence>) {
    let now = unix_now_seconds();
    let today_start = local_today_start_timestamp(now);
    let mut endpoint_results = Vec::new();

    let today_dashboard =
        collect_dashboard_usage_window(context, website_url, today_start, now, quota_per_unit)
            .await
            .map_endpoint_results(&mut endpoint_results);
    let total_dashboard =
        collect_dashboard_usage_total(context, website_url, self_data, now, quota_per_unit)
            .await
            .map_endpoint_results(&mut endpoint_results);
    let today_stat =
        collect_log_stat_window(context, website_url, today_start, now, quota_per_unit)
            .await
            .map_endpoint_results(&mut endpoint_results);
    let today_logs = collect_log_window(context, website_url, today_start, now)
        .await
        .map_endpoint_results(&mut endpoint_results);
    let total_stat = collect_log_stat_window(context, website_url, 0, now, quota_per_unit)
        .await
        .map_endpoint_results(&mut endpoint_results);
    let total_logs = collect_log_window(context, website_url, 0, now)
        .await
        .map_endpoint_results(&mut endpoint_results);

    let today_split_token_count = today_logs
        .as_ref()
        .and_then(|logs| logs.input_token_count.zip(logs.output_token_count))
        .map(|(input, output)| input + output);
    let self_request_count = numeric_i64_field(self_data, &["request_count"]);
    let total_split_token_count = total_logs
        .as_ref()
        .filter(|logs| logs.request_count.is_some() && logs.request_count == self_request_count)
        .and_then(|logs| logs.input_token_count.zip(logs.output_token_count))
        .map(|(input, output)| input + output);

    let stats = NewApiUsageStats {
        today_request_count: today_dashboard
            .as_ref()
            .and_then(|dashboard| dashboard.request_count)
            .or_else(|| today_logs.as_ref().and_then(|logs| logs.request_count)),
        today_consumption: today_dashboard
            .as_ref()
            .and_then(|dashboard| dashboard.consumption)
            .or_else(|| today_stat.as_ref().and_then(|stat| stat.consumption)),
        today_base_consumption: today_stat.as_ref().and_then(|stat| stat.base_consumption),
        total_base_consumption: total_stat.as_ref().and_then(|stat| stat.base_consumption),
        today_token_count: today_dashboard
            .as_ref()
            .and_then(|dashboard| dashboard.token_count)
            .or(today_split_token_count),
        total_token_count: total_dashboard
            .as_ref()
            .and_then(|dashboard| dashboard.token_count)
            .or(total_split_token_count),
        today_input_token_count: None,
        today_output_token_count: None,
        total_input_token_count: None,
        total_output_token_count: None,
    };

    (stats.has_any().then_some(stats), endpoint_results)
}

trait UsageCollectionResultExt<T> {
    fn map_endpoint_results(self, endpoint_results: &mut Vec<EndpointEvidence>) -> Option<T>;
}

impl<T> UsageCollectionResultExt<T> for Result<(T, Vec<EndpointEvidence>), DriverFailure> {
    fn map_endpoint_results(self, endpoint_results: &mut Vec<EndpointEvidence>) -> Option<T> {
        match self {
            Ok((value, mut results)) => {
                endpoint_results.append(&mut results);
                Some(value)
            }
            Err(_) => None,
        }
    }
}

async fn collect_log_stat_window(
    context: &CollectorContext<'_>,
    website_url: &str,
    start_timestamp: i64,
    end_timestamp: i64,
    quota_per_unit: Option<f64>,
) -> Result<(NewApiLogStatWindow, Vec<EndpointEvidence>), DriverFailure> {
    let path = newapi_log_stat_path(start_timestamp, end_timestamp);
    let (data, endpoint) =
        execute_newapi_data(context, EndpointRole::Balance, website_url, &path).await?;
    let consumption = quota_per_unit
        .zip(numeric_f64_field(&data, &["quota"]))
        .map(|(quota_per_unit, quota)| quota / quota_per_unit);
    Ok((
        NewApiLogStatWindow {
            consumption,
            base_consumption: None,
        },
        vec![endpoint],
    ))
}

async fn collect_log_window(
    context: &CollectorContext<'_>,
    website_url: &str,
    start_timestamp: i64,
    end_timestamp: i64,
) -> Result<(NewApiLogWindow, Vec<EndpointEvidence>), DriverFailure> {
    let mut page = 1_usize;
    let mut total = None;
    let mut fetched = 0_usize;
    let mut input_tokens = 0_i64;
    let mut output_tokens = 0_i64;
    let mut saw_token_count = false;
    let mut saw_incomplete_token_fields = false;
    let mut endpoint_results = Vec::new();
    let mut completed_window = false;

    loop {
        let path = newapi_log_page_path(page, start_timestamp, end_timestamp);
        let (data, endpoint) =
            execute_newapi_data(context, EndpointRole::Balance, website_url, &path).await?;
        endpoint_results.push(endpoint);
        let response_page = numeric_usize_field(&data, &["page"])
            .filter(|value| *value == page)
            .ok_or_else(|| {
                malformed(
                    EndpointRole::Balance,
                    None,
                    "NewAPI log pagination is missing a valid page number",
                )
            })?;
        let page_size = numeric_usize_field(&data, &["page_size"])
            .filter(|value| *value > 0)
            .ok_or_else(|| {
                malformed(
                    EndpointRole::Balance,
                    None,
                    "NewAPI log pagination is missing page_size",
                )
            })?;
        let response_total = numeric_usize_field(&data, &["total"]).ok_or_else(|| {
            malformed(
                EndpointRole::Balance,
                None,
                "NewAPI log pagination is missing total",
            )
        })?;
        if total.is_some_and(|expected| expected != response_total) {
            return Err(malformed(
                EndpointRole::Balance,
                None,
                "NewAPI log pagination total changed between pages",
            ));
        }
        total = Some(response_total);
        let items = data
            .get("items")
            .and_then(Value::as_array)
            .cloned()
            .ok_or_else(|| {
                malformed(
                    EndpointRole::Balance,
                    None,
                    "NewAPI log pagination is missing items",
                )
            })?;
        if items.len() > page_size {
            return Err(malformed(
                EndpointRole::Balance,
                None,
                "NewAPI log pagination returned more items than page_size",
            ));
        }
        for item in &items {
            let prompt_tokens = numeric_i64_field(item, &["prompt_tokens"]);
            let completion_tokens = numeric_i64_field(item, &["completion_tokens"]);
            match (
                prompt_tokens.filter(|value| *value >= 0),
                completion_tokens.filter(|value| *value >= 0),
            ) {
                (Some(prompt_tokens), Some(completion_tokens)) => {
                    if let (Some(next_input), Some(next_output)) = (
                        input_tokens.checked_add(prompt_tokens),
                        output_tokens.checked_add(completion_tokens),
                    ) {
                        saw_token_count = true;
                        input_tokens = next_input;
                        output_tokens = next_output;
                    } else {
                        saw_incomplete_token_fields = true;
                    }
                }
                _ => saw_incomplete_token_fields = true,
            }
        }

        fetched = fetched.checked_add(items.len()).ok_or_else(|| {
            malformed(
                EndpointRole::Balance,
                None,
                "NewAPI log pagination count overflowed",
            )
        })?;
        if response_total >= NEWAPI_LOG_PAGE_SIZE * NEWAPI_LOG_MAX_PAGES {
            break;
        }
        if fetched > response_total {
            return Err(malformed(
                EndpointRole::Balance,
                None,
                "NewAPI log pagination returned more items than total",
            ));
        }
        if fetched == response_total {
            completed_window = true;
            break;
        }
        if items.len() < page_size {
            return Err(malformed(
                EndpointRole::Balance,
                None,
                "NewAPI log pagination ended before reaching total",
            ));
        }
        if page >= NEWAPI_LOG_MAX_PAGES {
            break;
        }
        page = response_page.saturating_add(1);
    }

    Ok((
        NewApiLogWindow {
            request_count: completed_window
                .then(|| total.and_then(|value| i64::try_from(value).ok()))
                .flatten(),
            input_token_count: (saw_token_count
                && !saw_incomplete_token_fields
                && completed_window)
                .then_some(input_tokens),
            output_token_count: (saw_token_count
                && !saw_incomplete_token_fields
                && completed_window)
                .then_some(output_tokens),
        },
        endpoint_results,
    ))
}

async fn collect_dashboard_usage_window(
    context: &CollectorContext<'_>,
    website_url: &str,
    start_timestamp: i64,
    end_timestamp: i64,
    quota_per_unit: Option<f64>,
) -> Result<(NewApiDashboardUsageWindow, Vec<EndpointEvidence>), DriverFailure> {
    let path = newapi_dashboard_data_path(start_timestamp, end_timestamp);
    let (data, endpoint) =
        execute_newapi_data(context, EndpointRole::Balance, website_url, &path).await?;

    let mut request_count = 0_i64;
    let mut token_count = 0_i64;
    let mut quota = 0_i64;
    let mut saw_request_count = false;
    let mut saw_token_count = false;
    let mut saw_quota = false;
    let mut request_count_complete = true;
    let mut token_count_complete = true;
    let mut quota_complete = true;

    for item in dashboard_usage_items(&data) {
        match numeric_i64_field(item, &["count"]).filter(|value| *value >= 0) {
            Some(value) => match request_count.checked_add(value) {
                Some(next) => {
                    request_count = next;
                    saw_request_count = true;
                }
                None => request_count_complete = false,
            },
            None => request_count_complete = false,
        }
        match numeric_i64_field(item, &["token_used"]).filter(|value| *value >= 0) {
            Some(value) => match token_count.checked_add(value) {
                Some(next) => {
                    token_count = next;
                    saw_token_count = true;
                }
                None => token_count_complete = false,
            },
            None => token_count_complete = false,
        }
        match numeric_i64_field(item, &["quota"]) {
            Some(value) => match quota.checked_add(value) {
                Some(next) => {
                    quota = next;
                    saw_quota = true;
                }
                None => quota_complete = false,
            },
            None => quota_complete = false,
        }
    }
    let request_count = (saw_request_count && request_count_complete).then_some(request_count);
    let token_count = (saw_token_count && token_count_complete).then_some(token_count);
    let quota = (saw_quota && quota_complete).then_some(quota);

    Ok((
        NewApiDashboardUsageWindow {
            request_count,
            token_count,
            quota,
            consumption: quota_per_unit
                .zip(quota)
                .map(|(quota_per_unit, quota)| quota as f64 / quota_per_unit),
        },
        vec![endpoint],
    ))
}

async fn collect_dashboard_usage_total(
    context: &CollectorContext<'_>,
    website_url: &str,
    self_data: &Value,
    now: i64,
    quota_per_unit: Option<f64>,
) -> Result<(NewApiDashboardUsageWindow, Vec<EndpointEvidence>), DriverFailure> {
    let target = dashboard_total_target(self_data);
    collect_dashboard_usage_total_backwards(context, website_url, now, quota_per_unit, target).await
}

async fn collect_dashboard_usage_total_backwards(
    context: &CollectorContext<'_>,
    website_url: &str,
    now: i64,
    quota_per_unit: Option<f64>,
    target: Option<NewApiDashboardTotalTarget>,
) -> Result<(NewApiDashboardUsageWindow, Vec<EndpointEvidence>), DriverFailure> {
    let Some(target) = target else {
        return Err(malformed(
            EndpointRole::Balance,
            None,
            "NewAPI dashboard total requires used_quota and request_count",
        ));
    };

    let mut end_timestamp = now;
    let mut total = NewApiDashboardUsageWindow::default();
    let mut endpoint_results = Vec::new();
    let mut collected_any = false;

    for _ in 0..NEWAPI_DASHBOARD_TOTAL_MAX_WINDOWS {
        let start_timestamp = end_timestamp
            .saturating_sub(NEWAPI_DASHBOARD_MAX_WINDOW_SECONDS - 1)
            .max(NEWAPI_DASHBOARD_TOTAL_START_TIMESTAMP);
        let (window, mut results) = collect_dashboard_usage_window(
            context,
            website_url,
            start_timestamp,
            end_timestamp,
            quota_per_unit,
        )
        .await?;
        let window_has_any = window.has_any();
        if window_has_any {
            collected_any = true;
            total.add(window);
        } else if target.request_count == 0 && target.quota == 0 {
            return Err(malformed(
                EndpointRole::Balance,
                None,
                "NewAPI dashboard data response did not contain usage facts",
            ));
        }
        endpoint_results.append(&mut results);
        if dashboard_total_matches_target(&total, target) {
            return Ok((total, endpoint_results));
        }
        if start_timestamp <= NEWAPI_DASHBOARD_TOTAL_START_TIMESTAMP {
            break;
        }
        end_timestamp = start_timestamp.saturating_sub(1);
    }

    Err(malformed(
        EndpointRole::Balance,
        None,
        if collected_any {
            "NewAPI dashboard total response did not cover all-time usage"
        } else {
            "NewAPI dashboard data response did not contain usage facts"
        },
    ))
}

fn dashboard_total_matches_target(
    total: &NewApiDashboardUsageWindow,
    target: NewApiDashboardTotalTarget,
) -> bool {
    total.quota == Some(target.quota) && total.request_count == Some(target.request_count)
}

fn dashboard_total_target(self_data: &Value) -> Option<NewApiDashboardTotalTarget> {
    Some(NewApiDashboardTotalTarget {
        request_count: numeric_i64_field(self_data, &["request_count"])
            .filter(|value| *value >= 0)?,
        quota: numeric_i64_field(self_data, &["used_quota"]).filter(|value| *value >= 0)?,
    })
}

fn dashboard_usage_items(payload: &Value) -> Vec<&Value> {
    payload
        .as_array()
        .map(|items| items.iter().collect())
        .unwrap_or_default()
}

fn merge_usage_stats_into_balance_data(data: &mut Value, stats: NewApiUsageStats) {
    let Some(object) = data.as_object_mut() else {
        return;
    };
    for key in [
        "today_request_count",
        "today_consumption",
        "today_base_consumption",
        "total_base_consumption",
        "today_token_count",
        "total_token_count",
        "today_input_token_count",
        "today_output_token_count",
        "total_input_token_count",
        "total_output_token_count",
    ] {
        object.remove(key);
    }
    insert_i64(object, "today_request_count", stats.today_request_count);
    insert_f64(object, "today_consumption", stats.today_consumption);
    insert_f64(
        object,
        "today_base_consumption",
        stats.today_base_consumption,
    );
    insert_f64(
        object,
        "total_base_consumption",
        stats.total_base_consumption,
    );
    insert_i64(object, "today_token_count", stats.today_token_count);
    insert_i64(object, "total_token_count", stats.total_token_count);
    insert_i64(
        object,
        "today_input_token_count",
        stats.today_input_token_count,
    );
    insert_i64(
        object,
        "today_output_token_count",
        stats.today_output_token_count,
    );
    insert_i64(
        object,
        "total_input_token_count",
        stats.total_input_token_count,
    );
    insert_i64(
        object,
        "total_output_token_count",
        stats.total_output_token_count,
    );
}

fn merge_optional_usage_stats_into_balance_data(data: &mut Value, stats: Option<NewApiUsageStats>) {
    merge_usage_stats_into_balance_data(data, stats.unwrap_or_default());
}

fn insert_i64(object: &mut serde_json::Map<String, Value>, key: &str, value: Option<i64>) {
    if let Some(value) = value {
        object.insert(key.to_string(), json!(value));
    }
}

fn insert_f64(object: &mut serde_json::Map<String, Value>, key: &str, value: Option<f64>) {
    if let Some(value) = value {
        object.insert(key.to_string(), json!(value));
    }
}

async fn execute_newapi_data(
    context: &CollectorContext<'_>,
    role: EndpointRole,
    website_url: &str,
    path: &str,
) -> Result<(Value, EndpointEvidence), DriverFailure> {
    let (payload, endpoint) = execute_json(context, role, website_url, path, true).await?;
    let data = parsers::envelope_data(&payload)
        .map_err(|error| malformed(role, Some(endpoint.clone()), error.message))?;
    Ok((data.clone(), endpoint))
}

fn newapi_log_stat_path(start_timestamp: i64, end_timestamp: i64) -> String {
    format!(
        "/api/log/self/stat?type={NEWAPI_LOG_TYPE_CONSUME}&token_name=&model_name=&start_timestamp={start_timestamp}&end_timestamp={end_timestamp}&group="
    )
}

fn newapi_log_page_path(page: usize, start_timestamp: i64, end_timestamp: i64) -> String {
    format!(
        "/api/log/self?p={page}&page_size={NEWAPI_LOG_PAGE_SIZE}&type={NEWAPI_LOG_TYPE_CONSUME}&token_name=&model_name=&start_timestamp={start_timestamp}&end_timestamp={end_timestamp}&group=&request_id="
    )
}

fn newapi_dashboard_data_path(start_timestamp: i64, end_timestamp: i64) -> String {
    format!(
        "/api/data/self?start_timestamp={start_timestamp}&end_timestamp={end_timestamp}&default_time=hour"
    )
}

fn unix_now_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn local_today_start_timestamp(fallback_now: i64) -> i64 {
    let now = chrono::Local::now();
    let Some(midnight) = now.date_naive().and_hms_opt(0, 0, 0) else {
        return fallback_now;
    };
    midnight
        .and_local_timezone(chrono::Local)
        .earliest()
        .map(|value| value.timestamp())
        .unwrap_or(fallback_now)
}

fn numeric_f64_field(value: &Value, keys: &[&str]) -> Option<f64> {
    keys.iter().find_map(|key| {
        value
            .get(*key)
            .and_then(|item| item.as_f64().or_else(|| item.as_str()?.trim().parse().ok()))
            .filter(|value| value.is_finite())
    })
}

fn numeric_i64_field(value: &Value, keys: &[&str]) -> Option<i64> {
    keys.iter().find_map(|key| {
        value.get(*key).and_then(|item| {
            item.as_i64()
                .or_else(|| item.as_u64().and_then(|value| i64::try_from(value).ok()))
                .or_else(|| {
                    item.as_f64().and_then(|value| {
                        (value.is_finite()
                            && value.fract() == 0.0
                            && value >= i64::MIN as f64
                            && value <= i64::MAX as f64)
                            .then_some(value as i64)
                    })
                })
                .or_else(|| item.as_str()?.trim().parse().ok())
        })
    })
}

fn numeric_usize_field(value: &Value, keys: &[&str]) -> Option<usize> {
    numeric_i64_field(value, keys).and_then(|value| usize::try_from(value).ok())
}

fn checked_sum_i64(left: Option<i64>, right: Option<i64>) -> Option<i64> {
    left.zip(right)
        .and_then(|(left, right)| left.checked_add(right))
}

fn checked_sum_f64(left: Option<f64>, right: Option<f64>) -> Option<f64> {
    left.zip(right)
        .map(|(left, right)| left + right)
        .filter(|value| value.is_finite())
}

async fn execute_json(
    context: &CollectorContext<'_>,
    role: EndpointRole,
    website_url: &str,
    path: &str,
    authenticated: bool,
) -> Result<(Value, EndpointEvidence), DriverFailure> {
    let url = build_management_url(website_url, path).map_err(|error| invalid_request(error))?;
    let request = build_json_request(
        context,
        role,
        &url,
        authenticated,
        Some(context.correlation_id.clone()),
    )
    .await?;
    let response = context
        .outbound
        .execute(request, context.cancellation.clone())
        .await
        .map_err(|failure| driver_failure_from_outbound(role, failure))?;
    let endpoint = EndpointEvidence::new(
        role,
        "GET",
        Some(response.evidence.final_url.clone()),
        Some(response.status.as_u16()),
        None,
    );
    let payload = serde_json::from_slice::<Value>(&response.body)
        .map_err(|error| malformed(role, Some(endpoint.clone()), error.to_string()))?;
    if !response.status.is_success() {
        return Err(http_failure(role, response.status, payload, endpoint));
    }
    Ok((payload, endpoint))
}

async fn build_json_request(
    context: &CollectorContext<'_>,
    role: EndpointRole,
    url: &str,
    authenticated: bool,
    correlation_id: Option<String>,
) -> Result<OutboundRequest, DriverFailure> {
    let policy = OutboundHeaderPolicy::provider_default();
    let mut headers = OutboundHeaders::new();
    headers
        .insert_public(
            header::ACCEPT,
            HeaderValue::from_static("application/json"),
            &policy,
        )
        .map_err(|failure| driver_failure_from_outbound(role, failure))?;
    if authenticated {
        let ProviderAuthContext::NewApi {
            user_id,
            secret_purpose,
        } = newapi_auth(context)?;
        headers
            .insert_public(
                NEW_API_USER_HEADER,
                HeaderValue::from_str(&user_id)
                    .map_err(|_| invalid_request("NewAPI user id is not a valid header value"))?,
                &policy,
            )
            .map_err(|failure| driver_failure_from_outbound(role, failure))?;
        let secret = context
            .secrets
            .resolve_secret(&context.credential, secret_purpose)
            .await?;
        match secret_purpose {
            CredentialSecretPurpose::AuthorizationHeader => headers
                .insert_sensitive(
                    header::AUTHORIZATION,
                    SecretHeaderValue::new(format!("Bearer {}", secret.expose())),
                    &policy,
                )
                .map_err(|failure| driver_failure_from_outbound(role, failure))?,
            CredentialSecretPurpose::SessionCookie => headers
                .insert_sensitive(
                    header::COOKIE,
                    SecretHeaderValue::new(secret.expose().to_string()),
                    &policy,
                )
                .map_err(|failure| driver_failure_from_outbound(role, failure))?,
            CredentialSecretPurpose::LoginPassword => {
                return Err(invalid_request(
                    "NewAPI collector driver cannot use login passwords",
                ));
            }
        }
    }
    Ok(OutboundRequest {
        method: Method::GET,
        url: url.to_string(),
        correlation_id,
        headers,
        body: Vec::new(),
        proxy: context.proxy.clone(),
        budget: context.budget,
    })
}

fn website_url(context: &CollectorContext<'_>) -> Result<String, DriverFailure> {
    context
        .endpoints
        .website_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| invalid_request("NewAPI website URL is missing"))
}

fn newapi_auth(context: &CollectorContext<'_>) -> Result<ProviderAuthContext, DriverFailure> {
    context
        .auth
        .clone()
        .ok_or_else(|| invalid_request("NewAPI auth context is missing"))
}

fn invalid_request(detail: impl Into<String>) -> DriverFailure {
    DriverFailure {
        kind: DriverFailureKind::InvalidRequest,
        retry: RetryDisposition::Never,
        auth_effect: AuthEffect::None,
        endpoint: None,
        evidence: EvidenceSet::empty(),
        sanitized_detail: Some(redact_text(&detail.into())),
    }
}

fn malformed(
    role: EndpointRole,
    endpoint: Option<EndpointEvidence>,
    detail: impl Into<String>,
) -> DriverFailure {
    DriverFailure {
        kind: DriverFailureKind::MalformedPayload,
        retry: RetryDisposition::Never,
        auth_effect: AuthEffect::None,
        endpoint: Some(FailedEndpoint {
            role,
            status_code: endpoint.as_ref().and_then(|entry| entry.status_code),
        }),
        evidence: endpoint
            .map(|entry| EvidenceSet::new([entry]))
            .unwrap_or_else(EvidenceSet::empty),
        sanitized_detail: Some(redact_text(&detail.into())),
    }
}

fn http_failure(
    role: EndpointRole,
    status: StatusCode,
    payload: Value,
    endpoint: EndpointEvidence,
) -> DriverFailure {
    let retry =
        if status == StatusCode::TOO_MANY_REQUESTS || status == StatusCode::SERVICE_UNAVAILABLE {
            RetryDisposition::WithinBudget
        } else {
            RetryDisposition::Never
        };
    let (kind, auth_effect) = match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => (
            DriverFailureKind::AuthRejected,
            AuthEffect::InvalidateCredential,
        ),
        StatusCode::TOO_MANY_REQUESTS => (DriverFailureKind::RateLimited, AuthEffect::None),
        status if status.is_server_error() => {
            (DriverFailureKind::ProviderUnavailable, AuthEffect::None)
        }
        _ => (DriverFailureKind::Transport, AuthEffect::None),
    };
    DriverFailure {
        kind,
        retry,
        auth_effect,
        endpoint: Some(FailedEndpoint {
            role,
            status_code: Some(status.as_u16()),
        }),
        evidence: EvidenceSet::new([endpoint]),
        sanitized_detail: Some(redact_text(&payload.to_string())),
    }
}

fn driver_failure_from_outbound(
    role: EndpointRole,
    failure: crate::outbound::OutboundFailure,
) -> DriverFailure {
    let kind = match failure.kind {
        OutboundFailureKind::BudgetExhausted => DriverFailureKind::BudgetExhausted,
        OutboundFailureKind::Cancelled => DriverFailureKind::Cancelled,
        OutboundFailureKind::ConnectTimeout
        | OutboundFailureKind::FirstByteTimeout
        | OutboundFailureKind::BodyTimeout
        | OutboundFailureKind::TotalTimeout => DriverFailureKind::Timeout,
        OutboundFailureKind::InvalidUrl
        | OutboundFailureKind::InvalidHeader
        | OutboundFailureKind::HeaderNotAllowed(_)
        | OutboundFailureKind::ProxyPolicy
        | OutboundFailureKind::TransportPolicy
        | OutboundFailureKind::RedirectBlocked
        | OutboundFailureKind::RedirectLoop
        | OutboundFailureKind::RedirectLimitExceeded
        | OutboundFailureKind::RetryAfterExceedsBudget => DriverFailureKind::InvalidRequest,
        OutboundFailureKind::BodyLimitExceeded { .. } => DriverFailureKind::MalformedPayload,
        OutboundFailureKind::RequestFailed => DriverFailureKind::Transport,
    };
    DriverFailure {
        kind,
        retry: RetryDisposition::Never,
        auth_effect: AuthEffect::None,
        endpoint: Some(FailedEndpoint {
            role,
            status_code: None,
        }),
        evidence: EvidenceSet::empty(),
        sanitized_detail: Some(redact_text(&failure.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn newapi_detect_is_immediate_success_without_network_facts() {
        let output = detect_output();

        assert_eq!(output.status, DriverOutputStatus::Success);
        assert!(output.facts.models.is_empty());
        assert!(output.evidence.is_empty());
    }

    #[test]
    fn newapi_http_status_maps_auth_rate_and_server_failures() {
        let unauthorized = http_failure(
            EndpointRole::Balance,
            StatusCode::UNAUTHORIZED,
            json!({"message": "bad cookie session=sk-p8-secret-plaintext-canary"}),
            EndpointEvidence::new(EndpointRole::Balance, "GET", None, Some(401), None),
        );
        assert_eq!(unauthorized.kind, DriverFailureKind::AuthRejected);
        assert_eq!(unauthorized.auth_effect, AuthEffect::InvalidateCredential);
        assert!(!unauthorized
            .sanitized_detail
            .as_deref()
            .unwrap_or_default()
            .contains("sk-p8-secret-plaintext-canary"));

        let rate_limited = http_failure(
            EndpointRole::Groups,
            StatusCode::TOO_MANY_REQUESTS,
            json!({"message": "rate"}),
            EndpointEvidence::new(EndpointRole::Groups, "GET", None, Some(429), None),
        );
        assert_eq!(rate_limited.kind, DriverFailureKind::RateLimited);
        assert_eq!(rate_limited.retry, RetryDisposition::WithinBudget);

        let server = http_failure(
            EndpointRole::Models,
            StatusCode::BAD_GATEWAY,
            json!({"message": "upstream"}),
            EndpointEvidence::new(EndpointRole::Models, "GET", None, Some(502), None),
        );
        assert_eq!(server.kind, DriverFailureKind::ProviderUnavailable);
    }
}
