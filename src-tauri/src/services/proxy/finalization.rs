pub(crate) enum FinalizationOutcome {
    Completed,
    Failed {
        code: String,
        detail: Option<String>,
    },
    Interrupted {
        detail: Option<String>,
    },
}
