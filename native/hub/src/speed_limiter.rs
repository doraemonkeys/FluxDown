//! Global token-bucket speed limiter shared across all download tasks.
//!
//! The limiter uses a simple token-bucket algorithm:
//! - Tokens are replenished at `limit` bytes/sec.
//! - Each download chunk must acquire tokens before proceeding.
//! - When `limit == 0`, the limiter is disabled (unlimited speed).
//!
//! The limiter is designed to be cheaply cloneable (`Arc` inside) so every
//! download segment can hold a handle without additional allocation.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokio::sync::Notify;

/// Shared, cheaply-cloneable speed limiter.
#[derive(Clone)]
pub struct SpeedLimiter {
    inner: Arc<Inner>,
}

struct Inner {
    /// Current speed limit in bytes/sec.  0 = unlimited.
    limit_bps: AtomicU64,
    /// Available tokens (bytes that may be consumed immediately).
    tokens: AtomicU64,
    /// Notify waiters when tokens are replenished.
    notify: Notify,
}

/// Refill interval — 50 ms gives smooth throughput without too many wake-ups.
const REFILL_INTERVAL_MS: u64 = 50;

impl SpeedLimiter {
    /// Create a new limiter with the given initial limit (bytes/sec).
    /// Pass `0` for unlimited.
    pub fn new(limit_bps: u64) -> Self {
        Self {
            inner: Arc::new(Inner {
                limit_bps: AtomicU64::new(limit_bps),
                tokens: AtomicU64::new(0),
                notify: Notify::new(),
            }),
        }
    }

    /// Update the speed limit at runtime.  Takes effect on the next refill tick.
    pub fn set_limit(&self, limit_bps: u64) {
        self.inner.limit_bps.store(limit_bps, Ordering::Relaxed);
        // Wake any waiters so they re-evaluate immediately.
        self.inner.notify.notify_waiters();
    }

    /// Current configured limit (bytes/sec).  0 = unlimited.
    #[allow(dead_code)]
    pub fn limit(&self) -> u64 {
        self.inner.limit_bps.load(Ordering::Relaxed)
    }

    /// Consume up to `requested` bytes worth of tokens.
    ///
    /// - If the limiter is disabled (limit == 0), returns `requested` immediately.
    /// - Otherwise waits until at least 1 token is available, then returns
    ///   `min(requested, available)`.  The caller should only process that many
    ///   bytes, then call `consume` again for the remainder.
    ///
    /// This design avoids holding an async lock and naturally distributes
    /// bandwidth among all concurrent callers via contention on the atomic.
    pub async fn consume(&self, requested: u64) -> u64 {
        if requested == 0 {
            return 0;
        }

        loop {
            let limit = self.inner.limit_bps.load(Ordering::Relaxed);
            if limit == 0 {
                // Unlimited — pass through.
                return requested;
            }

            // Try to take some tokens.
            let available = self.inner.tokens.load(Ordering::Acquire);
            if available > 0 {
                let take = requested.min(available);
                // CAS loop to atomically subtract tokens.
                match self.inner.tokens.compare_exchange_weak(
                    available,
                    available - take,
                    Ordering::AcqRel,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => return take,
                    Err(_) => continue, // contention — retry
                }
            }

            // No tokens available — wait for the refill task to notify us.
            self.inner.notify.notified().await;
        }
    }

    /// Spawn the background refill task.  Must be called once after creation.
    /// The task runs until the `SpeedLimiter` (and all its clones) are dropped.
    pub fn spawn_refill_task(&self) {
        let inner = Arc::downgrade(&self.inner);
        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(std::time::Duration::from_millis(REFILL_INTERVAL_MS));
            // The first tick completes immediately — skip it.
            interval.tick().await;

            loop {
                interval.tick().await;
                let Some(inner) = inner.upgrade() else {
                    // All SpeedLimiter handles dropped — exit.
                    break;
                };

                let limit = inner.limit_bps.load(Ordering::Relaxed);
                if limit == 0 {
                    // Unlimited — clear any accumulated tokens and wake waiters
                    // (they will see limit==0 and pass through).
                    inner.tokens.store(0, Ordering::Relaxed);
                    inner.notify.notify_waiters();
                    continue;
                }

                // Add tokens proportional to (limit * interval).
                let refill = limit * REFILL_INTERVAL_MS / 1000;
                // Cap at 2× per-tick amount to prevent burst accumulation after
                // a period of inactivity.
                let cap = refill * 2;
                let prev = inner.tokens.fetch_add(refill, Ordering::AcqRel);
                let new_val = prev + refill;
                if new_val > cap {
                    inner.tokens.store(cap, Ordering::Release);
                }

                inner.notify.notify_waiters();
            }
        });
    }
}
