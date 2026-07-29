pub(crate) mod auth;
pub(crate) mod parsers;
#[cfg(test)]
pub(crate) mod test_support;

use futures_util::future::{BoxFuture, FutureExt};
use http::{header, HeaderName, HeaderValue, Method, StatusCode};
use serde_json::{json, Value};

use crate::{
    models::remote_keys::{RemoteKeyMatchStatus, RemoteStationKey},
    outbound::{
        OutboundFailureKind, OutboundHeaderPolicy, OutboundHeaders, OutboundRequest,
        OutboundRetryPolicy, SecretHeaderValue,
    },
    services::{
        collectors::{
            contract::{
                AuthorizationDriver, AuthorizationOutput, AuthorizationRequest,
                AuthorizationStatus, CollectorContext, CollectorDriver, CollectorTaskKind,
                CreateRemoteKeyRequest, CreatedRemoteKeyOutput, CredentialSecretPurpose,
                DeleteRemoteKeyRequest, DeletedRemoteKeyOutput, DriverOutput, DriverOutputStatus,
                ProviderAuthContext, ProviderKind, RedactedDiagnostics, RemoteKeyDriver,
                RemoteKeyOutput, RemoteKeyRequest, RemoteKeySecret, RevealRemoteKeyRequest,
                RevealedRemoteKeyOutput,
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
const NEWAPI_REMOTE_KEY_PAGE_SIZE: usize = 100;
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

pub struct NewApiRemoteKeyDriver;

pub struct NewApiAuthorizationDriver;

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

impl RemoteKeyDriver for NewApiRemoteKeyDriver {
    fn kind(&self) -> ProviderKind {
        ProviderKind::NewApi
    }

    fn list_remote_keys<'a>(
        &'a self,
        context: &'a CollectorContext<'a>,
        request: RemoteKeyRequest,
    ) -> BoxFuture<'a, Result<RemoteKeyOutput, DriverFailure>> {
        async move {
            validate_remote_key_request(context, &request.station, &request.endpoints)?;
            let website_url = request_website_url(&request.endpoints)?;
            let (items, evidence) = fetch_newapi_token_items(context, &website_url).await?;
            Ok(RemoteKeyOutput {
                keys: parse_remote_key_items(&request.station.station_id, &items),
                evidence,
                diagnostics: RedactedDiagnostics {
                    summary: Some(json!({"tokenCount": items.len()}).to_string()),
                    raw_json_redacted: None,
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
            let website_url = request_website_url(&request.endpoints)?;
            let (items, mut evidence) = fetch_newapi_token_items(context, &website_url).await?;
            for (index, value) in items.iter().enumerate() {
                let Some(remote_key) =
                    remote_key_from_value(&request.station.station_id, value, index)
                else {
                    continue;
                };
                if remote_key.id != request.remote_key_id {
                    continue;
                }
                let (remote_key, full_key, endpoint) =
                    reveal_full_key_for_token_value(context, &website_url, value, index).await?;
                evidence.push(endpoint);
                return Ok(RevealedRemoteKeyOutput {
                    remote_key,
                    full_key: RemoteKeySecret::new(full_key),
                    evidence,
                    diagnostics: RedactedDiagnostics {
                        summary: Some(json!({"revealed": true}).to_string()),
                        raw_json_redacted: None,
                    },
                });
            }

            Err(invalid_request(
                "NewAPI remote key no longer exists; reconcile before creating a local key",
            ))
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
            let website_url = request_website_url(&request.endpoints)?;
            create_remote_key_once(context, &website_url, &request).await?;
            let (items, mut evidence) = fetch_newapi_token_items(context, &website_url).await?;
            for (index, value) in items.iter().enumerate() {
                if !created_token_matches(value, &request.name) {
                    continue;
                }
                let (remote_key, full_key, endpoint) =
                    reveal_full_key_for_token_value(context, &website_url, value, index).await?;
                evidence.push(endpoint);
                return Ok(CreatedRemoteKeyOutput {
                    remote_key,
                    full_key_once: RemoteKeySecret::new(full_key),
                    evidence,
                    diagnostics: RedactedDiagnostics {
                        summary: Some(
                            json!({
                                "reconciledBy": "name",
                                "idempotencyKey": request.idempotency_key.as_deref().unwrap_or("unsupported")
                            })
                            .to_string(),
                        ),
                        raw_json_redacted: None,
                    },
                });
            }

            Err(result_unknown(
                EndpointRole::RemoteKeys,
                None,
                "NewAPI token create succeeded but reconciliation did not find the created token",
            ))
        }
        .boxed()
    }

    fn delete_remote_key<'a>(
        &'a self,
        context: &'a CollectorContext<'a>,
        request: DeleteRemoteKeyRequest,
    ) -> BoxFuture<'a, Result<DeletedRemoteKeyOutput, DriverFailure>> {
        async move {
            validate_remote_key_request(context, &request.station, &request.endpoints)?;
            let website_url = request_website_url(&request.endpoints)?;
            let (items, mut evidence) = fetch_newapi_token_items(context, &website_url).await?;
            let Some(token_id) = token_id_for_remote_key(
                &request.station.station_id,
                &request.remote_key_id,
                &items,
            )?
            else {
                return Ok(DeletedRemoteKeyOutput {
                    keys: parse_remote_key_items(&request.station.station_id, &items),
                    already_absent: true,
                    evidence,
                    diagnostics: RedactedDiagnostics {
                        summary: Some(json!({"alreadyAbsent": true}).to_string()),
                        raw_json_redacted: None,
                    },
                });
            };

            let delete_failure =
                match delete_remote_key_once(context, &website_url, &token_id).await {
                    Ok(endpoint) => {
                        evidence.push(endpoint);
                        None
                    }
                    Err(failure) => Some(failure),
                };
            let reconciliation = fetch_newapi_token_items(context, &website_url).await;
            let (remaining_items, reconciliation_evidence) = match reconciliation {
                Ok(output) => output,
                Err(_) => {
                    return Err(delete_failure.unwrap_or_else(|| {
                        result_unknown(
                            EndpointRole::RemoteKeys,
                            evidence.last().cloned(),
                            "NewAPI token delete was accepted but could not be reconciled",
                        )
                    }));
                }
            };
            evidence.extend(reconciliation_evidence);
            let keys = parse_remote_key_items(&request.station.station_id, &remaining_items);
            if keys.iter().any(|key| key.id == request.remote_key_id) {
                return Err(delete_failure.unwrap_or_else(|| {
                    result_unknown(
                        EndpointRole::RemoteKeys,
                        evidence.last().cloned(),
                        "NewAPI token delete returned success but the token still exists",
                    )
                }));
            }
            Ok(DeletedRemoteKeyOutput {
                keys,
                already_absent: false,
                evidence,
                diagnostics: RedactedDiagnostics {
                    summary: Some(json!({"deleted": true, "reconciled": true}).to_string()),
                    raw_json_redacted: None,
                },
            })
        }
        .boxed()
    }
}

impl AuthorizationDriver for NewApiAuthorizationDriver {
    fn kind(&self) -> ProviderKind {
        ProviderKind::NewApi
    }

    fn validate_authorization<'a>(
        &'a self,
        context: &'a CollectorContext<'a>,
        request: AuthorizationRequest,
    ) -> BoxFuture<'a, Result<AuthorizationOutput, DriverFailure>> {
        async move {
            validate_authorization_request(context, &request)?;
            let website_url = request_website_url(&request.endpoints)?;
            let (payload, endpoint) = execute_json(
                context,
                request.endpoint_role,
                &website_url,
                "/api/user/self",
                true,
            )
            .await?;
            let data = parsers::envelope_data(&payload).map_err(|error| {
                malformed(request.endpoint_role, Some(endpoint.clone()), error.message)
            })?;
            let expected_user_id = newapi_expected_user_id(context)?;
            let observed_user_id = user_id_from_self_data(data).ok_or_else(|| {
                malformed(
                    request.endpoint_role,
                    Some(endpoint.clone()),
                    "NewAPI authorization self probe did not return a user id",
                )
            })?;
            if observed_user_id != expected_user_id {
                return Err(DriverFailure::auth_rejected(
                    FailedEndpoint {
                        role: request.endpoint_role,
                        status_code: endpoint.status_code,
                    },
                    "NewAPI authorization self probe returned a different user id",
                )
                .with_evidence(EvidenceSet::new([endpoint])));
            }
            Ok(AuthorizationOutput {
                status: AuthorizationStatus::Authorized,
                evidence: vec![endpoint],
                diagnostics: RedactedDiagnostics {
                    summary: Some(json!({"validated": true}).to_string()),
                    raw_json_redacted: None,
                },
            })
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

fn validate_remote_key_request(
    context: &CollectorContext<'_>,
    station: &crate::services::collectors::contract::StationIdentity,
    endpoints: &crate::services::collectors::contract::ProviderEndpoints,
) -> Result<(), DriverFailure> {
    if station.provider != ProviderKind::NewApi || context.station.provider != ProviderKind::NewApi
    {
        return Err(invalid_request("remote-key request provider mismatch"));
    }
    if station.station_id != context.station.station_id
        || station.endpoint_revision != context.station.endpoint_revision
    {
        return Err(invalid_request(
            "remote-key request station revision mismatch",
        ));
    }
    if request_website_url(endpoints)? != website_url(context)? {
        return Err(invalid_request("remote-key request endpoint mismatch"));
    }
    Ok(())
}

fn validate_authorization_request(
    context: &CollectorContext<'_>,
    request: &AuthorizationRequest,
) -> Result<(), DriverFailure> {
    if request.station.provider != ProviderKind::NewApi
        || context.station.provider != ProviderKind::NewApi
    {
        return Err(invalid_request("authorization request provider mismatch"));
    }
    if request.station.station_id != context.station.station_id
        || request.station.endpoint_revision != context.station.endpoint_revision
    {
        return Err(invalid_request(
            "authorization request station revision mismatch",
        ));
    }
    if request.credential != context.credential {
        return Err(invalid_request("authorization request credential mismatch"));
    }
    if request_website_url(&request.endpoints)? != website_url(context)? {
        return Err(invalid_request("authorization request endpoint mismatch"));
    }
    Ok(())
}

async fn fetch_newapi_token_items(
    context: &CollectorContext<'_>,
    website_url: &str,
) -> Result<(Vec<Value>, Vec<EndpointEvidence>), DriverFailure> {
    let mut page = 1_usize;
    let mut items = Vec::new();
    let mut evidence = Vec::new();
    let mut expected_total = None;
    loop {
        let path = format!("/api/token/?p={page}&page_size={NEWAPI_REMOTE_KEY_PAGE_SIZE}");
        let (data, endpoint) =
            execute_newapi_data(context, EndpointRole::RemoteKeys, website_url, &path).await?;
        let response_page = numeric_usize_field(&data, &["page"])
            .filter(|value| *value == page)
            .ok_or_else(|| {
                malformed(
                    EndpointRole::RemoteKeys,
                    Some(endpoint.clone()),
                    "NewAPI token pagination is missing a valid page number",
                )
            })?;
        let page_size = page_size_from_payload(&data).ok_or_else(|| {
            malformed(
                EndpointRole::RemoteKeys,
                Some(endpoint.clone()),
                "NewAPI token pagination is missing page_size",
            )
        })?;
        let total = total_from_payload(&data).ok_or_else(|| {
            malformed(
                EndpointRole::RemoteKeys,
                Some(endpoint.clone()),
                "NewAPI token pagination is missing total",
            )
        })?;
        if expected_total.is_some_and(|expected| expected != total) {
            return Err(malformed(
                EndpointRole::RemoteKeys,
                Some(endpoint),
                "NewAPI token pagination total changed between pages",
            ));
        }
        expected_total = Some(total);
        let page_items = remote_key_items(&data)
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        let page_item_count = page_items.len();
        if page_item_count > page_size {
            return Err(malformed(
                EndpointRole::RemoteKeys,
                Some(endpoint),
                "NewAPI token pagination returned more items than page_size",
            ));
        }
        items.extend(page_items);
        evidence.push(endpoint.clone());
        if items.len() > total {
            return Err(malformed(
                EndpointRole::RemoteKeys,
                Some(endpoint),
                "NewAPI token pagination returned more items than total",
            ));
        }
        if items.len() == total {
            break;
        }
        if page_item_count == 0 || page_item_count < page_size {
            return Err(malformed(
                EndpointRole::RemoteKeys,
                Some(endpoint),
                "NewAPI token pagination ended before reaching total",
            ));
        }
        page = response_page.saturating_add(1);
        if page > 1000 {
            return Err(malformed(
                EndpointRole::RemoteKeys,
                None,
                "NewAPI token pagination exceeded the safety limit",
            ));
        }
    }
    Ok((items, evidence))
}

async fn create_remote_key_once(
    context: &CollectorContext<'_>,
    website_url: &str,
    request: &CreateRemoteKeyRequest,
) -> Result<EndpointEvidence, DriverFailure> {
    let mut body = json!({ "name": request.name });
    if let Some(group_name) = request
        .group_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        body["group"] = json!(group_name);
    }
    let (_, endpoint) = execute_json_with_method(
        context,
        EndpointRole::RemoteKeys,
        website_url,
        "/api/token/",
        Method::POST,
        Some(body),
        OutboundRetryPolicy::Never,
        true,
    )
    .await
    .map_err(|failure| match failure.kind {
        DriverFailureKind::Timeout
        | DriverFailureKind::BudgetExhausted
        | DriverFailureKind::Cancelled
        | DriverFailureKind::Transport => result_unknown(
            EndpointRole::RemoteKeys,
            failure.endpoint.as_ref().and_then(|endpoint| {
                endpoint.status_code.map(|status| {
                    EndpointEvidence::new(
                        EndpointRole::RemoteKeys,
                        "POST",
                        None,
                        Some(status),
                        None,
                    )
                })
            }),
            "NewAPI token create outcome is unknown; reconcile before retrying",
        ),
        _ => failure,
    })?;
    Ok(endpoint)
}

async fn delete_remote_key_once(
    context: &CollectorContext<'_>,
    website_url: &str,
    token_id: &str,
) -> Result<EndpointEvidence, DriverFailure> {
    let path = format!("/api/token/{token_id}");
    execute_json_with_method(
        context,
        EndpointRole::RemoteKeys,
        website_url,
        &path,
        Method::DELETE,
        None,
        OutboundRetryPolicy::Never,
        true,
    )
    .await
    .map(|(_, endpoint)| endpoint)
}

async fn reveal_full_key_for_token_value(
    context: &CollectorContext<'_>,
    website_url: &str,
    value: &Value,
    index: usize,
) -> Result<(RemoteStationKey, String, EndpointEvidence), DriverFailure> {
    let mut remote_key = remote_key_from_value(&context.station.station_id, value, index)
        .ok_or_else(|| {
            malformed(
                EndpointRole::RemoteKeys,
                None,
                "NewAPI remote-key response is missing a stable identity",
            )
        })?;
    let token_id = numeric_i64_field(value, &["id"])
        .filter(|value| *value > 0)
        .map(|value| value.to_string())
        .ok_or_else(|| {
            malformed(
                EndpointRole::RemoteKeys,
                None,
                "NewAPI remote-key response is missing token id",
            )
        })?;
    let path = format!("/api/token/{token_id}/key");
    let (payload, endpoint) = execute_json_with_method(
        context,
        EndpointRole::RemoteKeys,
        website_url,
        &path,
        Method::POST,
        Some(json!({})),
        OutboundRetryPolicy::default(),
        true,
    )
    .await?;
    let data = parsers::envelope_data(&payload).map_err(|error| {
        malformed(
            EndpointRole::RemoteKeys,
            Some(endpoint.clone()),
            error.message,
        )
    })?;
    let full_key = full_key_from_reveal_payload(data).ok_or_else(|| {
        malformed(
            EndpointRole::RemoteKeys,
            Some(endpoint.clone()),
            "NewAPI reveal response did not return a full key",
        )
    })?;
    remote_key.api_key_masked = Some(crate::services::secrets::mask::mask_secret(&full_key));
    remote_key.api_key_fingerprint = crate::services::remote_keys::api_key_fingerprint(&full_key);
    Ok((remote_key, full_key, endpoint))
}

fn parse_remote_key_items(station_id: &str, items: &[Value]) -> Vec<RemoteStationKey> {
    items
        .iter()
        .enumerate()
        .filter_map(|(index, value)| remote_key_from_value(station_id, value, index))
        .collect()
}

fn remote_key_items(payload: &Value) -> Vec<&Value> {
    payload
        .get("items")
        .and_then(Value::as_array)
        .map(|items| items.iter().collect())
        .unwrap_or_default()
}

fn created_token_matches(value: &Value, expected_name: &str) -> bool {
    let expected_name = expected_name.trim();
    string_field(value, "name")
        .as_deref()
        .map(str::trim)
        .is_some_and(|name| !expected_name.is_empty() && name == expected_name)
}

fn remote_key_from_value(
    station_id: &str,
    value: &Value,
    index: usize,
) -> Option<RemoteStationKey> {
    let remote_key_id = numeric_i64_field(value, &["id"])
        .filter(|value| *value > 0)
        .map(|value| value.to_string());
    let name = string_field(value, "name");
    let key_value = string_field(value, "key");
    let full_key = key_value
        .as_deref()
        .filter(|value| looks_like_full_api_key(value))
        .map(ToString::to_string);
    let masked = full_key
        .as_deref()
        .map(crate::services::secrets::mask::mask_secret)
        .or_else(|| {
            key_value
                .clone()
                .filter(|value| !looks_like_full_api_key(value))
        });
    let (identity_kind, identity, include_index) = remote_key_identity(
        remote_key_id.as_deref(),
        full_key.as_deref(),
        masked.as_deref(),
        name.as_deref(),
    )?;
    let group_name = string_field(value, "group");
    let group_id_hash = group_name
        .as_deref()
        .map(|group| stable_group_key_hash(station_id, "newapi", None, group));
    let identity_seed = if include_index {
        format!("{station_id}:{identity_kind}:{identity}:{index}")
    } else {
        format!("{station_id}:{identity_kind}:{identity}")
    };

    Some(RemoteStationKey {
        id: format!(
            "newapi-remote-key-{}",
            &sha256_hex(identity_seed.as_bytes())[..16]
        ),
        station_id: station_id.to_string(),
        remote_key_id_hash: remote_key_id
            .as_deref()
            .map(|value| sha256_hex(value.as_bytes())),
        remote_key_name: name,
        api_key_masked: masked,
        api_key_fingerprint: full_key
            .as_deref()
            .and_then(crate::services::remote_keys::api_key_fingerprint),
        group_id_hash,
        group_name,
        tier_label: None,
        rate_multiplier: None,
        rate_source: Some("newapi_tokens".to_string()),
        created_at: numeric_i64_field(value, &["created_time"]).map(|value| value.to_string()),
        last_used_at: numeric_i64_field(value, &["accessed_time"]).map(|value| value.to_string()),
        raw_source: "newapi_tokens".to_string(),
        match_status: RemoteKeyMatchStatus::Unbound,
        matched_station_key_id: None,
        match_confidence: 0.0,
        collected_at: crate::services::time::now_millis_for_services().to_string(),
    })
}

fn token_id_for_remote_key(
    station_id: &str,
    remote_key_id: &str,
    items: &[Value],
) -> Result<Option<String>, DriverFailure> {
    for (index, value) in items.iter().enumerate() {
        let Some(remote_key) = remote_key_from_value(station_id, value, index) else {
            continue;
        };
        if remote_key.id != remote_key_id {
            continue;
        }
        return numeric_i64_field(value, &["id"])
            .filter(|value| *value > 0)
            .map(|value| Some(value.to_string()))
            .ok_or_else(|| {
                invalid_request("NewAPI remote key does not expose a deletable token id")
            });
    }
    Ok(None)
}

fn remote_key_identity<'a>(
    remote_key_id: Option<&'a str>,
    full_key: Option<&'a str>,
    masked: Option<&'a str>,
    name: Option<&'a str>,
) -> Option<(&'static str, &'a str, bool)> {
    remote_key_id
        .map(|value| ("remote_id", value, false))
        .or_else(|| full_key.map(|value| ("full_key", value, false)))
        .or_else(|| masked.map(|value| ("masked_key", value, false)))
        .or_else(|| name.map(|value| ("name", value, true)))
}

fn full_key_from_reveal_payload(payload: &Value) -> Option<String> {
    string_field(payload, "key").filter(|value| looks_like_full_api_key(value))
}

fn page_size_from_payload(payload: &Value) -> Option<usize> {
    payload
        .get("page_size")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| *value > 0)
}

fn total_from_payload(payload: &Value) -> Option<usize> {
    payload
        .get("total")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)?
        .as_str()
        .map(ToString::to_string)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn looks_like_full_api_key(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.len() < 12 {
        return false;
    }
    let lower = trimmed.to_lowercase();
    if lower == "[redacted]"
        || lower == "<redacted>"
        || lower == "redacted"
        || lower == "masked"
        || lower.contains("redacted")
        || lower.contains("masked")
        || trimmed.contains('*')
        || trimmed.contains("...")
    {
        return false;
    }
    !(lower.starts_with("sk-") && lower.contains("xxx"))
}

fn stable_group_key_hash(
    station_id: &str,
    adapter: &str,
    group_id: Option<&str>,
    group_name: &str,
) -> String {
    let adapter = adapter.trim().to_lowercase();
    let source = if let Some(group_id) = group_id.filter(|value| !value.trim().is_empty()) {
        format!("id:{adapter}:{}", group_id.trim())
    } else {
        format!(
            "name:{}:{}:{}",
            station_id,
            adapter,
            group_name.trim().to_lowercase()
        )
    };
    sha256_hex(source.as_bytes())
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

fn request_website_url(
    endpoints: &crate::services::collectors::contract::ProviderEndpoints,
) -> Result<String, DriverFailure> {
    endpoints
        .website_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| invalid_request("NewAPI website URL is missing"))
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

fn user_id_from_self_data(value: &Value) -> Option<String> {
    plain_id_value(value).or_else(|| value.get("user").and_then(plain_id_value))
}

fn plain_id_value(value: &Value) -> Option<String> {
    value
        .as_object()
        .and_then(|map| map.get("id"))
        .and_then(string_or_i64_value)
}

fn string_or_i64_value(value: &Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    value.as_i64().map(|number| number.to_string())
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
    execute_json_with_method(
        context,
        role,
        website_url,
        path,
        Method::GET,
        None,
        OutboundRetryPolicy::default(),
        authenticated,
    )
    .await
}

async fn execute_json_with_method(
    context: &CollectorContext<'_>,
    role: EndpointRole,
    website_url: &str,
    path: &str,
    method: Method,
    body: Option<Value>,
    retry_policy: OutboundRetryPolicy,
    authenticated: bool,
) -> Result<(Value, EndpointEvidence), DriverFailure> {
    let url = build_management_url(website_url, path).map_err(|error| invalid_request(error))?;
    let request = build_json_request(
        context,
        role,
        &url,
        method.clone(),
        body,
        retry_policy,
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
        method.as_str(),
        Some(response.evidence.final_url.clone()),
        Some(response.status.as_u16()),
        None,
    );
    let payload = if response.body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice::<Value>(&response.body)
            .map_err(|error| malformed(role, Some(endpoint.clone()), error.to_string()))?
    };
    if !response.status.is_success() {
        return Err(http_failure(role, response.status, payload, endpoint));
    }
    Ok((payload, endpoint))
}

async fn build_json_request(
    context: &CollectorContext<'_>,
    role: EndpointRole,
    url: &str,
    method: Method,
    body: Option<Value>,
    retry_policy: OutboundRetryPolicy,
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
    let body = match body {
        Some(body) => {
            headers
                .insert_public(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("application/json"),
                    &policy,
                )
                .map_err(|failure| driver_failure_from_outbound(role, failure))?;
            serde_json::to_vec(&body).map_err(|error| malformed(role, None, error.to_string()))?
        }
        None => Vec::new(),
    };
    if authenticated {
        let (user_id, secret_purpose) = match newapi_auth(context)? {
            ProviderAuthContext::NewApi {
                user_id,
                secret_purpose,
            } => (user_id, secret_purpose),
            ProviderAuthContext::Sub2Api { .. } => {
                return Err(invalid_request(
                    "NewAPI auth context has the wrong provider",
                ));
            }
        };
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
        method,
        url: url.to_string(),
        correlation_id,
        headers,
        body,
        proxy: context.proxy.clone(),
        budget: context.budget,
        retry_policy,
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

fn newapi_expected_user_id(context: &CollectorContext<'_>) -> Result<String, DriverFailure> {
    let user_id = match newapi_auth(context)? {
        ProviderAuthContext::NewApi { user_id, .. } => user_id,
        ProviderAuthContext::Sub2Api { .. } => {
            return Err(invalid_request(
                "NewAPI auth context has the wrong provider",
            ));
        }
    };
    let trimmed = user_id.trim();
    if trimmed.is_empty() {
        return Err(invalid_request("NewAPI user id is missing"));
    }
    Ok(trimmed.to_string())
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
    use crate::{
        outbound::{AsyncOutboundClient, AsyncOutboundClientConfig, ProxyPolicy, RequestBudget},
        services::collectors::{
            contract::{
                CredentialScope, OpaqueCredentialHandle, ProviderEndpoints, StationIdentity,
            },
            drivers::newapi::test_support::{json_response, TestHttpServer},
        },
    };
    use futures_util::FutureExt;
    use serde_json::json;
    use std::time::Duration;
    use tokio_util::sync::CancellationToken;

    struct TestSecretAccessor(&'static str);

    impl crate::services::collectors::contract::DriverSecretAccessor for TestSecretAccessor {
        fn resolve_secret<'a>(
            &'a self,
            _handle: &'a crate::services::collectors::contract::OpaqueCredentialHandle,
            _purpose: CredentialSecretPurpose,
        ) -> BoxFuture<
            'a,
            Result<crate::services::collectors::contract::CredentialSecret, DriverFailure>,
        > {
            async move { Ok(crate::services::collectors::contract::CredentialSecret::new(self.0)) }
                .boxed()
        }
    }

    fn test_station_identity() -> StationIdentity {
        StationIdentity {
            station_id: "station-1".to_string(),
            endpoint_revision: 7,
            provider: ProviderKind::NewApi,
        }
    }

    fn test_credential() -> OpaqueCredentialHandle {
        OpaqueCredentialHandle {
            station_id: "station-1".to_string(),
            credential_revision: 7,
            scope: CredentialScope::LoginSession,
        }
    }

    fn test_endpoints(base_url: &str) -> ProviderEndpoints {
        ProviderEndpoints {
            api_base_url: None,
            website_url: Some(base_url.to_string()),
        }
    }

    fn test_context<'a>(
        base_url: &str,
        secrets: &'a TestSecretAccessor,
        outbound: &'a AsyncOutboundClient,
    ) -> CollectorContext<'a> {
        CollectorContext {
            station: test_station_identity(),
            endpoints: test_endpoints(base_url),
            credential: test_credential(),
            auth: Some(ProviderAuthContext::NewApi {
                user_id: "42".to_string(),
                secret_purpose: CredentialSecretPurpose::AuthorizationHeader,
            }),
            secrets,
            outbound,
            proxy: ProxyPolicy::Direct,
            budget: RequestBudget::from_now(Duration::from_secs(5)),
            cancellation: CancellationToken::new(),
            correlation_id: "test-correlation".to_string(),
        }
    }

    fn remote_key_request(base_url: &str) -> RemoteKeyRequest {
        RemoteKeyRequest {
            station: test_station_identity(),
            endpoints: test_endpoints(base_url),
            credential: test_credential(),
        }
    }

    fn create_remote_key_request(base_url: &str, name: &str) -> CreateRemoteKeyRequest {
        CreateRemoteKeyRequest {
            station: test_station_identity(),
            endpoints: test_endpoints(base_url),
            credential: test_credential(),
            name: name.to_string(),
            provider_group_id: None,
            group_name: Some("vip".to_string()),
            idempotency_key: None,
        }
    }

    fn delete_remote_key_request(base_url: &str, remote_key_id: String) -> DeleteRemoteKeyRequest {
        DeleteRemoteKeyRequest {
            station: test_station_identity(),
            endpoints: test_endpoints(base_url),
            credential: test_credential(),
            remote_key_id,
        }
    }

    async fn collect_balance_from_test_server(server: &TestHttpServer) -> DriverOutput {
        let outbound = AsyncOutboundClient::new(AsyncOutboundClientConfig::architecture_budget());
        let secrets = TestSecretAccessor("newapi-access-token");
        let context = test_context(&server.base_url, &secrets, &outbound);

        NewApiCollectorDriver
            .collect(&context, CollectorTaskKind::Balance)
            .await
            .expect("balance collect")
    }

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

    #[test]
    fn remote_key_parser_keeps_masked_keys_without_fingerprinting() {
        let key = remote_key_from_value(
            "station-1",
            &json!({
                "id": 101,
                "name": "primary",
                "key": "sk-abc**********7890",
                "group": "vip",
                "created_time": 1760000000,
                "accessed_time": 1760000100
            }),
            0,
        )
        .expect("remote key");

        assert_eq!(key.remote_key_name.as_deref(), Some("primary"));
        assert_eq!(key.api_key_masked.as_deref(), Some("sk-abc**********7890"));
        assert_eq!(key.api_key_fingerprint, None);
        assert_eq!(key.group_name.as_deref(), Some("vip"));
        assert_eq!(key.raw_source, "newapi_tokens");
    }

    #[tokio::test]
    async fn list_remote_keys_paginates_newapi_tokens_without_full_secret() {
        let server = TestHttpServer::sequence(vec![
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": {
                        "page": 1,
                        "page_size": 2,
                        "total": 3,
                        "items": [
                            {
                                "id": 101,
                                "name": "primary",
                                "key": "sk-abc**********7890",
                                "group": "default",
                                "created_time": 1760000000,
                                "accessed_time": 1760000100
                            },
                            {
                                "id": 102,
                                "name": "secondary",
                                "key": "sk-def**********4567",
                                "group": "vip"
                            }
                        ]
                    }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": {
                        "page": 2,
                        "page_size": 2,
                        "total": 3,
                        "items": [
                            {
                                "id": 103,
                                "name": "third",
                                "key": "sk-ghi**********1234",
                                "group": "vip"
                            }
                        ]
                    }
                }),
            )),
        ]);
        let outbound = AsyncOutboundClient::new(AsyncOutboundClientConfig::architecture_budget());
        let secrets = TestSecretAccessor("newapi-access-token");
        let context = test_context(&server.base_url, &secrets, &outbound);

        let output = NewApiRemoteKeyDriver
            .list_remote_keys(&context, remote_key_request(&server.base_url))
            .await
            .expect("scan keys");
        let requests = server.finish();

        assert_eq!(output.keys.len(), 3);
        assert_eq!(output.keys[0].remote_key_name.as_deref(), Some("primary"));
        assert_eq!(
            output.keys[0].api_key_masked.as_deref(),
            Some("sk-abc**********7890")
        );
        assert_eq!(output.keys[0].api_key_fingerprint, None);
        assert_eq!(output.keys[0].group_name.as_deref(), Some("default"));
        assert_eq!(output.keys[0].raw_source, "newapi_tokens");
        assert_eq!(output.evidence.len(), 2);
        assert!(requests[0].starts_with("GET /api/token/?p=1&page_size=100 "));
        assert!(requests[1].starts_with("GET /api/token/?p=2&page_size=100 "));
        assert!(requests[0]
            .to_ascii_lowercase()
            .contains("new-api-user: 42"));
    }

    #[tokio::test]
    async fn delete_remote_key_resolves_provider_id_and_reconciles_absence() {
        let item = json!({
            "id": 301,
            "name": "relay-delete",
            "key": "sk-del**********f260",
            "group": "vip"
        });
        let remote_key_id = remote_key_from_value("station-1", &item, 0)
            .expect("remote key")
            .id;
        let server = TestHttpServer::sequence(vec![
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": { "page": 1, "page_size": 100, "total": 1, "items": [item] }
                }),
            )),
            Some(json_response(200, json!({ "success": true }))),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": { "page": 1, "page_size": 100, "total": 0, "items": [] }
                }),
            )),
        ]);
        let outbound = AsyncOutboundClient::new(AsyncOutboundClientConfig::architecture_budget());
        let secrets = TestSecretAccessor("newapi-access-token");
        let context = test_context(&server.base_url, &secrets, &outbound);

        let output = NewApiRemoteKeyDriver
            .delete_remote_key(
                &context,
                delete_remote_key_request(&server.base_url, remote_key_id),
            )
            .await
            .expect("delete remote key");
        let requests = server.finish();

        assert!(!output.already_absent);
        assert!(output.keys.is_empty());
        assert_eq!(requests.len(), 3);
        assert!(requests[0].starts_with("GET /api/token/?p=1&page_size=100 "));
        assert!(requests[1].starts_with("DELETE /api/token/301 "));
        assert!(requests[2].starts_with("GET /api/token/?p=1&page_size=100 "));
    }

    #[tokio::test]
    async fn delete_remote_key_is_idempotent_when_target_is_already_absent() {
        let server = TestHttpServer::sequence(vec![Some(json_response(
            200,
            json!({
                "success": true,
                "data": { "page": 1, "page_size": 100, "total": 0, "items": [] }
            }),
        ))]);
        let outbound = AsyncOutboundClient::new(AsyncOutboundClientConfig::architecture_budget());
        let secrets = TestSecretAccessor("newapi-access-token");
        let context = test_context(&server.base_url, &secrets, &outbound);

        let output = NewApiRemoteKeyDriver
            .delete_remote_key(
                &context,
                delete_remote_key_request(&server.base_url, "missing-discovery-id".to_string()),
            )
            .await
            .expect("already absent is success");
        let requests = server.finish();

        assert!(output.already_absent);
        assert_eq!(requests.len(), 1);
        assert!(requests[0].starts_with("GET /api/token/?p=1&page_size=100 "));
    }

    #[tokio::test]
    async fn delete_remote_key_reports_unknown_when_reconciliation_still_contains_target() {
        let item = json!({
            "id": 302,
            "name": "relay-still-present",
            "key": "sk-stl**********f260"
        });
        let remote_key_id = remote_key_from_value("station-1", &item, 0)
            .expect("remote key")
            .id;
        let list_response = || {
            json_response(
                200,
                json!({
                    "success": true,
                    "data": { "page": 1, "page_size": 100, "total": 1, "items": [item.clone()] }
                }),
            )
        };
        let server = TestHttpServer::sequence(vec![
            Some(list_response()),
            Some(json_response(200, json!({ "success": true }))),
            Some(list_response()),
        ]);
        let outbound = AsyncOutboundClient::new(AsyncOutboundClientConfig::architecture_budget());
        let secrets = TestSecretAccessor("newapi-access-token");
        let context = test_context(&server.base_url, &secrets, &outbound);

        let result = NewApiRemoteKeyDriver
            .delete_remote_key(
                &context,
                delete_remote_key_request(&server.base_url, remote_key_id),
            )
            .await;
        let error = match result {
            Ok(_) => panic!("reconciliation should report an unknown result"),
            Err(error) => error,
        };
        let requests = server.finish();

        assert_eq!(error.kind, DriverFailureKind::ResultUnknown);
        assert_eq!(requests.len(), 3);
    }

    #[tokio::test]
    async fn token_scan_rejects_missing_pagination_metadata() {
        let server = TestHttpServer::sequence(vec![Some(json_response(
            200,
            json!({
                "success": true,
                "data": {
                    "page": 1,
                    "page_size": 100,
                    "items": [
                        { "id": 101, "name": "primary", "key": "sk-abc**********7890" }
                    ]
                }
            }),
        ))]);
        let outbound = AsyncOutboundClient::new(AsyncOutboundClientConfig::architecture_budget());
        let secrets = TestSecretAccessor("newapi-access-token");
        let context = test_context(&server.base_url, &secrets, &outbound);

        let error = NewApiRemoteKeyDriver
            .list_remote_keys(&context, remote_key_request(&server.base_url))
            .await
            .unwrap_err();
        server.finish();

        assert_eq!(error.kind, DriverFailureKind::MalformedPayload);
        assert!(error
            .sanitized_detail
            .as_deref()
            .unwrap_or_default()
            .contains("pagination"));
    }

    #[tokio::test]
    async fn list_remote_keys_errors_before_returning_partial_pages() {
        let server = TestHttpServer::sequence(vec![
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": {
                        "page": 1,
                        "page_size": 2,
                        "total": 3,
                        "items": [
                            { "id": 101, "name": "primary", "key": "sk-abc**********7890" },
                            { "id": 102, "name": "secondary", "key": "sk-def**********4567" }
                        ]
                    }
                }),
            )),
            Some(json_response(
                502,
                json!({"success": false, "message": "bad gateway"}),
            )),
            Some(json_response(
                502,
                json!({"success": false, "message": "bad gateway"}),
            )),
        ]);
        let outbound = AsyncOutboundClient::new(AsyncOutboundClientConfig::architecture_budget());
        let secrets = TestSecretAccessor("newapi-access-token");
        let context = test_context(&server.base_url, &secrets, &outbound);

        let error = NewApiRemoteKeyDriver
            .list_remote_keys(&context, remote_key_request(&server.base_url))
            .await
            .unwrap_err();
        let requests = server.finish();

        assert_eq!(error.kind, DriverFailureKind::ProviderUnavailable);
        assert_eq!(requests.len(), 2);
        assert!(requests[1].starts_with("GET /api/token/?p=2&page_size=100 "));
    }

    #[tokio::test]
    async fn create_remote_key_posts_token_then_reconciles_and_reveals_secret() {
        let server = TestHttpServer::sequence(vec![
            Some(json_response(200, json!({"success": true, "message": ""}))),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": {
                        "page": 1,
                        "page_size": 100,
                        "total": 1,
                        "items": [{
                            "id": 301,
                            "name": "relay-created",
                            "key": "sk-crt**********f260",
                            "group": "vip"
                        }]
                    }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": { "key": "sk-created-secret-f260" }
                }),
            )),
        ]);
        let outbound = AsyncOutboundClient::new(AsyncOutboundClientConfig::architecture_budget());
        let secrets = TestSecretAccessor("newapi-access-token");
        let context = test_context(&server.base_url, &secrets, &outbound);

        let created = NewApiRemoteKeyDriver
            .create_remote_key(
                &context,
                create_remote_key_request(&server.base_url, "relay-created"),
            )
            .await
            .expect("created remote key");
        let requests = server.finish();

        assert_eq!(
            created.remote_key.remote_key_name.as_deref(),
            Some("relay-created")
        );
        assert_eq!(created.remote_key.group_name.as_deref(), Some("vip"));
        assert_eq!(created.full_key_once.expose(), "sk-created-secret-f260");
        assert!(requests[0].starts_with("POST /api/token/ "));
        assert!(requests[0].contains("\"name\":\"relay-created\""));
        assert!(requests[0].contains("\"group\":\"vip\""));
        assert!(requests[1].starts_with("GET /api/token/?p=1&page_size=100 "));
        assert!(requests[2].starts_with("POST /api/token/301/key "));
    }

    #[tokio::test]
    async fn collect_groups_marks_empty_successful_payload_partial() {
        let server = TestHttpServer::sequence(vec![Some(json_response(
            200,
            json!({"success": true, "data": {}}),
        ))]);
        let outbound = AsyncOutboundClient::new(AsyncOutboundClientConfig::architecture_budget());
        let secrets = TestSecretAccessor("newapi-access-token");
        let context = test_context(&server.base_url, &secrets, &outbound);

        let output = NewApiCollectorDriver
            .collect(&context, CollectorTaskKind::Groups)
            .await
            .expect("groups collect");
        server.finish();

        assert_eq!(output.status, DriverOutputStatus::Partial);
        assert!(output.facts.groups.is_empty());
        assert!(output.facts.rates.is_empty());
    }

    #[tokio::test]
    async fn collect_models_keeps_top_level_models_contract() {
        let server = TestHttpServer::sequence(vec![Some(json_response(
            200,
            json!({"success": true, "data": ["gpt-4.1-mini", "claude-sonnet"]}),
        ))]);
        let outbound = AsyncOutboundClient::new(AsyncOutboundClientConfig::architecture_budget());
        let secrets = TestSecretAccessor("newapi-access-token");
        let context = test_context(&server.base_url, &secrets, &outbound);

        let output = NewApiCollectorDriver
            .collect(&context, CollectorTaskKind::Models)
            .await
            .expect("models collect");
        server.finish();

        assert_eq!(output.status, DriverOutputStatus::Success);
        assert_eq!(
            output
                .facts
                .models
                .iter()
                .map(|model| model.model.as_str())
                .collect::<Vec<_>>(),
            vec!["gpt-4.1-mini", "claude-sonnet"]
        );
    }

    #[tokio::test]
    async fn newapi_balance_collects_usage_logs_for_request_count_cost_and_total_tokens() {
        let server = TestHttpServer::sequence(vec![
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": { "quota_per_unit": 500000 }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": {
                        "quota": 1000000,
                        "used_quota": 9250000,
                        "request_count": 1200
                    }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": [
                        { "count": 2, "quota": 375000, "token_used": 49567 }
                    ]
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": [
                        { "count": 1200, "quota": 9250000, "token_used": 422890 }
                    ]
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": { "quota": 375000, "rpm": 2, "tpm": 0 }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": {
                        "page": 1,
                        "page_size": 100,
                        "total": 2,
                        "items": [
                            { "prompt_tokens": 30000, "completion_tokens": 4567 },
                            { "prompt_tokens": 10000, "completion_tokens": 5000 }
                        ]
                    }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": { "quota": 9250000, "rpm": 3, "tpm": 0 }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": {
                        "page": 1,
                        "page_size": 100,
                        "total": 3,
                        "items": [
                            { "prompt_tokens": 30000, "completion_tokens": 4567 },
                            { "prompt_tokens": 10000, "completion_tokens": 5000 },
                            { "prompt_tokens": 250000, "completion_tokens": 123323 }
                        ]
                    }
                }),
            )),
        ]);

        let output = collect_balance_from_test_server(&server).await;
        let requests = server.finish();
        let balance = output.facts.balances.first().expect("balance fact");

        assert_eq!(output.status, DriverOutputStatus::Success);
        assert_eq!(balance.today_request_count, Some(2));
        assert_eq!(balance.total_request_count, Some(1200));
        assert_eq!(balance.today_consumption, Some(0.75));
        assert_eq!(balance.total_consumption, Some(18.5));
        assert_eq!(balance.today_token_count, Some(49567));
        assert_eq!(balance.today_input_token_count, None);
        assert_eq!(balance.today_output_token_count, None);
        assert_eq!(balance.total_token_count, Some(422890));
        assert_eq!(balance.total_input_token_count, None);
        assert_eq!(balance.total_output_token_count, None);
        assert!(requests
            .iter()
            .any(|request| request.starts_with("GET /api/log/self/stat?type=2&")));
        assert!(requests
            .iter()
            .any(|request| request.starts_with("GET /api/log/self?p=1&page_size=100&type=2&")));
    }

    #[tokio::test]
    async fn newapi_balance_does_not_treat_used_quota_as_tokens_in_token_display_mode() {
        let server = TestHttpServer::sequence(vec![
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": {
                        "quota_per_unit": 500000,
                        "quota_display_type": "TOKENS"
                    }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": {
                        "quota": 1000000,
                        "used_quota": 9250000,
                        "request_count": 1200
                    }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": [
                        { "count": 2, "quota": 375000, "token_used": 175200000 }
                    ]
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": [
                        { "count": 1200, "quota": 9250000, "token_used": 470000000 }
                    ]
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": { "quota": 375000, "rpm": 2, "tpm": 0 }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": {
                        "page": 1,
                        "page_size": 100,
                        "total": 2,
                        "items": [
                            { "prompt_tokens": 100000000, "completion_tokens": 50000000 },
                            { "prompt_tokens": 20000000, "completion_tokens": 5200000 }
                        ]
                    }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": { "quota": 9250000, "rpm": 3, "tpm": 0 }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": {
                        "page": 1,
                        "page_size": 100,
                        "total": 10000,
                        "items": [
                            { "prompt_tokens": 100000000, "completion_tokens": 50000000 },
                            { "prompt_tokens": 20000000, "completion_tokens": 5200000 }
                        ]
                    }
                }),
            )),
        ]);

        let output = collect_balance_from_test_server(&server).await;
        let requests = server.finish();
        let balance = output.facts.balances.first().expect("balance fact");

        assert_eq!(output.status, DriverOutputStatus::Success);
        assert_eq!(balance.today_token_count, Some(175200000));
        assert_eq!(balance.total_token_count, Some(470000000));
        assert_eq!(
            requests
                .iter()
                .filter(|request| request.starts_with("GET /api/data/self?"))
                .count(),
            2
        );
        assert!(requests
            .iter()
            .any(|request| request.starts_with("GET /api/data/self?")));
    }

    #[tokio::test]
    async fn newapi_balance_does_not_treat_recent_dashboard_window_as_total() {
        let server = TestHttpServer::sequence(vec![
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": { "quota_per_unit": 500000 }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": {
                        "quota": 1000000,
                        "used_quota": 1000000,
                        "request_count": 1200
                    }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": [
                        { "count": 2, "quota": 300000, "token_used": 111000000 }
                    ]
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": false,
                    "message": "时间跨度不能超过 1 个月"
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": [
                        { "count": 3, "quota": 300000, "token_used": 111000000 }
                    ]
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": [
                        { "count": 7, "quota": 700000, "token_used": 359000000 }
                    ]
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": { "quota": 300000, "rpm": 2, "tpm": 999999 }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": {
                        "page": 1,
                        "page_size": 100,
                        "total": 0,
                        "items": []
                    }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": { "quota": 1000000, "rpm": 2, "tpm": 888888 }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": {
                        "page": 1,
                        "page_size": 100,
                        "total": 10000,
                        "items": [
                            { "prompt_tokens": 111000000, "completion_tokens": 0 }
                        ]
                    }
                }),
            )),
        ]);

        let output = collect_balance_from_test_server(&server).await;
        let requests = server.finish();
        let balance = output.facts.balances.first().expect("balance fact");

        assert_eq!(output.status, DriverOutputStatus::Success);
        assert_eq!(balance.today_token_count, Some(111000000));
        assert_eq!(balance.total_token_count, None);
        assert_eq!(balance.total_consumption, Some(2.0));
        let dashboard_requests = requests
            .iter()
            .filter(|request| request.starts_with("GET /api/data/self?"))
            .count();
        assert!(dashboard_requests >= 2);
    }

    #[tokio::test]
    async fn newapi_balance_rejects_partial_dashboard_total_token_data() {
        let created_at = unix_now_seconds().saturating_sub(3600);
        let server = TestHttpServer::sequence(vec![
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": { "quota_per_unit": 500000 }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": {
                        "quota": 1000000,
                        "used_quota": 1000000,
                        "created_at": created_at,
                        "request_count": 1200
                    }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": [
                        { "count": 2, "quota": 300000, "token_used": 111000000 }
                    ]
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": [
                        { "count": 2, "quota": 300000, "token_used": 111000000 }
                    ]
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": { "quota": 300000, "rpm": 2, "tpm": 999999 }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": {
                        "page": 1,
                        "page_size": 100,
                        "total": 0,
                        "items": []
                    }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": { "quota": 300000, "rpm": 2, "tpm": 888888 }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": {
                        "page": 1,
                        "page_size": 100,
                        "total": 10000,
                        "items": [
                            { "prompt_tokens": 111000000, "completion_tokens": 0 }
                        ]
                    }
                }),
            )),
        ]);

        let output = collect_balance_from_test_server(&server).await;
        let balance = output.facts.balances.first().expect("balance fact");

        assert_eq!(output.status, DriverOutputStatus::Success);
        assert_eq!(balance.today_token_count, Some(111000000));
        assert_eq!(balance.total_request_count, Some(1200));
        assert_eq!(balance.total_consumption, Some(2.0));
        assert_eq!(balance.total_token_count, None);
    }

    #[tokio::test]
    async fn newapi_balance_rejects_dashboard_total_when_self_used_quota_is_zero() {
        let created_at = unix_now_seconds().saturating_sub(3600);
        let server = TestHttpServer::sequence(vec![
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": { "quota_per_unit": 500000 }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": {
                        "quota": 1000000,
                        "used_quota": 0,
                        "created_at": created_at,
                        "request_count": 0
                    }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": []
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": []
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": []
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": [
                        { "count": 4, "quota": 500000, "token_used": 123456789 }
                    ]
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": { "quota": 0, "rpm": 0, "tpm": 0 }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": {
                        "page": 1,
                        "page_size": 100,
                        "total": 0,
                        "items": []
                    }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": { "quota": 0, "rpm": 0, "tpm": 0 }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": {
                        "page": 1,
                        "page_size": 100,
                        "total": 0,
                        "items": []
                    }
                }),
            )),
        ]);

        let output = collect_balance_from_test_server(&server).await;
        let balance = output.facts.balances.first().expect("balance fact");

        assert_eq!(output.status, DriverOutputStatus::Success);
        assert_eq!(balance.total_consumption, Some(0.0));
        assert_eq!(balance.total_request_count, Some(0));
        assert_eq!(balance.total_token_count, None);
    }

    #[tokio::test]
    async fn newapi_balance_leaves_tokens_unknown_when_logs_have_no_token_counts() {
        let server = TestHttpServer::sequence(vec![
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": { "quota_per_unit": 500000 }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": {
                        "quota": 1000000,
                        "used_quota": 0,
                        "request_count": 0
                    }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": []
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": []
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": { "quota": 0, "rpm": 0, "tpm": 54321 }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": {
                        "page": 1,
                        "page_size": 100,
                        "total": 0,
                        "items": []
                    }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": { "quota": 0, "rpm": 0, "tpm": 987654 }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": {
                        "page": 1,
                        "page_size": 100,
                        "total": 0,
                        "items": []
                    }
                }),
            )),
        ]);

        let output = collect_balance_from_test_server(&server).await;
        let balance = output.facts.balances.first().expect("balance fact");

        assert_eq!(output.status, DriverOutputStatus::Success);
        assert_eq!(balance.today_request_count, Some(0));
        assert_eq!(balance.today_consumption, Some(0.0));
        assert_eq!(balance.today_input_token_count, None);
        assert_eq!(balance.today_output_token_count, None);
        assert_eq!(balance.today_token_count, None);
        assert_eq!(balance.total_request_count, Some(0));
        assert_eq!(balance.total_consumption, Some(0.0));
        assert_eq!(balance.total_input_token_count, None);
        assert_eq!(balance.total_output_token_count, None);
        assert_eq!(balance.total_token_count, None);
    }

    #[tokio::test]
    async fn truncated_log_window_does_not_report_exact_request_count() {
        let server = TestHttpServer::sequence(vec![Some(json_response(
            200,
            json!({
                "success": true,
                "data": {
                    "page": 1,
                    "page_size": 100,
                    "total": 10000,
                    "items": [
                        { "prompt_tokens": 10, "completion_tokens": 5 }
                    ]
                }
            }),
        ))]);
        let outbound = AsyncOutboundClient::new(AsyncOutboundClientConfig::architecture_budget());
        let secrets = TestSecretAccessor("newapi-access-token");
        let context = test_context(&server.base_url, &secrets, &outbound);

        let (window, _) = collect_log_window(&context, &server.base_url, 0, 1)
            .await
            .expect("log window");
        server.finish();

        assert_eq!(window.request_count, None);
        assert_eq!(window.input_token_count, None);
        assert_eq!(window.output_token_count, None);
    }

    #[tokio::test]
    async fn log_window_with_missing_token_field_keeps_token_totals_unknown() {
        let server = TestHttpServer::sequence(vec![Some(json_response(
            200,
            json!({
                "success": true,
                "data": {
                    "page": 1,
                    "page_size": 100,
                    "total": 1,
                    "items": [
                        { "prompt_tokens": 10 }
                    ]
                }
            }),
        ))]);
        let outbound = AsyncOutboundClient::new(AsyncOutboundClientConfig::architecture_budget());
        let secrets = TestSecretAccessor("newapi-access-token");
        let context = test_context(&server.base_url, &secrets, &outbound);

        let (window, _) = collect_log_window(&context, &server.base_url, 0, 1)
            .await
            .expect("log window");
        server.finish();

        assert_eq!(window.request_count, Some(1));
        assert_eq!(window.input_token_count, None);
        assert_eq!(window.output_token_count, None);
    }

    #[tokio::test]
    async fn log_window_rejects_missing_standard_total() {
        let server = TestHttpServer::sequence(vec![Some(json_response(
            200,
            json!({
                "success": true,
                "data": {
                    "page": 1,
                    "page_size": 100,
                    "items": [
                        { "prompt_tokens": 10, "completion_tokens": 5 }
                    ]
                }
            }),
        ))]);
        let outbound = AsyncOutboundClient::new(AsyncOutboundClientConfig::architecture_budget());
        let secrets = TestSecretAccessor("newapi-access-token");
        let context = test_context(&server.base_url, &secrets, &outbound);

        let error = collect_log_window(&context, &server.base_url, 0, 1)
            .await
            .unwrap_err();
        server.finish();

        assert_eq!(error.kind, DriverFailureKind::MalformedPayload);
        assert!(error
            .sanitized_detail
            .as_deref()
            .unwrap_or_default()
            .contains("pagination"));
    }

    #[tokio::test]
    async fn log_stat_without_quota_keeps_consumption_unknown() {
        let server = TestHttpServer::sequence(vec![Some(json_response(
            200,
            json!({
                "success": true,
                "data": { "rpm": 1, "tpm": 25 }
            }),
        ))]);
        let outbound = AsyncOutboundClient::new(AsyncOutboundClientConfig::architecture_budget());
        let secrets = TestSecretAccessor("newapi-access-token");
        let context = test_context(&server.base_url, &secrets, &outbound);

        let (window, _) = collect_log_stat_window(&context, &server.base_url, 0, 1, Some(500000.0))
            .await
            .expect("log stat window");
        server.finish();

        assert_eq!(window.consumption, None);
    }

    #[tokio::test]
    async fn log_stat_does_not_guess_nonstandard_base_consumption() {
        let server = TestHttpServer::sequence(vec![Some(json_response(
            200,
            json!({
                "success": true,
                "data": { "quota": 500000, "base_cost": 9.5 }
            }),
        ))]);
        let outbound = AsyncOutboundClient::new(AsyncOutboundClientConfig::architecture_budget());
        let secrets = TestSecretAccessor("newapi-access-token");
        let context = test_context(&server.base_url, &secrets, &outbound);

        let (window, _) = collect_log_stat_window(&context, &server.base_url, 0, 1, Some(500000.0))
            .await
            .expect("log stat window");
        server.finish();

        assert_eq!(window.consumption, Some(1.0));
        assert_eq!(window.base_consumption, None);
    }

    #[tokio::test]
    async fn dashboard_window_does_not_sum_partial_rows() {
        let server = TestHttpServer::sequence(vec![Some(json_response(
            200,
            json!({
                "success": true,
                "data": [
                    { "count": 2, "quota": 300000, "token_used": 100 },
                    { "count": 3, "quota": 400000 }
                ]
            }),
        ))]);
        let outbound = AsyncOutboundClient::new(AsyncOutboundClientConfig::architecture_budget());
        let secrets = TestSecretAccessor("newapi-access-token");
        let context = test_context(&server.base_url, &secrets, &outbound);

        let (window, _) =
            collect_dashboard_usage_window(&context, &server.base_url, 0, 1, Some(500000.0))
                .await
                .expect("dashboard window");
        server.finish();

        assert_eq!(window.request_count, Some(5));
        assert_eq!(window.quota, Some(700000));
        assert_eq!(window.consumption, Some(1.4));
        assert_eq!(window.token_count, None);
    }

    #[tokio::test]
    async fn log_window_rejects_negative_token_values() {
        let server = TestHttpServer::sequence(vec![Some(json_response(
            200,
            json!({
                "success": true,
                "data": {
                    "page": 1,
                    "page_size": 100,
                    "total": 1,
                    "items": [
                        { "prompt_tokens": -1, "completion_tokens": 5 }
                    ]
                }
            }),
        ))]);
        let outbound = AsyncOutboundClient::new(AsyncOutboundClientConfig::architecture_budget());
        let secrets = TestSecretAccessor("newapi-access-token");
        let context = test_context(&server.base_url, &secrets, &outbound);

        let (window, _) = collect_log_window(&context, &server.base_url, 0, 1)
            .await
            .expect("log window");
        server.finish();

        assert_eq!(window.input_token_count, None);
        assert_eq!(window.output_token_count, None);
    }

    #[tokio::test]
    async fn dashboard_total_searches_past_empty_recent_windows() {
        let server = TestHttpServer::sequence(vec![
            Some(json_response(200, json!({ "success": true, "data": [] }))),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": [
                        { "count": 12, "quota": 900000, "token_used": 456789 }
                    ]
                }),
            )),
        ]);
        let outbound = AsyncOutboundClient::new(AsyncOutboundClientConfig::architecture_budget());
        let secrets = TestSecretAccessor("newapi-access-token");
        let context = test_context(&server.base_url, &secrets, &outbound);

        let (total, _) = collect_dashboard_usage_total_backwards(
            &context,
            &server.base_url,
            unix_now_seconds(),
            Some(500000.0),
            Some(NewApiDashboardTotalTarget {
                request_count: 12,
                quota: 900000,
            }),
        )
        .await
        .expect("dashboard total");
        server.finish();

        assert_eq!(total.request_count, Some(12));
        assert_eq!(total.quota, Some(900000));
        assert_eq!(total.token_count, Some(456789));
    }

    #[test]
    fn reveal_payload_requires_top_level_full_key() {
        assert_eq!(
            full_key_from_reveal_payload(&json!({"key": "sk-created-secret-f260"})),
            Some("sk-created-secret-f260".to_string())
        );
        assert_eq!(
            full_key_from_reveal_payload(&json!({"data": {"key": "sk-nested-secret-f260"}})),
            None
        );
        assert_eq!(
            full_key_from_reveal_payload(&json!({"key": "sk-created**********f260"})),
            None
        );
    }

    #[test]
    fn token_parsers_reject_nonstandard_wrappers_and_aliases() {
        assert!(remote_key_items(&json!({
            "tokens": [{ "id": 1, "name": "wrong-wrapper" }]
        }))
        .is_empty());
        assert!(remote_key_from_value(
            "station-1",
            &json!({
                "tokenId": 1,
                "keyName": "wrong-alias",
                "apiKey": "sk-abc**********7890"
            }),
            0,
        )
        .is_none());
        assert_eq!(
            full_key_from_reveal_payload(&json!({
                "data": { "key": "sk-nested-secret-value" }
            })),
            None,
        );
    }

    #[test]
    fn status_payload_rejects_failed_envelope() {
        let error = parsers::envelope_data(&json!({
            "success": false,
            "message": "status unavailable",
            "quota_per_unit": 500000
        }))
        .unwrap_err();

        assert_eq!(error.message, "status unavailable");
    }

    #[tokio::test]
    async fn create_token_request_disables_status_retry() {
        let outbound = AsyncOutboundClient::new(AsyncOutboundClientConfig::architecture_budget());
        let secrets = TestSecretAccessor("newapi-access-token");
        let credential = crate::services::collectors::contract::OpaqueCredentialHandle {
            station_id: "station-1".to_string(),
            credential_revision: 7,
            scope: CredentialScope::LoginSession,
        };
        let context = CollectorContext {
            station: StationIdentity {
                station_id: "station-1".to_string(),
                endpoint_revision: 7,
                provider: ProviderKind::NewApi,
            },
            endpoints: ProviderEndpoints {
                api_base_url: None,
                website_url: Some("https://newapi.example".to_string()),
            },
            credential,
            auth: Some(ProviderAuthContext::NewApi {
                user_id: "42".to_string(),
                secret_purpose: CredentialSecretPurpose::AuthorizationHeader,
            }),
            secrets: &secrets,
            outbound: &outbound,
            proxy: ProxyPolicy::Direct,
            budget: RequestBudget::from_now(Duration::from_secs(5)),
            cancellation: CancellationToken::new(),
            correlation_id: "test-correlation".to_string(),
        };

        let request = build_json_request(
            &context,
            EndpointRole::RemoteKeys,
            "https://newapi.example/api/token/",
            Method::POST,
            Some(json!({"name": "relay-created"})),
            OutboundRetryPolicy::Never,
            true,
            Some("test-correlation".to_string()),
        )
        .await
        .expect("request");

        assert_eq!(request.method, Method::POST);
        assert_eq!(request.retry_policy, OutboundRetryPolicy::Never);
        assert!(std::str::from_utf8(&request.body)
            .expect("json body")
            .contains("relay-created"));
        assert!(!format!("{:?}", request.headers).contains("newapi-access-token"));
    }

    #[tokio::test]
    async fn authorization_request_uses_cookie_session_headers() {
        let outbound = AsyncOutboundClient::new(AsyncOutboundClientConfig::architecture_budget());
        let secrets = TestSecretAccessor("session=secret-canary");
        let credential = crate::services::collectors::contract::OpaqueCredentialHandle {
            station_id: "station-1".to_string(),
            credential_revision: 7,
            scope: CredentialScope::LoginSession,
        };
        let context = CollectorContext {
            station: StationIdentity {
                station_id: "station-1".to_string(),
                endpoint_revision: 7,
                provider: ProviderKind::NewApi,
            },
            endpoints: ProviderEndpoints {
                api_base_url: None,
                website_url: Some("https://newapi.example".to_string()),
            },
            credential: credential.clone(),
            auth: Some(ProviderAuthContext::NewApi {
                user_id: "42".to_string(),
                secret_purpose: CredentialSecretPurpose::SessionCookie,
            }),
            secrets: &secrets,
            outbound: &outbound,
            proxy: ProxyPolicy::Direct,
            budget: RequestBudget::from_now(Duration::from_secs(5)),
            cancellation: CancellationToken::new(),
            correlation_id: "test-correlation".to_string(),
        };

        let request = build_json_request(
            &context,
            EndpointRole::Authorization,
            "https://newapi.example/api/user/self",
            Method::GET,
            None,
            OutboundRetryPolicy::default(),
            true,
            Some("test-correlation".to_string()),
        )
        .await
        .expect("request");

        assert_eq!(request.method, Method::GET);
        assert_eq!(request.url, "https://newapi.example/api/user/self");
        assert_eq!(request.retry_policy, OutboundRetryPolicy::StatusRetry);
        assert!(request.body.is_empty());
        assert!(!format!("{:?}", request.headers).contains("session=secret-canary"));
    }

    #[test]
    fn authorization_self_data_accepts_data_id_or_user_id_only() {
        assert_eq!(
            user_id_from_self_data(&json!({"id": 42, "quota": 1})).as_deref(),
            Some("42")
        );
        assert_eq!(
            user_id_from_self_data(&json!({"user": {"id": "newapi-user-99"}})).as_deref(),
            Some("newapi-user-99")
        );
        assert_eq!(
            user_id_from_self_data(&json!({"profile": {"id": "not-trusted"}})),
            None
        );
    }

    #[test]
    fn dashboard_usage_items_require_standard_array_shape() {
        assert_eq!(dashboard_usage_items(&json!([{ "count": 1 }])).len(), 1);
        assert!(dashboard_usage_items(&json!({
            "items": [{ "count": 1 }]
        }))
        .is_empty());
        assert!(dashboard_usage_items(&json!({ "count": 1 })).is_empty());
    }

    #[test]
    fn dashboard_total_requires_exact_raw_quota_match() {
        let target = NewApiDashboardTotalTarget {
            request_count: 1200,
            quota: 9_250_000,
        };
        assert!(dashboard_total_matches_target(
            &NewApiDashboardUsageWindow {
                request_count: Some(1200),
                quota: Some(9_250_000),
                ..Default::default()
            },
            target,
        ));
        assert!(!dashboard_total_matches_target(
            &NewApiDashboardUsageWindow {
                request_count: Some(1199),
                quota: Some(9_250_000),
                ..Default::default()
            },
            target,
        ));
        assert!(!dashboard_total_matches_target(
            &NewApiDashboardUsageWindow {
                request_count: Some(1200),
                quota: Some(9_249_999),
                ..Default::default()
            },
            target,
        ));
    }

    #[test]
    fn dashboard_total_merge_propagates_missing_window_metrics() {
        let mut total = NewApiDashboardUsageWindow::default();
        total.add(NewApiDashboardUsageWindow {
            request_count: Some(2),
            token_count: Some(100),
            quota: Some(300000),
            consumption: Some(0.6),
        });
        total.add(NewApiDashboardUsageWindow {
            request_count: Some(3),
            token_count: None,
            quota: Some(400000),
            consumption: Some(0.8),
        });

        assert_eq!(total.request_count, Some(5));
        assert_eq!(total.quota, Some(700000));
        assert_eq!(total.consumption, Some(1.4));
        assert_eq!(total.token_count, None);
    }

    #[test]
    fn usage_merge_removes_unverified_self_usage_fields() {
        let mut data = json!({
            "request_count": 12,
            "today_request_count": 999,
            "today_consumption": 999.0,
            "today_token_count": 999,
            "total_token_count": 999,
            "today_base_consumption": 999.0,
            "total_base_consumption": 999.0
        });

        merge_usage_stats_into_balance_data(
            &mut data,
            NewApiUsageStats {
                today_request_count: Some(2),
                today_consumption: Some(0.75),
                today_token_count: Some(123),
                ..Default::default()
            },
        );

        assert_eq!(data["request_count"], 12);
        assert_eq!(data["today_request_count"], 2);
        assert_eq!(data["today_consumption"], 0.75);
        assert_eq!(data["today_token_count"], 123);
        assert!(data.get("total_token_count").is_none());
        assert!(data.get("today_base_consumption").is_none());
        assert!(data.get("total_base_consumption").is_none());
    }

    #[test]
    fn empty_usage_merge_still_removes_unverified_self_usage_fields() {
        let mut data = json!({
            "request_count": 12,
            "today_token_count": 999,
            "total_token_count": 999
        });

        merge_optional_usage_stats_into_balance_data(&mut data, None);

        assert_eq!(data["request_count"], 12);
        assert!(data.get("today_token_count").is_none());
        assert!(data.get("total_token_count").is_none());
    }

    #[test]
    fn integer_metrics_reject_fractional_values() {
        assert_eq!(
            numeric_i64_field(&json!({ "count": 1.4 }), &["count"]),
            None
        );
        assert_eq!(
            numeric_i64_field(&json!({ "count": 2.0 }), &["count"]),
            Some(2)
        );
        assert_eq!(
            numeric_i64_field(&json!({ "count": "3" }), &["count"]),
            Some(3)
        );
    }
}
