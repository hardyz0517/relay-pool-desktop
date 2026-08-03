use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::models::dashboard_metrics::{
    DashboardCumulativeRequestMetricsSnapshot, DashboardLiveRequestMetricsSnapshot,
    DashboardRequestMetricsInput,
};

use super::{invalid_input, TypeDescriptor};

pub type DashboardLiveRequestMetricsSnapshotDto = DashboardLiveRequestMetricsSnapshot;
pub type DashboardCumulativeRequestMetricsSnapshotDto = DashboardCumulativeRequestMetricsSnapshot;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DashboardRequestMetricsInputDto {
    pub local_day_start_ms: i64,
    pub local_day_end_ms: i64,
}

impl DashboardRequestMetricsInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        serde_json::from_value(value).map_err(|_| {
            invalid_input(
                "input",
                "invalid_shape",
                "The dashboard request metrics payload is invalid.",
            )
        })
    }

    pub fn into_domain(self) -> DashboardRequestMetricsInput {
        DashboardRequestMetricsInput {
            local_day_start_ms: self.local_day_start_ms,
            local_day_end_ms: self.local_day_end_ms,
        }
    }
}

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "contract=ipc-dto-type-descriptor; owner=ipc; remove_when=descriptor is registered in production binding export"
    )
)]
pub const DASHBOARD_READS_TYPE: TypeDescriptor = TypeDescriptor {
    name: "DashboardReadsDto",
    typescript: include_str!("dashboard_reads.typescript.txt"),
};

#[cfg(test)]
pub(crate) fn serialization_fixtures() -> Vec<Value> {
    let live = DashboardLiveRequestMetricsSnapshot::default();
    let cumulative = DashboardCumulativeRequestMetricsSnapshot::default();
    vec![
        serde_json::json!({
            "command":"load_dashboard_live_request_metrics",
            "input":{"localDayStartMs":1700000000000_i64,"localDayEndMs":1700086400000_i64},
            "output":live
        }),
        serde_json::json!({
            "command":"load_dashboard_cumulative_request_metrics",
            "input":{},
            "output":cumulative
        }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::error::CommandErrorCode;

    #[test]
    fn dashboard_metrics_input_rejects_unknown_fields_and_wrong_shapes() {
        let error = DashboardRequestMetricsInputDto::parse(serde_json::json!({
            "localDayStartMs": 1,
            "localDayEndMs": 2,
            "unexpected": true
        }))
        .expect_err("unknown field should fail");
        assert_eq!(error.code, CommandErrorCode::InvalidInput);

        let error = DashboardRequestMetricsInputDto::parse(serde_json::json!({
            "localDayStartMs": "1",
            "localDayEndMs": 2
        }))
        .expect_err("wrong type should fail");
        assert_eq!(error.code, CommandErrorCode::InvalidInput);
    }
}
