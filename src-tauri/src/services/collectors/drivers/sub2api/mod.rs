pub mod request_recovery;

use std::time::{Duration, Instant};

use futures_util::future::{BoxFuture, FutureExt};
use http::{header, HeaderValue, Method};
use serde_json::{json, Value};

use crate::{
    models::remote_keys::RemoteStationKey,
    outbound::{
        OutboundFailureKind, OutboundHeaderPolicy, OutboundHeaders, OutboundRequest,
        OutboundRetryPolicy, SecretHeaderValue,
    },
    services::{
        collectors::{
            adapters,
            contract::{
                CollectorContext, CollectorDriver, CollectorTaskKind, CreateRemoteKeyRequest,
                CreatedRemoteKeyOutput, CredentialSecretPurpose, DriverOutput, DriverOutputStatus,
                ProviderAuthContext, ProviderKind, RedactedDiagnostics, RemoteKeyDriver,
                RemoteKeyOutput, RemoteKeyRequest, RemoteKeySecret, RevealRemoteKeyRequest,
                RevealedRemoteKeyOutput, Sub2ApiLoginCredential, Sub2ApiStationKeyCredential,
            },
            evidence::{redact_text, EndpointEvidence, EndpointRole, EvidenceSet},
            facts::CollectorFacts,
            failure::{
                AuthEffect, DriverFailure, DriverFailureKind, FailedEndpoint, RetryDisposition,
            },
        },
        station_endpoints::{build_api_url, build_management_url},
    },
};

const LOGIN_PATHS: [&str; 3] = ["/api/v1/auth/login", "/auth/login", "/api/login"];
const LOGIN_FIELDS: [&str; 3] = ["email", "username", "user"];
const REQUEST_MAX_ATTEMPTS: usize = 3;
const MALFORMED_JSON_MAX_ATTEMPTS: usize = 2;
const RETRY_DELAYS: [Duration; 2] = [Duration::from_millis(300), Duration::from_secs(1)];

pub const SUPPORTED_COLLECTOR_TASKS: &[CollectorTaskKind] = &[
    CollectorTaskKind::Detect,
    CollectorTaskKind::Balance,
    CollectorTaskKind::Groups,
];

pub struct Sub2ApiCollectorDriver;

pub struct Sub2ApiRemoteKeyDriver;

impl CollectorDriver for Sub2ApiCollectorDriver {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Sub2Api
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
                CollectorTaskKind::Models => Err(DriverFailure::unsupported(
                    "Sub2API collector does not support models",
                )),
                CollectorTaskKind::Full => Err(DriverFailure::unsupported(
                    "Sub2API full collection is split by the collector parent task",
                )),
            }
        }
        .boxed()
    }
}

impl RemoteKeyDriver for Sub2ApiRemoteKeyDriver {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Sub2Api
    }

    fn list_remote_keys<'a>(
        &'a self,
        context: &'a CollectorContext<'a>,
        request: RemoteKeyRequest,
    ) -> BoxFuture<'a, Result<RemoteKeyOutput, DriverFailure>> {
        async move {
            validate_remote_key_request(context, &request.station, &request.endpoints)?;
            let website_url = website_url_from_endpoints(&request.endpoints)?;
            let auth = sub2api_auth(context)?;
            let mut access_token = resolve_access_token(context, &website_url, &auth).await?;
            let execution =
                fetch_remote_key_list(context, &website_url, &auth, &mut access_token).await?;
            if !execution.ok {
                return Err(failed(
                    failure_kind_from_endpoint_results(&[execution.redacted.clone()]),
                    EndpointRole::RemoteKeys,
                    Some(vec![execution.evidence]),
                    "Sub2API remote-key list returned no canonical keys",
                ));
            }
            let keys = adapters::sub2api::parse_remote_key_payload(
                &request.station.station_id,
                &execution.payload,
            );
            Ok(RemoteKeyOutput {
                keys,
                evidence: vec![execution.evidence],
                diagnostics: RedactedDiagnostics {
                    summary: Some(json!({"endpointResults": [execution.redacted]}).to_string()),
                    raw_json_redacted: Some(json!({"endpointResults": [execution.redacted]})),
                },
            })
        }
        .boxed()
    }

    fn reveal_remote_key<'a>(
        &'a self,
        context: &'a CollectorContext<'a>,
        request: RevealRemoteKeyRequest,
    ) -> BoxFuture<'a, Result<RevealedRemoteKeyOutput, DriverFailure>> {
        async move {
            validate_remote_key_request(context, &request.station, &request.endpoints)?;
            let website_url = website_url_from_endpoints(&request.endpoints)?;
            let auth = sub2api_auth(context)?;
            let mut access_token = resolve_access_token(context, &website_url, &auth).await?;
            let execution =
                fetch_remote_key_list(context, &website_url, &auth, &mut access_token).await?;
            if !execution.ok {
                return Err(failed(
                    failure_kind_from_endpoint_results(&[execution.redacted.clone()]),
                    EndpointRole::RemoteKeys,
                    Some(vec![execution.evidence]),
                    "Sub2API remote-key reveal list request failed",
                ));
            }
            let (remote_key, full_key) = remote_key_secret_from_list_payload(
                &request.station.station_id,
                &request.remote_key_id,
                &execution.payload,
            )?;
            Ok(RevealedRemoteKeyOutput {
                remote_key,
                full_key: RemoteKeySecret::new(full_key),
                evidence: vec![execution.evidence],
                diagnostics: RedactedDiagnostics {
                    summary: Some(json!({"revealed": true}).to_string()),
                    raw_json_redacted: Some(json!({"endpointResults": [execution.redacted]})),
                },
            })
        }
        .boxed()
    }

    fn create_remote_key<'a>(
        &'a self,
        context: &'a CollectorContext<'a>,
        request: CreateRemoteKeyRequest,
    ) -> BoxFuture<'a, Result<CreatedRemoteKeyOutput, DriverFailure>> {
        async move {
            validate_remote_key_request(context, &request.station, &request.endpoints)?;
            let website_url = website_url_from_endpoints(&request.endpoints)?;
            let auth = sub2api_auth(context)?;
            let mut access_token = resolve_access_token(context, &website_url, &auth).await?;
            let create =
                create_remote_key_once(context, &website_url, &auth, &mut access_token, &request)
                    .await?;
            let full_key_once = adapters::sub2api::full_key_from_create_payload(&create.payload);
            let mut remote_key = adapters::sub2api::parse_remote_key_payload(
                &request.station.station_id,
                &create.payload,
            )
            .into_iter()
            .next()
            .unwrap_or_else(|| {
                adapters::sub2api::remote_key_from_create_input(
                    &request.station.station_id,
                    &crate::models::remote_keys::CreateRemoteStationKeyInput {
                        station_id: request.station.station_id.clone(),
                        name: request.name.clone(),
                        group_binding_id: None,
                        group_id_hash: request.provider_group_id.clone(),
                        group_name: request.group_name.clone(),
                    },
                    full_key_once.as_deref(),
                )
            });
            if let Some(full_key) = full_key_once {
                return Ok(CreatedRemoteKeyOutput {
                    remote_key,
                    full_key_once: RemoteKeySecret::new(full_key),
                    evidence: vec![create.evidence],
                    diagnostics: RedactedDiagnostics {
                        summary: Some(
                            json!({"created": true, "reconciledBy": "create_response"}).to_string(),
                        ),
                        raw_json_redacted: Some(json!({"endpointResults": [create.redacted]})),
                    },
                });
            }

            let list =
                fetch_remote_key_list(context, &website_url, &auth, &mut access_token).await?;
            if list.ok {
                if let Some((listed_key, full_key)) = remote_key_secret_by_name_from_list_payload(
                    &request.station.station_id,
                    &request.name,
                    &list.payload,
                )? {
                    remote_key = listed_key;
                    return Ok(CreatedRemoteKeyOutput {
                        remote_key,
                        full_key_once: RemoteKeySecret::new(full_key),
                        evidence: vec![create.evidence, list.evidence],
                        diagnostics: RedactedDiagnostics {
                            summary: Some(
                                json!({"created": true, "reconciledBy": "name"}).to_string(),
                            ),
                            raw_json_redacted: Some(
                                json!({"endpointResults": [create.redacted, list.redacted]}),
                            ),
                        },
                    });
                }
            }

            Err(result_unknown(
                EndpointRole::RemoteKeys,
                Some(create.evidence),
                "Sub2API remote-key create completed but the full key could not be reconciled",
            ))
        }
        .boxed()
    }
}

