//! 实盘快照的 TTL 缓存（设计文档 §6.2.3）。
//!
//! 为什么需要它：实盘刷新是**用户操作触发**的，而「进页面 → 切走 → 再回来」
//! 会连续触发刷新。没有缓存的话，用户的手指就能把限流配额打爆。

use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::market::live::LiveSnapshot;

/// 实盘数据 TTL。
///
/// 30 秒：足够覆盖「切走再回来」的操作节奏，又能防止连点刷新。
pub const LIVE_TTL: Duration = Duration::from_secs(30);

pub struct LiveCache {
    ttl: Duration,
    entry: Mutex<Option<Entry>>,
}

struct Entry {
    snapshot: LiveSnapshot,
    stored_at: Instant,
}

impl Default for LiveCache {
    fn default() -> Self {
        Self::with_ttl(LIVE_TTL)
    }
}

impl LiveCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// 指定 TTL。存在的意义是让 TTL 行为可被确定性测试——测试不必依赖真实睡眠。
    pub fn with_ttl(ttl: Duration) -> Self {
        Self {
            ttl,
            entry: Mutex::new(None),
        }
    }

    /// 命中未过期的快照，返回的副本会被标记为 `cache_hit = true`。
    pub fn get_fresh(&self) -> Option<LiveSnapshot> {
        self.get_fresh_at(Instant::now())
    }

    pub fn store(&self, snapshot: &LiveSnapshot) {
        self.store_at(snapshot, Instant::now());
    }

    /// 注入时间点的命中判定，便于测试。
    fn get_fresh_at(&self, now: Instant) -> Option<LiveSnapshot> {
        let entry = self.entry.lock().ok()?;
        let entry = entry.as_ref()?;

        if now.saturating_duration_since(entry.stored_at) > self.ttl {
            return None;
        }

        let mut snapshot = entry.snapshot.clone();
        snapshot.cache_hit = true;
        Some(snapshot)
    }

    fn store_at(&self, snapshot: &LiveSnapshot, now: Instant) {
        if let Ok(mut entry) = self.entry.lock() {
            *entry = Some(Entry {
                snapshot: snapshot.clone(),
                stored_at: now,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market::live::LiveSnapshot;

    fn snapshot() -> LiveSnapshot {
        LiveSnapshot {
            ts: 1_790_000_000_000,
            cache_hit: false,
            fetched_at: 1_790_000_000_000,
            watchlist: vec!["BTC-USDT-SWAP".to_string()],
            instruments: Vec::new(),
            warnings: Vec::new(),
        }
    }

    #[test]
    fn miss_when_empty() {
        let cache = LiveCache::with_ttl(Duration::from_secs(30));
        assert!(cache.get_fresh().is_none());
    }

    #[test]
    fn hit_within_ttl_and_marked_as_cached() {
        let cache = LiveCache::with_ttl(Duration::from_secs(30));
        let start = Instant::now();
        cache.store_at(&snapshot(), start);

        let hit = cache
            .get_fresh_at(start + Duration::from_secs(29))
            .expect("29 秒时应命中");
        assert!(hit.cache_hit, "命中缓存必须标记，UI 要据此提示数据新鲜度");
        assert_eq!(hit.ts, 1_790_000_000_000, "缓存命中不应篡改原始数据时间");
    }

    #[test]
    fn miss_after_ttl() {
        let cache = LiveCache::with_ttl(Duration::from_secs(30));
        let start = Instant::now();
        cache.store_at(&snapshot(), start);

        assert!(
            cache
                .get_fresh_at(start + Duration::from_secs(31))
                .is_none(),
            "超过 TTL 必须重新拉取"
        );
    }

    #[test]
    fn store_overwrites_previous_entry() {
        let cache = LiveCache::with_ttl(Duration::from_secs(30));
        let start = Instant::now();
        cache.store_at(&snapshot(), start);

        let mut newer = snapshot();
        newer.ts = 1_790_000_999_000;
        cache.store_at(&newer, start + Duration::from_secs(5));

        let hit = cache
            .get_fresh_at(start + Duration::from_secs(6))
            .expect("应命中");
        assert_eq!(hit.ts, 1_790_000_999_000);
    }
}
