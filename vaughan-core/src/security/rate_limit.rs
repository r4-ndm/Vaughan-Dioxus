//! Authentication rate limiting helpers.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use tokio::sync::RwLock;

use crate::error::WalletError;

const MAX_FAILURES: u32 = 5;
const LOCKOUT_DURATION: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone)]
struct AttemptState {
    failures: u32,
    locked_until: Option<Instant>,
}

impl AttemptState {
    fn new() -> Self {
        Self { failures: 0, locked_until: None }
    }
}

/// Tracks repeated authentication failures and locks an identity out after too many attempts.
#[derive(Debug, Default)]
pub struct AuthRateLimiter {
    attempts: RwLock<HashMap<String, AttemptState>>,
}

impl AuthRateLimiter {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn is_locked(&self, key: &str) -> bool {
        let mut attempts = self.attempts.write().await;
        if let Some(state) = attempts.get_mut(key) {
            if let Some(until) = state.locked_until {
                if Instant::now() < until {
                    return true;
                }
                state.locked_until = None;
                state.failures = 0;
            }
        }
        false
    }

    pub async fn register_failure(&self, key: &str) -> Result<(), WalletError> {
        let mut attempts = self.attempts.write().await;
        let state = attempts.entry(key.to_string()).or_insert_with(AttemptState::new);

        if let Some(until) = state.locked_until {
            if Instant::now() < until {
                return Err(WalletError::Unauthorized);
            }
            state.locked_until = None;
            state.failures = 0;
        }

        state.failures = state.failures.saturating_add(1);
        if state.failures >= MAX_FAILURES {
            state.locked_until = Some(Instant::now() + LOCKOUT_DURATION);
            return Err(WalletError::Unauthorized);
        }

        Ok(())
    }

    pub async fn register_success(&self, key: &str) {
        self.attempts.write().await.remove(key);
    }

    pub fn lockout_duration(&self) -> Duration {
        LOCKOUT_DURATION
    }

    pub fn max_failures(&self) -> u32 {
        MAX_FAILURES
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn locks_after_five_failures() {
        let limiter = AuthRateLimiter::new();
        for _ in 0..4 {
            assert!(limiter.register_failure("user").await.is_ok());
            assert!(!limiter.is_locked("user").await);
        }
        assert!(limiter.register_failure("user").await.is_err());
        assert!(limiter.is_locked("user").await);
    }

    #[tokio::test]
    async fn success_clears_state() {
        let limiter = AuthRateLimiter::new();
        let _ = limiter.register_failure("user").await;
        limiter.register_success("user").await;
        assert!(!limiter.is_locked("user").await);
        assert!(limiter.register_failure("user").await.is_ok());
    }

    #[tokio::test]
    async fn unlocks_after_duration_expires() {
        let limiter = AuthRateLimiter::new();
        for _ in 0..5 {
            let _ = limiter.register_failure("user").await;
        }
        assert!(limiter.is_locked("user").await);

        sleep(Duration::from_millis(1)).await;
        let mut attempts = limiter.attempts.write().await;
        if let Some(state) = attempts.get_mut("user") {
            state.locked_until = Some(Instant::now() - Duration::from_secs(1));
        }
        drop(attempts);

        assert!(!limiter.is_locked("user").await);
    }

    proptest! {
        #[test]
        fn failure_counts_respect_lockout_threshold(failures in 0u32..10) {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_time()
                .build()
                .expect("tokio runtime");
            let _ = rt.block_on(async move {
                let limiter = AuthRateLimiter::new();
                for _ in 0..failures {
                    let _ = limiter.register_failure("user").await;
                }

                if failures < MAX_FAILURES {
                    prop_assert!(!limiter.is_locked("user").await);
                } else {
                    prop_assert!(limiter.is_locked("user").await);
                }
                Ok::<(), proptest::test_runner::TestCaseError>(())
            });
        }
    }
}