fn validate_remote_key_request(
    context: &CollectorContext<'_>,
    station: &crate::services::collectors::contract::StationIdentity,
    endpoints: &crate::services::collectors::contract::ProviderEndpoints,
) -> Result<(), DriverFailure> {
    if context.station.provider != ProviderKind::Sub2Api
        || station.provider != ProviderKind::Sub2Api
    {
        return Err(invalid_request(
            "Sub2API remote-key request has the wrong provider",
        ));
    }
    if context.station.station_id != station.station_id {
        return Err(invalid_request(
            "Sub2API remote-key request station mismatch",
        ));
    }
    if context.station.endpoint_revision != station.endpoint_revision
        || context.credential.credential_revision != station.endpoint_revision
    {
        return Err(invalid_request(
            "Sub2API remote-key request endpoint revision mismatch",
        ));
    }
    website_url_from_endpoints(endpoints).map(|_| ())
}

fn website_url_from_endpoints(
    endpoints: &crate::services::collectors::contract::ProviderEndpoints,
) -> Result<String, DriverFailure> {
    endpoints
        .website_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| invalid_request("Sub2API website URL is missing"))
}

async fn fetch_remote_key_list(
    context: &CollectorContext<'_>,
    website_url: &str,
    auth: &Sub2ApiAuth,
    access_token: &mut String,
) -> Result<JsonExecution, DriverFailure> {
    execute_bearer_json_with_recovery(
        context,
        EndpointRole::RemoteKeys,
        website_url,
        "/api/v1/keys?page=1&page_size=100",
        access_token,
        auth.login.as_ref(),
    )
    .await
}

async fn create_remote_key_once(
    context: &CollectorContext<'_>,
    website_url: &str,
    auth: &Sub2ApiAuth,
    access_token: &mut String,
    request: &CreateRemoteKeyRequest,
) -> Result<JsonExecution, DriverFailure> {
    let url = build_management_url(website_url, "/api/v1/keys")
        .map_err(|error| invalid_request(redact_text(&error)))?;
    let mut body = json!({ "name": request.name });
    if let Some(group_name) = request
        .group_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        body["group"] = json!(group_name);
    }
    if let Some(group_id) = request
        .provider_group_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        body["group_id"] = adapters::sub2api::sub2api_group_id_value(group_id);
    }

    let mut result = execute_bearer_json_once(
        context,
        EndpointRole::RemoteKeys,
        &url,
        access_token,
        Some(body.clone()),
        Method::POST,
    )
    .await;
    if matches!(result.status, Some(401 | 403)) {
        if let Some(login) = auth.login.as_ref() {
            if let Some(fresh) = login_access_token(context, website_url, login).await? {
                *access_token = fresh;
                result = execute_bearer_json_once(
                    context,
                    EndpointRole::RemoteKeys,
                    &url,
                    access_token,
                    Some(body),
                    Method::POST,
                )
                .await;
            }
        }
    }
    if let Some(failure) = fatal_attempt_failure(&result, EndpointRole::RemoteKeys) {
        return Err(match failure.kind {
            DriverFailureKind::Timeout
            | DriverFailureKind::BudgetExhausted
            | DriverFailureKind::Cancelled
            | DriverFailureKind::Transport => result_unknown(
                EndpointRole::RemoteKeys,
                failure.evidence.entries().first().cloned(),
                "Sub2API remote-key create outcome is unknown; reconcile before retrying",
            ),
            _ => failure,
        });
    }
    let failure = classify_json_result(&result);
    let redacted = json!({
        "url": result.url,
        "status": result.status,
        "ok": result.ok && failure.is_none(),
        "durationMs": result.duration_ms,
        "path": "/api/v1/keys",
        "attemptCount": 1,
        "failureKind": failure,
    });
    if result.ok && failure.is_none() {
        return Ok(JsonExecution {
            payload: result.payload,
            ok: true,
            redacted,
            evidence: result.evidence,
        });
    }
    if matches!(result.status, None) {
        return Err(result_unknown(
            EndpointRole::RemoteKeys,
            Some(result.evidence),
            "Sub2API remote-key create outcome is unknown; reconcile before retrying",
        ));
    }
    Err(failed(
        failure_kind_from_endpoint_results(&[redacted]),
        EndpointRole::RemoteKeys,
        Some(vec![result.evidence]),
        result
            .error_message
            .unwrap_or_else(|| "Sub2API remote-key create failed".to_string()),
    ))
}

