use std::collections::{BTreeSet, HashMap, VecDeque};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScheduledMonitor {
    pub(crate) monitor_id: String,
    pub(crate) station_id: String,
    pub(crate) station_key_ids: Vec<String>,
    pub(crate) next_due_at_ms: i64,
    pub(crate) interval_ms: i64,
    pub(crate) jitter_ms: i64,
    pub(crate) schedule_revision: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MonitorTriggerKind {
    Scheduled,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SchedulerCommand {
    pub(crate) monitor_id: String,
    pub(crate) station_id: String,
    pub(crate) station_key_ids: Vec<String>,
    pub(crate) trigger_kind: MonitorTriggerKind,
    pub(crate) due_at_ms: i64,
    pub(crate) lag_ms: i64,
    pub(crate) schedule_revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SchedulerTick {
    pub(crate) admitted_count: usize,
    pub(crate) queue_full_count: usize,
    pub(crate) max_lag_ms: i64,
    pub(crate) next_wakeup_at_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SchedulerDiagnostics {
    pub(crate) monitor_count: usize,
    pub(crate) queue_depth: usize,
    pub(crate) max_queue_depth: usize,
    pub(crate) last_lag_ms: Option<i64>,
    pub(crate) queue_full_count: usize,
    pub(crate) next_wakeup_at_ms: Option<i64>,
}

#[derive(Debug)]
pub(crate) struct MonitoringScheduler {
    monitors: HashMap<String, ScheduledMonitor>,
    due_index: BTreeSet<(i64, String)>,
    queue: VecDeque<SchedulerCommand>,
    max_queue_depth: usize,
    queue_full_count: usize,
    last_lag_ms: Option<i64>,
}

impl MonitoringScheduler {
    pub(crate) fn new(max_queue_depth: usize) -> Self {
        Self {
            monitors: HashMap::new(),
            due_index: BTreeSet::new(),
            queue: VecDeque::new(),
            max_queue_depth: max_queue_depth.max(1),
            queue_full_count: 0,
            last_lag_ms: None,
        }
    }

    pub(crate) fn startup_stagger(
        entries: impl IntoIterator<Item = ScheduledMonitor>,
        now_ms: i64,
        stagger_step_ms: i64,
    ) -> Vec<ScheduledMonitor> {
        let mut entries = entries.into_iter().collect::<Vec<_>>();
        entries.sort_by(|left, right| left.monitor_id.cmp(&right.monitor_id));
        let step = stagger_step_ms.max(0);
        entries
            .into_iter()
            .enumerate()
            .map(|(index, mut entry)| {
                let staggered_due = now_ms.saturating_add((index as i64).saturating_mul(step));
                entry.next_due_at_ms = entry.next_due_at_ms.max(staggered_due);
                entry
            })
            .collect()
    }

    pub(crate) fn upsert_monitor(&mut self, entry: ScheduledMonitor) {
        self.remove_from_due_index(&entry.monitor_id);
        self.due_index
            .insert((entry.next_due_at_ms, entry.monitor_id.clone()));
        self.monitors.insert(entry.monitor_id.clone(), entry);
    }

    pub(crate) fn notify_definition_edit(&mut self, mut entry: ScheduledMonitor, now_ms: i64) {
        entry.next_due_at_ms = entry.next_due_at_ms.max(now_ms);
        self.upsert_monitor(entry);
    }

    pub(crate) fn tick(&mut self, now_ms: i64) -> SchedulerTick {
        let mut admitted_count = 0;
        let mut queue_full_count = 0;
        let mut max_lag_ms = 0;
        let due = self
            .due_index
            .iter()
            .take_while(|(due_at_ms, _)| *due_at_ms <= now_ms)
            .cloned()
            .collect::<Vec<_>>();

        for (due_at_ms, monitor_id) in due {
            self.due_index.remove(&(due_at_ms, monitor_id.clone()));
            let Some(entry) = self.monitors.get(&monitor_id).cloned() else {
                continue;
            };
            let lag_ms = now_ms.saturating_sub(due_at_ms);
            max_lag_ms = max_lag_ms.max(lag_ms);
            self.last_lag_ms = Some(lag_ms);

            if self.queue.len() >= self.max_queue_depth {
                self.queue_full_count = self.queue_full_count.saturating_add(1);
                queue_full_count += 1;
            } else {
                self.queue.push_back(SchedulerCommand {
                    monitor_id: entry.monitor_id.clone(),
                    station_id: entry.station_id.clone(),
                    station_key_ids: entry.station_key_ids.clone(),
                    trigger_kind: MonitorTriggerKind::Scheduled,
                    due_at_ms,
                    lag_ms,
                    schedule_revision: entry.schedule_revision,
                });
                admitted_count += 1;
            }

            let next_due_at_ms = next_due_after_tick(&entry, now_ms);
            let mut next_entry = entry;
            next_entry.next_due_at_ms = next_due_at_ms;
            self.upsert_monitor(next_entry);
        }

        SchedulerTick {
            admitted_count,
            queue_full_count,
            max_lag_ms,
            next_wakeup_at_ms: self.next_wakeup_at_ms(),
        }
    }

    pub(crate) fn enqueue_manual(
        &mut self,
        monitor_id: &str,
        now_ms: i64,
    ) -> Option<SchedulerCommand> {
        let entry = self.monitors.get(monitor_id)?;
        Some(SchedulerCommand {
            monitor_id: entry.monitor_id.clone(),
            station_id: entry.station_id.clone(),
            station_key_ids: entry.station_key_ids.clone(),
            trigger_kind: MonitorTriggerKind::Manual,
            due_at_ms: now_ms,
            lag_ms: 0,
            schedule_revision: entry.schedule_revision,
        })
    }

    pub(crate) fn pop_ready(&mut self) -> Option<SchedulerCommand> {
        self.queue.pop_front()
    }

    pub(crate) fn next_wakeup_at_ms(&self) -> Option<i64> {
        self.due_index
            .iter()
            .next()
            .map(|(due_at_ms, _)| *due_at_ms)
    }

    pub(crate) fn diagnostics(&self) -> SchedulerDiagnostics {
        SchedulerDiagnostics {
            monitor_count: self.monitors.len(),
            queue_depth: self.queue.len(),
            max_queue_depth: self.max_queue_depth,
            last_lag_ms: self.last_lag_ms,
            queue_full_count: self.queue_full_count,
            next_wakeup_at_ms: self.next_wakeup_at_ms(),
        }
    }

    fn remove_from_due_index(&mut self, monitor_id: &str) {
        if let Some(existing) = self.monitors.get(monitor_id) {
            self.due_index
                .remove(&(existing.next_due_at_ms, monitor_id.to_string()));
        }
    }
}

fn next_due_after_tick(entry: &ScheduledMonitor, now_ms: i64) -> i64 {
    let interval = entry.interval_ms.max(1);
    let jitter = deterministic_forward_jitter_ms(
        &entry.monitor_id,
        entry.schedule_revision,
        now_ms,
        entry.jitter_ms.max(0),
    );
    now_ms.saturating_add(interval).saturating_add(jitter)
}

fn deterministic_forward_jitter_ms(
    monitor_id: &str,
    schedule_revision: i64,
    now_ms: i64,
    max_jitter_ms: i64,
) -> i64 {
    if max_jitter_ms <= 0 {
        return 0;
    }
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in monitor_id.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
    }
    hash ^= schedule_revision as u64;
    hash = hash.rotate_left(13) ^ now_ms as u64;
    (hash % (max_jitter_ms as u64 + 1)) as i64
}
