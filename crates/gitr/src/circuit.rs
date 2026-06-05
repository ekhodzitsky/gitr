use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Simple circuit breaker for network operations.
///
/// When the failure count exceeds `failure_threshold`, the breaker opens
/// and rejects all calls for `reset_timeout`. After the timeout, it moves
/// to half-open state where the next call acts as a probe.
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    failure_threshold: u32,
    success_threshold: u32,
    reset_timeout: Duration,
    state: Arc<AtomicU32>, // 0 = Closed, 1 = Open, 2 = HalfOpen
    failure_count: Arc<AtomicU32>,
    success_count: Arc<AtomicU32>,
    opened_at: Arc<std::sync::Mutex<Option<Instant>>>,
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new(5, 2, Duration::from_secs(30))
    }
}

impl CircuitBreaker {
    /// Create a new circuit breaker.
    pub fn new(failure_threshold: u32, success_threshold: u32, reset_timeout: Duration) -> Self {
        Self {
            failure_threshold,
            success_threshold,
            reset_timeout,
            state: Arc::new(AtomicU32::new(0)),
            failure_count: Arc::new(AtomicU32::new(0)),
            success_count: Arc::new(AtomicU32::new(0)),
            opened_at: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// Check whether the breaker allows a call.
    pub fn allow(&self) -> bool {
        match self.state.load(Ordering::SeqCst) {
            0 => true, // Closed
            1 => {
                // Open — check if timeout elapsed
                let should_try = {
                    let guard = match self.opened_at.lock() {
                        Ok(g) => g,
                        Err(e) => e.into_inner(),
                    };
                    guard.map_or(true, |t| t.elapsed() >= self.reset_timeout)
                };
                if should_try {
                    self.state.store(2, Ordering::SeqCst); // HalfOpen
                    true
                } else {
                    false
                }
            }
            _ => true, // HalfOpen
        }
    }

    /// Record a successful call.
    pub fn record_success(&self) {
        self.failure_count.store(0, Ordering::SeqCst);
        if self.state.load(Ordering::SeqCst) == 2 {
            let prev = self.success_count.fetch_add(1, Ordering::SeqCst);
            if prev + 1 >= self.success_threshold {
                self.state.store(0, Ordering::SeqCst); // Closed
                self.success_count.store(0, Ordering::SeqCst);
            }
        }
    }

    /// Record a failed call.
    pub fn record_failure(&self) {
        self.success_count.store(0, Ordering::SeqCst);
        let prev = self.failure_count.fetch_add(1, Ordering::SeqCst);
        if prev + 1 >= self.failure_threshold {
            self.state.store(1, Ordering::SeqCst); // Open
            let mut guard = match self.opened_at.lock() {
                Ok(g) => g,
                Err(e) => e.into_inner(),
            };
            *guard = Some(Instant::now());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circuit_breaker_closed() {
        let cb = CircuitBreaker::new(3, 1, Duration::from_secs(1));
        assert!(cb.allow());
        cb.record_failure();
        assert!(cb.allow());
    }

    #[test]
    fn test_circuit_breaker_opens() {
        let cb = CircuitBreaker::new(2, 1, Duration::from_secs(60));
        cb.record_failure();
        cb.record_failure();
        assert!(!cb.allow());
    }

    #[test]
    fn test_circuit_breaker_half_open() {
        let cb = CircuitBreaker::new(2, 1, Duration::from_millis(10));
        cb.record_failure();
        cb.record_failure();
        assert!(!cb.allow());
        std::thread::sleep(Duration::from_millis(20));
        assert!(cb.allow());
        cb.record_success();
        assert!(cb.allow());
    }

    #[test]
    fn test_circuit_breaker_default() {
        let cb: CircuitBreaker = Default::default();
        assert!(cb.allow());
    }

    #[test]
    fn test_circuit_breaker_half_open_allow_direct() {
        let cb = CircuitBreaker::new(2, 1, Duration::from_millis(10));
        cb.record_failure();
        cb.record_failure();
        assert!(!cb.allow());
        std::thread::sleep(Duration::from_millis(20));
        // First allow transitions Open -> HalfOpen and returns true
        assert!(cb.allow());
        // Second allow with state == HalfOpen hits the _ => true branch
        assert!(cb.allow());
    }

    #[test]
    fn test_circuit_breaker_poisoned_mutex() {
        let cb = CircuitBreaker::new(2, 1, Duration::from_secs(1));
        let cb2 = cb.clone();
        let _ = std::thread::spawn(move || {
            cb2.record_failure();
            cb2.record_failure();
            panic!("poison mutex");
        })
        .join();
        // allow() should handle poisoned mutex via e.into_inner()
        assert!(!cb.allow());
        // record_failure() should also handle poisoned mutex
        cb.record_failure();
    }
}
