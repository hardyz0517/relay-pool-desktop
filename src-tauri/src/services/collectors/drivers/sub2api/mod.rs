mod mapping;

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
            contract::{
                CollectorContext, CollectorDriver, CollectorTaskKind, CreateRemoteKeyRequest,
                CreatedRemoteKeyOutput, CredentialSecretPurpose, DeleteRemoteKeyRequest,
                DeletedRemoteKeyOutput, DriverOutput, DriverOutputStatus, ProviderAuthContext,
                ProviderKind, RedactedDiagnostics, RemoteKeyDriver, RemoteKeyOutput,
                RemoteKeyRequest, RemoteKeySecret, RevealRemoteKeyRequest, RevealedRemoteKeyOutput,
                Sub2ApiLoginCredential, Sub2ApiStationKeyCredential,
            },
            evidence::{redact_text, EndpointEvidence, EndpointRole, EvidenceSet},
            facts::CollectorFacts,
            failure::{
                AuthEffect, DriverFailure, DriverFailureKind, FailedEndpoint, RetryDisposition,
            },
            manual_authorization::response_requires_manual_authorization,
        },
        station_endpoints::{build_api_url, build_management_url},
    },
};

const LOGIN_PATHS: [&str; 3] = ["/api/v1/auth/login", "/auth/login", "/api/login"];
const LOGIN_FIELDS: [&str; 3] = ["email", "username", "user"];
const REMOTE_KEY_PAGE_SIZE: usize = 100;
const REMOTE_KEY_MAX_PAGES: usize = 10_000;
const REQUEST_MAX_ATTEMPTS: usize = 3;
const MALFORMED_JSON_MAX_ATTEMPTS: usize = 2;
const RETRY_DELAYS: [Duration; 2] = [Duration::from_millis(300), Duration::from_secs(1)];