fn remote_key_secret_from_list_payload(
    station_id: &str,
    remote_key_id: &str,
    payload: &Value,
) -> Result<(RemoteStationKey, String), DriverFailure> {
    for (index, value) in adapters::sub2api::remote_key_items(payload)
        .into_iter()
        .enumerate()
    {
        let Some(remote_key) = adapters::sub2api::remote_key_from_value(station_id, value, index)
        else {
            continue;
        };
        if remote_key.id != remote_key_id {
            continue;
        }
        let full_key = adapters::sub2api::full_key_from_key_value(value).ok_or_else(|| {
            malformed(
                EndpointRole::RemoteKeys,
                None,
                "Sub2API remote-key list did not return a full key",
            )
        })?;
        return Ok((remote_key, full_key));
    }
    Err(invalid_request(
        "Sub2API remote key no longer exists; reconcile before creating a local key",
    ))
}

fn remote_key_secret_by_name_from_list_payload(
    station_id: &str,
    expected_name: &str,
    payload: &Value,
) -> Result<Option<(RemoteStationKey, String)>, DriverFailure> {
    let expected_name = expected_name.trim();
    for (index, value) in adapters::sub2api::remote_key_items(payload)
        .into_iter()
        .enumerate()
    {
        let Some(remote_key) = adapters::sub2api::remote_key_from_value(station_id, value, index)
        else {
            continue;
        };
        if !remote_key
            .remote_key_name
            .as_deref()
            .map(str::trim)
            .is_some_and(|name| !expected_name.is_empty() && name == expected_name)
        {
            continue;
        }
        let Some(full_key) = adapters::sub2api::full_key_from_key_value(value) else {
            continue;
        };
        return Ok(Some((remote_key, full_key)));
    }
    Ok(None)
}

fn detect_output() -> DriverOutput {
    DriverOutput {
        facts: CollectorFacts::default(),
        evidence: Vec::new(),
        status: DriverOutputStatus::Success,
        diagnostics: RedactedDiagnostics {
            summary: Some(json!({"adapter": "sub2api", "task": "detect"}).to_string()),
            raw_json_redacted: None,
        },
    }
}

async fn collect_groups(context: &CollectorContext<'_>) -> Result<DriverOutput, DriverFailure> {
    let website_url = website_url(context)?;
    let auth = sub2api_auth(context)?;
    let mut access_token = resolve_access_token(context, &website_url, &auth).await?;

    let mut evidence = Vec::new();
    let mut endpoint_results = Vec::new();
    let available = execute_bearer_json_with_recovery(
        context,
        EndpointRole::Groups,
        &website_url,
        "/api/v1/groups/available",
        &mut access_token,
        auth.login.as_ref(),
    )
    .await?;
    evidence.push(available.evidence.clone());
    endpoint_results.push(available.redacted.clone());

    let rates = execute_bearer_json_with_recovery(
        context,
        EndpointRole::Groups,
        &website_url,
        "/api/v1/groups/rates",
        &mut access_token,
        auth.login.as_ref(),
    )
    .await?;
    evidence.push(rates.evidence.clone());
    endpoint_results.push(rates.redacted.clone());

    let mut facts = adapters::sub2api::parse_group_rate_facts(
        &context.station.station_id,
        &available.payload,
        &rates.payload,
        auth.credit_per_cny,
    );
    let station_keys = auth
        .station_keys
        .iter()
        .map(|key| crate::models::station_keys::StationKey {
            id: key.station_key_id.clone(),
            station_id: context.station.station_id.clone(),
            name: String::new(),
            api_key_masked: String::new(),
            api_key_present: true,
            enabled: true,
            priority: 0,
            max_concurrency: 1,
            load_factor: None,
            schedulable: true,
            group_name: None,
            tier_label: None,
            group_binding_id: None,
            group_id_hash: None,
            rate_multiplier: None,
            manual_rate_multiplier: None,
            manual_rate_updated_at: None,
            rate_source: None,
            rate_collected_at: None,
            balance_scope: None,
            status: "unchecked".to_string(),
            last_used_at: None,
            last_checked_at: None,
            note: None,
            created_at: String::new(),
            updated_at: String::new(),
        })
        .collect::<Vec<_>>();
    adapters::sub2api::add_single_group_key_bindings(&mut facts, &station_keys);

    let success_count = [available.ok, rates.ok]
        .into_iter()
        .filter(|ok| *ok)
        .count();
    let status = match success_count {
        2 => DriverOutputStatus::Success,
        1 if !facts.groups.is_empty() => DriverOutputStatus::Partial,
        _ if !facts.groups.is_empty() || !facts.rates.is_empty() => DriverOutputStatus::Partial,
        _ => {
            return Err(failed(
                failure_kind_from_endpoint_results(&endpoint_results),
                EndpointRole::Groups,
                Some(evidence),
                "Sub2API groups/rates returned no canonical facts",
            ));
        }
    };

    Ok(DriverOutput {
        facts,
        evidence,
        status,
        diagnostics: RedactedDiagnostics {
            summary: Some(json!({"endpointResults": endpoint_results}).to_string()),
            raw_json_redacted: Some(json!({"endpointResults": endpoint_results})),
        },
    })
}

