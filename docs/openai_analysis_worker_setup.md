# OpenAI Analysis Worker Setup Guide

## Overview

This guide explains how to enable OpenAI models for analysis workers while avoiding rate limit conflicts with decision workers. The system now supports both Ollama and OpenAI models simultaneously with comprehensive rate limiting.

## Prerequisites

- OpenAI API key
- Understanding of OpenAI tier limits (Tier 1: 500 RPM / 10,000 RPD)
- Existing Argus system with working Ollama setup

## Features Implemented

### ✅ Complete OpenAI JSON Mode Support
- **Entity Extraction**: Full JSON schema support for extracting entities
- **Threat Location Analysis**: Geographic analysis with continent/country/region breakdown
- **Generic JSON**: Fallback JSON mode for other use cases
- **Chat Completions API**: Proper OpenAI API integration with automatic JSON instruction injection

### ✅ Enhanced Error Handling
- **Rate Limit Detection**: Automatic detection of OpenAI 429 errors
- **Retry Logic**: Smart retry decisions that distinguish rate limits from permanent failures
- **RSS Worker Integration**: Rate-limited articles are NOT marked as processed, allowing RSS worker to retry

### ✅ Comprehensive Rate Limiting
- **Token Bucket Algorithm**: Prevents hitting RPM limits proactively
- **Daily Counter**: Tracks requests per day with midnight UTC reset
- **Configurable Limits**: Environment-based configuration for different OpenAI tiers
- **Burst Allowance**: Handles short spikes in request volume
- **Shared Rate Limiter**: Both decision and analysis workers coordinate to prevent conflicts

## Configuration

### Environment Variables

Add these to your environment configuration:

```bash
# Rate Limiting Configuration
OPENAI_RATE_LIMIT_ENABLED="true"     # Enable/disable rate limiting
OPENAI_RATE_LIMIT_RPM="500"          # Requests per minute limit (Tier 1)
OPENAI_RATE_LIMIT_RPD="10000"        # Requests per day limit (Tier 1)
OPENAI_RATE_LIMIT_BURST="10"         # Burst allowance for short spikes

# Analysis Worker Configuration (example)
ANALYSIS_OPENAI_CONFIGS="sk-proj-your-api-key-here|gpt-4o-mini"

# Decision Worker Configuration (example) 
DECISION_OPENAI_CONFIGS="sk-proj-your-api-key-here|gpt-4o-mini"
```

### Worker Configuration Format

For OpenAI workers, use this format:
```
api_key|model
```

Example configurations:
- `sk-proj-abc123...|gpt-4o-mini` - GPT-4o Mini (recommended for cost)
- `sk-proj-abc123...|gpt-3.5-turbo` - GPT-3.5 Turbo
- `sk-proj-abc123...|gpt-4` - GPT-4 (expensive but highest quality)

For multiple workers, separate with semicolons:
```bash
DECISION_OPENAI_CONFIGS="sk-proj-key1|gpt-4o-mini;sk-proj-key2|gpt-3.5-turbo"
```

## Rate Limiting Strategies

### Option 1: Shared Rate Limiter (Recommended)
- Both decision and analysis workers share the same rate limiter
- Automatic coordination prevents exceeding limits
- Simple configuration and monitoring
- Total usage: 500 RPM / 10,000 RPD across all workers

### Option 2: Split Allocation
- Decision workers: 300 RPM / 6000 RPD
- Analysis workers: 200 RPM / 4000 RPD
- Requires separate rate limiter instances
- More complex but provides guaranteed allocation

### Option 3: Hybrid Approach
- Decision workers: OpenAI (time-sensitive)
- Analysis workers: Mix of OpenAI + Ollama (fallback to local when rate limited)
- Best of both worlds but more complex configuration

## How Rate Limiting Works

### Proactive Prevention
```rust
// Rate limiter prevents hitting limits before they occur
rate_limiter.acquire_permit().await?;
// Only proceeds when safe to make request
```

### Error Handling
```rust
match llm_error {
    LLMError::RateLimit(_) => {
        // Article NOT added to database
        // RSS worker will retry in 10 minutes
    }
    LLMError::PermanentFailure(_) => {
        // Article marked as non-relevant
        // Prevents infinite retries
    }
}
```

### Daily Reset
- Counters reset at midnight UTC
- Automatic handling of timezone changes
- Logging when reset occurs

## Testing

### Test OpenAI JSON Functionality
```bash
# Test entity extraction
cargo run --bin test_openai_json -- --openai-api-key YOUR_KEY --test-type entity

# Test threat location analysis  
cargo run --bin test_openai_json -- --openai-api-key YOUR_KEY --test-type threat
```

