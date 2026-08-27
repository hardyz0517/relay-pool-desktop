use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
    time::{Duration, Instant},
};

use chrono::Utc;
use http::{header, HeaderValue};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

#[cfg(test)]
use crate::outbound::OutboundHeaderPolicy;

use crate::{
    application::{error::ApplicationError, pagination::PageLimit, pricing::PricingService},
    background_tasks::{TaskFailure, TaskId, TaskRunContext, TaskSpec, TaskSupervisor},
    models::pricing::{ModelBasePrice, UpsertModelBasePriceInput},
    outbound::{
        AsyncOutboundClient, ManualProxy, OutboundFailure, OutboundFailureKind, OutboundRequest,
        OutboundResponse, ProxyPolicy, RequestBudget,
    },
};

pub(crate) const MODELS_DEV_URL: &str = "https://models.dev/api.json";
pub(crate) const MODEL_PRICE_SYNC_TASK_ID: &str = "model-price-sync-v1";
const DOCUMENT_VERSION: u32 = 3;
const SOURCE_LABEL: &str = "models.dev";
const SYNC_INTERVAL_MS: i64 = 6 * 60 * 60 * 1000;
const MAX_REMOTE_CATALOG_BYTES: usize = 8 * 1024 * 1024;
const MAX_LOCAL_CATALOG_BYTES: usize = 32 * 1024 * 1024;
const MAX_MODEL_SELECTION_KEYS: usize = 20_000;
const MODELS_DEV_FETCH_ATTEMPTS: usize = 2;
const MODELS_DEV_FETCH_BUDGET: Duration = Duration::from_secs(15);
const MODELS_DEV_RETRY_DELAY: Duration = Duration::from_millis(750);
const COMMON_MODEL_LIMIT_PER_FAMILY: usize = 6;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelPriceSyncConfig {
    pub auto_sync_enabled: bool,
    #[serde(default = "default_true")]
    pub include_common_models: bool,
    #[serde(default)]
    pub selected_model_keys: Vec<String>,
    #[serde(default)]
    pub excluded_common_model_keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelPriceCatalogDocument {
    version: u32,
    source: CatalogSource,
    sync: CatalogSync,
    models: Vec<CatalogModel>,
    #[serde(default)]
    overrides: Vec<UpsertModelBasePriceInput>,
    #[serde(default)]
    deleted_model_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogSource {
    kind: String,
    url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogSync {
    auto_sync_enabled: bool,
    #[serde(default = "default_true")]
    include_common_models: bool,
    selected_model_keys: Vec<String>,
    excluded_common_model_keys: Vec<String>,
    last_sync_at: Option<String>,
    last_sync_error: Option<String>,
    etag: Option<String>,
}

impl Default for CatalogSync {
    fn default() -> Self {
        Self {
            auto_sync_enabled: false,
            include_common_models: true,
            selected_model_keys: Vec::new(),
            excluded_common_model_keys: Vec::new(),
            last_sync_at: None,
            last_sync_error: None,
            etag: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogModel {
    key: String,
    provider: String,
    model: String,
    name: String,
    #[serde(default)]
    family: Option<String>,
    common: bool,
    #[serde(default)]
    selected: bool,
    release_date: Option<String>,
    input_price: Option<f64>,
    output_price: Option<f64>,
    cache_creation_price: Option<f64>,
    cache_read_price: Option<f64>,
    source_url: String,
    source_checked_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelPriceSyncState {
    pub source_url: String,
    pub auto_sync_enabled: bool,
    pub include_common_models: bool,
    pub selected_model_keys: Vec<String>,
    pub excluded_common_model_keys: Vec<String>,
    pub last_sync_at: Option<String>,
    pub last_sync_error: Option<String>,
    pub model_count: usize,
    pub common_model_count: usize,
    pub auto_sync_model_count: usize,
    pub file_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelPriceCatalogEntry {
    pub key: String,
    pub provider: String,
    pub model: String,
    pub name: String,
    pub common: bool,
    pub release_date: Option<String>,
    pub input_price: Option<f64>,
    pub output_price: Option<f64>,
    pub cache_creation_price: Option<f64>,
    pub cache_read_price: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelPriceSyncResult {
    pub state: ModelPriceSyncState,
    pub imported_count: usize,
    pub skipped_count: usize,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ModelPriceSyncError {
    #[error(transparent)]
    Application(#[from] ApplicationError),
    #[error("models.dev is unavailable")]
    ExternalUnavailable { upstream_status: Option<u16> },
}

#[derive(Clone)]
pub(crate) struct ModelPriceSyncService {
    pricing: Arc<PricingService>,
    outbound: AsyncOutboundClient,
    catalog_path: PathBuf,
    operation_lock: Arc<tokio::sync::Mutex<()>>,
}

impl ModelPriceSyncService {
    pub(crate) fn new(
        pricing: Arc<PricingService>,
        outbound: AsyncOutboundClient,
        data_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            pricing,
            outbound,
            catalog_path: data_dir.into().join("model-pricing.json"),
            operation_lock: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    pub(crate) fn state(&self) -> Result<ModelPriceSyncState, ApplicationError> {
        let document = self
            .load_document()
            .map_err(|_| ApplicationError::IoFailed)?;
        Ok(state_from_document(&document, &self.catalog_path))
    }

    pub(crate) fn catalog_entries(&self) -> Result<Vec<ModelPriceCatalogEntry>, ApplicationError> {
        let document = self
            .load_document()
            .map_err(|_| ApplicationError::IoFailed)?;
        Ok(filtered_catalog_models(document.models)
            .into_iter()
            .map(model_to_catalog_entry)
            .collect())
    }

    pub(crate) fn open_catalog_directory(&self) -> Result<(), ApplicationError> {
        let directory = self
            .catalog_path
            .parent()
            .ok_or(ApplicationError::IoFailed)?;
        fs::create_dir_all(directory).map_err(|_| ApplicationError::IoFailed)?;
        open_path_with_system(directory).map_err(|_| ApplicationError::IoFailed)
    }

    pub(crate) async fn upsert_local_price(
        &self,
        mut input: UpsertModelBasePriceInput,
    ) -> Result<ModelBasePrice, ApplicationError> {
        let _guard = self.operation_lock.lock().await;
        let was_builtin = input.built_in;
        input.built_in = false;
        if was_builtin
            || input.source_label == SOURCE_LABEL
            || input.source_label.trim().is_empty()
            || input.source_label.contains("model pricing catalog")
        {
            input.source_label = "Manual override".into();
        }
        let saved = self.pricing.upsert_model_base_price(input).await?;
        let mut document = self
            .load_document()
            .map_err(|_| ApplicationError::IoFailed)?;
        let override_input = model_base_price_to_input(&saved);
        upsert_override(&mut document.overrides, override_input);
        document
            .deleted_model_ids
            .retain(|id| !id.eq_ignore_ascii_case(&saved.id));
        self.write_document(&document)
            .map_err(|_| ApplicationError::IoFailed)?;
        Ok(saved)
    }

    pub(crate) async fn delete_local_price(&self, id: String) -> Result<(), ApplicationError> {
        let id = id.trim().to_string();
        if id.is_empty() {
            return Err(ApplicationError::ConstraintViolation);
        }
        let _guard = self.operation_lock.lock().await;
        self.pricing.delete_model_base_price(id.clone()).await?;
        let mut document = self
            .load_document()
            .map_err(|_| ApplicationError::IoFailed)?;
        document.overrides.retain(|entry| {
            entry
                .id
                .as_deref()
                .is_none_or(|override_id| !override_id.eq_ignore_ascii_case(&id))
        });
        if !document
            .deleted_model_ids
            .iter()
            .any(|deleted_id| deleted_id.eq_ignore_ascii_case(&id))
        {
            document.deleted_model_ids.push(id);
            document.deleted_model_ids.sort();
        }
        self.write_document(&document)
            .map_err(|_| ApplicationError::IoFailed)
    }

    pub(crate) async fn reset_to_builtins(&self) -> Result<Vec<ModelBasePrice>, ApplicationError> {
        let _guard = self.operation_lock.lock().await;
        let rows = self
            .pricing
            .reset_model_base_prices_to_builtins(
                PageLimit::new(500).expect("bounded model price limit"),
            )
            .await?;
        let builtin_ids = rows
            .iter()
            .filter(|row| row.built_in)
            .map(|row| row.id.as_str())
            .collect::<BTreeSet<_>>();
        let mut document = self
            .load_document()
            .map_err(|_| ApplicationError::IoFailed)?;
        document
            .deleted_model_ids
            .retain(|id| !builtin_ids.contains(id.as_str()));
        document.overrides.retain(|entry| {
            entry
                .id
                .as_deref()
                .is_none_or(|id| !builtin_ids.contains(id))
        });
        self.write_document(&document)
            .map_err(|_| ApplicationError::IoFailed)?;
        Ok(rows)
    }

    pub(crate) async fn save_config(
        &self,
        config: ModelPriceSyncConfig,
    ) -> Result<ModelPriceSyncState, ApplicationError> {
        validate_config(&config)?;
        let _guard = self.operation_lock.lock().await;
        let mut document = self
            .load_document()
            .map_err(|_| ApplicationError::IoFailed)?;
        document.sync.auto_sync_enabled = config.auto_sync_enabled;
        document.sync.include_common_models = config.include_common_models;
        document.sync.selected_model_keys = normalize_keys(config.selected_model_keys);
        document.sync.excluded_common_model_keys =
            normalize_keys(config.excluded_common_model_keys);
        self.write_document(&document)
            .map_err(|_| ApplicationError::IoFailed)?;
        Ok(state_from_document(&document, &self.catalog_path))
    }

    pub(crate) async fn sync(
        &self,
        force: bool,
    ) -> Result<ModelPriceSyncResult, ModelPriceSyncError> {
        let _guard = self.operation_lock.lock().await;
        let mut document = self
            .load_document()
            .map_err(|_| ApplicationError::IoFailed)?;
        let migrating_legacy_catalog = document.version < DOCUMENT_VERSION;
        if !force && !document.sync.auto_sync_enabled {
            return Ok(ModelPriceSyncResult {
                state: state_from_document(&document, &self.catalog_path),
                imported_count: 0,
                skipped_count: 0,
            });
        }
        if !force && !sync_is_due(document.sync.last_sync_at.as_deref()) {
            return Ok(ModelPriceSyncResult {
                state: state_from_document(&document, &self.catalog_path),
                imported_count: 0,
                skipped_count: 0,
            });
        }
        if !force
            && !document.sync.include_common_models
            && document.sync.selected_model_keys.is_empty()
        {
            return Ok(ModelPriceSyncResult {
                state: state_from_document(&document, &self.catalog_path),
                imported_count: 0,
                skipped_count: 0,
            });
        }

        let response = match self
            .fetch_models_dev_catalog(
                (!migrating_legacy_catalog)
                    .then_some(document.sync.etag.as_deref())
                    .flatten(),
            )
            .await
        {
            Ok(response) => response,
            Err(failure) => {
                document.sync.last_sync_error = Some(outbound_failure_message(&failure).into());
                self.write_document(&document)
                    .map_err(|_| ApplicationError::IoFailed)?;
                return Err(ModelPriceSyncError::ExternalUnavailable {
                    upstream_status: None,
                });
            }
        };
        if response.status == http::StatusCode::NOT_MODIFIED && !migrating_legacy_catalog {
            document.version = DOCUMENT_VERSION;
            document.sync.last_sync_at = Some(Utc::now().to_rfc3339());
            document.sync.last_sync_error = None;
            self.write_document(&document)
                .map_err(|_| ApplicationError::IoFailed)?;
            return Ok(ModelPriceSyncResult {
                state: state_from_document(&document, &self.catalog_path),
                imported_count: 0,
                skipped_count: 0,
            });
        }
        if !response.status.is_success() {
            document.sync.last_sync_error =
                Some(format!("models.dev 返回 HTTP {}", response.status));
            self.write_document(&document)
                .map_err(|_| ApplicationError::IoFailed)?;
            return Err(ModelPriceSyncError::ExternalUnavailable {
                upstream_status: Some(response.status.as_u16()),
            });
        }
        let remote: Value = match serde_json::from_slice(&response.body) {
            Ok(remote) => remote,
            Err(_) => {
                document.sync.last_sync_error = Some("models.dev 返回了无效 JSON".into());
                self.write_document(&document)
                    .map_err(|_| ApplicationError::IoFailed)?;
                return Err(ModelPriceSyncError::ExternalUnavailable {
                    upstream_status: Some(response.status.as_u16()),
                });
            }
        };
        let parsed = match parse_models_dev(&remote) {
            Ok(parsed) => parsed,
            Err(_) => {
                document.sync.last_sync_error = Some("models.dev 目录格式不受支持".into());
                self.write_document(&document)
                    .map_err(|_| ApplicationError::IoFailed)?;
                return Err(ModelPriceSyncError::ExternalUnavailable {
                    upstream_status: Some(response.status.as_u16()),
                });
            }
        };
        document.version = DOCUMENT_VERSION;
        let selected = select_models(parsed, &document.sync);
        let skipped_count = selected.skipped_count;
        let now = Utc::now().to_rfc3339();
        let inputs = selected
            .models
            .iter()
            .map(catalog_to_input)
            .collect::<Vec<_>>();
        let previous_overrides = document
            .overrides
            .iter()
            .filter_map(|input| {
                input
                    .id
                    .as_deref()
                    .map(|id| (id.to_ascii_lowercase(), input))
            })
            .collect::<BTreeMap<_, _>>();
        let mut changed_inputs = Vec::new();
        let mut imported_count = 0;
        for input in &inputs {
            let previous = input
                .id
                .as_deref()
                .and_then(|id| previous_overrides.get(&id.to_ascii_lowercase()).copied());
            if sync_price_changed(previous, input) {
                imported_count += 1;
            }
            if sync_input_changed(previous, input) {
                changed_inputs.push(input.clone());
            }
        }
        if migrating_legacy_catalog {
            self.pricing
                .replace_models_dev_prices(inputs.clone())
                .await?;
            document
                .overrides
                .retain(|entry| !entry.source_label.eq_ignore_ascii_case(SOURCE_LABEL));
            document
                .deleted_model_ids
                .retain(|id| !id.starts_with("models-dev-"));
        } else if !changed_inputs.is_empty() {
            self.pricing
                .upsert_models_dev_prices(changed_inputs)
                .await?;
        }
        if !inputs.is_empty() {
            for input in inputs {
                if let Some(id) = input.id.as_deref() {
                    document
                        .deleted_model_ids
                        .retain(|deleted_id| !deleted_id.eq_ignore_ascii_case(id));
                }
                upsert_override(&mut document.overrides, input);
            }
        }
        document.models = selected.catalog_models;
        document.sync.last_sync_at = Some(now.clone());
        document.sync.last_sync_error = None;
        document.sync.etag = response
            .headers
            .get("etag")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        self.write_document(&document)
            .map_err(|_| ApplicationError::IoFailed)?;
        Ok(ModelPriceSyncResult {
            state: state_from_document(&document, &self.catalog_path),
            imported_count,
            skipped_count,
        })
    }

    pub(crate) async fn reload(&self) -> Result<ModelPriceSyncState, ApplicationError> {
        let _guard = self.operation_lock.lock().await;
        let document = self
            .load_document()
            .map_err(|_| ApplicationError::IoFailed)?;
        self.pricing.ensure_builtin_model_base_prices().await?;
        if !document.overrides.is_empty() {
            for input in document.overrides.clone() {
                self.pricing.upsert_model_base_price(input).await?;
            }
        }
        self.pricing
            .delete_model_base_prices_if_present(document.deleted_model_ids.clone())
            .await?;
        Ok(state_from_document(&document, &self.catalog_path))
    }

    fn load_document(&self) -> Result<ModelPriceCatalogDocument, std::io::Error> {
        match fs::read(&self.catalog_path) {
            Ok(bytes) => {
                if bytes.len() > MAX_LOCAL_CATALOG_BYTES {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "catalog too large",
                    ));
                }
                let document: ModelPriceCatalogDocument = serde_json::from_slice(&bytes)
                    .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
                if document.version > DOCUMENT_VERSION {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!(
                            "model pricing document version {} is newer than supported version {}",
                            document.version, DOCUMENT_VERSION
                        ),
                    ));
                }
                Ok(document)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(default_document()),
            Err(error) => Err(error),
        }
    }

    fn write_document(&self, document: &ModelPriceCatalogDocument) -> Result<(), std::io::Error> {
        let bytes = serde_json::to_vec_pretty(document).map_err(std::io::Error::other)?;
        if bytes.len() > MAX_LOCAL_CATALOG_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "catalog too large",
            ));
        }
        if let Some(parent) = self.catalog_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temporary = self.catalog_path.with_extension("json.tmp");
        fs::write(&temporary, bytes)?;
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&temporary)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, &self.catalog_path)?;
        Ok(())
    }

    async fn fetch_models_dev_catalog(
        &self,
        etag: Option<&str>,
    ) -> Result<OutboundResponse, OutboundFailure> {
        let deadline = Instant::now() + MODELS_DEV_FETCH_BUDGET;
        for attempt in 0..MODELS_DEV_FETCH_ATTEMPTS {
            let request = models_dev_request_with_budget(
                crate::services::outbound::current_system_proxy_url().as_deref(),
                RequestBudget::from_deadline(deadline),
                etag,
            )?;
            match self
                .outbound
                .execute_with_success_body_limit(
                    request,
                    CancellationToken::new(),
                    Some(MAX_REMOTE_CATALOG_BYTES),
                )
                .await
            {
                Ok(response) => return Ok(response),
                Err(failure)
                    if attempt + 1 < MODELS_DEV_FETCH_ATTEMPTS
                        && is_retryable_models_dev_failure(&failure) =>
                {
                    let Some(remaining) = RequestBudget::from_deadline(deadline).remaining() else {
                        return Err(failure);
                    };
                    tokio::time::sleep(MODELS_DEV_RETRY_DELAY.min(remaining)).await;
                }
                Err(failure) => return Err(failure),
            }
        }
        unreachable!("models.dev fetch attempts must be positive")
    }
}

#[cfg(test)]
fn models_dev_request_with_system_proxy(
    system_proxy_url: Option<&str>,
) -> Result<OutboundRequest, OutboundFailure> {
    models_dev_request_with_budget(
        system_proxy_url,
        RequestBudget::from_now(MODELS_DEV_FETCH_BUDGET),
        None,
    )
}

fn models_dev_request_with_budget(
    system_proxy_url: Option<&str>,
    budget: RequestBudget,
    etag: Option<&str>,
) -> Result<OutboundRequest, OutboundFailure> {
    let mut request = OutboundRequest::get(MODELS_DEV_URL, budget);
    request.proxy = match system_proxy_url {
        Some(url) => ProxyPolicy::Manual(ManualProxy::parse(url)?),
        None => ProxyPolicy::System,
    };
    if let Some(etag) = etag.and_then(|value| HeaderValue::from_str(value).ok()) {
        let policy = crate::outbound::OutboundHeaderPolicy::provider_default();
        request
            .headers
            .insert_public(header::IF_NONE_MATCH, etag, &policy)?;
    }
    Ok(request)
}

fn outbound_failure_message(failure: &OutboundFailure) -> &'static str {
    match &failure.kind {
        OutboundFailureKind::ConnectTimeout
        | OutboundFailureKind::FirstByteTimeout
        | OutboundFailureKind::BodyTimeout
        | OutboundFailureKind::TotalTimeout
        | OutboundFailureKind::BudgetExhausted => "连接 models.dev 超时，请检查系统代理或网络设置",
        OutboundFailureKind::BodyLimitExceeded { .. } => "models.dev 定价文件超过 8 MiB 安全限制",
        OutboundFailureKind::RedirectBlocked
        | OutboundFailureKind::RedirectLoop
        | OutboundFailureKind::RedirectLimitExceeded => "models.dev 返回了异常重定向",
        OutboundFailureKind::Cancelled => "模型价格同步已取消",
        OutboundFailureKind::InvalidUrl
        | OutboundFailureKind::InvalidHeader
        | OutboundFailureKind::HeaderNotAllowed(_)
        | OutboundFailureKind::ProxyPolicy
        | OutboundFailureKind::TransportPolicy => "模型价格同步网络配置初始化失败",
        OutboundFailureKind::RetryAfterExceedsBudget | OutboundFailureKind::RequestFailed => {
            "无法连接 models.dev，请检查系统代理或网络设置"
        }
    }
}

fn is_retryable_models_dev_failure(failure: &OutboundFailure) -> bool {
    matches!(
        &failure.kind,
        OutboundFailureKind::ConnectTimeout
            | OutboundFailureKind::FirstByteTimeout
            | OutboundFailureKind::BodyTimeout
            | OutboundFailureKind::TotalTimeout
            | OutboundFailureKind::RequestFailed
    )
}

fn open_path_with_system(path: &Path) -> Result<(), std::io::Error> {
    #[cfg(target_os = "windows")]
    {
        // Explorer can successfully open a directory while returning a non-zero exit code
        // when it hands the request to an existing process. A successful spawn is the only
        // reliable acknowledgement available to this fire-and-forget shell operation.
        Command::new("explorer.exe").arg(path).spawn()?;
        return Ok(());
    }

    #[cfg(not(target_os = "windows"))]
    {
        #[cfg(target_os = "macos")]
        let result = Command::new("open").arg(path).status();
        #[cfg(all(unix, not(target_os = "macos")))]
        let result = Command::new("xdg-open").arg(path).status();

        result.and_then(|status| {
            if status.success() {
                Ok(())
            } else {
                Err(std::io::Error::other(format!(
                    "launcher exited with status {status}"
                )))
            }
        })
    }
}

pub(crate) fn register_model_price_sync_task(
    supervisor: &TaskSupervisor,
    sync: Arc<ModelPriceSyncService>,
) -> Result<TaskId, String> {
    let task_id = TaskId::from(MODEL_PRICE_SYNC_TASK_ID);
    supervisor
        .register(
            TaskSpec::new(
                task_id.clone(),
                "model_price_sync_v1",
                move |context: TaskRunContext| {
                    let sync = Arc::clone(&sync);
                    Box::pin(async move {
                        loop {
                            tokio::select! {
                                _ = context.cancellation_token.cancelled() => return Err(TaskFailure::cancelled()),
                                _ = sync.sync(false) => {}
                            }
                            tokio::select! {
                                _ = context.cancellation_token.cancelled() => return Err(TaskFailure::cancelled()),
                                _ = tokio::time::sleep(Duration::from_millis(SYNC_INTERVAL_MS as u64)) => {}
                            }
                        }
                    })
                },
            )
            .with_concurrency_key(MODEL_PRICE_SYNC_TASK_ID)
            .with_shutdown_timeout(Duration::from_secs(10)),
        )
        .map_err(|error| error.to_string())?;
    Ok(task_id)
}

fn default_document() -> ModelPriceCatalogDocument {
    ModelPriceCatalogDocument {
        version: DOCUMENT_VERSION,
        source: CatalogSource {
            kind: "models.dev".into(),
            url: MODELS_DEV_URL.into(),
        },
        sync: CatalogSync::default(),
        models: Vec::new(),
        overrides: Vec::new(),
        deleted_model_ids: Vec::new(),
    }
}

fn state_from_document(document: &ModelPriceCatalogDocument, path: &Path) -> ModelPriceSyncState {
    let models = filtered_catalog_models(document.models.clone());
    ModelPriceSyncState {
        source_url: document.source.url.clone(),
        auto_sync_enabled: document.sync.auto_sync_enabled,
        include_common_models: document.sync.include_common_models,
        selected_model_keys: document.sync.selected_model_keys.clone(),
        excluded_common_model_keys: document.sync.excluded_common_model_keys.clone(),
        last_sync_at: document.sync.last_sync_at.clone(),
        last_sync_error: document.sync.last_sync_error.clone(),
        model_count: models.len(),
        common_model_count: models.iter().filter(|model| model.common).count(),
        auto_sync_model_count: select_models(models.clone(), &document.sync).models.len(),
        file_path: path.display().to_string(),
    }
}

fn model_to_catalog_entry(model: CatalogModel) -> ModelPriceCatalogEntry {
    ModelPriceCatalogEntry {
        key: model.key,
        provider: model.provider,
        model: model.model,
        name: model.name,
        common: model.common,
        release_date: model.release_date,
        input_price: model.input_price,
        output_price: model.output_price,
        cache_creation_price: model.cache_creation_price,
        cache_read_price: model.cache_read_price,
    }
}

fn validate_config(config: &ModelPriceSyncConfig) -> Result<(), ApplicationError> {
    if config.selected_model_keys.len() > MAX_MODEL_SELECTION_KEYS
        || config.excluded_common_model_keys.len() > MAX_MODEL_SELECTION_KEYS
    {
        return Err(ApplicationError::ConstraintViolation);
    }
    if config
        .selected_model_keys
        .iter()
        .chain(config.excluded_common_model_keys.iter())
        .any(|key| key.trim().is_empty() || key.len() > 256 || key.chars().any(char::is_control))
    {
        return Err(ApplicationError::ConstraintViolation);
    }
    Ok(())
}

fn default_true() -> bool {
    true
}

fn normalize_keys(keys: Vec<String>) -> Vec<String> {
    let mut set = BTreeSet::new();
    for key in keys {
        let value = key.trim().to_lowercase();
        if !value.is_empty() {
            set.insert(value);
        }
    }
    set.into_iter().collect()
}

fn sync_is_due(last_sync_at: Option<&str>) -> bool {
    let Some(value) = last_sync_at else {
        return true;
    };
    let Ok(last) = chrono::DateTime::parse_from_rfc3339(value) else {
        return true;
    };
    (Utc::now() - last.with_timezone(&Utc)).num_milliseconds() >= SYNC_INTERVAL_MS
}

struct ParsedSelection {
    models: Vec<CatalogModel>,
    catalog_models: Vec<CatalogModel>,
    skipped_count: usize,
}

fn parse_models_dev(root: &Value) -> Result<Vec<CatalogModel>, ApplicationError> {
    let object = root.as_object().ok_or(ApplicationError::IntegrityFailed)?;
    let mut output = Vec::new();
    for (provider, provider_value) in object {
        let Some(models) = provider_value.get("models").and_then(Value::as_object) else {
            continue;
        };
        let source_url = provider_value
            .get("doc")
            .and_then(Value::as_str)
            .unwrap_or(MODELS_DEV_URL)
            .to_string();
        for (model_id, model_value) in models {
            let model = model_id.trim();
            if model.is_empty() || model.len() > 256 || !is_text_pricing_model(model, model_value) {
                continue;
            }
            let family = model_value
                .get("family")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let Some(cost) = model_value.get("cost").and_then(Value::as_object) else {
                continue;
            };
            let input_price = finite_non_negative(cost.get("input"));
            let output_price = finite_non_negative(cost.get("output"));
            if input_price.is_none() && output_price.is_none() {
                continue;
            }
            let key = format!(
                "{}/{}",
                provider.trim().to_lowercase(),
                model.to_lowercase()
            );
            output.push(CatalogModel {
                key,
                provider: provider.trim().to_lowercase(),
                model: model.to_string(),
                name: model_value
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or(model)
                    .to_string(),
                family,
                common: false,
                selected: false,
                release_date: model_value
                    .get("last_updated")
                    .and_then(Value::as_str)
                    .or_else(|| model_value.get("release_date").and_then(Value::as_str))
                    .map(str::to_owned),
                input_price,
                output_price,
                cache_creation_price: finite_non_negative(cost.get("cache_write")),
                cache_read_price: finite_non_negative(cost.get("cache_read")),
                source_url: source_url.clone(),
                source_checked_at: Utc::now().to_rfc3339(),
            });
        }
    }
    if output.is_empty() {
        return Err(ApplicationError::IntegrityFailed);
    }
    let mut output = sort_catalog_models(output);
    apply_common_model_limit(&mut output);
    Ok(output)
}

fn select_models(models: Vec<CatalogModel>, sync: &CatalogSync) -> ParsedSelection {
    let selected = sync
        .selected_model_keys
        .iter()
        .map(|key| key.to_lowercase())
        .collect::<BTreeSet<_>>();
    let excluded_common = sync
        .excluded_common_model_keys
        .iter()
        .map(|key| key.to_lowercase())
        .collect::<BTreeSet<_>>();
    let catalog_models = models
        .into_iter()
        .map(|mut model| {
            model.selected = selected.contains(&model.key)
                || (sync.include_common_models
                    && model.common
                    && !excluded_common.contains(&model.key));
            model
        })
        .collect::<Vec<_>>();
    let selected_quote_count = catalog_models.iter().filter(|model| model.selected).count();
    let models = preferred_effective_models(
        catalog_models
            .iter()
            .filter(|model| model.selected)
            .cloned(),
    );
    ParsedSelection {
        skipped_count: selected_quote_count.saturating_sub(models.len()),
        models,
        catalog_models,
    }
}

fn preferred_effective_models(models: impl IntoIterator<Item = CatalogModel>) -> Vec<CatalogModel> {
    let mut deduped = BTreeMap::<String, CatalogModel>::new();
    for model in models {
        let normalized = normalize_model_id_for_catalog(&model.model);
        if normalized.is_empty() {
            continue;
        }
        let replace = deduped.get(&normalized).is_none_or(|current| {
            model.selected && !current.selected
                || model.selected == current.selected
                    && (pricing_source_priority(&model) < pricing_source_priority(current)
                        || pricing_source_priority(&model) == pricing_source_priority(current)
                            && model.common
                            && !current.common)
        });
        if replace {
            deduped.insert(normalized, model);
        }
    }
    deduped.into_values().collect()
}

fn filtered_catalog_models(models: Vec<CatalogModel>) -> Vec<CatalogModel> {
    let mut by_key = BTreeMap::new();
    for model in models
        .into_iter()
        .filter(is_persisted_catalog_model_visible)
    {
        by_key.insert(model.key.clone(), model);
    }
    sort_catalog_models(by_key.into_values().collect())
}

fn sort_catalog_models(mut models: Vec<CatalogModel>) -> Vec<CatalogModel> {
    models.sort_by(|left, right| {
        right
            .release_date
            .cmp(&left.release_date)
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.key.cmp(&right.key))
    });
    models
}

fn apply_common_model_limit(models: &mut [CatalogModel]) {
    let mut family_counts = BTreeMap::<String, usize>::new();
    for model in models {
        let family = common_model_family(&model.provider, &model.model);
        model.common = family.is_some_and(|family| {
            let count = family_counts.entry(family).or_default();
            if *count >= COMMON_MODEL_LIMIT_PER_FAMILY {
                return false;
            }
            *count += 1;
            true
        });
    }
}

fn common_model_family(provider: &str, model: &str) -> Option<String> {
    let provider = provider.trim().to_ascii_lowercase();
    let model = model.trim().to_ascii_lowercase();
    let family = match provider.as_str() {
        "anthropic" if model.starts_with("claude-") => "claude",
        "openai"
            if ["gpt-", "o1-", "o3-", "o4-"]
                .iter()
                .any(|prefix| model.starts_with(prefix)) =>
        {
            "openai"
        }
        "google" if model.starts_with("gemini-") => "gemini",
        "xai" if model.starts_with("grok-") => "grok",
        "deepseek" if model.starts_with("deepseek-") => "deepseek",
        "alibaba" if model.starts_with("qwen") => "qwen",
        "xiaomi" if model.starts_with("mimo-") => "mimo",
        "longcat" if model.starts_with("longcat-") => "longcat",
        "moonshotai" if model.starts_with("kimi-") => "kimi",
        "minimax-cn" if model.starts_with("minimax-m") => "minimax",
        "zai" if model.starts_with("glm-") => "glm",
        _ => return None,
    };
    Some(family.to_string())
}

fn is_persisted_catalog_model_visible(model: &CatalogModel) -> bool {
    (model.input_price.is_some() || model.output_price.is_some())
        && !has_non_text_model_marker(&model.model, &model.name)
}

fn catalog_to_input(model: &CatalogModel) -> UpsertModelBasePriceInput {
    let normalized_model = normalize_model_id_for_catalog(&model.model);
    UpsertModelBasePriceInput {
        id: Some(stable_id(&normalized_model)),
        provider: model.provider.clone(),
        model: normalized_model,
        input_price: model.input_price,
        output_price: model.output_price,
        input_price_priority: None,
        output_price_priority: None,
        cache_creation_price: model.cache_creation_price,
        cache_creation_price_priority: None,
        cache_creation_price_above_1hr: None,
        cache_read_price: model.cache_read_price,
        cache_read_price_priority: None,
        long_context_input_token_threshold: None,
        long_context_input_cost_multiplier: None,
        long_context_output_cost_multiplier: None,
        supports_service_tier: false,
        supports_prompt_caching: model.cache_creation_price.is_some()
            || model.cache_read_price.is_some(),
        currency: "USD".into(),
        unit: "M".into(),
        source_url: model.source_url.clone(),
        source_label: SOURCE_LABEL.into(),
        source_checked_at: Some(model.source_checked_at.clone()),
        enabled: true,
        built_in: false,
        note: Some(match model.release_date.as_deref() {
            Some(release_date) => format!(
                "{}; released {}; USD per M tokens",
                model.name, release_date
            ),
            None => format!("{}; USD per M tokens", model.name),
        }),
    }
}

fn sync_price_changed(
    previous: Option<&UpsertModelBasePriceInput>,
    current: &UpsertModelBasePriceInput,
) -> bool {
    let Some(previous) = previous else {
        return true;
    };
    previous.input_price != current.input_price
        || previous.output_price != current.output_price
        || previous.cache_creation_price != current.cache_creation_price
        || previous.cache_read_price != current.cache_read_price
}

fn sync_input_changed(
    previous: Option<&UpsertModelBasePriceInput>,
    current: &UpsertModelBasePriceInput,
) -> bool {
    let Some(previous) = previous else {
        return true;
    };
    let mut comparable_previous = previous.clone();
    let mut comparable_current = current.clone();
    comparable_previous.source_checked_at = None;
    comparable_current.source_checked_at = None;
    comparable_previous != comparable_current
}

fn stable_id(model: &str) -> String {
    let suffix = model
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    // CC Switch stores built-in and synchronized prices under the same normalized
    // model key. Reusing the built-in identifier gives this database the same
    // overwrite/delete semantics without exposing provider quotes as effective rows.
    format!("builtin-{}", suffix.trim_matches('-'))
}

fn model_base_price_to_input(model: &ModelBasePrice) -> UpsertModelBasePriceInput {
    UpsertModelBasePriceInput {
        id: Some(model.id.clone()),
        provider: model.provider.clone(),
        model: model.model.clone(),
        input_price: model.input_price,
        output_price: model.output_price,
        input_price_priority: model.input_price_priority,
        output_price_priority: model.output_price_priority,
        cache_creation_price: model.cache_creation_price,
        cache_creation_price_priority: model.cache_creation_price_priority,
        cache_creation_price_above_1hr: model.cache_creation_price_above_1hr,
        cache_read_price: model.cache_read_price,
        cache_read_price_priority: model.cache_read_price_priority,
        long_context_input_token_threshold: model.long_context_input_token_threshold,
        long_context_input_cost_multiplier: model.long_context_input_cost_multiplier,
        long_context_output_cost_multiplier: model.long_context_output_cost_multiplier,
        supports_service_tier: model.supports_service_tier,
        supports_prompt_caching: model.supports_prompt_caching,
        currency: model.currency.clone(),
        unit: model.unit.clone(),
        source_url: model.source_url.clone(),
        source_label: model.source_label.clone(),
        source_checked_at: model.source_checked_at.clone(),
        enabled: model.enabled,
        built_in: false,
        note: model.note.clone(),
    }
}

fn upsert_override(
    overrides: &mut Vec<UpsertModelBasePriceInput>,
    input: UpsertModelBasePriceInput,
) {
    let Some(id) = input.id.clone() else {
        return;
    };
    overrides.retain(|entry| {
        entry
            .id
            .as_deref()
            .is_none_or(|entry_id| !entry_id.eq_ignore_ascii_case(&id))
    });
    overrides.push(input);
    overrides.sort_by(|left, right| left.id.cmp(&right.id));
}

fn finite_non_negative(value: Option<&Value>) -> Option<f64> {
    let number = value?.as_f64()?;
    (number.is_finite() && number >= 0.0).then_some(number)
}

const NON_TEXT_MODEL_MARKERS: [&str; 9] = [
    "audio",
    "deprecated",
    "embedding",
    "image",
    "moderation",
    "realtime",
    "transcribe",
    "tts",
    "video",
];

const OFFICIAL_MODEL_PROVIDERS: [&str; 35] = [
    "alibaba",
    "alibaba-cn",
    "anthropic",
    "arcee",
    "bailing",
    "cohere",
    "deepseek",
    "google",
    "inception",
    "llama",
    "longcat",
    "meta",
    "minimax",
    "minimax-cn",
    "mistral",
    "moonshotai",
    "moonshotai-cn",
    "morph",
    "nova",
    "openai",
    "perplexity",
    "poolside",
    "qvac",
    "sakana",
    "sarvam",
    "stepfun",
    "stepfun-ai",
    "subconscious",
    "synthetic",
    "thinkingmachines",
    "upstage",
    "xai",
    "xiaomi",
    "zai",
    "zhipuai",
];

const PREFERRED_OFFICIAL_PROVIDER_ORDER: [&str; 35] = [
    "anthropic",
    "openai",
    "google",
    "xai",
    "deepseek",
    "alibaba",
    "alibaba-cn",
    "moonshotai",
    "moonshotai-cn",
    "minimax",
    "minimax-cn",
    "zai",
    "zhipuai",
    "stepfun",
    "stepfun-ai",
    "xiaomi",
    "longcat",
    "mistral",
    "cohere",
    "perplexity",
    "nova",
    "llama",
    "meta",
    "arcee",
    "bailing",
    "inception",
    "morph",
    "poolside",
    "qvac",
    "sakana",
    "sarvam",
    "synthetic",
    "upstage",
    "thinkingmachines",
    "subconscious",
];

const TRUSTED_FALLBACK_PROVIDERS: [&str; 17] = [
    "amazon-bedrock",
    "azure",
    "azure-cognitive-services",
    "baseten",
    "cerebras",
    "cloudflare-workers-ai",
    "deepinfra",
    "fireworks-ai",
    "google-vertex",
    "google-vertex-anthropic",
    "groq",
    "modal",
    "nvidia",
    "siliconflow",
    "siliconflow-cn",
    "togetherai",
    "cloudflare-ai-gateway",
];

const AGGREGATOR_FALLBACK_PROVIDERS: [&str; 15] = [
    "302ai",
    "abacus",
    "aihubmix",
    "edenai",
    "helicone",
    "kilo",
    "llmgateway",
    "llmgateway-providers",
    "merge-gateway",
    "nano-gpt",
    "ofox",
    "openrouter",
    "requesty",
    "vercel",
    "zenmux",
];

fn is_text_pricing_model(model_id: &str, model: &Value) -> bool {
    if model
        .get("status")
        .and_then(Value::as_str)
        .is_some_and(|status| status.eq_ignore_ascii_case("deprecated"))
    {
        return false;
    }

    let output_modalities = model
        .pointer("/modalities/output")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_ascii_lowercase)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if !output_modalities.is_empty()
        && (!output_modalities.iter().any(|item| item == "text")
            || output_modalities
                .iter()
                .any(|item| matches!(item.as_str(), "audio" | "image" | "video")))
    {
        return false;
    }

    let name = model.get("name").and_then(Value::as_str).unwrap_or("");
    !has_non_text_model_marker(model_id, name)
}

fn has_non_text_model_marker(model_id: &str, name: &str) -> bool {
    let searchable = format!("{model_id} {name}").to_ascii_lowercase();
    NON_TEXT_MODEL_MARKERS
        .iter()
        .any(|marker| searchable.contains(marker))
}

fn normalize_model_id_for_catalog(model_id: &str) -> String {
    let after_slash = model_id.rsplit('/').next().unwrap_or(model_id);
    let before_colon = after_slash.split(':').next().unwrap_or_default();
    let mut normalized = before_colon.trim().replace('@', "-").to_ascii_lowercase();
    if normalized.ends_with("[1m]") {
        normalized.truncate(normalized.len() - "[1m]".len());
        normalized = normalized.trim().to_string();
    }
    normalized
}

fn is_official_pricing_source(provider: &str, model_id: &str, family: Option<&str>) -> bool {
    let provider = provider.trim().to_ascii_lowercase();
    let Some(owner_providers) = official_owner_providers(model_id, family) else {
        return OFFICIAL_MODEL_PROVIDERS.contains(&provider.as_str());
    };
    owner_providers.contains(&provider.as_str())
}

fn official_owner_providers(
    model_id: &str,
    family: Option<&str>,
) -> Option<&'static [&'static str]> {
    let model = normalize_model_id_for_catalog(model_id);
    let family = family.unwrap_or("").trim().to_ascii_lowercase();

    if family.starts_with("nemotron") {
        return Some(&["nvidia"]);
    }
    if family.starts_with("deepseek") || model.starts_with("deepseek-") {
        return Some(&["deepseek"]);
    }
    if family.starts_with("claude") || model.starts_with("claude-") {
        return Some(&["anthropic"]);
    }
    if family.starts_with("gpt")
        || model.starts_with("gpt-")
        || matches_model_series(&family, &["o1", "o3", "o4"])
        || matches_model_series(&model, &["o1", "o3", "o4"])
    {
        return Some(&["openai"]);
    }
    if family.starts_with("gemini")
        || family.starts_with("gemma")
        || model.starts_with("gemini-")
        || model.starts_with("gemma-")
    {
        return Some(&["google"]);
    }
    if family.starts_with("grok") || model.starts_with("grok-") {
        return Some(&["xai"]);
    }
    if family.starts_with("qwen") || model.starts_with("qwen") {
        return Some(&["alibaba", "alibaba-cn"]);
    }
    if family.starts_with("kimi") || model.starts_with("kimi-") || model.starts_with("moonshot-") {
        return Some(&["moonshotai", "moonshotai-cn"]);
    }
    if family.starts_with("minimax") || model.starts_with("minimax-") {
        return Some(&["minimax", "minimax-cn"]);
    }
    if family.starts_with("glm") || model.starts_with("glm-") {
        return Some(&["zai", "zhipuai"]);
    }
    if family.starts_with("mimo") || model.starts_with("mimo-") {
        return Some(&["xiaomi"]);
    }
    if family.starts_with("step") || model.starts_with("step-") {
        return Some(&["stepfun", "stepfun-ai"]);
    }
    if family.starts_with("longcat") || model.starts_with("longcat-") {
        return Some(&["longcat"]);
    }
    if matches_model_series(
        &family,
        &[
            "mistral",
            "mixtral",
            "ministral",
            "devstral",
            "codestral",
            "pixtral",
            "magistral",
        ],
    ) || matches_model_series(
        &model,
        &[
            "mistral",
            "mixtral",
            "ministral",
            "devstral",
            "codestral",
            "pixtral",
            "magistral",
            "open-mistral",
            "open-mixtral",
        ],
    ) {
        return Some(&["mistral"]);
    }
    if family.starts_with("command") || model.starts_with("command-") || model.starts_with("c4ai-")
    {
        return Some(&["cohere"]);
    }
    if family.starts_with("sonar") || model.starts_with("sonar") {
        return Some(&["perplexity"]);
    }
    if family.starts_with("llama") || model.starts_with("llama-") {
        return Some(&["llama", "meta"]);
    }
    if family.starts_with("nova") || model.starts_with("nova-") {
        return Some(&["nova"]);
    }
    None
}