async fn collect_balance(context: &CollectorContext<'_>) -> Result<DriverOutput, DriverFailure> {
    let api_base_url = api_base_url(context)?;
    let website_url = website_url(context).ok();
    let auth = sub2api_auth(context)?;
    let usage_url = build_api_url(&api_base_url, "/v1/usage")
        .map_err(|error| invalid_request(redact_text(&error)))?;
    let mut facts = CollectorFacts::default();
    let mut evidence = Vec::new();
    let mut endpoint_results = Vec::new();
    let mut transient_retry_keys = Vec::new();

    for key in &auth.station_keys {
        let api_key = match context
            .secrets
            .resolve_secret(
                &key.credential,
                CredentialSecretPurpose::AuthorizationHeader,
            )
            .await
        {
            Ok(secret) => secret,
            Err(error) => {
                endpoint_results.push(json!({
                    "endpoint": usage_url,
                    "stationKeyId": key.station_key_id,
                    "status": "secret_error",
                    "message": error.sanitized_detail.unwrap_or_else(|| "secret unavailable".to_string()),
                }));
                continue;
            }
        };
        let result = execute_bearer_json_once(
            context,
            EndpointRole::Balance,
            &usage_url,
            api_key.expose(),
            None,
            Method::GET,
        )
        .await;
        if let Some(failure) = fatal_attempt_failure(&result, EndpointRole::Balance) {
            return Err(failure);
        }
        let mut redacted = balance_endpoint_json(&result, &usage_url, &key.station_key_id);
        redacted["attemptCount"] = json!(1);
        evidence.push(result.evidence.clone());
        endpoint_results.push(redacted);
        let endpoint_index = endpoint_results.len() - 1;
        if result.ok {
            facts.balances.push(adapters::sub2api::parse_usage_balance(
                &context.station.station_id,
                Some(key.station_key_id.clone()),
                &result.payload,
                auth.credit_per_cny,
            ));
        } else if result.is_transient() {
            transient_retry_keys.push((key, api_key.expose().to_string(), endpoint_index));
        }
    }

    for _round in 0..2 {
        if transient_retry_keys.is_empty() {
            break;
        }
        let retrying = std::mem::take(&mut transient_retry_keys);
        for (key, api_key, endpoint_index) in retrying {
            let result = execute_bearer_json_once(
                context,
                EndpointRole::Balance,
                &usage_url,
                &api_key,
                None,
                Method::GET,
            )
            .await;
            if let Some(failure) = fatal_attempt_failure(&result, EndpointRole::Balance) {
                return Err(failure);
            }
            evidence.push(result.evidence.clone());
            if let Some(endpoint) = endpoint_results.get_mut(endpoint_index) {
                append_balance_endpoint_attempt(endpoint, &result);
            }
            if result.ok {
                facts.balances.push(adapters::sub2api::parse_usage_balance(
                    &context.station.station_id,
                    Some(key.station_key_id.clone()),
                    &result.payload,
                    auth.credit_per_cny,
                ));
            } else if result.is_transient() {
                transient_retry_keys.push((key, api_key, endpoint_index));
            }
        }
    }

    if let Some(website_url) = website_url {
        let mut access_token = resolve_access_token(context, &website_url, &auth)
            .await
            .ok();
        if let Some(profile_balance) = collect_account_profile_balance(
            context,
            &website_url,
            &auth,
            &mut access_token,
            &mut evidence,
            &mut endpoint_results,
        )
        .await?
        {
            if facts.balances.is_empty() {
                facts.balances.push(profile_balance);
            } else {
                adapters::sub2api::merge_account_profile_balance(
                    &mut facts.balances,
                    profile_balance,
                );
            }
        }
        if !facts.balances.is_empty() {
            if let Some(stats) = collect_dashboard_usage_stats(
                context,
                &website_url,
                &auth,
                &mut access_token,
                &mut evidence,
                &mut endpoint_results,
            )
            .await?
            {
                adapters::sub2api::merge_dashboard_usage_stats(
                    &mut facts.balances,
                    &context.station.station_id,
                    stats,
                );
            }
        }
    }

    if facts.balances.is_empty() {
        return Err(failed(
            failure_kind_from_endpoint_results(&endpoint_results),
            EndpointRole::Balance,
            Some(evidence),
            "Sub2API usage/profile returned no balance facts",
        ));
    }

    Ok(DriverOutput {
        facts,
        evidence,
        status: DriverOutputStatus::Success,
        diagnostics: RedactedDiagnostics {
            summary: Some(json!({"endpointResults": endpoint_results}).to_string()),
            raw_json_redacted: Some(json!({"endpointResults": endpoint_results})),
        },
    })
}

async fn collect_account_profile_balance(
    context: &CollectorContext<'_>,
    website_url: &str,
    auth: &Sub2ApiAuth,
    access_token: &mut Option<String>,
    evidence: &mut Vec<EndpointEvidence>,
    endpoint_results: &mut Vec<Value>,
) -> Result<Option<crate::services::collectors::facts::CollectedBalanceFact>, DriverFailure> {
    for path in ["/api/v1/user/profile", "/api/v1/auth/me"] {
        let Some(token) = ensure_access_token(context, website_url, auth, access_token).await?
        else {
            return Ok(None);
        };
        let mut token = token;
        let execution = execute_bearer_json_with_recovery(
            context,
            EndpointRole::Balance,
            website_url,
            path,
            &mut token,
            auth.login.as_ref(),
        )
        .await?;
        *access_token = Some(token);
        evidence.push(execution.evidence.clone());
        endpoint_results.push(execution.redacted);
        if !execution.ok {
            continue;
        }
        if let Some(balance) = adapters::sub2api::parse_account_balance(
            &context.station.station_id,
            &execution.payload,
            auth.credit_per_cny,
        ) {
            return Ok(Some(balance));
        }
    }
    Ok(None)
}

