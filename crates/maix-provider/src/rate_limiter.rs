//! API rate limiting and retry with exponential backoff.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Rate limiter for API requests using a sliding window.
pub struct RateLimiter {
    /// Maximum requests per minute.
    requests_per_minute: u32,
    /// Request timestamps in the current window.
    window: Mutex<VecDeque<Instant>>,
}

impl RateLimiter {
    pub fn new(requests_per_minute: u32) -> Self {
        Self {
            requests_per_minute,
            window: Mutex::new(VecDeque::new()),
        }
    }

    /// Check if a request is allowed. If not, returns the duration to wait.
    pub fn check(&self) -> Result<(), Duration> {
        let mut window = self.window.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();

        // Remove expired entries (older than 1 minute)
        while let Some(&front) = window.front() {
            if now.duration_since(front) > Duration::from_secs(60) {
                window.pop_front();
            } else {
                break;
            }
        }

        if window.len() >= self.requests_per_minute as usize {
            // Calculate how long to wait until the oldest entry expires
            if let Some(&oldest) = window.front() {
                let wait = Duration::from_secs(60).saturating_sub(now.duration_since(oldest));
                return Err(wait);
            }
        }

        window.push_back(now);
        Ok(())
    }

    /// Wait until a request is allowed.
    pub async fn acquire(&self) {
        loop {
            match self.check() {
                Ok(()) => return,
                Err(wait) => {
                    tracing::debug!("Rate limited, waiting {:?}", wait);
                    tokio::time::sleep(wait).await;
                }
            }
        }
    }
}

/// Retry configuration for API calls.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retries.
    pub max_retries: u32,
    /// Base delay for exponential backoff.
    pub base_delay: Duration,
    /// Maximum delay between retries.
    pub max_delay: Duration,
    /// Whether to retry on rate limit (429) responses.
    pub retry_on_rate_limit: bool,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(30),
            retry_on_rate_limit: true,
        }
    }
}

/// Execute an async operation with retry and exponential backoff.
pub async fn with_retry<F, Fut, T, E>(
    config: &RetryConfig,
    rate_limiter: Option<&RateLimiter>,
    mut operation: F,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut last_error = None;

    for attempt in 0..=config.max_retries {
        // Acquire rate limit permit if available
        if let Some(limiter) = rate_limiter {
            limiter.acquire().await;
        }

        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                let error_msg = e.to_string();
                let is_retryable = is_retryable_error(&error_msg);

                if !is_retryable || attempt == config.max_retries {
                    return Err(e);
                }

                let delay = calculate_backoff(attempt, config);
                tracing::warn!(
                    "API call failed (attempt {}/{}): {}. Retrying in {:?}",
                    attempt + 1,
                    config.max_retries + 1,
                    error_msg,
                    delay
                );

                tokio::time::sleep(delay).await;
                last_error = Some(e);
            }
        }
    }

    // This should be unreachable, but just in case
    Err(last_error.unwrap())
}

/// Check if an error is retryable.
fn is_retryable_error(error: &str) -> bool {
    let lower = error.to_lowercase();

    // HTTP status codes that are retryable
    if lower.contains("429") || lower.contains("rate limit") {
        return true;
    }
    if lower.contains("500") || lower.contains("502") || lower.contains("503") || lower.contains("504") {
        return true;
    }

    // Network errors
    if lower.contains("timeout") || lower.contains("connection refused") || lower.contains("connection reset") {
        return true;
    }
    if lower.contains("eof") || lower.contains("broken pipe") || lower.contains("network") {
        return true;
    }

    // Transient errors
    if lower.contains("temporary") || lower.contains("unavailable") || lower.contains("overloaded") {
        return true;
    }

    false
}

/// Calculate exponential backoff delay.
fn calculate_backoff(attempt: u32, config: &RetryConfig) -> Duration {
    let shift = attempt.min(31);
    let multiplier = 1u64 << shift;
    let exponential = config.base_delay * multiplier.min(u32::MAX as u64) as u32;
    std::cmp::min(exponential, config.max_delay)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter_allows_within_limit() {
        let limiter = RateLimiter::new(10);
        for _ in 0..10 {
            assert!(limiter.check().is_ok());
        }
    }

    #[test]
    fn test_rate_limiter_blocks_over_limit() {
        let limiter = RateLimiter::new(2);
        assert!(limiter.check().is_ok());
        assert!(limiter.check().is_ok());
        assert!(limiter.check().is_err());
    }

    #[test]
    fn test_retryable_errors() {
        assert!(is_retryable_error("HTTP 429 Too Many Requests"));
        assert!(is_retryable_error("HTTP 500 Internal Server Error"));
        assert!(is_retryable_error("connection timeout"));
        assert!(is_retryable_error("rate limit exceeded"));
        assert!(!is_retryable_error("HTTP 400 Bad Request"));
        assert!(!is_retryable_error("invalid API key"));
    }

    #[test]
    fn test_backoff_calculation() {
        let config = RetryConfig {
            max_retries: 5,
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(30),
            retry_on_rate_limit: true,
        };

        assert_eq!(calculate_backoff(0, &config), Duration::from_secs(1));
        assert_eq!(calculate_backoff(1, &config), Duration::from_secs(2));
        assert_eq!(calculate_backoff(2, &config), Duration::from_secs(4));
        assert_eq!(calculate_backoff(3, &config), Duration::from_secs(8));
        assert_eq!(calculate_backoff(4, &config), Duration::from_secs(16));
        assert_eq!(calculate_backoff(5, &config), Duration::from_secs(30)); // capped
    }
}
