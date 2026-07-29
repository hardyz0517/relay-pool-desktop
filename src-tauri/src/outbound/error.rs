use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutboundFailure {
    pub kind: OutboundFailureKind,
    pub url: Option<String>,
    pub retry_after_ms: Option<u64>,
}

impl OutboundFailure {
    pub fn new(kind: OutboundFailureKind) -> Self {
        Self {
            kind,
            url: None,
            retry_after_ms: None,
        }
    }

    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    pub fn with_retry_after(mut self, retry_after: Option<Duration>) -> Self {
        self.retry_after_ms = retry_after.map(duration_ms);
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OutboundFailureKind {
    InvalidUrl,
    InvalidHeader,
    HeaderNotAllowed(String),
    ProxyPolicy,
    TransportPolicy,
    ConnectTimeout,
    FirstByteTimeout,
    BodyTimeout,
    TotalTimeout,
    BudgetExhausted,
    Cancelled,
    BodyLimitExceeded { limit_bytes: usize },
    RedirectBlocked,
    RedirectLoop,
    RedirectLimitExceeded,
    RetryAfterExceedsBudget,
    RequestFailed,
}

impl std::fmt::Display for OutboundFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{:?}", self.kind)?;
        if let Some(url) = &self.url {
            write!(formatter, " at {url}")?;
        }
        if let Some(retry_after_ms) = self.retry_after_ms {
            write!(formatter, " retry_after_ms={retry_after_ms}")?;
        }
        Ok(())
    }
}

impl std::error::Error for OutboundFailure {}

pub(crate) fn duration_ms(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}