async fn collect_dashboard_usage_stats(
    context: &CollectorContext<'_>,
    website_url: &str,
    auth: &Sub2ApiAuth,
    access_token: &mut Option<String>,
    evidence: &mut Vec<EndpointEvidence>,
    endpoint_results: &mut Vec<Value>,
) -> Result<Option<adapters::sub2api::DashboardUsageStats>, DriverFailure> {
    let Some(token) = ensure_access_token(context, website_url, auth, access_token).await? else {
        return Ok(None);
    };
    let mut token = token;
    let execution = execute_bearer_json_with_recovery(
        context,
        EndpointRole::Balance,
        website_url,
        "/api/v1/usage/dashboard/stats",
        &mut token,
        auth.login.as_ref(),
    )
    .await?;
    *access_token = Some(token);
    evidence.push(execution.evidence.clone());
    endpoint_results.push(execution.redacted);
    if !execution.ok {
        return Ok(None);
    }
    Ok(adapters::sub2api::parse_dashboard_usage_stats(
        &execution.payload,
    ))
}

async fn ensure_access_token(
    context: &CollectorContext<'_>,
    website_url: &str,
    auth: &Sub2ApiAuth,
    access_token: &mut Option<String>,
) -> Result<Option<String>, DriverFailure> {
    if access_token
        .as_deref()
        .is_some_and(|token| !token.trim().is_empty())
    {
        return Ok(access_token.clone());
    }
    match auth.login.as_ref() {
        Some(login) => {
            let token = login_access_token(context, website_url, login).await?;
            *access_token = token.clone();
            Ok(token)
        }
        None => Ok(None),
    }
}

#[derive(Clone)]
struct Sub2ApiAuth {
    station_keys: Vec<Sub2ApiStationKeyCredential>,
    access_token: Option<crate::services::collectors::contract::OpaqueCredentialHandle>,
    login: Option<Sub2ApiLoginCredential>,
    credit_per_cny: f64,
}

fn sub2api_auth(context: &CollectorContext<'_>) -> Result<Sub2ApiAuth, DriverFailure> {
    match context.auth.clone() {
        Some(ProviderAuthContext::Sub2Api {
            station_keys,
            access_token,
            login,
            credit_per_cny,
        }) => Ok(Sub2ApiAuth {
            station_keys,
            access_token,
            login,
            credit_per_cny,
        }),
        Some(ProviderAuthContext::NewApi { .. }) => Err(invalid_request(
            "Sub2API auth context has the wrong provider",
        )),
        None => Err(invalid_request("Sub2API auth context is missing")),
    }
}

async fn resolve_access_token(
    context: &CollectorContext<'_>,
    website_url: &str,
    auth: &Sub2ApiAuth,
) -> Result<String, DriverFailure> {
    if let Some(handle) = &auth.access_token {
        let secret = context
            .secrets
            .resolve_secret(handle, CredentialSecretPurpose::SessionCookie)
            .await?;
        let token = secret.expose().trim();
        if !token.is_empty() {
            return Ok(token.to_string());
        }
    }
    if let Some(login) = &auth.login {
        if let Some(token) = login_access_token(context, website_url, login).await? {
            return Ok(token);
        }
    }
    Err(manual_required(
        "Sub2API collector requires an access token or saved login password",
    ))
}

#[derive(Debug, Clone)]
struct JsonExecution {
    payload: Value,
    ok: bool,
    redacted: Value,
    evidence: EndpointEvidence,
}

async fn execute_bearer_json_with_recovery(
    context: &CollectorContext<'_>,
    role: EndpointRole,
    base_url: &str,
    path: &str,
    access_token: &mut String,
    login: Option<&Sub2ApiLoginCredential>,
) -> Result<JsonExecution, DriverFailure> {
    let url = build_management_url(base_url, path)
        .map_err(|error| invalid_request(redact_text(&error)))?;
    let started_at = Instant::now();
    let mut attempts = Vec::new();
    let mut recovery_actions = Vec::new();
    let mut auth_refreshed = false;
    let mut malformed_attempts = 0;
    let mut latest = None;

    for attempt in 1..=REQUEST_MAX_ATTEMPTS {
        let result =
            execute_bearer_json_once(context, role, &url, access_token, None, Method::GET).await;
        if let Some(failure) = fatal_attempt_failure(&result, role) {
            return Err(failure);
        }
        let mut failure = classify_json_result(&result);
        if result.ok && result.payload.is_null() {
            failure = Some("invalid_json");
            malformed_attempts += 1;
        }
        let action = if failure.is_none() {
            "complete"
        } else if failure == Some("auth_rejected") && !auth_refreshed && login.is_some() {
            "auth_refresh"
        } else if is_retryable_failure(failure, malformed_attempts)
            && attempt < REQUEST_MAX_ATTEMPTS
        {
            "transient_retry"
        } else {
            "complete"
        };
        attempts.push(json!({
            "attempt": attempt,
            "status": result.status,
            "ok": result.ok && failure.is_none(),
            "durationMs": result.duration_ms,
            "failureKind": failure,
            "action": action,
        }));
        latest = Some(result);

        if failure.is_none() {
            break;
        }
        if action == "auth_refresh" {
            auth_refreshed = true;
            recovery_actions.push("auth_refresh");
            if let Some(login) = login {
                if let Some(fresh) = login_access_token(context, base_url, login).await? {
                    *access_token = fresh;
                    continue;
                }
            }
            break;
        }
        if action == "transient_retry" {
            recovery_actions.push("transient_retry");
            let delay = latest
                .as_ref()
                .and_then(|result| result.retry_after)
                .unwrap_or_else(|| RETRY_DELAYS.get(attempt - 1).copied().unwrap_or_default());
            if !delay.is_zero() {
                tokio::select! {
                    _ = context.cancellation.cancelled() => {
                        return Err(failed(DriverFailureKind::Cancelled, role, None, "request cancelled"));
                    }
                    _ = tokio::time::sleep(delay) => {}
                }
            }
            continue;
        }
        break;
    }

    let result = latest.unwrap_or_else(|| JsonAttemptResult::budget_exhausted(path, role));
    let mut redacted = json!({
        "url": result.url,
        "status": result.status,
        "ok": result.ok,
        "durationMs": started_at.elapsed().as_millis() as i64,
        "path": path,
        "attemptCount": attempts.len(),
        "failureKind": classify_json_result(&result),
        "attempts": attempts,
    });
    if !recovery_actions.is_empty() {
        redacted["recoveryActions"] = json!(recovery_actions);
    }
    Ok(JsonExecution {
        payload: result.payload,
        ok: result.ok,
        redacted,
        evidence: result.evidence,
    })
}