### Monitor Rate Limiting
```bash
# Check rate limiter status
grep "Rate limit" logs/app.log

# Monitor successful requests
grep "Rate limit permit acquired" logs/app.log

# Check for rate limit hits
grep "Rate limit reached" logs/app.log
```

## Production Deployment

### Step 1: Configure Environment
```bash
# Set OpenAI API key
export OPENAI_API_KEY="your-key-here"

# Configure rate limits for your tier
export OPENAI_RATE_LIMIT_RPM="500"    # Adjust for your tier
export OPENAI_RATE_LIMIT_RPD="10000"  # Adjust for your tier
```

### Step 2: Update Worker Configurations
```bash
# Enable OpenAI for analysis workers
export ANALYSIS_OPENAI_CONFIGS="sk-proj-your-api-key-here|gpt-4o-mini"

# Keep decision workers on OpenAI or switch to Ollama
export DECISION_OLLAMA_CONFIGS="localhost|11434|qwen2.5:32b"
```

### Step 3: Start Workers
```bash
# Start with new configuration
cargo run --release
```

### Step 4: Monitor Performance
- Watch rate limit logs
- Monitor article processing speed
- Check for rate limit errors
- Verify JSON response quality

## Troubleshooting

### Rate Limit Errors
```
[analysis worker 1 gpt-4o-mini]: LLM error: Rate limit exceeded
```
**Solution**: Reduce RPM limit or increase burst allowance

### JSON Parsing Errors
```
Failed to parse JSON response from OpenAI
```
**Solution**: Check model supports JSON mode, verify prompt formatting

### Articles Not Being Processed
```
Article will be retried by RSS worker
```
**Solution**: This is normal for rate-limited articles, they'll be retried

### High Costs
**Solution**: 
- Use gpt-4o-mini instead of gpt-4
- Implement hybrid Ollama/OpenAI approach
- Reduce rate limits to control costs

## Cost Optimization

### Model Selection
- **gpt-4o-mini**: $0.15/1M input tokens (recommended)
- **gpt-3.5-turbo**: $0.50/1M input tokens
- **gpt-4**: $30/1M input tokens (expensive)

### Hybrid Strategy
```bash
# Analysis workers: Local models for bulk processing
ANALYSIS_OLLAMA_CONFIGS="localhost|11434|qwen2.5:32b"

# Decision workers: OpenAI for accuracy
DECISION_OPENAI_CONFIGS="sk-proj-your-api-key-here|gpt-4o-mini"
```

### Rate Limit Tuning
```bash
# Conservative limits to control costs
OPENAI_RATE_LIMIT_RPM="100"    # Lower than tier limit
OPENAI_RATE_LIMIT_RPD="2000"   # Lower than tier limit
```

## Architecture Benefits

### Dual Provider Support
- Both Ollama and OpenAI can be used simultaneously
- Easy switching between providers
- Fallback options available

### Provider Flexibility
- Switch between local (Ollama) and cloud (OpenAI) models as needed
- Different models for different worker types
- Cost optimization through strategic model selection

### Fault Tolerance
- Rate-limited content gets retried automatically
- No lost articles due to rate limiting
- Graceful degradation under load

## Monitoring and Observability

### Key Metrics to Monitor
- Rate limit permit acquisitions per minute
- Rate limit errors per hour
- Daily request count vs limit
- Article processing success rate
- Average response time per provider

### Log Patterns
```bash
# Successful rate limiting
"Rate limit permit acquired for OpenAI request"

# Rate limit prevention
"Rate limit reached, waiting 30s before next request"

# Daily reset
"Daily rate limit counter reset"

# Error handling
"Article will be retried by RSS worker"
```

## Security Considerations

### API Key Management
- Store OpenAI API key securely
- Rotate keys regularly
- Monitor usage for anomalies
- Use environment variables, not hardcoded keys

### Rate Limit Protection
- Never disable rate limiting in production
- Set conservative limits initially
- Monitor and adjust based on usage patterns
- Implement alerting for limit breaches

## Future Enhancements

### Planned Improvements
- Per-worker rate limiting
- Dynamic rate limit adjustment
- Cost tracking and budgets
- Model performance analytics
- Automatic fallback to Ollama when rate limited

### Configuration Database Migration
When PostgreSQL migration is complete, all rate limiting configuration will move to the database for runtime management without restarts.

## Support

For issues or questions:
1. Check logs for rate limiting messages
2. Verify OpenAI API key and permissions
3. Test with the provided test binaries
4. Monitor rate limit usage patterns
5. Adjust configuration based on actual usage

The system is now production-ready for OpenAI analysis workers with comprehensive rate limiting and error handling.
