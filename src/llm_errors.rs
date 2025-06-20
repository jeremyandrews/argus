//! Enhanced error types for LLM operations with better classification.

use std::time::Duration;

/// Enhanced error classification for LLM operations
#[derive(Debug, Clone)]
pub enum LLMError {
    /// Rate limit exceeded - should not add article to database for retry
    RateLimit {
        retry_after: Option<Duration>,
        message: String,
        provider: String,
    },
    /// Temporary failure (network, timeout) - should not add article to database
    TemporaryFailure {
        message: String,
        provider: String,
        retryable: bool,
    },
    /// Permanent failure (auth, invalid model) - should mark article as processed
    PermanentFailure { message: String, provider: String },
}

impl LLMError {
    /// Create a rate limit error for OpenAI
    pub fn openai_rate_limit(message: String, retry_after: Option<Duration>) -> Self {
        Self::RateLimit {
            retry_after,
            message,
            provider: "OpenAI".to_string(),
        }
    }

    /// Create a rate limit error for Ollama (though Ollama doesn't typically rate limit)
    pub fn ollama_rate_limit(message: String) -> Self {
        Self::RateLimit {
            retry_after: None,
            message,
            provider: "Ollama".to_string(),
        }
    }

    /// Create a temporary failure error
    pub fn temporary_failure(provider: &str, message: String, retryable: bool) -> Self {
        Self::TemporaryFailure {
            message,
            provider: provider.to_string(),
            retryable,
        }
    }

    /// Create a permanent failure error
    pub fn permanent_failure(provider: &str, message: String) -> Self {
        Self::PermanentFailure {
            message,
            provider: provider.to_string(),
        }
    }

    /// Check if this error should trigger a retry by the RSS system
    pub fn should_allow_rss_retry(&self) -> bool {
        match self {
            LLMError::RateLimit { .. } => true, // Rate limits should allow RSS retry
            LLMError::TemporaryFailure { retryable, .. } => *retryable, // Some temp failures are retryable
            LLMError::PermanentFailure { .. } => false, // Permanent failures should not retry
        }
    }

    /// Check if this is a retryable error
    pub fn is_retryable(&self) -> bool {
        match self {
            LLMError::RateLimit { .. } => true,
            LLMError::TemporaryFailure { retryable, .. } => *retryable,
            LLMError::PermanentFailure { .. } => false,
        }
    }

    /// Get the error message
    pub fn message(&self) -> &str {
        match self {
            LLMError::RateLimit { message, .. } => message,
            LLMError::TemporaryFailure { message, .. } => message,
            LLMError::PermanentFailure { message, .. } => message,
        }
    }

    /// Get the provider name
    pub fn provider(&self) -> &str {
        match self {
            LLMError::RateLimit { provider, .. } => provider,
            LLMError::TemporaryFailure { provider, .. } => provider,
            LLMError::PermanentFailure { provider, .. } => provider,
        }
    }

    /// Get retry after duration if available
    pub fn retry_after(&self) -> Option<Duration> {
        match self {
            LLMError::RateLimit { retry_after, .. } => *retry_after,
            _ => None,
        }
    }
}

impl std::fmt::Display for LLMError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LLMError::RateLimit {
                message,
                provider,
                retry_after,
            } => {
                if let Some(duration) = retry_after {
                    write!(
                        f,
                        "{} rate limit: {} (retry after {:?})",
                        provider, message, duration
                    )
                } else {
                    write!(f, "{} rate limit: {}", provider, message)
                }
            }
            LLMError::TemporaryFailure {
                message,
                provider,
                retryable,
            } => {
                write!(
                    f,
                    "{} temporary failure: {} (retryable: {})",
                    provider, message, retryable
                )
            }
            LLMError::PermanentFailure { message, provider } => {
                write!(f, "{} permanent failure: {}", provider, message)
            }
        }
    }
}

impl std::error::Error for LLMError {}

/// Helper functions for parsing specific provider errors
pub mod parsing {
    use super::*;
    use async_openai::error::OpenAIError;
    use std::time::Duration;

    /// Parse OpenAI errors into our enhanced error types
    pub fn parse_openai_error(error: &OpenAIError) -> LLMError {
        let error_string = error.to_string();

        // Check for rate limiting
        if error_string.contains("Rate limit") || error_string.contains("429") {
            let retry_after = extract_retry_after_from_openai_error(error);
            return LLMError::openai_rate_limit(error_string, retry_after);
        }

        // Check for authentication issues
        if error_string.contains("401")
            || error_string.contains("Unauthorized")
            || error_string.contains("Invalid API key")
        {
            return LLMError::permanent_failure(
                "OpenAI",
                format!("Authentication failed: {}", error_string),
            );
        }

        // Check for model not found
        if error_string.contains("404") || error_string.contains("model not found") {
            return LLMError::permanent_failure(
                "OpenAI",
                format!("Model not found: {}", error_string),
            );
        }

        // Check for server errors (5xx)
        if error_string.contains("500")
            || error_string.contains("502")
            || error_string.contains("503")
            || error_string.contains("504")
        {
            return LLMError::temporary_failure(
                "OpenAI",
                format!("Server error: {}", error_string),
                true,
            );
        }

        // Check for network/timeout issues
        if error_string.contains("timeout")
            || error_string.contains("network")
            || error_string.contains("connection")
        {
            return LLMError::temporary_failure(
                "OpenAI",
                format!("Network error: {}", error_string),
                true,
            );
        }

        // Default to temporary failure for unknown errors
        LLMError::temporary_failure("OpenAI", format!("Unknown error: {}", error_string), false)
    }

    /// Extract retry-after duration from OpenAI error if available
    fn extract_retry_after_from_openai_error(error: &OpenAIError) -> Option<Duration> {
        // This is a simplified implementation - OpenAI errors might contain
        // retry-after information in different formats
        let error_string = error.to_string();

        // Look for patterns like "retry after 60 seconds"
        if let Some(start) = error_string.find("retry after") {
            let remaining = &error_string[start + 11..];
            if let Some(end) = remaining.find(" ") {
                let number_str = &remaining[..end];
                if let Ok(seconds) = number_str.trim().parse::<u64>() {
                    return Some(Duration::from_secs(seconds));
                }
            }
        }

        // Default retry after duration for rate limits
        Some(Duration::from_secs(60))
    }

    /// Parse Ollama errors into our enhanced error types
    pub fn parse_ollama_error(error: &ollama_rs::error::OllamaError) -> LLMError {
        let error_string = error.to_string();

        // Ollama doesn't typically have rate limits, but check for server issues
        if error_string.contains("timeout")
            || error_string.contains("network")
            || error_string.contains("connection")
        {
            return LLMError::temporary_failure(
                "Ollama",
                format!("Network error: {}", error_string),
                true,
            );
        }

        // Check for model not found
        if error_string.contains("model not found") || error_string.contains("not found") {
            return LLMError::permanent_failure(
                "Ollama",
                format!("Model not found: {}", error_string),
            );
        }

        // Default to temporary failure
        LLMError::temporary_failure("Ollama", format!("Unknown error: {}", error_string), false)
    }
}