#[derive(Debug, Clone)]
struct JsonAttemptResult {
    url: String,
    status: Option<u16>,
    ok: bool,
    payload: Value,
    error_message: Option<String>,
    failure_kind: Option<DriverFailureKind>,
    retry_after: Option<Duration>,
    duration_ms: i64,
    evidence: EndpointEvidence,
}

impl JsonAttemptResult {
    fn budget_exhausted(url: &str, role: EndpointRole) -> Self {
        Self {
            url: url.to_string(),
            status: None,
            ok: false,
            payload: Value::Null,
            error_message: Some("task budget exhausted".to_string()),
            failure_kind: Some(DriverFailureKind::BudgetExhausted),
            retry_after: None,
            duration_ms: 0,
            evidence: EndpointEvidence::new(
                role,
                "GET",
                Some(url.to_string()),
                None,
                Some("task budget exhausted".to_string()),
            ),
        }
    }

    fn is_transient(&self) -> bool {
        matches!(self.status, None | Some(408 | 429 | 500..=599))
    }
}

async fn execute_bearer_json_once(
    context: &CollectorContext<'_>,
    role: EndpointRole,
    url: &str,
    bearer: &str,
    body: Option<Value>,
    method: Method,
) -> JsonAttemptResult {
    let started_at = Instant::now();
    let request = match build_json_request(context, role, url, bearer, body, method.clone()) {
        Ok(request) => request,
        Err(error) => {
            return JsonAttemptResult {
                url: url.to_string(),
                status: None,
                ok: false,
                payload: Value::Null,
                error_message: error.sanitized_detail,
                failure_kind: Some(error.kind),
                retry_after: None,
                duration_ms: started_at.elapsed().as_millis() as i64,
                evidence: EndpointEvidence::new(
                    role,
                    method.as_str(),
                    Some(url.to_string()),
                    None,
                    None,
                ),
            };
        }
    };
    match context
        .outbound
        .execute(request, context.cancellation.clone())
        .await
    {
        Ok(response) => {
            let status = response.status.as_u16();
            let payload = serde_json::from_slice::<Value>(&response.body).unwrap_or(Value::Null);
            let ok = response.status.is_success() && !payload.is_null();
            let error_message =
                (!ok).then(|| redact_text(std::str::from_utf8(&response.body).unwrap_or_default()));
            JsonAttemptResult {
                url: response.evidence.final_url.clone(),
                status: Some(status),
                ok,
                payload,
                error_message,
                failure_kind: None,
                retry_after: response.evidence.retry_after,
                duration_ms: started_at.elapsed().as_millis() as i64,
                evidence: EndpointEvidence::new(
                    role,
                    method.as_str(),
                    Some(response.evidence.final_url),
                    Some(status),
                    None,
                ),
            }
        }
        Err(error) => JsonAttemptResult {
            url: url.to_string(),
            status: None,
            ok: false,
            payload: Value::Null,
            error_message: Some(redact_text(&error.to_string())),
            failure_kind: Some(driver_failure_kind_from_outbound(&error.kind)),
            retry_after: None,
            duration_ms: started_at.elapsed().as_millis() as i64,
            evidence: EndpointEvidence::new(
                role,
                method.as_str(),
                Some(url.to_string()),
                None,
                Some(error.to_string()),
            ),
        },
    }
}

fn build_json_request(
    context: &CollectorContext<'_>,
    role: EndpointRole,
    url: &str,
    bearer: &str,
    body: Option<Value>,
    method: Method,
) -> Result<OutboundRequest, DriverFailure> {
    let policy = OutboundHeaderPolicy::provider_default();
    let mut headers = OutboundHeaders::new();
    headers
        .insert_public(
            header::ACCEPT,
            HeaderValue::from_static("application/json"),
            &policy,
        )
        .map_err(|failure| driver_failure_from_outbound(role, failure.kind))?;
    headers
        .insert_sensitive(
            header::AUTHORIZATION,
            SecretHeaderValue::new(format!("Bearer {bearer}")),
            &policy,
        )
        .map_err(|failure| driver_failure_from_outbound(role, failure.kind))?;
    let body = match body {
        Some(body) => {
            headers
                .insert_public(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("application/json"),
                    &policy,
                )
                .map_err(|failure| driver_failure_from_outbound(role, failure.kind))?;
            serde_json::to_vec(&body).map_err(|error| malformed(role, None, error.to_string()))?
        }
        None => Vec::new(),
    };
    Ok(OutboundRequest {
        method,
        url: url.to_string(),
        correlation_id: Some(context.correlation_id.clone()),
        headers,
        body,
        proxy: context.proxy.clone(),
        budget: context.budget,
        retry_policy: OutboundRetryPolicy::Never,
    })
}

