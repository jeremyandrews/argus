//! Rate limiting for OpenAI API calls to prevent hitting quota limits.

use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;
use tokio::time::sleep;
use tracing::{debug, info, warn};

use crate::TARGET_LLM_REQUEST;

/// Error types for rate limiting
#[derive(Debug, Clone)]
pub enum RateLimitError {
    DailyLimitExceeded,
    WaitRequired(Duration),
    ConfigurationError(String),
}

impl std::fmt::Display for RateLimitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RateLimitError::DailyLimitExceeded => write!(f, "Daily request limit exceeded"),
            RateLimitError::WaitRequired(duration) => {
                write!(f, "Rate limit wait required: {:?}", duration)
            }
            RateLimitError::ConfigurationError(msg) => {
                write!(f, "Rate limiter configuration error: {}", msg)
            }
        }
    }
}

impl std::error::Error for RateLimitError {}

/// Token bucket for requests per minute limiting
#[derive(Debug)]
struct TokenBucket {
    tokens: f64,
    capacity: f64,
    refill_rate: f64, // tokens per second
    last_refill: Instant,
}

impl TokenBucket {
    fn new(capacity: f64, refill_rate_per_second: f64) -> Self {
        Self {
            tokens: capacity,
            capacity,
            refill_rate: refill_rate_per_second,
            last_refill: Instant::now(),
        }
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        let tokens_to_add = elapsed * self.refill_rate;

        self.tokens = (self.tokens + tokens_to_add).min(self.capacity);
        self.last_refill = now;
    }

    fn try_consume(&mut self, tokens: f64) -> bool {
        self.refill();
        if self.tokens >= tokens {
            self.tokens -= tokens;
            true
        } else {
            false
        }
    }

    fn time_until_available(&mut self, tokens: f64) -> Duration {
        self.refill();
        if self.tokens >= tokens {
            Duration::from_secs(0)
        } else {
            let tokens_needed = tokens - self.tokens;
            let seconds_needed = tokens_needed / self.refill_rate;
            Duration::from_secs_f64(seconds_needed)
        }
    }
}

/// Daily counter for requests per day limiting
#[derive(Debug)]
struct DailyCounter {
    count: u32,
    limit: u32,
    reset_time: SystemTime,
}

impl DailyCounter {
    fn new(limit: u32) -> Self {
        Self {
            count: 0,
            limit,
            reset_time: next_midnight(),
        }
    }

    fn increment(&mut self) -> Result<(), RateLimitError> {
        let now = SystemTime::now();

        // Check if we need to reset (new day)
        if now >= self.reset_time {
            self.count = 0;
            self.reset_time = next_midnight();
            info!(target: TARGET_LLM_REQUEST, "Daily rate limit counter reset");
        }

        if self.count >= self.limit {
            return Err(RateLimitError::DailyLimitExceeded);
        }

        self.count += 1;
        Ok(())
    }

    fn remaining(&mut self) -> u32 {
        let now = SystemTime::now();

        // Check if we need to reset (new day)
        if now >= self.reset_time {
            self.count = 0;
            self.reset_time = next_midnight();
        }

        self.limit.saturating_sub(self.count)
    }
}

/// Calculate next midnight UTC
fn next_midnight() -> SystemTime {
    let now = SystemTime::now();
    let since_epoch = now.duration_since(UNIX_EPOCH).unwrap_or_default();
    let seconds_in_day = 24 * 60 * 60;
    let seconds_today = since_epoch.as_secs() % seconds_in_day;
    let seconds_until_midnight = seconds_in_day - seconds_today;

    now + Duration::from_secs(seconds_until_midnight)
}

/// Main rate limiter for OpenAI API calls
#[derive(Clone)]
pub struct OpenAIRateLimiter {
    rpm_bucket: Arc<Mutex<TokenBucket>>,
    rpd_counter: Arc<Mutex<DailyCounter>>,
    enabled: bool,
    burst_allowance: u32,
}

impl OpenAIRateLimiter {
    /// Create a new rate limiter from environment variables
    pub fn from_env() -> Result<Self, RateLimitError> {
        let enabled = std::env::var("OPENAI_RATE_LIMIT_ENABLED")
            .unwrap_or_else(|_| "true".to_string())
            .parse::<bool>()
            .map_err(|e| {
                RateLimitError::ConfigurationError(format!(
                    "Invalid OPENAI_RATE_LIMIT_ENABLED value: {}",
                    e
                ))
            })?;

        if !enabled {
            info!(target: TARGET_LLM_REQUEST, "OpenAI rate limiting disabled");
            return Ok(Self::disabled());
        }

        let rpm = std::env::var("OPENAI_RATE_LIMIT_RPM")
            .unwrap_or_else(|_| "500".to_string())
            .parse::<u32>()
            .map_err(|e| {
                RateLimitError::ConfigurationError(format!(
                    "Invalid OPENAI_RATE_LIMIT_RPM value: {}",
                    e
                ))
            })?;

        let rpd = std::env::var("OPENAI_RATE_LIMIT_RPD")
            .unwrap_or_else(|_| "10000".to_string())
            .parse::<u32>()
            .map_err(|e| {
                RateLimitError::ConfigurationError(format!(
                    "Invalid OPENAI_RATE_LIMIT_RPD value: {}",
                    e
                ))
            })?;

        let burst = std::env::var("OPENAI_RATE_LIMIT_BURST")
            .unwrap_or_else(|_| "10".to_string())
            .parse::<u32>()
            .map_err(|e| {
                RateLimitError::ConfigurationError(format!(
                    "Invalid OPENAI_RATE_LIMIT_BURST value: {}",
                    e
                ))
            })?;

        info!(
            target: TARGET_LLM_REQUEST,
            "OpenAI rate limiter configured: {} RPM, {} RPD, burst={}", rpm, rpd, burst
        );

        Ok(Self::new(rpm, rpd, burst))
    }

