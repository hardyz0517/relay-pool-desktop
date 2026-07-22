use serde::Serialize;

pub type CollectorTaskType = String;
pub type CollectorRunStatus = String;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectorRun {
    pub id: String,
    pub station_id: String,
    pub endpoint_revision: i64,
    pub parent_run_id: Option<String>,
    pub adapter: String,
    pub task_type: CollectorTaskType,
    pub status: CollectorRunStatus,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub duration_ms: Option<i64>,
    pub endpoint_count: i64,
    pub success_count: i64,
    pub failure_count: i64,
    pub manual_action_required: bool,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub snapshot_id: Option<String>,
    pub created_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collector_run_serializes_camel_case() {
        let run = CollectorRun {
            id: "run-1".to_string(),
            station_id: "station-1".to_string(),
            endpoint_revision: 1,
            parent_run_id: Some("run-parent".to_string()),
            adapter: "sub2api".to_string(),
            task_type: "full".to_string(),
            status: "partial".to_string(),
            started_at: "1000".to_string(),
            finished_at: Some("1100".to_string()),
            duration_ms: Some(100),
            endpoint_count: 3,
            success_count: 2,
            failure_count: 1,
            manual_action_required: false,
            error_code: None,
            error_message: None,
            snapshot_id: Some("snapshot-1".to_string()),
            created_at: "1000".to_string(),
        };
        let value = serde_json::to_value(run).expect("json");

        assert_eq!(value["stationId"], "station-1");
        assert_eq!(value["endpointRevision"], 1);
        assert_eq!(value["parentRunId"], "run-parent");
        assert_eq!(value["durationMs"], 100);
        assert_eq!(value["manualActionRequired"], false);
    }
}