async fn login_access_token(
    context: &CollectorContext<'_>,
    base_url: &str,
    login: &Sub2ApiLoginCredential,
) -> Result<Option<String>, DriverFailure> {
    let password = context
        .secrets
        .resolve_secret(&login.password, CredentialSecretPurpose::LoginPassword)
        .await?;
    for path in LOGIN_PATHS {
        let url = build_management_url(base_url, path)
            .map_err(|error| invalid_request(redact_text(&error)))?;
        for field in LOGIN_FIELDS {
            let payload = json!({ field: login.username, "password": password.expose() });
            let request = build_login_request(context, &url, payload)?;
            let response = context
                .outbound
                .execute(request, context.cancellation.clone())
                .await
                .map_err(|error| {
                    driver_failure_from_outbound(EndpointRole::Authorization, error.kind)
                })?;
            let status = response.status.as_u16();
            let parsed = serde_json::from_slice::<Value>(&response.body).unwrap_or(Value::Null);
            if let Some(token) = extract_token(&parsed) {
                return Ok(Some(token));
            }
            if is_manual_login_required(&parsed, status) || response.status.is_success() {
                return Ok(None);
            }
        }
    }
    Ok(None)
}

fn build_login_request(
    context: &CollectorContext<'_>,
    url: &str,
    body: Value,
) -> Result<OutboundRequest, DriverFailure> {
    let policy = OutboundHeaderPolicy::provider_default();
    let mut headers = OutboundHeaders::new();
    headers
        .insert_public(
            header::ACCEPT,
            HeaderValue::from_static("application/json"),
            &policy,
        )
        .map_err(|failure| {
            driver_failure_from_outbound(EndpointRole::Authorization, failure.kind)
        })?;
    headers
        .insert_public(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
            &policy,
        )
        .map_err(|failure| {
            driver_failure_from_outbound(EndpointRole::Authorization, failure.kind)
        })?;
    Ok(OutboundRequest {
        method: Method::POST,
        url: url.to_string(),
        correlation_id: Some(context.correlation_id.clone()),
        headers,
        body: serde_json::to_vec(&body)
            .map_err(|error| malformed(EndpointRole::Authorization, None, error.to_string()))?,
        proxy: context.proxy.clone(),
        budget: context.budget,
        retry_policy: OutboundRetryPolicy::Never,
    })
}

fn extract_token(value: &Value) -> Option<String> {
    value
        .get("access_token")
        .or_else(|| value.get("token"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(ToString::to_string)
        .or_else(|| value.get("data").and_then(extract_token))
}

fn is_manual_login_required(value: &Value, status: u16) -> bool {
    if matches!(status, 401 | 403) {
        return true;
    }
    let text = value.to_string().to_lowercase();
    text.contains("geetest")
        || text.contains("captcha")
        || text.contains("turnstile")
        || text.contains("verification_failed")
        || value
            .get("requires_2fa")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        || value
            .get("captcha_required")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        || value
            .get("manual_required")
            .and_then(Value::as_bool)
            .unwrap_or(false)
}

fn classify_json_result(result: &JsonAttemptResult) -> Option<&'static str> {
    match result.failure_kind {
        Some(DriverFailureKind::Cancelled) => return Some("cancelled"),
        Some(DriverFailureKind::BudgetExhausted) => return Some("budget_exhausted"),
        Some(DriverFailureKind::Timeout) => return Some("network_timeout"),
        _ => {}
    }
    match result.status {
        None => Some("network_timeout"),
        Some(401 | 403) => Some("auth_rejected"),
        Some(408) => Some("network_timeout"),
        Some(429) => Some("rate_limited"),
        Some(500..=599) => Some("upstream_5xx"),
        Some(400..=499) => Some("permanent_http"),
        Some(_) if result.ok => None,
        Some(_) => Some("permanent_http"),
    }
}

fn is_retryable_failure(failure: Option<&str>, malformed_attempts: usize) -> bool {
    matches!(
        failure,
        Some("network_timeout" | "rate_limited" | "upstream_5xx")
    ) || (failure == Some("invalid_json") && malformed_attempts < MALFORMED_JSON_MAX_ATTEMPTS)
}

fn fatal_attempt_failure(result: &JsonAttemptResult, role: EndpointRole) -> Option<DriverFailure> {
    let kind = result.failure_kind?;
    if !matches!(
        kind,
        DriverFailureKind::Cancelled | DriverFailureKind::BudgetExhausted
    ) {
        return None;
    }
    Some(failed(
        kind,
        role,
        Some(vec![result.evidence.clone()]),
        result
            .error_message
            .clone()
            .unwrap_or_else(|| "request did not complete".to_string()),
    ))
}

fn failure_kind_from_endpoint_results(results: &[Value]) -> DriverFailureKind {
    let has_failure = |label: &str| {
        results.iter().any(|result| {
            result
                .get("failureKind")
                .and_then(Value::as_str)
                .is_some_and(|value| value == label)
                || result
                    .get("attempts")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .any(|attempt| {
                        attempt
                            .get("failureKind")
                            .and_then(Value::as_str)
                            .is_some_and(|value| value == label)
                    })
        })
    };
    let has_status = |matches: fn(u64) -> bool| {
        results.iter().any(|result| {
            result
                .get("status")
                .and_then(Value::as_u64)
                .is_some_and(matches)
                || result
                    .get("attempts")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .any(|attempt| {
                        attempt
                            .get("status")
                            .and_then(Value::as_u64)
                            .is_some_and(matches)
                    })
        })
    };

    if has_failure("cancelled") {
        DriverFailureKind::Cancelled
    } else if has_failure("budget_exhausted") {
        DriverFailureKind::BudgetExhausted
    } else if has_failure("auth_rejected") || has_status(|status| matches!(status, 401 | 403)) {
        DriverFailureKind::AuthRejected
    } else if has_failure("rate_limited") || has_status(|status| status == 429) {
        DriverFailureKind::RateLimited
    } else if has_failure("invalid_json") {
        DriverFailureKind::MalformedPayload
    } else {
        DriverFailureKind::ProviderUnavailable
    }
}

fn balance_endpoint_json(
    result: &JsonAttemptResult,
    endpoint: &str,
    station_key_id: &str,
) -> Value {
    json!({
        "endpoint": endpoint,
        "url": result.url,
        "stationKeyId": station_key_id,
        "status": result.status,
        "durationMs": result.duration_ms,
        "ok": result.ok,
        "errorMessage": result.error_message,
    })
}

fn append_balance_endpoint_attempt(endpoint: &mut Value, result: &JsonAttemptResult) {
    let attempt = json!({
        "url": result.url,
        "status": result.status,
        "durationMs": result.duration_ms,
        "ok": result.ok,
        "errorMessage": result.error_message,
    });
    if endpoint.get("attempts").is_none() {
        let first_attempt = json!({
            "url": endpoint.get("url").cloned().unwrap_or(Value::Null),
            "status": endpoint.get("status").cloned().unwrap_or(Value::Null),
            "durationMs": endpoint.get("durationMs").cloned().unwrap_or(Value::Null),
            "ok": endpoint.get("ok").cloned().unwrap_or(Value::Null),
            "errorMessage": endpoint.get("errorMessage").cloned().unwrap_or(Value::Null),
        });
        endpoint["attempts"] = json!([first_attempt]);
    }
    if let Some(attempts) = endpoint["attempts"].as_array_mut() {
        attempts.push(attempt);
        endpoint["attemptCount"] = json!(attempts.len());
    }
    endpoint["url"] = json!(result.url);
    endpoint["status"] = json!(result.status);
    endpoint["durationMs"] = json!(result.duration_ms);
    endpoint["ok"] = json!(result.ok);
    endpoint["errorMessage"] = json!(result.error_message);
    endpoint["recoveryActions"] = json!(["transient_retry"]);
}

fn api_base_url(context: &CollectorContext<'_>) -> Result<String, DriverFailure> {
    context
        .endpoints
        .api_base_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| invalid_request("Sub2API API base URL is missing"))
}

