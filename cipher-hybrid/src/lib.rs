//! Hybrid adaptive throughput for Cipher data plane + documented L1 pairing.
//!
//! **Low load:** operate near `floor_tps` (~1k) — slower batch seals, minimal CPU churn.
//! **High load:** scale toward `ceiling_aggregate_tps` (~500k) across `logical_shard_count` shards.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

pub const DEFAULT_FLOOR_TPS: u64 = 1_000;
pub const DEFAULT_CEILING_AGGREGATE_TPS: u64 = 500_000;
pub const DEFAULT_LOGICAL_SHARD_COUNT: u16 = 8;

/// Tunables for embedded node / fleet.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HybridConfig {
    /// Baseline throughput regime when ingress is light (~1k msg/s).
    pub floor_tps: u64,
    /// Aggregate ceiling across all logical shards (500k spec target).
    pub ceiling_aggregate_tps: u64,
    /// Virtual shards for routing + capacity math (8 × 62.5k ≈ 500k).
    pub logical_shard_count: u16,
}

impl Default for HybridConfig {
    fn default() -> Self {
        Self {
            floor_tps: DEFAULT_FLOOR_TPS,
            ceiling_aggregate_tps: DEFAULT_CEILING_AGGREGATE_TPS,
            logical_shard_count: DEFAULT_LOGICAL_SHARD_COUNT,
        }
    }
}

/// Operating mode derived from measured ingress.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HybridMode {
    /// Ingress well below floor — conserve resources (~1k class).
    Idle,
    /// Between floor and mid — linear ramp.
    Ramp,
    /// Heavy load — aggressive sealing, full shard parallelism.
    Peak,
}

/// Executor / relay knobs for the current window.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HybridSettings {
    pub mode: HybridMode,
    /// Target aggregate msg/s for this regime.
    pub target_aggregate_tps: u64,
    /// Per-shard target (ceiling / shard_count at peak).
    pub target_per_shard_tps: u64,
    /// Seal when pending ≥ this or after `seal_interval_ms`.
    pub max_batch_size: u32,
    pub seal_interval_ms: u64,
    /// Recommended L1 block gas floor (~1k EVM TPS at 50k gas/tx).
    pub l1_reference_min_gas: u64,
}

impl HybridSettings {
    pub fn per_shard_ceiling(config: &HybridConfig) -> u64 {
        let n = config.logical_shard_count.max(1) as u64;
        config.ceiling_aggregate_tps / n
    }
}

/// Thread-safe load tracker shared by relay + shard executor.
pub struct HybridController {
    config: HybridConfig,
    window_start: Mutex<Instant>,
    window_count: AtomicU64,
    lifetime_ingress: AtomicU64,
    lifetime_sealed: AtomicU64,
    last_measured_tps: AtomicU64,
}

impl HybridController {
    pub fn new(config: HybridConfig) -> Self {
        Self {
            config,
            window_start: Mutex::new(Instant::now()),
            window_count: AtomicU64::new(0),
            lifetime_ingress: AtomicU64::new(0),
            lifetime_sealed: AtomicU64::new(0),
            last_measured_tps: AtomicU64::new(0),
        }
    }

    pub fn config(&self) -> &HybridConfig {
        &self.config
    }

    /// Call on every accepted `/store` or ingest.
    pub fn record_ingress(&self) {
        self.lifetime_ingress.fetch_add(1, Ordering::Relaxed);
        self.window_count.fetch_add(1, Ordering::Relaxed);
        self.refresh_rate();
    }

    pub fn record_sealed(&self, count: u32) {
        self.lifetime_sealed
            .fetch_add(count as u64, Ordering::Relaxed);
    }

    fn refresh_rate(&self) {
        let mut start = self.window_start.lock().expect("hybrid window lock");
        let elapsed = start.elapsed();
        if elapsed < Duration::from_millis(500) {
            return;
        }
        let count = self.window_count.swap(0, Ordering::Relaxed);
        let secs = elapsed.as_secs_f64().max(0.001);
        let tps = (count as f64 / secs).round() as u64;
        self.last_measured_tps.store(tps, Ordering::Relaxed);
        *start = Instant::now();
    }

