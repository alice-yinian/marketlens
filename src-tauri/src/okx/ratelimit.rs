//! 按分组计数的令牌桶限流。
//!
//! 为什么不是全局一个桶：OKX 的限流规则既有按 IP 的（公开行情）也有按 User ID 的
//! （私有账户）。共用一个桶会让私有端点的配额被公开行情吃掉，反之亦然。

use std::collections::HashMap;
use std::time::Duration;

use tokio::sync::Mutex;
use tokio::time::Instant;

use crate::okx::endpoints::RateGroup;

pub struct RateLimiter {
    buckets: Mutex<HashMap<RateGroup, Bucket>>,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            buckets: Mutex::new(HashMap::new()),
        }
    }

    /// 取得一次请求许可；配额用尽时**等待**到有令牌为止，而不是报错。
    ///
    /// 等待而非失败是刻意的：M1/M3 的采集是用户主动触发的批量拉取，
    /// 排队等待只会让操作慢一点，直接失败却会让用户面对一个残缺的快照。
    pub async fn acquire(&self, group: RateGroup) {
        loop {
            let wait = {
                let mut buckets = self.buckets.lock().await;
                let bucket = buckets.entry(group).or_insert_with(|| Bucket::new(group));
                bucket.try_consume()
            };

            match wait {
                None => return,
                Some(delay) => tokio::time::sleep(delay).await,
            }
        }
    }
}

struct Bucket {
    tokens: f64,
    quota: u32,
    window: Duration,
    last_refill: Instant,
}

impl Bucket {
    fn new(group: RateGroup) -> Self {
        let (quota, window_ms) = group.quota();
        Self {
            tokens: f64::from(quota),
            quota,
            window: Duration::from_millis(window_ms),
            last_refill: Instant::now(),
        }
    }

    /// 按经过时间补充令牌；能取到则消费并返回 `None`，
    /// 否则返回需要等待的时长。
    fn try_consume(&mut self) -> Option<Duration> {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill);
        let rate = f64::from(self.quota) / self.window.as_secs_f64();

        self.tokens = (self.tokens + elapsed.as_secs_f64() * rate).min(f64::from(self.quota));
        self.last_refill = now;

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            None
        } else {
            let deficit = 1.0 - self.tokens;
            Some(Duration::from_secs_f64(deficit / rate))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 配额内不等待；超出后必须按速率排队。
    /// 用 `start_paused` 让时间由 tokio 驱动，测试不依赖真实时钟。
    #[tokio::test(start_paused = true)]
    async fn waits_only_after_quota_exhausted() {
        let limiter = RateLimiter::new();
        let group = RateGroup::Rubik; // 5 req / 2s → 每 400ms 补一个
        let start = Instant::now();

        for _ in 0..5 {
            limiter.acquire(group).await;
        }
        assert_eq!(
            start.elapsed(),
            Duration::ZERO,
            "配额内的 5 次请求不应该产生任何等待"
        );

        limiter.acquire(group).await;
        let waited = start.elapsed();
        assert!(
            waited >= Duration::from_millis(400) && waited < Duration::from_millis(450),
            "第 6 次请求应等待约 400ms（2s/5），实际 {waited:?}"
        );
    }

    /// 不同分组必须各自计数：私有端点的配额不能被公开行情消耗。
    #[tokio::test(start_paused = true)]
    async fn groups_are_counted_independently() {
        let limiter = RateLimiter::new();
        let start = Instant::now();

        // 耗尽 rubik 配额（5）
        for _ in 0..5 {
            limiter.acquire(RateGroup::Rubik).await;
        }
        // 另一个分组的首次请求应立即通过
        limiter.acquire(RateGroup::Market).await;

        assert_eq!(
            start.elapsed(),
            Duration::ZERO,
            "耗尽 rubik 配额不应影响 public.market 分组"
        );
    }

    /// 令牌不能无限累积：长时间空闲后最多恢复到配额上限。
    #[tokio::test(start_paused = true)]
    async fn tokens_do_not_accumulate_beyond_quota() {
        let limiter = RateLimiter::new();
        let group = RateGroup::Rubik;

        for _ in 0..5 {
            limiter.acquire(group).await;
        }
        tokio::time::sleep(Duration::from_secs(600)).await;

        let start = Instant::now();
        for _ in 0..5 {
            limiter.acquire(group).await;
        }
        assert_eq!(start.elapsed(), Duration::ZERO);

        // 第 6 次仍需等待，说明空闲 10 分钟没有攒出额外令牌
        limiter.acquire(group).await;
        assert!(start.elapsed() >= Duration::from_millis(400));
    }
}