fn website_url(context: &CollectorContext<'_>) -> Result<String, DriverFailure> {
    context
        .endpoints
        .website_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| invalid_request("Sub2API website URL is missing"))
}

fn manual_required(detail: impl Into<String>) -> DriverFailure {
    DriverFailure {
        kind: DriverFailureKind::Unsupported,
        retry: RetryDisposition::Never,
        auth_effect: AuthEffect::Reauthorize,
        endpoint: None,
        evidence: EvidenceSet::empty(),
        sanitized_detail: Some(redact_text(&detail.into())),
    }
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
        evidence: EvidenceSet::new(endpoint),
        sanitized_detail: Some(redact_text(&detail.into())),
    }
}

fn failed(
    kind: DriverFailureKind,
    role: EndpointRole,
    evidence: Option<Vec<EndpointEvidence>>,
    detail: impl Into<String>,
) -> DriverFailure {
    DriverFailure {
        kind,
        retry: RetryDisposition::Never,
        auth_effect: AuthEffect::None,
        endpoint: Some(FailedEndpoint {
            role,
            status_code: None,
        }),
        evidence: evidence
            .map(EvidenceSet::new)
            .unwrap_or_else(EvidenceSet::empty),
        sanitized_detail: Some(redact_text(&detail.into())),
    }
}

fn result_unknown(
    role: EndpointRole,
    endpoint: Option<EndpointEvidence>,
    detail: impl Into<String>,
) -> DriverFailure {
    DriverFailure {
        kind: DriverFailureKind::ResultUnknown,
        retry: RetryDisposition::Never,
        auth_effect: AuthEffect::None,
        endpoint: Some(FailedEndpoint {
            role,
            status_code: endpoint.as_ref().and_then(|entry| entry.status_code),
        }),
        evidence: EvidenceSet::new(endpoint),
        sanitized_detail: Some(redact_text(&detail.into())),
    }
}

fn driver_failure_from_outbound(role: EndpointRole, kind: OutboundFailureKind) -> DriverFailure {
    let failure_kind = driver_failure_kind_from_outbound(&kind);
    failed(failure_kind, role, None, format!("{kind:?}"))
}

fn driver_failure_kind_from_outbound(kind: &OutboundFailureKind) -> DriverFailureKind {
    match kind {
        OutboundFailureKind::InvalidUrl
        | OutboundFailureKind::InvalidHeader
        | OutboundFailureKind::HeaderNotAllowed(_)
        | OutboundFailureKind::ProxyPolicy
        | OutboundFailureKind::TransportPolicy => DriverFailureKind::InvalidRequest,
        OutboundFailureKind::ConnectTimeout
        | OutboundFailureKind::FirstByteTimeout
        | OutboundFailureKind::BodyTimeout
        | OutboundFailureKind::TotalTimeout
        | OutboundFailureKind::RetryAfterExceedsBudget => DriverFailureKind::Timeout,
        OutboundFailureKind::BudgetExhausted => DriverFailureKind::BudgetExhausted,
        OutboundFailureKind::Cancelled => DriverFailureKind::Cancelled,
        OutboundFailureKind::BodyLimitExceeded { .. } => DriverFailureKind::MalformedPayload,
        OutboundFailureKind::RedirectBlocked
        | OutboundFailureKind::RedirectLoop
        | OutboundFailureKind::RedirectLimitExceeded
        | OutboundFailureKind::RequestFailed => DriverFailureKind::Transport,
    }
}