    /// Create a new rate limiter with specified limits
    pub fn new(requests_per_minute: u32, requests_per_day: u32, burst_allowance: u32) -> Self {
        let refill_rate_per_second = requests_per_minute as f64 / 60.0;
        let bucket_capacity = (burst_allowance + requests_per_minute / 4) as f64; // Allow some burst

        Self {
            rpm_bucket: Arc::new(Mutex::new(TokenBucket::new(
                bucket_capacity,
                refill_rate_per_second,
            ))),
            rpd_counter: Arc::new(Mutex::new(DailyCounter::new(requests_per_day))),
            enabled: true,
            burst_allowance,
        }
    }

    /// Create a disabled rate limiter (no limiting)
    pub fn disabled() -> Self {
        Self {
            rpm_bucket: Arc::new(Mutex::new(TokenBucket::new(1000.0, 1000.0))), // Effectively unlimited
            rpd_counter: Arc::new(Mutex::new(DailyCounter::new(1_000_000))), // Effectively unlimited
            enabled: false,
            burst_allowance: 1000,
        }
    }

    /// Acquire a permit to make an API call, waiting if necessary
    pub async fn acquire_permit(&self) -> Result<(), RateLimitError> {
        if !self.enabled {
            return Ok(());
        }

        // Check daily limit first
        {
            let mut daily_counter = self.rpd_counter.lock().await;
            daily_counter.increment()?;
        }

        // Check per-minute rate limiting
        loop {
            let wait_duration = {
                let mut bucket = self.rpm_bucket.lock().await;
                if bucket.try_consume(1.0) {
                    debug!(target: TARGET_LLM_REQUEST, "Rate limit permit acquired");
                    return Ok(());
                } else {
                    bucket.time_until_available(1.0)
                }
            };

            if wait_duration > Duration::from_secs(0) {
                warn!(
                    target: TARGET_LLM_REQUEST,
                    "Rate limit reached, waiting {:?} before next request", wait_duration
                );
                sleep(wait_duration).await;
            }
        }
    }

    /// Check if we should wait before making a request (non-blocking)
    pub async fn should_wait(&self) -> Option<Duration> {
        if !self.enabled {
            return None;
        }

        // Check if daily limit is exceeded
        {
            let mut daily_counter = self.rpd_counter.lock().await;
            if daily_counter.remaining() == 0 {
                return Some(Duration::from_secs(3600)); // Wait an hour if daily limit hit
            }
        }

        // Check per-minute limiting
        let mut bucket = self.rpm_bucket.lock().await;
        let wait_time = bucket.time_until_available(1.0);
        if wait_time > Duration::from_secs(0) {
            Some(wait_time)
        } else {
            None
        }
    }

    /// Record a successful request (for monitoring)
    pub async fn record_success(&self) {
        if !self.enabled {
            return;
        }

        debug!(target: TARGET_LLM_REQUEST, "OpenAI API request completed successfully");
    }

    /// Record a failed request (for monitoring)
    pub async fn record_failure(&self, error_type: &str) {
        warn!(
            target: TARGET_LLM_REQUEST,
            "OpenAI API request failed: {}", error_type
        );
    }

    /// Get current status (for debugging)
    pub async fn status(&self) -> (u32, f64) {
        if !self.enabled {
            return (0, 0.0);
        }

        let daily_remaining = {
            let mut daily_counter = self.rpd_counter.lock().await;
            daily_counter.remaining()
        };

        let rpm_tokens = {
            let mut bucket = self.rpm_bucket.lock().await;
            bucket.refill();
            bucket.tokens
        };

        (daily_remaining, rpm_tokens)
    }

    /// Get the configured burst allowance
    pub fn burst_allowance(&self) -> u32 {
        self.burst_allowance
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn test_token_bucket_basic() {
        let mut bucket = TokenBucket::new(5.0, 1.0); // 5 capacity, 1 token per second

        // Should be able to consume initial tokens
        assert!(bucket.try_consume(1.0));
        assert!(bucket.try_consume(2.0));
        assert!(bucket.try_consume(2.0));

        // Should be empty now
        assert!(!bucket.try_consume(1.0));

        // Wait and try again
        sleep(Duration::from_millis(1100)).await;
        assert!(bucket.try_consume(1.0)); // Should have refilled ~1 token
    }

    #[tokio::test]
    async fn test_daily_counter() {
        let mut counter = DailyCounter::new(3);

        assert!(counter.increment().is_ok());
        assert!(counter.increment().is_ok());
        assert!(counter.increment().is_ok());
        assert!(counter.increment().is_err()); // Should exceed limit
    }

    #[tokio::test]
    async fn test_rate_limiter_disabled() {
        let limiter = OpenAIRateLimiter::disabled();

        // Should always succeed when disabled
        for _ in 0..100 {
            assert!(limiter.acquire_permit().await.is_ok());
        }
    }
}