pub const SUPPORTED_COLLECTOR_TASKS: &[CollectorTaskKind] = &[
    CollectorTaskKind::Detect,
    CollectorTaskKind::Balance,
    CollectorTaskKind::Groups,
];
pub const FULL_COLLECTOR_TASKS: &[CollectorTaskKind] =
    &[CollectorTaskKind::Balance, CollectorTaskKind::Groups];

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
            let mut session = resolve_remote_key_list_session(context, &website_url, &auth).await?;
            let execution = fetch_remote_key_list(
                context,
                &website_url,
                &auth,
                &mut session.access_token,
                session.cookie.as_deref(),
            )
            .await?;
            if !execution.ok {
                return Err(failed_from_endpoint_results(
                    &[execution.redacted.clone()],
                    EndpointRole::RemoteKeys,
                    Some(execution.evidence),
                    "Sub2API remote-key list returned no canonical keys",
                ));
            }
            let keys =
                mapping::parse_remote_key_payload(&request.station.station_id, &execution.payload);
            Ok(RemoteKeyOutput {
                keys,
                evidence: execution.evidence,
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
            let mut session = resolve_remote_key_list_session(context, &website_url, &auth).await?;
            let execution = fetch_remote_key_list(
                context,
                &website_url,
                &auth,
                &mut session.access_token,
                session.cookie.as_deref(),
            )
            .await?;
            if !execution.ok {
                return Err(failed_from_endpoint_results(
                    &[execution.redacted.clone()],
                    EndpointRole::RemoteKeys,
                    Some(execution.evidence),
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
                evidence: execution.evidence,
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
            let full_key_once = mapping::full_key_from_create_payload(&create.payload);
            let mut remote_key =
                mapping::parse_remote_key_payload(&request.station.station_id, &create.payload)
                    .into_iter()
                    .next()
                    .unwrap_or_else(|| {
                        mapping::remote_key_from_create_input(
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

            let list = fetch_remote_key_list(context, &website_url, &auth, &mut access_token, None)
                .await?;
            if list.ok {
                if let Some((listed_key, full_key)) = remote_key_secret_by_name_from_list_payload(
                    &request.station.station_id,
                    &request.name,
                    &list.payload,
                )? {
                    remote_key = listed_key;
                    let mut evidence = vec![create.evidence];
                    evidence.extend(list.evidence);
                    return Ok(CreatedRemoteKeyOutput {
                        remote_key,
                        full_key_once: RemoteKeySecret::new(full_key),
                        evidence,
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

    fn delete_remote_key<'a>(
        &'a self,
        context: &'a CollectorContext<'a>,
        request: DeleteRemoteKeyRequest,
    ) -> BoxFuture<'a, Result<DeletedRemoteKeyOutput, DriverFailure>> {
        async move {
            validate_remote_key_request(context, &request.station, &request.endpoints)?;
            let website_url = website_url_from_endpoints(&request.endpoints)?;
            let auth = sub2api_auth(context)?;
            let mut access_token = resolve_access_token(context, &website_url, &auth).await?;
            let initial =
                fetch_remote_key_list(context, &website_url, &auth, &mut access_token, None)
                    .await?;
            if !initial.ok {
                return Err(failed_from_endpoint_results(
                    &[initial.redacted.clone()],
                    EndpointRole::RemoteKeys,
                    Some(initial.evidence),
                    "Sub2API remote-key delete could not read the current key list",
                ));
            }
            let Some(provider_key_id) = remote_key_provider_id_from_list_payload(
                &request.station.station_id,
                &request.remote_key_id,
                &initial.payload,
            )?
            else {
                return Ok(DeletedRemoteKeyOutput {
                    keys: mapping::parse_remote_key_payload(
                        &request.station.station_id,
                        &initial.payload,
                    ),
                    already_absent: true,
                    evidence: initial.evidence,
                    diagnostics: RedactedDiagnostics {
                        summary: Some(json!({"alreadyAbsent": true}).to_string()),
                        raw_json_redacted: Some(json!({"endpointResults": [initial.redacted]})),
                    },
                });
            };

            let url =
                build_management_url(&website_url, &format!("/api/v1/keys/{provider_key_id}"))
                    .map_err(|error| invalid_request(redact_text(&error)))?;
            let session_cookie = resolve_session_cookie(context, &auth).await?;
            let mut deletion = execute_bearer_json_once(
                context,
                EndpointRole::RemoteKeys,
                &url,
                &access_token,
                session_cookie.as_deref(),
                None,
                Method::DELETE,
            )
            .await;
            if matches!(deletion.status, Some(401 | 403)) {
                if let Some(login) = auth.login.as_ref() {
                    if let Some(fresh) = login_access_token(context, &website_url, login).await? {
                        access_token = fresh;
                        deletion = execute_bearer_json_once(
                            context,
                            EndpointRole::RemoteKeys,
                            &url,
                            &access_token,
                            session_cookie.as_deref(),
                            None,
                            Method::DELETE,
                        )
                        .await;
                    }
                }
            }
            let delete_accepted = deletion
                .status
                .is_some_and(|status| (200..300).contains(&status));
            let delete_failure = if delete_accepted {
                None
            } else {
                fatal_attempt_failure(&deletion, EndpointRole::RemoteKeys).or_else(|| {
                    Some(failed_from_endpoint_results(
                        &[json!({
                            "status": deletion.status,
                            "ok": false,
                        })],
                        EndpointRole::RemoteKeys,
                        Some(vec![deletion.evidence.clone()]),
                        deletion
                            .error_message
                            .clone()
                            .unwrap_or_else(|| "Sub2API remote-key delete failed".to_string()),
                    ))
                })
            };

            let reconciliation =
                fetch_remote_key_list(context, &website_url, &auth, &mut access_token, None).await;
            let remaining = match reconciliation {
                Ok(output) if output.ok => output,
                _ => {
                    return Err(delete_failure.unwrap_or_else(|| {
                        result_unknown(
                            EndpointRole::RemoteKeys,
                            Some(deletion.evidence.clone()),
                            "Sub2API remote-key delete was accepted but could not be reconciled",
                        )
                    }));
                }
            };
            let keys =
                mapping::parse_remote_key_payload(&request.station.station_id, &remaining.payload);
            if keys.iter().any(|key| key.id == request.remote_key_id) {
                return Err(delete_failure.unwrap_or_else(|| {
                    result_unknown(
                        EndpointRole::RemoteKeys,
                        Some(deletion.evidence.clone()),
                        "Sub2API remote-key delete returned success but the key still exists",
                    )
                }));
            }
            Ok(DeletedRemoteKeyOutput {
                keys,
                already_absent: false,
                evidence: {
                    let mut evidence = initial.evidence;
                    evidence.push(deletion.evidence);
                    evidence.extend(remaining.evidence);
                    evidence
                },
                diagnostics: RedactedDiagnostics {
                    summary: Some(json!({"deleted": true, "reconciled": true}).to_string()),
                    raw_json_redacted: Some(json!({
                        "endpointResults": [initial.redacted, remaining.redacted]
                    })),
                },
            })
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
    session_cookie: Option<&str>,
) -> Result<RemoteKeyListExecution, DriverFailure> {
    let mut requested_page = 1usize;
    let mut all_items = Vec::new();
    let mut evidence = Vec::new();
    let mut page_diagnostics = Vec::new();

    loop {
        let path = format!("/api/v1/keys?page={requested_page}&page_size={REMOTE_KEY_PAGE_SIZE}");
        let execution = execute_bearer_json_with_recovery_with_cookie(
            context,
            EndpointRole::RemoteKeys,
            website_url,
            &path,
            access_token,
            auth,
            session_cookie,
        )
        .await?;
        evidence.push(execution.evidence.clone());
        page_diagnostics.push(execution.redacted.clone());
        if !execution.ok {
            return Ok(RemoteKeyListExecution {
                payload: Value::Null,
                ok: false,
                redacted: execution.redacted,
                evidence,
            });
        }

        let endpoint = execution.evidence.clone();
        let data = execution.payload.get("data").ok_or_else(|| {
            malformed(
                EndpointRole::RemoteKeys,
                Some(endpoint.clone()),
                "Sub2API remote-key pagination is missing data",
            )
        })?;
        let items = data.get("items").and_then(Value::as_array).ok_or_else(|| {
            malformed(
                EndpointRole::RemoteKeys,
                Some(endpoint.clone()),
                "Sub2API remote-key pagination is missing items",
            )
        })?;
        let page = optional_pagination_usize(data, "page");
        let page_size = optional_pagination_usize(data, "page_size");
        let total = optional_pagination_usize(data, "total");
        let pages = optional_pagination_usize(data, "pages");
        if page.is_some_and(|page| page != requested_page)
            || page_size.is_some_and(|page_size| page_size == 0 || items.len() > page_size)
        {
            return Err(malformed(
                EndpointRole::RemoteKeys,
                Some(endpoint),
                "Sub2API remote-key pagination metadata is inconsistent",
            ));
        }
        if pages.is_some_and(|pages| pages > REMOTE_KEY_MAX_PAGES) {
            return Err(malformed(
                EndpointRole::RemoteKeys,
                Some(endpoint),
                "Sub2API remote-key pagination exceeds the supported page limit",
            ));
        }
        all_items.extend(items.iter().cloned());
        if total.is_some_and(|total| all_items.len() > total) {
            return Err(malformed(
                EndpointRole::RemoteKeys,
                evidence.last().cloned(),
                "Sub2API remote-key pagination returned more items than total",
            ));
        }

        let effective_page_size = page_size.unwrap_or(REMOTE_KEY_PAGE_SIZE);
        let complete = match total {
            Some(total) if all_items.len() == total => true,
            Some(_) if items.is_empty() || pages.is_some_and(|pages| requested_page >= pages) => {
                return Err(malformed(
                    EndpointRole::RemoteKeys,
                    evidence.last().cloned(),
                    "Sub2API remote-key pagination returned a partial list",
                ));
            }
            Some(_) => false,
            None => {
                items.is_empty()
                    || items.len() < effective_page_size
                    || pages.is_some_and(|pages| requested_page >= pages)
            }
        };
        if complete {
            break;
        }
        if requested_page >= REMOTE_KEY_MAX_PAGES {
            return Err(malformed(
                EndpointRole::RemoteKeys,
                evidence.last().cloned(),
                "Sub2API remote-key pagination exceeded the supported page limit",
            ));
        }
        requested_page += 1;
    }

    Ok(RemoteKeyListExecution {
        payload: json!({ "data": { "items": all_items } }),
        ok: true,
        redacted: json!({ "pages": page_diagnostics }),
        evidence,
    })
}

fn optional_pagination_usize(data: &Value, field: &str) -> Option<usize> {
    data.get(field).and_then(|value| {
        value
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .or_else(|| value.as_str()?.trim().parse::<usize>().ok())
    })
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
        body["group_id"] = mapping::sub2api_group_id_value(group_id);
    }
    let session_cookie = resolve_session_cookie(context, auth).await?;

    let mut result = execute_bearer_json_once(
        context,
        EndpointRole::RemoteKeys,
        &url,
        access_token,
        session_cookie.as_deref(),
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
                    session_cookie.as_deref(),
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
    Err(failed_from_endpoint_results(
        &[redacted],
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
    for (index, value) in mapping::remote_key_items(payload).into_iter().enumerate() {
        let Some(remote_key) = mapping::remote_key_from_value(station_id, value, index) else {
            continue;
        };
        if remote_key.id != remote_key_id {
            continue;
        }
        let full_key = mapping::full_key_from_key_value(value).ok_or_else(|| {
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

fn remote_key_provider_id_from_list_payload(
    station_id: &str,
    remote_key_id: &str,
    payload: &Value,
) -> Result<Option<String>, DriverFailure> {
    for (index, value) in mapping::remote_key_items(payload).into_iter().enumerate() {
        let Some(remote_key) = mapping::remote_key_from_value(station_id, value, index) else {
            continue;
        };
        if remote_key.id != remote_key_id {
            continue;
        }
        return mapping::remote_key_provider_id(value)
            .map(Some)
            .ok_or_else(|| {
                invalid_request("Sub2API remote key does not expose a deletable key id")
            });
    }
    Ok(None)
}

fn remote_key_secret_by_name_from_list_payload(
    station_id: &str,
    expected_name: &str,
    payload: &Value,
) -> Result<Option<(RemoteStationKey, String)>, DriverFailure> {
    let expected_name = expected_name.trim();
    for (index, value) in mapping::remote_key_items(payload).into_iter().enumerate() {
        let Some(remote_key) = mapping::remote_key_from_value(station_id, value, index) else {
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
        let Some(full_key) = mapping::full_key_from_key_value(value) else {
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
        &auth,
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
        &auth,
    )
    .await?;
    evidence.push(rates.evidence.clone());
    endpoint_results.push(rates.redacted.clone());

    let mut facts = mapping::parse_group_rate_facts(
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
    mapping::add_single_group_key_bindings(&mut facts, &station_keys);

    let success_count = [available.ok, rates.ok]
        .into_iter()
        .filter(|ok| *ok)
        .count();
    let status = match success_count {
        2 => DriverOutputStatus::Success,
        1 if !facts.groups.is_empty() => DriverOutputStatus::Partial,
        _ if !facts.groups.is_empty() || !facts.rates.is_empty() => DriverOutputStatus::Partial,
        _ => {
            return Err(failed_from_endpoint_results(
                &endpoint_results,
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
            facts.balances.push(mapping::parse_usage_balance(
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
                facts.balances.push(mapping::parse_usage_balance(
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
                mapping::merge_account_profile_balance(&mut facts.balances, profile_balance);
            }
        }
        if let Some(subscription_quota) = collect_subscription_quota(
            context,
            &website_url,
            &auth,
            &mut access_token,
            &mut evidence,
            &mut endpoint_results,
        )
        .await?
        {
            mapping::merge_subscription_quota(
                &mut facts.balances,
                &context.station.station_id,
                subscription_quota,
            );
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
                mapping::merge_dashboard_usage_stats(
                    &mut facts.balances,
                    &context.station.station_id,
                    stats,
                );
            }
        }
    }

    if facts.balances.is_empty() {
        return Err(failed_from_endpoint_results(
            &endpoint_results,
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
            auth,
        )
        .await?;
        *access_token = Some(token);
        evidence.push(execution.evidence.clone());
        endpoint_results.push(execution.redacted);
        if !execution.ok {
            continue;
        }
        if let Some(balance) = mapping::parse_account_balance(
            &context.station.station_id,
            &execution.payload,
            auth.credit_per_cny,
        ) {
            return Ok(Some(balance));
        }
    }
    Ok(None)
}

async fn collect_subscription_quota(
    context: &CollectorContext<'_>,
    website_url: &str,
    auth: &Sub2ApiAuth,
    access_token: &mut Option<String>,
    evidence: &mut Vec<EndpointEvidence>,
    endpoint_results: &mut Vec<Value>,
) -> Result<Option<mapping::SubscriptionQuotaSummary>, DriverFailure> {
    for (path, parser) in [
        (
            "/api/v1/subscriptions/active",
            mapping::parse_active_subscription_quota
                as fn(&Value, f64) -> Option<mapping::SubscriptionQuotaSummary>,
        ),
        (
            "/api/v1/user/platform-quotas",
            mapping::parse_platform_quota
                as fn(&Value, f64) -> Option<mapping::SubscriptionQuotaSummary>,
        ),
    ] {
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
            auth,
        )
        .await?;
        *access_token = Some(token);
        evidence.push(execution.evidence.clone());
        endpoint_results.push(execution.redacted);
        if execution.ok {
            if let Some(quota) = parser(&execution.payload, auth.credit_per_cny) {
                return Ok(Some(quota));
            }
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
) -> Result<Option<mapping::DashboardUsageStats>, DriverFailure> {
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
        auth,
    )
    .await?;
    *access_token = Some(token);
    evidence.push(execution.evidence.clone());
    endpoint_results.push(execution.redacted);
    if !execution.ok {
        return Ok(None);
    }
    Ok(mapping::parse_dashboard_usage_stats(&execution.payload))
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
    session_cookie: Option<crate::services::collectors::contract::OpaqueCredentialHandle>,
    login: Option<Sub2ApiLoginCredential>,
    credit_per_cny: f64,
}

struct RemoteKeyListSession {
    access_token: String,
    cookie: Option<String>,
}

struct LoginSession {
    access_token: String,
    cookie: Option<String>,
}

enum LoginSessionResolution {
    Session(LoginSession),
    InteractiveAuthorizationRequired { status: u16 },
    SuccessfulWithoutSession { status: u16 },
    SupportedFormsFailed { statuses: Vec<u16> },
}

impl LoginSessionResolution {
    fn into_session(self) -> Option<LoginSession> {
        match self {
            Self::Session(session) => Some(session),
            Self::InteractiveAuthorizationRequired { .. }
            | Self::SuccessfulWithoutSession { .. }
            | Self::SupportedFormsFailed { .. } => None,
        }
    }

    fn remote_key_detail(&self) -> String {
        match self {
            Self::Session(_) => "Sub2API management session is available".to_string(),
            Self::InteractiveAuthorizationRequired { status } => {
                format!("Sub2API login was rejected or requires interactive authorization (HTTP {status}).")
            }
            Self::SuccessfulWithoutSession { status } => {
                format!(
                    "Sub2API login succeeded but did not return a usable management session (HTTP {status})."
                )
            }
            Self::SupportedFormsFailed { statuses } if !statuses.is_empty() => {
                let statuses = statuses
                    .iter()
                    .map(u16::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "Sub2API login did not succeed for the supported login forms (HTTP {statuses})."
                )
            }
            Self::SupportedFormsFailed { .. } => {
                "Sub2API login did not return a usable response for the supported login forms."
                    .to_string()
            }
        }
    }
}

fn sub2api_auth(context: &CollectorContext<'_>) -> Result<Sub2ApiAuth, DriverFailure> {
    match context.auth.clone() {
        Some(ProviderAuthContext::Sub2Api {
            station_keys,
            access_token,
            session_cookie,
            login,
            credit_per_cny,
        }) => Ok(Sub2ApiAuth {
            station_keys,
            access_token,
            session_cookie,
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
    if let Some(token) = resolve_saved_access_token(context, auth).await? {
        return Ok(token);
    }
    if let Some(handle) = &auth.session_cookie {
        let secret = context
            .secrets
            .resolve_secret(handle, CredentialSecretPurpose::SessionCookie)
            .await?;
        let cookie = secret.expose().trim();
        if !cookie.is_empty() {
            return Ok(cookie.to_string());
        }
    }
    if let Some(login) = &auth.login {
        if let Some(session) = login_session(context, website_url, login)
            .await?
            .into_session()
        {
            return Ok(session.access_token);
        }
    }
    Err(manual_required(
        "Sub2API requires usable management credentials",
    ))
}

async fn resolve_remote_key_list_session(
    context: &CollectorContext<'_>,
    website_url: &str,
    auth: &Sub2ApiAuth,
) -> Result<RemoteKeyListSession, DriverFailure> {
    let cookie = resolve_session_cookie(context, auth).await?;
    if let Some(access_token) = resolve_saved_access_token(context, auth).await? {
        return Ok(RemoteKeyListSession {
            access_token,
            cookie,
        });
    }
    if let Some(cookie) = cookie {
        return Ok(RemoteKeyListSession {
            access_token: cookie.clone(),
            cookie: Some(cookie),
        });
    }
    if let Some(login) = &auth.login {
        match login_session(context, website_url, login).await? {
            LoginSessionResolution::Session(session) => {
                return Ok(RemoteKeyListSession {
                    access_token: session.access_token,
                    cookie: session.cookie,
                });
            }
            resolution => return Err(manual_required(resolution.remote_key_detail())),
        }
    }
    Err(manual_required(
        "Sub2API remote-key listing could not establish a management session",
    ))
}

async fn resolve_saved_access_token(
    context: &CollectorContext<'_>,
    auth: &Sub2ApiAuth,
) -> Result<Option<String>, DriverFailure> {
    if let Some(handle) = &auth.access_token {
        if let Ok(secret) = context
            .secrets
            .resolve_secret(handle, CredentialSecretPurpose::AuthorizationHeader)
            .await
        {
            let token = secret.expose().trim();
            if !token.is_empty() {
                return Ok(Some(token.to_string()));
            }
        }
        // Older prepared contexts stored the browser token under the session
        // cookie purpose. Keep accepting that shape while new contexts use
        // AuthorizationHeader for JWTs.
        if let Ok(secret) = context
            .secrets
            .resolve_secret(handle, CredentialSecretPurpose::SessionCookie)
            .await
        {
            let token = secret.expose().trim();
            if !token.is_empty() {
                return Ok(Some(token.to_string()));
            }
        }
    }
    Ok(None)
}

async fn resolve_session_cookie(
    context: &CollectorContext<'_>,
    auth: &Sub2ApiAuth,
) -> Result<Option<String>, DriverFailure> {
    if let Some(handle) = &auth.session_cookie {
        let secret = context
            .secrets
            .resolve_secret(handle, CredentialSecretPurpose::SessionCookie)
            .await?;
        let cookie = secret.expose().trim();
        if !cookie.is_empty() {
            return Ok(Some(cookie.to_string()));
        }
    }
    // Compatibility for contexts created before session_cookie was added.
    if let Some(handle) = &auth.access_token {
        if let Ok(secret) = context
            .secrets
            .resolve_secret(handle, CredentialSecretPurpose::SessionCookie)
            .await
        {
            let cookie = secret.expose().trim();
            if looks_like_cookie_header(cookie) {
                return Ok(Some(cookie.to_string()));
            }
        }
    }
    Ok(None)
}

#[derive(Debug, Clone)]
struct JsonExecution {
    payload: Value,
    ok: bool,
    redacted: Value,
    evidence: EndpointEvidence,
}

#[derive(Debug, Clone)]
struct RemoteKeyListExecution {
    payload: Value,
    ok: bool,
    redacted: Value,
    evidence: Vec<EndpointEvidence>,
}

async fn execute_bearer_json_with_recovery(
    context: &CollectorContext<'_>,
    role: EndpointRole,
    base_url: &str,
    path: &str,
    access_token: &mut String,
    auth: &Sub2ApiAuth,
) -> Result<JsonExecution, DriverFailure> {
    execute_bearer_json_with_recovery_with_cookie(
        context,
        role,
        base_url,
        path,
        access_token,
        auth,
        None,
    )
    .await
}

async fn execute_bearer_json_with_recovery_with_cookie(
    context: &CollectorContext<'_>,
    role: EndpointRole,
    base_url: &str,
    path: &str,
    access_token: &mut String,
    auth: &Sub2ApiAuth,
    session_cookie_override: Option<&str>,
) -> Result<JsonExecution, DriverFailure> {
    let url = build_management_url(base_url, path)
        .map_err(|error| invalid_request(redact_text(&error)))?;
    let started_at = Instant::now();
    let mut attempts = Vec::new();
    let mut recovery_actions = Vec::new();
    let mut auth_refreshed = false;
    let mut cookie_fallback_used = false;
    let mut malformed_attempts = 0;
    let mut latest = None;
    let session_cookie = session_cookie_override
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or(resolve_session_cookie(context, auth).await?);

    for attempt in 1..=REQUEST_MAX_ATTEMPTS {
        let result = execute_bearer_json_once(
            context,
            role,
            &url,
            access_token,
            session_cookie.as_deref(),
            None,
            Method::GET,
        )
        .await;
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
        } else if failure == Some("auth_rejected")
            && !cookie_fallback_used
            && session_cookie.is_some()
            && !looks_like_cookie_header(access_token)
        {
            "cookie_fallback"
        } else if failure == Some("auth_rejected") && !auth_refreshed && auth.login.is_some() {
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
        if action == "cookie_fallback" {
            cookie_fallback_used = true;
            recovery_actions.push("cookie_fallback");
            if let Some(cookie) = session_cookie.as_deref() {
                *access_token = cookie.to_string();
                continue;
            }
            break;
        }
        if action == "auth_refresh" {
            auth_refreshed = true;
            recovery_actions.push("auth_refresh");
            if let Some(login) = auth.login.as_ref() {
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

    let mut result = latest.unwrap_or_else(|| JsonAttemptResult::budget_exhausted(path, role));
    let automated_session_recovery_exhausted = result.status == Some(401)
        && (auth.access_token.is_some() || auth.session_cookie.is_some() || auth.login.is_some());
    if automated_session_recovery_exhausted {
        result.manual_authorization_required = true;
        recovery_actions
            .push(crate::services::collectors::manual_authorization::RECOMMENDED_ACTION);
    }
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
    let ok = result.ok && classify_json_result(&result).is_none();
    Ok(JsonExecution {
        payload: result.payload,
        ok,
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
    manual_authorization_required: bool,
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
            manual_authorization_required: false,
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
    session_cookie: Option<&str>,
    body: Option<Value>,
    method: Method,
) -> JsonAttemptResult {
    let started_at = Instant::now();
    let request = match build_json_request(
        context,
        role,
        url,
        bearer,
        session_cookie,
        body,
        method.clone(),
    ) {
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
                manual_authorization_required: false,
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
            let manual_authorization_required = response_requires_manual_authorization(
                status,
                &response.headers,
                &response.evidence.final_url,
                &response.body,
            );
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
                manual_authorization_required,
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
            manual_authorization_required: false,
        },
    }
}

fn build_json_request(
    context: &CollectorContext<'_>,
    role: EndpointRole,
    url: &str,
    bearer: &str,
    session_cookie: Option<&str>,
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
    if let Some(user_agent) = context
        .user_agent
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let value = HeaderValue::try_from(user_agent)
            .map_err(|_| invalid_request("captured User-Agent is not a valid header value"))?;
        headers
            .insert_public(header::USER_AGENT, value, &policy)
            .map_err(|failure| driver_failure_from_outbound(role, failure.kind))?;
    }
    // CF-protected browser sessions may require the browser Cookie alongside
    // a captured JWT. Cookie-only sessions continue to use Cookie auth.
    let bearer = bearer.trim();
    let cookie = session_cookie
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| looks_like_cookie_header(bearer).then_some(bearer));
    if !bearer.is_empty() && !looks_like_cookie_header(bearer) {
        headers
            .insert_sensitive(
                header::AUTHORIZATION,
                SecretHeaderValue::new(format!("Bearer {bearer}")),
                &policy,
            )
            .map_err(|failure| driver_failure_from_outbound(role, failure.kind))?;
    }
    if let Some(cookie) = cookie {
        headers
            .insert_sensitive(
                header::COOKIE,
                SecretHeaderValue::new(cookie.to_string()),
                &policy,
            )
            .map_err(|failure| driver_failure_from_outbound(role, failure.kind))?;
    }
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

fn looks_like_cookie_header(value: &str) -> bool {
    value.split(';').any(|part| {
        part.trim()
            .split_once('=')
            .is_some_and(|(name, value)| !name.trim().is_empty() && !value.trim().is_empty())
    })
}

async fn login_access_token(
    context: &CollectorContext<'_>,
    base_url: &str,
    login: &Sub2ApiLoginCredential,
) -> Result<Option<String>, DriverFailure> {
    Ok(login_session(context, base_url, login)
        .await?
        .into_session()
        .map(|session| session.access_token))
}

async fn login_session(
    context: &CollectorContext<'_>,
    base_url: &str,
    login: &Sub2ApiLoginCredential,
) -> Result<LoginSessionResolution, DriverFailure> {
    let password = context
        .secrets
        .resolve_secret(&login.password, CredentialSecretPurpose::LoginPassword)
        .await?;
    let mut statuses = Vec::new();
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
            if response.status.is_success() {
                let access_token = extract_token(&parsed);
                let cookie = extract_login_cookie(&response.headers);
                if let Some(access_token) = access_token.or_else(|| cookie.clone()) {
                    return Ok(LoginSessionResolution::Session(LoginSession {
                        access_token,
                        cookie,
                    }));
                }
            }
            if is_manual_login_required(&parsed, status) {
                return Ok(LoginSessionResolution::InteractiveAuthorizationRequired { status });
            }
            if response.status.is_success() {
                return Ok(LoginSessionResolution::SuccessfulWithoutSession { status });
            }
            if !statuses.contains(&status) {
                statuses.push(status);
            }
        }
    }
    Ok(LoginSessionResolution::SupportedFormsFailed { statuses })
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
    if let Some(user_agent) = context
        .user_agent
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let value = HeaderValue::try_from(user_agent)
            .map_err(|_| invalid_request("captured User-Agent is not a valid header value"))?;
        headers
            .insert_public(header::USER_AGENT, value, &policy)
            .map_err(|failure| {
                driver_failure_from_outbound(EndpointRole::Authorization, failure.kind)
            })?;
    }
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

fn extract_login_cookie(headers: &http::HeaderMap) -> Option<String> {
    let pairs = headers
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .filter_map(|value| value.split(';').next())
        .map(str::trim)
        .filter_map(|pair| {
            let (name, value) = pair.split_once('=')?;
            let name = name.trim();
            let value = value.trim();
            (!name.is_empty() && !value.is_empty()).then(|| format!("{name}={value}"))
        })
        .collect::<Vec<_>>();
    (!pairs.is_empty()).then(|| pairs.join("; "))
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
    if result.manual_authorization_required {
        return Some("manual_authorization_required");
    }
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

fn recovery_action_from_endpoint_results(results: &[Value]) -> AuthEffect {
    let has_manual_authorization_signal = results.iter().any(|result| {
        result
            .get("failureKind")
            .and_then(Value::as_str)
            .is_some_and(|value| value == "manual_authorization_required")
            || result
                .get("attempts")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .any(|attempt| {
                    attempt
                        .get("failureKind")
                        .and_then(Value::as_str)
                        .is_some_and(|value| value == "manual_authorization_required")
                })
    });

    if has_manual_authorization_signal {
        AuthEffect::Reauthorize
    } else {
        AuthEffect::None
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
        "failureKind": classify_json_result(result),
        "errorMessage": result.error_message,
    })
}

fn append_balance_endpoint_attempt(endpoint: &mut Value, result: &JsonAttemptResult) {
    let attempt = json!({
        "url": result.url,
        "status": result.status,
        "durationMs": result.duration_ms,
        "ok": result.ok,
        "failureKind": classify_json_result(result),
        "errorMessage": result.error_message,
    });
    if endpoint.get("attempts").is_none() {
        let first_attempt = json!({
            "url": endpoint.get("url").cloned().unwrap_or(Value::Null),
            "status": endpoint.get("status").cloned().unwrap_or(Value::Null),
            "durationMs": endpoint.get("durationMs").cloned().unwrap_or(Value::Null),
            "ok": endpoint.get("ok").cloned().unwrap_or(Value::Null),
            "failureKind": endpoint.get("failureKind").cloned().unwrap_or(Value::Null),
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
    endpoint["failureKind"] = json!(classify_json_result(result));
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

fn failed_from_endpoint_results(
    results: &[Value],
    role: EndpointRole,
    evidence: Option<Vec<EndpointEvidence>>,
    detail: impl Into<String>,
) -> DriverFailure {
    let auth_effect = recovery_action_from_endpoint_results(results);
    let kind = if auth_effect == AuthEffect::Reauthorize {
        DriverFailureKind::AuthRejected
    } else {
        failure_kind_from_endpoint_results(results)
    };
    let mut failure = failed(kind, role, evidence, detail);
    failure.auth_effect = auth_effect;
    failure
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
    use tokio_util::sync::CancellationToken;

    struct TestSecretAccessor;

    impl crate::services::collectors::contract::DriverSecretAccessor for TestSecretAccessor {
        fn resolve_secret<'a>(
            &'a self,
            _handle: &'a OpaqueCredentialHandle,
            _purpose: CredentialSecretPurpose,
        ) -> BoxFuture<
            'a,
            Result<crate::services::collectors::contract::CredentialSecret, DriverFailure>,
        > {
            async move {
                Ok(
                    crate::services::collectors::contract::CredentialSecret::new(
                        "sub2api-access-token",
                    ),
                )
            }
            .boxed()
        }
    }

    struct LoginSecretAccessor;

    impl crate::services::collectors::contract::DriverSecretAccessor for LoginSecretAccessor {
        fn resolve_secret<'a>(
            &'a self,
            _handle: &'a OpaqueCredentialHandle,
            purpose: CredentialSecretPurpose,
        ) -> BoxFuture<
            'a,
            Result<crate::services::collectors::contract::CredentialSecret, DriverFailure>,
        > {
            async move {
                match purpose {
                    CredentialSecretPurpose::LoginPassword => Ok(
                        crate::services::collectors::contract::CredentialSecret::new(
                            "fixture-login-password",
                        ),
                    ),
                    _ => Err(DriverFailure::unsupported(
                        "fixture only provides the login password",
                    )),
                }
            }
            .boxed()
        }
    }

    struct HybridSessionSecretAccessor;

    impl crate::services::collectors::contract::DriverSecretAccessor for HybridSessionSecretAccessor {
        fn resolve_secret<'a>(
            &'a self,
            _handle: &'a OpaqueCredentialHandle,
            purpose: CredentialSecretPurpose,
        ) -> BoxFuture<
            'a,
            Result<crate::services::collectors::contract::CredentialSecret, DriverFailure>,
        > {
            async move {
                let secret = match purpose {
                    CredentialSecretPurpose::AuthorizationHeader => "captured-jwt",
                    CredentialSecretPurpose::SessionCookie => {
                        "cf_clearance=clearance; session=browser"
                    }
                    CredentialSecretPurpose::LoginPassword => {
                        return Err(DriverFailure::unsupported("login password is unavailable"));
                    }
                };
                Ok(crate::services::collectors::contract::CredentialSecret::new(secret))
            }
            .boxed()
        }
    }

    fn test_station_identity() -> StationIdentity {
        StationIdentity {
            station_id: "station-1".to_string(),
            endpoint_revision: 7,
            provider: ProviderKind::Sub2Api,
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

    fn test_login_credential() -> Sub2ApiLoginCredential {
        Sub2ApiLoginCredential {
            username: "fixture-user".to_string(),
            password: OpaqueCredentialHandle {
                station_id: "station-1".to_string(),
                credential_revision: 7,
                scope: CredentialScope::LoginPassword,
            },
        }
    }

    fn login_test_context<'a>(
        base_url: &str,
        secrets: &'a LoginSecretAccessor,
        outbound: &'a AsyncOutboundClient,
    ) -> CollectorContext<'a> {
        CollectorContext {
            station: test_station_identity(),
            endpoints: test_endpoints(base_url),
            credential: test_credential(),
            auth: None,
            user_agent: None,
            secrets,
            outbound,
            proxy: ProxyPolicy::Direct,
            budget: RequestBudget::from_now(Duration::from_secs(5)),
            cancellation: CancellationToken::new(),
            correlation_id: "test-correlation".to_string(),
        }
    }

    #[test]
    fn cookie_shaped_session_is_distinguished_from_bearer_token() {
        assert!(looks_like_cookie_header("cf_clearance=fake; session=abc"));
        assert!(!looks_like_cookie_header("eyJhbGciOiJIUzI1NiJ9.fake"));
    }

    #[test]
    fn login_cookie_headers_become_a_reusable_cookie_credential() {
        let mut headers = http::HeaderMap::new();
        headers.append(
            header::SET_COOKIE,
            HeaderValue::from_static("session=login-session; Path=/; HttpOnly"),
        );
        headers.append(
            header::SET_COOKIE,
            HeaderValue::from_static("cf_clearance=clearance-token; Path=/; Secure"),
        );

        assert_eq!(
            extract_login_cookie(&headers).as_deref(),
            Some("session=login-session; cf_clearance=clearance-token")
        );
    }

    #[tokio::test]
    async fn login_session_reports_a_rejected_login_status_without_response_content() {
        let server = TestHttpServer::sequence(vec![Some(json_response(
            401,
            json!({"message": "fixture rejection"}),
        ))]);
        let outbound = AsyncOutboundClient::new(AsyncOutboundClientConfig::architecture_budget());
        let secrets = LoginSecretAccessor;
        let context = login_test_context(&server.base_url, &secrets, &outbound);

        let resolution = login_session(&context, &server.base_url, &test_login_credential())
            .await
            .expect("login response should be classified");
        let requests = server.finish();

        assert!(matches!(
            &resolution,
            LoginSessionResolution::InteractiveAuthorizationRequired { status: 401 }
        ));
        assert_eq!(
            resolution.remote_key_detail(),
            "Sub2API login was rejected or requires interactive authorization (HTTP 401)."
        );
        assert_eq!(requests.len(), 1);
    }

    #[tokio::test]
    async fn login_session_reports_a_successful_response_without_a_session() {
        let server = TestHttpServer::sequence(vec![Some(json_response(200, json!({"data": {}})))]);
        let outbound = AsyncOutboundClient::new(AsyncOutboundClientConfig::architecture_budget());
        let secrets = LoginSecretAccessor;
        let context = login_test_context(&server.base_url, &secrets, &outbound);

        let resolution = login_session(&context, &server.base_url, &test_login_credential())
            .await
            .expect("login response should be classified");
        let requests = server.finish();

        assert!(matches!(
            &resolution,
            LoginSessionResolution::SuccessfulWithoutSession { status: 200 }
        ));
        assert_eq!(
            resolution.remote_key_detail(),
            "Sub2API login succeeded but did not return a usable management session (HTTP 200)."
        );
        assert_eq!(requests.len(), 1);
    }

    #[tokio::test]
    async fn login_session_accepts_a_nested_access_token_response() {
        let server = TestHttpServer::sequence(vec![Some(json_response(
            200,
            json!({"data": {"access_token": "fixture-jwt"}}),
        ))]);
        let outbound = AsyncOutboundClient::new(AsyncOutboundClientConfig::architecture_budget());
        let secrets = LoginSecretAccessor;
        let context = login_test_context(&server.base_url, &secrets, &outbound);

        let resolution = login_session(&context, &server.base_url, &test_login_credential())
            .await
            .expect("nested session should be accepted");
        let requests = server.finish();
        let session = resolution.into_session().expect("a login session");

        assert_eq!(session.access_token, "fixture-jwt");
        assert!(session.cookie.is_none());
        assert_eq!(requests.len(), 1);
    }

    #[tokio::test]
    async fn remote_key_list_reuses_token_and_cookie_from_password_login() {
        let login_body = json!({"access_token": "fixture-jwt"}).to_string();
        let login_response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nSet-Cookie: session=fixture-session; Path=/; HttpOnly\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{login_body}",
            login_body.len(),
        );
        let server = TestHttpServer::sequence(vec![
            Some(login_response),
            Some(json_response(
                200,
                json!({
                    "data": {
                        "items": [
                            { "id": "key-1", "name": "fixture-key", "key_masked": "sk-fixture********" }
                        ],
                        "total": 1,
                        "page": 1,
                        "page_size": 50,
                        "pages": 1
                    }
                }),
            )),
        ]);
        let outbound = AsyncOutboundClient::new(AsyncOutboundClientConfig::architecture_budget());
        let secrets = LoginSecretAccessor;
        let credential = test_credential();
        let context = CollectorContext {
            station: test_station_identity(),
            endpoints: test_endpoints(&server.base_url),
            credential: credential.clone(),
            auth: Some(ProviderAuthContext::Sub2Api {
                station_keys: Vec::new(),
                access_token: None,
                session_cookie: None,
                login: Some(Sub2ApiLoginCredential {
                    username: "fixture-user".to_string(),
                    password: OpaqueCredentialHandle {
                        station_id: "station-1".to_string(),
                        credential_revision: 7,
                        scope: CredentialScope::LoginPassword,
                    },
                }),
                credit_per_cny: 1.0,
            }),
            user_agent: None,
            secrets: &secrets,
            outbound: &outbound,
            proxy: ProxyPolicy::Direct,
            budget: RequestBudget::from_now(Duration::from_secs(5)),
            cancellation: CancellationToken::new(),
            correlation_id: "test-correlation".to_string(),
        };
        let auth = sub2api_auth(&context).expect("Sub2API auth context");
        let mut session = resolve_remote_key_list_session(&context, &server.base_url, &auth)
            .await
            .expect("password login should create a remote-key session");
        assert_eq!(
            session.cookie.as_deref(),
            Some("session=fixture-session"),
            "password login session cookie should be retained"
        );

        let output = fetch_remote_key_list(
            &context,
            &server.base_url,
            &auth,
            &mut session.access_token,
            session.cookie.as_deref(),
        )
        .await
        .expect("remote key list should use the password-login session");
        let requests = server.finish();

        assert!(output.ok);
        assert_eq!(mapping::remote_key_items(&output.payload).len(), 1);
        assert_eq!(requests.len(), 2);
        assert!(requests[0].starts_with("POST /api/v1/auth/login HTTP/1.1"));
        let remote_key_request = requests[1].to_ascii_lowercase();
        assert!(remote_key_request.contains("authorization: bearer fixture-jwt"));
        assert!(
            remote_key_request.contains("cookie: session=fixture-session"),
            "session cookie: {:?}; captured remote-key request: {remote_key_request}",
            session.cookie
        );
    }

    #[test]
    fn management_request_keeps_bearer_and_browser_cookie_together() {
        let outbound = AsyncOutboundClient::new(AsyncOutboundClientConfig::architecture_budget());
        let secrets = TestSecretAccessor;
        let context = test_context("https://relay.example", &secrets, &outbound);
        let policy = OutboundHeaderPolicy::provider_default();

        let request = build_json_request(
            &context,
            EndpointRole::Groups,
            "https://relay.example/api/v1/groups/available",
            "captured-jwt",
            Some("cf_clearance=clearance; session=browser"),
            None,
            Method::GET,
        )
        .expect("request should build");
        let headers = request
            .headers
            .materialize(&policy)
            .expect("headers should materialize");

        assert_eq!(
            headers
                .get(header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            Some("Bearer captured-jwt")
        );
        assert_eq!(
            headers
                .get(header::COOKIE)
                .and_then(|value| value.to_str().ok()),
            Some("cf_clearance=clearance; session=browser")
        );
    }

    #[test]
    fn management_request_reuses_captured_browser_user_agent() {
        let outbound = AsyncOutboundClient::new(AsyncOutboundClientConfig::architecture_budget());
        let secrets = TestSecretAccessor;
        let mut context = test_context("https://relay.example", &secrets, &outbound);
        context.user_agent = Some(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/151.0.0.0"
                .to_string(),
        );
        let policy = OutboundHeaderPolicy::provider_default();

        let request = build_json_request(
            &context,
            EndpointRole::Groups,
            "https://relay.example/api/v1/groups/available",
            "captured-jwt",
            Some("cf_clearance=clearance; session=browser"),
            None,
            Method::GET,
        )
        .expect("request should build");
        let headers = request
            .headers
            .materialize(&policy)
            .expect("headers should materialize");

        assert_eq!(
            headers
                .get(header::USER_AGENT)
                .and_then(|value| value.to_str().ok()),
            Some("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/151.0.0.0")
        );
    }

    #[test]
    fn cookie_only_management_request_does_not_add_bearer_header() {
        let outbound = AsyncOutboundClient::new(AsyncOutboundClientConfig::architecture_budget());
        let secrets = TestSecretAccessor;
        let context = test_context("https://relay.example", &secrets, &outbound);
        let policy = OutboundHeaderPolicy::provider_default();

        let request = build_json_request(
            &context,
            EndpointRole::Groups,
            "https://relay.example/api/v1/groups/available",
            "cf_clearance=clearance; session=browser",
            Some("cf_clearance=clearance; session=browser"),
            None,
            Method::GET,
        )
        .expect("request should build");
        let headers = request
            .headers
            .materialize(&policy)
            .expect("headers should materialize");

        assert!(headers.get(header::AUTHORIZATION).is_none());
        assert_eq!(
            headers
                .get(header::COOKIE)
                .and_then(|value| value.to_str().ok()),
            Some("cf_clearance=clearance; session=browser")
        );
    }

    #[tokio::test]
    async fn auth_rejection_retries_the_fresh_browser_cookie_without_bearer() {
        let server = TestHttpServer::sequence(vec![
            Some(json_response(401, json!({"error": "expired token"}))),
            Some(json_response(200, json!({"data": []}))),
        ]);
        let outbound = AsyncOutboundClient::new(AsyncOutboundClientConfig::architecture_budget());
        let secrets = HybridSessionSecretAccessor;
        let credential = test_credential();
        let context = CollectorContext {
            station: test_station_identity(),
            endpoints: test_endpoints(&server.base_url),
            credential: credential.clone(),
            auth: Some(ProviderAuthContext::Sub2Api {
                station_keys: Vec::new(),
                access_token: Some(credential.clone()),
                session_cookie: Some(credential),
                login: None,
                credit_per_cny: 1.0,
            }),
            user_agent: None,
            secrets: &secrets,
            outbound: &outbound,
            proxy: ProxyPolicy::Direct,
            budget: RequestBudget::from_now(Duration::from_secs(5)),
            cancellation: CancellationToken::new(),
            correlation_id: "test-correlation".to_string(),
        };
        let auth = sub2api_auth(&context).expect("auth context should resolve");
        let mut token = "captured-jwt".to_string();

        let execution = execute_bearer_json_with_recovery(
            &context,
            EndpointRole::Groups,
            &server.base_url,
            "/api/v1/groups/available",
            &mut token,
            &auth,
        )
        .await
        .expect("cookie fallback should succeed");
        let requests = server.finish();

        assert!(execution.ok);
        assert_eq!(token, "cf_clearance=clearance; session=browser");
        assert_eq!(requests.len(), 2);
        let first_request = requests[0].to_ascii_lowercase();
        let second_request = requests[1].to_ascii_lowercase();
        assert!(first_request.contains("authorization: bearer captured-jwt"));
        assert!(first_request.contains("cookie: cf_clearance=clearance; session=browser"));
        assert!(!second_request.contains("authorization: bearer captured-jwt"));
        assert!(second_request.contains("cookie: cf_clearance=clearance; session=browser"));
    }

    #[tokio::test]
    async fn exhausted_saved_session_is_classified_for_manual_authorization() {
        let server = TestHttpServer::sequence(vec![
            Some(json_response(401, json!({"error": "expired token"}))),
            Some(json_response(401, json!({"error": "expired session"}))),
        ]);
        let outbound = AsyncOutboundClient::new(AsyncOutboundClientConfig::architecture_budget());
        let secrets = HybridSessionSecretAccessor;
        let credential = test_credential();
        let context = CollectorContext {
            station: test_station_identity(),
            endpoints: test_endpoints(&server.base_url),
            credential: credential.clone(),
            auth: Some(ProviderAuthContext::Sub2Api {
                station_keys: Vec::new(),
                access_token: Some(credential.clone()),
                session_cookie: Some(credential),
                login: None,
                credit_per_cny: 1.0,
            }),
            user_agent: None,
            secrets: &secrets,
            outbound: &outbound,
            proxy: ProxyPolicy::Direct,
            budget: RequestBudget::from_now(Duration::from_secs(5)),
            cancellation: CancellationToken::new(),
            correlation_id: "test-correlation".to_string(),
        };
        let auth = sub2api_auth(&context).expect("auth context should resolve");
        let mut token = "captured-jwt".to_string();

        let execution = execute_bearer_json_with_recovery(
            &context,
            EndpointRole::Groups,
            &server.base_url,
            "/api/v1/groups/available",
            &mut token,
            &auth,
        )
        .await
        .expect("authorization failure should remain a typed result");
        server.finish();

        assert!(!execution.ok);
        assert_eq!(
            execution.redacted["failureKind"],
            json!("manual_authorization_required")
        );
        let failure = failed_from_endpoint_results(
            &[execution.redacted],
            EndpointRole::Groups,
            Some(vec![execution.evidence]),
            "saved session is no longer usable",
        );
        assert_eq!(failure.kind, DriverFailureKind::AuthRejected);
        assert_eq!(failure.auth_effect, AuthEffect::Reauthorize);
    }

    fn test_context<'a>(
        base_url: &str,
        secrets: &'a TestSecretAccessor,
        outbound: &'a AsyncOutboundClient,
    ) -> CollectorContext<'a> {
        let access_token = test_credential();
        CollectorContext {
            station: test_station_identity(),
            endpoints: test_endpoints(base_url),
            credential: access_token.clone(),
            auth: Some(ProviderAuthContext::Sub2Api {
                station_keys: Vec::new(),
                access_token: Some(access_token),
                session_cookie: None,
                login: None,
                credit_per_cny: 1.0,
            }),
            user_agent: None,
            secrets,
            outbound,
            proxy: ProxyPolicy::Direct,
            budget: RequestBudget::from_now(Duration::from_secs(5)),
            cancellation: CancellationToken::new(),
            correlation_id: "test-correlation".to_string(),
        }
    }

    #[tokio::test]
    async fn remote_key_list_accepts_a_server_clamped_page_size_and_missing_pages() {
        let server = TestHttpServer::sequence(vec![
            Some(json_response(
                200,
                json!({
                    "data": {
                        "items": [
                            { "id": "key-1", "name": "first", "key_masked": "sk-one********" },
                            { "id": "key-2", "name": "second", "key_masked": "sk-two********" }
                        ],
                        "total": 3,
                        "page": 1,
                        "page_size": 20,
                        "pages": 2
                    }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "data": {
                        "items": [
                            { "id": "key-3", "name": "third", "key_masked": "sk-three******" }
                        ],
                        "total": 3,
                        "page": 2,
                        "page_size": 20
                    }
                }),
            )),
        ]);
        let outbound = AsyncOutboundClient::new(AsyncOutboundClientConfig::architecture_budget());
        let secrets = TestSecretAccessor;
        let context = test_context(&server.base_url, &secrets, &outbound);
        let auth = sub2api_auth(&context).expect("Sub2API auth context");
        let mut access_token = "sub2api-access-token".to_string();

        let output =
            fetch_remote_key_list(&context, &server.base_url, &auth, &mut access_token, None)
                .await
                .expect("remote key list should tolerate compatible pagination");
        let requests = server.finish();

        assert!(output.ok);
        assert_eq!(mapping::remote_key_items(&output.payload).len(), 3);
        assert_eq!(requests.len(), 2);
        assert!(requests[0].starts_with("GET /api/v1/keys?page=1&page_size=100 "));
        assert!(requests[1].starts_with("GET /api/v1/keys?page=2&page_size=100 "));
    }

    #[tokio::test]
    async fn remote_key_list_accepts_an_empty_list_with_one_reported_page() {
        let server = TestHttpServer::sequence(vec![Some(json_response(
            200,
            json!({
                "data": {
                    "items": [],
                    "total": 0,
                    "page": 1,
                    "page_size": 20,
                    "pages": 1
                }
            }),
        ))]);
        let outbound = AsyncOutboundClient::new(AsyncOutboundClientConfig::architecture_budget());
        let secrets = TestSecretAccessor;
        let context = test_context(&server.base_url, &secrets, &outbound);
        let auth = sub2api_auth(&context).expect("Sub2API auth context");
        let mut access_token = "sub2api-access-token".to_string();

        let output =
            fetch_remote_key_list(&context, &server.base_url, &auth, &mut access_token, None)
                .await
                .expect("an empty remote key list is valid");
        let requests = server.finish();

        assert!(output.ok);
        assert!(mapping::remote_key_items(&output.payload).is_empty());
        assert_eq!(requests.len(), 1);
        assert!(requests[0].starts_with("GET /api/v1/keys?page=1&page_size=100 "));
    }

    fn test_balance_context<'a>(
        base_url: &str,
        secrets: &'a TestSecretAccessor,
        outbound: &'a AsyncOutboundClient,
    ) -> CollectorContext<'a> {
        let access_token = test_credential();
        let station_key = OpaqueCredentialHandle {
            station_id: "station-1".to_string(),
            credential_revision: 7,
            scope: CredentialScope::StationKey,
        };
        CollectorContext {
            station: test_station_identity(),
            endpoints: ProviderEndpoints {
                api_base_url: Some(base_url.to_string()),
                website_url: Some(base_url.to_string()),
            },
            credential: access_token.clone(),
            auth: Some(ProviderAuthContext::Sub2Api {
                station_keys: vec![Sub2ApiStationKeyCredential {
                    station_key_id: "key-1".to_string(),
                    credential: station_key,
                }],
                access_token: Some(access_token),
                session_cookie: None,
                login: None,
                credit_per_cny: 27.0,
            }),
            user_agent: None,
            secrets,
            outbound,
            proxy: ProxyPolicy::Direct,
            budget: RequestBudget::from_now(Duration::from_secs(5)),
            cancellation: CancellationToken::new(),
            correlation_id: "test-correlation".to_string(),
        }
    }

    #[tokio::test]
    async fn balance_collection_adds_subscription_quota_to_station_balance() {
        let server = TestHttpServer::sequence(vec![
            Some(json_response(
                200,
                json!({"quota": {"remaining": 0.0, "used": 0.0}}),
            )),
            Some(json_response(200, json!({"data": {"balance": 0.0}}))),
            Some(json_response(
                200,
                json!({
                    "data": [{
                        "id": 12,
                        "status": "active",
                        "daily_usage_usd": 0.0,
                        "group": { "daily_limit_usd": 135.0 }
                    }]
                }),
            )),
            Some(json_response(200, json!({}))),
        ]);
        let outbound = AsyncOutboundClient::new(AsyncOutboundClientConfig::architecture_budget());
        let secrets = TestSecretAccessor;
        let context = test_balance_context(&server.base_url, &secrets, &outbound);

        let output = collect_balance(&context).await.expect("collect balance");
        let requests = server.finish();
        let station_balance = output
            .facts
            .balances
            .iter()
            .find(|balance| balance.scope == "station")
            .expect("station balance");

        assert_eq!(
            station_balance.value,
            Some(5.0),
            "unexpected balances: {:?}; requests: {:?}",
            output.facts.balances,
            requests
        );
        assert_eq!(requests.len(), 4);
        assert!(
            requests[0].starts_with("GET /usage "),
            "unexpected request sequence: {requests:?}"
        );
        assert!(requests[1].starts_with("GET /api/v1/user/profile "));
        assert!(requests[2].starts_with("GET /api/v1/subscriptions/active "));
        assert!(requests[3].starts_with("GET /api/v1/usage/dashboard/stats "));
        assert!(!requests
            .iter()
            .any(|request| request.contains("/api/v1/user/platform-quotas")));
    }

    #[tokio::test]
    async fn delete_remote_key_uses_provider_id_and_reconciles_absence() {
        let target_item = json!({
            "id": "key-301",
            "name": "relay-delete",
            "key_masked": "sk-del**********f260",
            "group_name": "VIP"
        });
        let remote_key_id = mapping::parse_remote_key_payload(
            "station-1",
            &json!({ "data": { "items": [target_item.clone()] } }),
        )
        .into_iter()
        .next()
        .expect("remote key")
        .id;
        let server = TestHttpServer::sequence(vec![
            Some(json_response(
                200,
                json!({
                    "data": {
                        "items": [{ "id": "key-100", "name": "keep" }],
                        "total": 2, "page": 1, "page_size": 1, "pages": 2
                    }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "data": {
                        "items": [target_item],
                        "total": 2, "page": 2, "page_size": 1, "pages": 2
                    }
                }),
            )),
            Some(json_response(200, json!({ "success": true }))),
            Some(json_response(
                200,
                json!({
                    "data": {
                        "items": [{ "id": "key-100", "name": "keep" }],
                        "total": 1, "page": 1, "page_size": 1, "pages": 1
                    }
                }),
            )),
        ]);
        let outbound = AsyncOutboundClient::new(AsyncOutboundClientConfig::architecture_budget());
        let secrets = TestSecretAccessor;
        let context = test_context(&server.base_url, &secrets, &outbound);
        let request = DeleteRemoteKeyRequest {
            station: test_station_identity(),
            endpoints: test_endpoints(&server.base_url),
            credential: test_credential(),
            remote_key_id,
        };

        let output = Sub2ApiRemoteKeyDriver
            .delete_remote_key(&context, request)
            .await
            .expect("delete remote key");
        let requests = server.finish();

        assert!(!output.already_absent);
        assert_eq!(output.keys.len(), 1);
        assert_eq!(requests.len(), 4);
        assert!(requests[0].starts_with("GET /api/v1/keys?page=1&page_size=100 "));
        assert!(requests[1].starts_with("GET /api/v1/keys?page=2&page_size=100 "));
        assert!(requests[2].starts_with("DELETE /api/v1/keys/key-301 "));
        assert!(requests[3].starts_with("GET /api/v1/keys?page=1&page_size=100 "));
        assert!(requests[2]
            .to_ascii_lowercase()
            .contains("authorization: bearer sub2api-access-token"));
    }

    #[tokio::test]
    async fn delete_remote_key_reports_unknown_when_reconciliation_still_contains_target() {
        let item = json!({
            "id": "key-302",
            "name": "relay-still-present",
            "key_masked": "sk-stl**********f260"
        });
        let list_payload = || {
            json!({
                "data": {
                    "items": [item.clone()],
                    "total": 1, "page": 1, "page_size": 100, "pages": 1
                }
            })
        };
        let remote_key_id = mapping::parse_remote_key_payload("station-1", &list_payload())
            .into_iter()
            .next()
            .expect("remote key")
            .id;
        let server = TestHttpServer::sequence(vec![
            Some(json_response(200, list_payload())),
            Some(json_response(200, json!({ "success": true }))),
            Some(json_response(200, list_payload())),
        ]);
        let outbound = AsyncOutboundClient::new(AsyncOutboundClientConfig::architecture_budget());
        let secrets = TestSecretAccessor;
        let context = test_context(&server.base_url, &secrets, &outbound);
        let request = DeleteRemoteKeyRequest {
            station: test_station_identity(),
            endpoints: test_endpoints(&server.base_url),
            credential: test_credential(),
            remote_key_id,
        };

        let result = Sub2ApiRemoteKeyDriver
            .delete_remote_key(&context, request)
            .await;
        let error = match result {
            Ok(_) => panic!("reconciliation should report an unknown result"),
            Err(error) => error,
        };
        let requests = server.finish();

        assert_eq!(error.kind, DriverFailureKind::ResultUnknown);
        assert_eq!(requests.len(), 3);
    }

    #[test]
    fn delete_remote_key_requires_a_provider_id_for_a_matching_discovery() {
        let payload = json!({ "data": { "items": [{ "name": "name-only" }] } });
        let remote_key_id = mapping::parse_remote_key_payload("station-1", &payload)[0]
            .id
            .clone();

        let error = remote_key_provider_id_from_list_payload("station-1", &remote_key_id, &payload)
            .unwrap_err();

        assert_eq!(error.kind, DriverFailureKind::InvalidRequest);
    }
}
