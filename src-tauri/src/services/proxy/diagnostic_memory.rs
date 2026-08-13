use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

pub(crate) const DEFAULT_DIAGNOSTIC_MEMORY_LIMIT_BYTES: usize = 32 * 1024 * 1024;
pub(crate) const MAX_DIAGNOSTIC_JSON_BYTES: usize = 256 * 1024;
pub(crate) const MAX_DIAGNOSTIC_JSON_DEPTH: usize = 32;
pub(crate) const MAX_DIAGNOSTIC_JSON_NODES: usize = 4_096;
/// Conservative upper bound reserved before constructing a `serde_json::Value`
/// from a maximum-size diagnostic payload. Input/string storage is charged
/// twice and every admitted JSON node receives 128 bytes for `Value`,
/// container entries, and allocator metadata.
pub(crate) const JSON_PARSER_SCRATCH_BYTES: usize =
    MAX_DIAGNOSTIC_JSON_BYTES * 2 + MAX_DIAGNOSTIC_JSON_NODES * 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JsonComplexity {
    WithinLimit,
    TooDeep,
    TooComplex,
}

/// Counts structural tokens before serde allocates a recursive object graph.
/// Strings and escapes are skipped, so braces/commas supplied by an upstream
/// message cannot inflate the counter. For valid JSON, object fields and array
/// elements necessarily add `:` or `,`, making this a conservative node gate.
pub(crate) fn json_complexity(body: &[u8]) -> JsonComplexity {
    let mut in_string = false;
    let mut escaped = false;
    let mut depth = 0_usize;
    let mut nodes = 1_usize;
    for byte in body {
        if in_string {
            if escaped {
                escaped = false;
            } else if *byte == b'\\' {
                escaped = true;
            } else if *byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match *byte {
            b'"' => in_string = true,
            b'{' | b'[' => {
                depth = depth.saturating_add(1);
                if depth > MAX_DIAGNOSTIC_JSON_DEPTH {
                    return JsonComplexity::TooDeep;
                }
                nodes = nodes.saturating_add(1);
            }
            b'}' | b']' => depth = depth.saturating_sub(1),
            b',' | b':' => nodes = nodes.saturating_add(1),
            _ => {}
        }
        if nodes > MAX_DIAGNOSTIC_JSON_NODES {
            return JsonComplexity::TooComplex;
        }
    }
    JsonComplexity::WithinLimit
}

#[derive(Debug, Clone)]
pub(crate) struct DiagnosticMemoryBudget {
    limit: usize,
    retained: Arc<AtomicUsize>,
}

impl DiagnosticMemoryBudget {
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            limit,
            retained: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub(crate) fn try_reserve(
        &self,
        bytes: usize,
    ) -> Result<DiagnosticMemoryPermit, DiagnosticMemoryMiss> {
        if bytes == 0 {
            return Ok(DiagnosticMemoryPermit {
                retained: Arc::clone(&self.retained),
                limit: self.limit,
                bytes: 0,
            });
        }
        let result = self
            .retained
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                current
                    .checked_add(bytes)
                    .filter(|next| *next <= self.limit)
            });
        match result {
            Ok(_) => Ok(DiagnosticMemoryPermit {
                retained: Arc::clone(&self.retained),
                limit: self.limit,
                bytes,
            }),
            Err(retained) => Err(DiagnosticMemoryMiss {
                requested: bytes,
                retained,
                limit: self.limit,
            }),
        }
    }

    #[cfg(test)]
    pub(crate) fn retained(&self) -> usize {
        self.retained.load(Ordering::Acquire)
    }

    #[cfg(test)]
    pub(crate) fn limit(&self) -> usize {
        self.limit
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DiagnosticMemoryMiss {
    pub(crate) requested: usize,
    pub(crate) retained: usize,
    pub(crate) limit: usize,
}

#[derive(Debug)]
pub(crate) struct DiagnosticMemoryPermit {
    retained: Arc<AtomicUsize>,
    limit: usize,
    bytes: usize,
}

impl DiagnosticMemoryPermit {
    pub(crate) fn budget(&self) -> DiagnosticMemoryBudget {
        DiagnosticMemoryBudget {
            limit: self.limit,
            retained: Arc::clone(&self.retained),
        }
    }

    pub(crate) fn release(&mut self) {
        if self.bytes == 0 {
            return;
        }
        self.retained.fetch_sub(self.bytes, Ordering::AcqRel);
        self.bytes = 0;
    }
}

impl Drop for DiagnosticMemoryPermit {
    fn drop(&mut self) {
        self.release();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_memory_is_shared_and_released_by_raii() {
        let budget = DiagnosticMemoryBudget::new(10);
        let first = budget.try_reserve(6).expect("first reservation");
        assert_eq!(budget.retained(), 6);
        assert_eq!(
            budget.try_reserve(5).expect_err("shared limit"),
            DiagnosticMemoryMiss {
                requested: 5,
                retained: 6,
                limit: 10,
            }
        );
        drop(first);
        assert_eq!(budget.retained(), 0);
        assert!(budget.try_reserve(10).is_ok());
    }

    #[test]
    fn diagnostic_memory_stays_bounded_under_one_hundred_concurrent_reservations() {
        use std::{
            sync::{Arc, Barrier},
            thread,
        };

        const WORKERS: usize = 100;
        const RESERVATION_BYTES: usize = 512 * 1024;
        let budget = DiagnosticMemoryBudget::new(DEFAULT_DIAGNOSTIC_MEMORY_LIMIT_BYTES);
        let rendezvous = Arc::new(Barrier::new(WORKERS + 1));
        let mut workers = Vec::with_capacity(WORKERS);
        for _ in 0..WORKERS {
            let budget = budget.clone();
            let rendezvous = Arc::clone(&rendezvous);
            workers.push(thread::spawn(move || {
                let permit = budget.try_reserve(RESERVATION_BYTES).ok();
                rendezvous.wait();
                rendezvous.wait();
                permit.is_some()
            }));
        }

        rendezvous.wait();
        assert_eq!(budget.retained(), budget.limit());
        assert!(budget.retained() <= DEFAULT_DIAGNOSTIC_MEMORY_LIMIT_BYTES);
        rendezvous.wait();

        let admitted = workers
            .into_iter()
            .map(|worker| worker.join().expect("diagnostic worker"))
            .filter(|admitted| *admitted)
            .count();
        assert_eq!(admitted, 64);
        assert_eq!(budget.retained(), 0, "every RAII permit must release");
    }
}
