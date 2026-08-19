//! Shared document-sync vocabulary.
//!
//! Persistence owns the durable projection. Future coordinators may reuse
//! these states without creating document-specific watcher or outbox tables.

pub(crate) const ROUTING_POLICY_DOCUMENT_KIND: &str = "routing_policy";
pub(crate) const MODEL_MAPPING_DOCUMENT_KIND: &str = "model_mapping";

/// Document kinds are deliberately shared by persistence and the file
/// coordinator.  Keeping the filename mapping here prevents each consumer
/// from inventing a second spelling or a user-controlled path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum DocumentKind {
    RoutingPolicy,
    ModelMapping,
}

/// Provenance attached by an internal document adapter before it enters the
/// aggregate CAS service.  The IPC payload never constructs this value, so a
/// caller cannot relabel a UI edit as a restore/import or otherwise alter the
/// audit semantics by sending an arbitrary string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DocumentSourceKind {
    Ui,
    FileWatch,
    HistoryRestore,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TrustedDocumentSource {
    kind: DocumentSourceKind,
}

impl TrustedDocumentSource {
    pub(crate) const fn ui() -> Self {
        Self {
            kind: DocumentSourceKind::Ui,
        }
    }

    pub(crate) const fn file_watch() -> Self {
        Self {
            kind: DocumentSourceKind::FileWatch,
        }
    }

    pub(crate) const fn history_restore() -> Self {
        Self {
            kind: DocumentSourceKind::HistoryRestore,
        }
    }

    pub(crate) const fn system() -> Self {
        Self {
            kind: DocumentSourceKind::System,
        }
    }

    /// Stable storage provenance used by existing history rows.  This is a
    /// projection of the typed source, never a parse target for callers.
    pub(crate) const fn history_label(self) -> &'static str {
        match self.kind {
            DocumentSourceKind::Ui => "user",
            DocumentSourceKind::FileWatch => "file_sync",
            DocumentSourceKind::HistoryRestore => "restore",
            DocumentSourceKind::System => "system",
        }
    }
}

impl DocumentKind {
    pub(crate) const fn file_name(self) -> &'static str {
        match self {
            Self::RoutingPolicy => "routing-policy.json",
            Self::ModelMapping => "model-mapping.json",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DocumentSyncState {
    Synchronized,
    PendingMaterialization,
    ExternalChange,
    Error,
}

impl DocumentSyncState {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Synchronized => "synchronized",
            Self::PendingMaterialization => "pending_materialization",
            Self::ExternalChange => "external_change",
            Self::Error => "error",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "synchronized" => Some(Self::Synchronized),
            "pending_materialization" => Some(Self::PendingMaterialization),
            "external_change" => Some(Self::ExternalChange),
            "error" => Some(Self::Error),
            _ => None,
        }
    }
}