fn matches_model_series(value: &str, prefixes: &[&str]) -> bool {
    prefixes.iter().any(|prefix| {
        value == *prefix
            || value
                .strip_prefix(prefix)
                .and_then(|suffix| suffix.chars().next())
                .is_some_and(|separator| matches!(separator, '-' | '.' | '_'))
    })
}

fn pricing_source_priority(model: &CatalogModel) -> (u8, usize) {
    let provider = model.provider.as_str();
    if is_official_pricing_source(provider, &model.model, model.family.as_deref()) {
        return (
            0,
            PREFERRED_OFFICIAL_PROVIDER_ORDER
                .iter()
                .position(|item| *item == provider)
                .unwrap_or(usize::MAX),
        );
    }
    if let Some(index) = PREFERRED_OFFICIAL_PROVIDER_ORDER
        .iter()
        .position(|item| *item == provider)
    {
        return (1, index);
    }
    if let Some(index) = TRUSTED_FALLBACK_PROVIDERS
        .iter()
        .position(|item| *item == provider)
    {
        return (2, index);
    }
    if let Some(index) = AGGREGATOR_FALLBACK_PROVIDERS
        .iter()
        .position(|item| *item == provider)
    {
        return (4, index);
    }
    (3, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_keeps_null_cache_prices_and_includes_chinese_providers() {
        let value = serde_json::json!({
            "deepseek": {"doc":"https://deepseek.com", "models": {
                "deepseek-chat": {"name":"DeepSeek Chat", "modalities":{"output":["text"]}, "cost":{"input":0.14,"output":0.28,"cache_read":0.0028}}
            }},
            "alibaba": {"models": {"qwen-max": {"modalities":{"output":["text"]}, "cost":{"input":1.0,"output":5.0}}}}
        });
        let models = parse_models_dev(&value).expect("models");
        assert_eq!(models.len(), 2);
        assert!(models.iter().any(|model| model.provider == "deepseek"));
        assert!(models
            .iter()
            .any(|model| model.cache_creation_price.is_none()));
    }

    #[test]
    fn parser_keeps_all_supported_provider_quotes_for_search() {
        let value = serde_json::json!({
            "deepseek": {"models": {
                "deepseek-chat": {"family":"deepseek", "name":"DeepSeek Chat", "modalities":{"output":["text"]}, "cost":{"input":0.14,"output":0.28}},
                "deepseek-old": {"family":"deepseek", "name":"DeepSeek Old", "status":"deprecated", "modalities":{"output":["text"]}, "cost":{"input":0.1,"output":0.2}},
                "deepseek-vision": {"family":"deepseek", "name":"DeepSeek Vision", "modalities":{"output":["text","image"]}, "cost":{"input":0.1,"output":0.2}},
                "deepseek-embedding": {"family":"deepseek", "name":"DeepSeek Embedding", "modalities":{"output":["text"]}, "cost":{"input":0.1,"output":0.2}},
                "deepseek-unknown-modalities": {"family":"deepseek", "name":"DeepSeek Chat", "cost":{"input":0.1,"output":0.2}}
            }},
            "openrouter": {"models": {
                "deepseek/deepseek-chat": {"family":"deepseek", "name":"DeepSeek Chat", "modalities":{"output":["text"]}, "cost":{"input":1.0,"output":2.0}}
            }},
            "alibaba": {"models": {
                "qwen-max": {"family":"qwen", "name":"Qwen Max", "modalities":{"output":["text"]}, "cost":{"input":1.0,"output":5.0}},
                "deepseek-r1": {"family":"deepseek-thinking", "name":"DeepSeek R1", "modalities":{"output":["text"]}, "cost":{"input":1.0,"output":5.0}}
            }},
            "mistral": {"models": {
                "devstral-latest": {"family":"devstral", "name":"Devstral", "modalities":{"output":["text"]}, "cost":{"input":0.2,"output":0.8}}
            }},
            "nvidia": {"models": {
                "nvidia/nemotron-3": {"family":"nemotron", "name":"Nemotron 3", "modalities":{"output":["text"]}, "cost":{"input":0,"output":0}},
                "nvidia/active-speaker-detection": {"name":"Active Speaker Detection", "modalities":{"output":["text"]}, "cost":{"input":0,"output":0}}
            }}
        });

        let models = parse_models_dev(&value).expect("models");

        assert_eq!(models.len(), 8);
        let deepseek_chat = models
            .iter()
            .find(|model| model.key == "deepseek/deepseek-chat")
            .expect("official DeepSeek quote");
        assert_eq!(deepseek_chat.input_price, Some(0.14));
        assert_eq!(deepseek_chat.output_price, Some(0.28));
        assert!(models
            .iter()
            .any(|model| model.key == "deepseek/deepseek-unknown-modalities"));
        assert!(models.iter().any(|model| model.key == "alibaba/qwen-max"));
        assert!(models
            .iter()
            .any(|model| model.key == "mistral/devstral-latest"));
        assert!(models
            .iter()
            .any(|model| model.key == "nvidia/nvidia/nemotron-3"));
        assert!(models
            .iter()
            .any(|model| model.key == "alibaba/deepseek-r1"));
        assert!(models
            .iter()
            .any(|model| model.key == "nvidia/nvidia/active-speaker-detection"));
        assert!(models.iter().any(|model| model.provider == "openrouter"));
    }

    #[test]
    fn parser_keeps_trusted_aggregator_and_long_tail_quotes() {
        let value = serde_json::json!({
            "openrouter": {"models": {
                "vendor/long-tail-chat": {"modalities":{"output":["text"]}, "cost":{"input":9.0,"output":18.0}}
            }},
            "amazon-bedrock": {"models": {
                "long-tail-chat": {"modalities":{"output":["text"]}, "cost":{"input":1.0,"output":2.0}}
            }},
            "specialist-api": {"models": {
                "maker/niche-chat": {"modalities":{"output":["text"]}, "cost":{"input":0.4,"output":0.8}}
            }}
        });

        let models = parse_models_dev(&value).expect("models");

        assert_eq!(models.len(), 3);
        let long_tail = models
            .iter()
            .find(|model| model.model == "long-tail-chat")
            .expect("trusted fallback");
        assert_eq!(long_tail.provider, "amazon-bedrock");
        assert_eq!(long_tail.input_price, Some(1.0));
        assert!(models
            .iter()
            .any(|model| model.key == "specialist-api/maker/niche-chat"));
    }

    #[test]
    fn parser_keeps_regional_official_quotes_as_separate_search_options() {
        let value = serde_json::json!({
            "alibaba": {"models": {
                "qwen-max": {"family":"qwen", "modalities":{"output":["text"]}, "cost":{"input":1.0,"output":5.0}}
            }},
            "alibaba-cn": {"models": {
                "QWEN-MAX": {"family":"qwen", "modalities":{"output":["text"]}, "cost":{"input":0.8,"output":4.0}}
            }}
        });

        let models = parse_models_dev(&value).expect("models");

        assert_eq!(models.len(), 2);
        assert!(models.iter().any(|model| model.provider == "alibaba"));
        assert!(models.iter().any(|model| model.provider == "alibaba-cn"));
    }

    #[test]
    fn catalog_normalization_is_only_used_for_grouping_source_aliases() {
        assert_eq!(
            normalize_model_id_for_catalog("deepseek/deepseek-chat:free"),
            "deepseek-chat"
        );
        assert_eq!(
            normalize_model_id_for_catalog("claude-opus-4@20250514[1m]"),
            "claude-opus-4-20250514"
        );
    }

    #[test]
    fn automatic_selection_only_includes_explicit_keys() {
        let make = |provider: &str| CatalogModel {
            key: format!("{provider}/same"),
            provider: provider.into(),
            model: "same".into(),
            name: "same".into(),
            family: None,
            common: true,
            selected: false,
            release_date: None,
            input_price: Some(1.0),
            output_price: Some(1.0),
            cache_creation_price: None,
            cache_read_price: None,
            source_url: MODELS_DEV_URL.into(),
            source_checked_at: "2026-01-01T00:00:00Z".into(),
        };
        let empty_sync = CatalogSync {
            include_common_models: false,
            ..CatalogSync::default()
        };
        let no_selection = select_models(vec![make("openai"), make("deepseek")], &empty_sync);
        assert!(no_selection.models.is_empty());
        assert_eq!(no_selection.catalog_models.len(), 2);

        let selected_sync = CatalogSync {
            include_common_models: false,
            selected_model_keys: vec!["openai/same".into(), "deepseek/same".into()],
            ..CatalogSync::default()
        };
        let result = select_models(vec![make("openai"), make("deepseek")], &selected_sync);
        assert_eq!(result.models.len(), 1);
        assert_eq!(result.models[0].provider, "openai");
        assert_eq!(result.catalog_models.len(), 2);
    }

    #[test]
    fn common_and_explicit_selection_deduplicates_effective_model_ids() {
        let make = |provider: &str, model: &str| CatalogModel {
            key: format!("{provider}/{model}"),
            provider: provider.into(),
            model: model.into(),
            name: model.into(),
            family: None,
            common: false,
            selected: false,
            release_date: None,
            input_price: Some(1.0),
            output_price: Some(2.0),
            cache_creation_price: None,
            cache_read_price: None,
            source_url: MODELS_DEV_URL.into(),
            source_checked_at: "2026-01-01T00:00:00Z".into(),
        };

        let sync = CatalogSync {
            include_common_models: true,
            selected_model_keys: vec!["relay/unique".into()],
            ..CatalogSync::default()
        };
        let result = select_models(
            vec![
                CatalogModel {
                    common: true,
                    ..make("openai", "same")
                },
                CatalogModel {
                    common: true,
                    ..make("deepseek", "same")
                },
                make("relay", "unique"),
            ],
            &sync,
        );

        assert_eq!(result.models.len(), 2);
        assert_eq!(result.catalog_models.len(), 3);
        assert!(result.models.iter().any(|model| model.model == "unique"));
        assert_eq!(result.skipped_count, 1);
    }

    #[test]
    fn common_model_families_match_cc_switch_canonical_providers() {
        for (provider, model) in [
            ("alibaba", "qwen3-max"),
            ("moonshotai", "kimi-k2"),
            ("minimax-cn", "minimax-m2"),
            ("zai", "glm-5"),
            ("xiaomi", "mimo-v2-pro"),
        ] {
            assert!(common_model_family(provider, model).is_some());
        }
        for (provider, model) in [
            ("alibaba-cn", "qwen3-max"),
            ("moonshotai-cn", "kimi-k2"),
            ("minimax", "minimax-m2"),
            ("zhipuai", "glm-5"),
        ] {
            assert!(common_model_family(provider, model).is_none());
        }
        assert!(common_model_family("stepfun", "step-3.5-flash").is_none());
    }

    #[test]
    fn synchronized_prices_share_the_builtin_model_identifier() {
        assert_eq!(stable_id("deepseek-chat"), "builtin-deepseek-chat");
        assert_eq!(
            stable_id("claude-opus-4@20250514"),
            "builtin-claude-opus-4-20250514"
        );
    }

    fn sync_input(input_price: f64) -> UpsertModelBasePriceInput {
        UpsertModelBasePriceInput {
            id: Some("builtin-gpt-test".into()),
            provider: "openai".into(),
            model: "gpt-test".into(),
            input_price: Some(input_price),
            output_price: Some(2.0),
            input_price_priority: None,
            output_price_priority: None,
            cache_creation_price: Some(0.5),
            cache_creation_price_priority: None,
            cache_creation_price_above_1hr: None,
            cache_read_price: Some(0.1),
            cache_read_price_priority: None,
            long_context_input_token_threshold: None,
            long_context_input_cost_multiplier: None,
            long_context_output_cost_multiplier: None,
            supports_service_tier: false,
            supports_prompt_caching: true,
            currency: "USD".into(),
            unit: "M".into(),
            source_url: MODELS_DEV_URL.into(),
            source_label: SOURCE_LABEL.into(),
            source_checked_at: Some("2026-01-01T00:00:00Z".into()),
            enabled: true,
            built_in: false,
            note: Some("GPT Test; USD per M tokens".into()),
        }
    }

    #[test]
    fn sync_ignores_check_timestamp_when_detecting_changes() {
        let previous = sync_input(1.0);
        let mut current = previous.clone();
        current.source_checked_at = Some("2026-01-02T00:00:00Z".into());

        assert!(!sync_price_changed(Some(&previous), &current));
        assert!(!sync_input_changed(Some(&previous), &current));
    }

    #[test]
    fn sync_counts_and_persists_only_real_price_changes() {
        let previous = sync_input(1.0);
        let current = sync_input(1.1);

        assert!(sync_price_changed(Some(&previous), &current));
        assert!(sync_input_changed(Some(&previous), &current));
    }

    #[test]
    fn sync_request_preserves_system_fallback_without_a_configured_proxy() {
        let request = models_dev_request_with_system_proxy(None).expect("request");

        assert_eq!(request.url, MODELS_DEV_URL);
        assert_eq!(request.proxy, ProxyPolicy::System);
        assert!(request.budget.remaining().is_some());
    }

    #[test]
    fn sync_request_materializes_the_windows_user_proxy() {
        let request = models_dev_request_with_system_proxy(Some("http://127.0.0.1:7890"))
            .expect("configured proxy");
        let ProxyPolicy::Manual(proxy) = request.proxy else {
            panic!("configured user proxy must become an explicit transport proxy");
        };

        assert_eq!(proxy.endpoint, "http://127.0.0.1:7890");
    }

    #[test]
    fn sync_request_uses_a_valid_cached_etag() {
        let request = models_dev_request_with_budget(
            None,
            RequestBudget::from_now(Duration::from_secs(15)),
            Some("W/\"catalog-etag\""),
        )
        .expect("request");
        let headers = request
            .headers
            .materialize(&OutboundHeaderPolicy::provider_default())
            .expect("headers");

        assert_eq!(headers[header::IF_NONE_MATCH], "W/\"catalog-etag\"");
    }

    #[test]
    fn outbound_failures_are_persisted_as_actionable_messages() {
        let timeout = OutboundFailure::new(OutboundFailureKind::ConnectTimeout);
        let transport = OutboundFailure::new(OutboundFailureKind::RequestFailed);
        let oversized = OutboundFailure::new(OutboundFailureKind::BodyLimitExceeded {
            limit_bytes: MAX_REMOTE_CATALOG_BYTES,
        });

        assert!(outbound_failure_message(&timeout).contains("系统代理"));
        assert!(outbound_failure_message(&transport).contains("系统代理"));
        assert!(outbound_failure_message(&oversized).contains("8 MiB"));
    }

    #[test]
    fn catalog_fetch_only_retries_transient_transport_failures() {
        for kind in [
            OutboundFailureKind::ConnectTimeout,
            OutboundFailureKind::FirstByteTimeout,
            OutboundFailureKind::BodyTimeout,
            OutboundFailureKind::TotalTimeout,
            OutboundFailureKind::RequestFailed,
        ] {
            assert!(is_retryable_models_dev_failure(&OutboundFailure::new(kind)));
        }

        assert!(!is_retryable_models_dev_failure(&OutboundFailure::new(
            OutboundFailureKind::BodyLimitExceeded {
                limit_bytes: MAX_REMOTE_CATALOG_BYTES,
            },
        )));
        assert!(!is_retryable_models_dev_failure(&OutboundFailure::new(
            OutboundFailureKind::ProxyPolicy,
        )));
    }
}