    pub fn measured_tps(&self) -> u64 {
        self.refresh_rate();
        self.last_measured_tps.load(Ordering::Relaxed)
    }

    pub fn mode(&self) -> HybridMode {
        let m = self.measured_tps();
        let floor = self.config.floor_tps;
        if m < floor {
            HybridMode::Idle
        } else if m < floor.saturating_mul(10) {
            HybridMode::Ramp
        } else {
            HybridMode::Peak
        }
    }

    pub fn settings(&self) -> HybridSettings {
        let mode = self.mode();
        let measured = self.measured_tps();
        let cfg = &self.config;
        let per_shard_cap = HybridSettings::per_shard_ceiling(cfg);

        let (target_aggregate, seal_interval_ms, max_batch_size) = match mode {
            HybridMode::Idle => (
                cfg.floor_tps,
                1_000,
                (cfg.floor_tps as u32).clamp(100, 2_000),
            ),
            HybridMode::Ramp => {
                let t = measured.clamp(cfg.floor_tps, cfg.ceiling_aggregate_tps);
                let ratio = (t - cfg.floor_tps) as f64
                    / (cfg.floor_tps.saturating_mul(9).max(1) as f64);
                let interval = (1_000.0 - ratio * 990.0).max(10.0) as u64;
                let batch = (t / cfg.logical_shard_count as u64).clamp(500, 20_000) as u32;
                (t, interval, batch)
            }
            HybridMode::Peak => (
                cfg.ceiling_aggregate_tps,
                10,
                (per_shard_cap as u32).clamp(5_000, 100_000),
            ),
        };

        HybridSettings {
            mode,
            target_aggregate_tps: target_aggregate,
            target_per_shard_tps: per_shard_cap,
            max_batch_size,
            seal_interval_ms,
            l1_reference_min_gas: 50_000_000, // ~1k TPS @ 50k gas, 1s blocks
        }
    }

    pub fn snapshot(&self) -> HybridSnapshot {
        HybridSnapshot {
            config: self.config.clone(),
            settings: self.settings(),
            measured_tps: self.measured_tps(),
            lifetime_ingress: self.lifetime_ingress.load(Ordering::Relaxed),
            lifetime_sealed: self.lifetime_sealed.load(Ordering::Relaxed),
            metrics_note: "settings.target_* and config.ceiling_* are configured ceilings, not verified fleet TPS; measured_tps is a local rolling estimate; lifetime_ingress counts relay /store accepts only",
        }
    }

    /// Route key → local shard index `[0, logical_shard_count)`.
    pub fn shard_route_index(&self, route_key: &str) -> u16 {
        let n = self.config.logical_shard_count.max(1);
        let mut hash: u64 = 0;
        for b in route_key.as_bytes() {
            hash = hash.wrapping_mul(31).wrapping_add(*b as u64);
        }
        (hash % n as u64) as u16
    }
}

impl std::fmt::Debug for HybridController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HybridController")
            .field("snapshot", &self.snapshot())
            .finish()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HybridSnapshot {
    pub config: HybridConfig,
    pub settings: HybridSettings,
    /// Rolling estimate from recent window (not a capacity guarantee).
    pub measured_tps: u64,
    /// Accepted messages at the relay edge (`/store`); one count per client upload.
    pub lifetime_ingress: u64,
    pub lifetime_sealed: u64,
    /// How to interpret this snapshot for investors and benchmarks.
    pub metrics_note: &'static str,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_settings_near_floor() {
        let c = HybridController::new(HybridConfig::default());
        let s = c.settings();
        assert_eq!(s.mode, HybridMode::Idle);
        assert_eq!(s.target_aggregate_tps, DEFAULT_FLOOR_TPS);
    }

    #[test]
    fn peak_after_burst() {
        let c = HybridController::new(HybridConfig::default());
        for _ in 0..20_000 {
            c.record_ingress();
        }
        std::thread::sleep(Duration::from_millis(600));
        let s = c.settings();
        assert_eq!(s.mode, HybridMode::Peak);
        assert_eq!(
            s.target_aggregate_tps,
            DEFAULT_CEILING_AGGREGATE_TPS
        );
    }
}
