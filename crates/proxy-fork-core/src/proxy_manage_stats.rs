// 对外可见的快照结构，包含普通 usize 字段方便断言/打印
#[derive(Debug, Clone, Default)]
pub struct ProxyStatsSnapshot {
    pub cache_hits: usize,
    pub exact_hits: usize,
    pub pattern_hits: usize,
    pub misses: usize,
    pub total_lookups: usize,
}

impl ProxyStatsSnapshot {
    pub fn hit_rate(&self) -> f64 {
        if self.total_lookups == 0 {
            0.0
        } else {
            (self.cache_hits + self.exact_hits + self.pattern_hits) as f64
                / self.total_lookups as f64
        }
    }

    pub fn cache_hit_rate(&self) -> f64 {
        if self.total_lookups == 0 {
            0.0
        } else {
            self.cache_hits as f64 / self.total_lookups as f64
        }
    }
}

#[cfg(feature = "proxy_manage_stats")]
pub mod stats_impl {
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug, Default)]
    pub struct ProxyStats {
        pub(crate) cache_hits: AtomicUsize,
        pub(crate) exact_hits: AtomicUsize,
        pub(crate) pattern_hits: AtomicUsize,
        pub(crate) misses: AtomicUsize,
        pub(crate) total_lookups: AtomicUsize,
    }

    impl ProxyStats {
        pub fn inc_total(&self) {
            self.total_lookups.fetch_add(1, Ordering::Relaxed);
        }

        pub fn inc_cache(&self) {
            self.cache_hits.fetch_add(1, Ordering::Relaxed);
        }

        pub fn inc_exact(&self) {
            self.exact_hits.fetch_add(1, Ordering::Relaxed);
        }

        pub fn inc_pattern(&self) {
            self.pattern_hits.fetch_add(1, Ordering::Relaxed);
        }

        pub fn inc_miss(&self) {
            self.misses.fetch_add(1, Ordering::Relaxed);
        }

        pub fn snapshot(&self) -> super::ProxyStatsSnapshot {
            super::ProxyStatsSnapshot {
                cache_hits: self.cache_hits.load(Ordering::Relaxed),
                exact_hits: self.exact_hits.load(Ordering::Relaxed),
                pattern_hits: self.pattern_hits.load(Ordering::Relaxed),
                misses: self.misses.load(Ordering::Relaxed),
                total_lookups: self.total_lookups.load(Ordering::Relaxed),
            }
        }

        pub fn reset(&self) {
            self.cache_hits.store(0, Ordering::Relaxed);
            self.exact_hits.store(0, Ordering::Relaxed);
            self.pattern_hits.store(0, Ordering::Relaxed);
            self.misses.store(0, Ordering::Relaxed);
            self.total_lookups.store(0, Ordering::Relaxed);
        }
    }
}

#[cfg(not(feature = "proxy_manage_stats"))]
pub mod stats_impl {

    #[derive(Debug, Default)]
    pub struct ProxyStats {}

    impl ProxyStats {
        pub fn inc_total(&self) {}
        pub fn inc_cache(&self) {}
        pub fn inc_exact(&self) {}
        pub fn inc_pattern(&self) {}
        pub fn inc_miss(&self) {}
        pub fn snapshot(&self) -> super::ProxyStatsSnapshot {
            super::ProxyStatsSnapshot::default()
        }
        pub fn reset(&self) {}
    }
}
