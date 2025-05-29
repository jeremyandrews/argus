# Argus

Argus is a sophisticated multi-worker AI agent system designed to autonomously monitor, analyze, and report on information from numerous sources. Built in Rust, Argus employs multiple concurrent workers using both local (Ollama) and cloud-based (OpenAI) language models to provide intelligent decision-making and comprehensive analysis.

<img src="https://github.com/jeremyandrews/argus/blob/main/assets/argus-logo.png" alt="Argus Logo" width="200"/>

The name "Argus" is inspired by Argus Panoptes, the all-seeing giant in Greek mythology, reflecting the program's ability to monitor and analyze numerous information sources simultaneously.

## Architecture Overview

Argus employs a modern multi-worker architecture with specialized components:

```
RSS Sources → Content Extraction → Worker Pool → Analysis & Decision → Slack Notifications
                                       ↓
                                 SQLite Database
                                (Persistence & History)
```

**Core Components:**
- **RSS Monitor**: Continuously monitors multiple RSS feeds
- **Decision Workers**: Determine article relevance using AI models
- **Analysis Workers**: Perform deep content analysis and entity extraction
- **Database Layer**: SQLite-based persistence with clustering and relationships
- **Notification System**: Rich Slack integration with geographic relevance

## Key Features

### 🚀 **Multi-Worker Processing**
- Concurrent Decision and Analysis workers for scalable processing
- Independent worker pools with automatic load balancing
- Fallback support for high availability

### 🧠 **Dual LLM Support** 
- **Ollama Integration**: Local model deployment (privacy-focused)
- **OpenAI Integration**: Cloud-based models (GPT-4, etc)
- Mix and match providers within the same deployment

### ⚡ **Intelligent Model Selection**
- **Thinking Models**: Enhanced reasoning with structured analysis (temp=0.6, top_p=0.95)
- **Non-Thinking Models**: Faster processing for simple tasks (temp=0.7, top_p=0.8) 
- Automatic parameter optimization based on model type

### 📊 **Advanced Analysis**
- Entity extraction and normalization
- Article clustering and relationship mapping
- Geographic relevance detection
- Temporal analysis and trending

### 🔧 **Flexible Configuration**
- Environment variable-based configuration
- Per-worker model and parameter customization
- Runtime parameter overrides
- Hot-swappable worker configurations

### 🌍 **Place-Specific Intelligence**
- Geographic relevance detection for global teams
- Continent/country/city-level analysis
- Automated team member notification

### 📱 **Rich Slack Integration**
- Formatted notifications with context
- Geographic impact summaries
- Real-time status updates

## Model Configuration

### Thinking vs Non-Thinking Models

Argus automatically optimizes model parameters based on the model type:

#### **Thinking Mode** (Default)
- **When**: Models without `/no_think` suffix
- **Use Case**: Complex analysis requiring structured reasoning
- **Parameters**: 
  - Temperature: 0.6 (prevents greedy decoding)
  - Top-P: 0.95 (broad token consideration)
  - Top-K: 20, Min-P: 0.0

#### **Non-Thinking Mode** 
- **When**: Models with `/no_think` suffix (e.g., `qwen2.5:7b/no_think`)
- **Use Case**: Fast processing for simple decisions
- **Parameters**:
  - Temperature: 0.7 (optimized for speed)
  - Top-P: 0.8 (focused responses)
  - Top-K: 20, Min-P: 0.0

### Parameter Explanations

| Parameter | Range | Description |
|-----------|-------|-------------|
| **Temperature** | 0.0-2.0 | Controls randomness/creativity. Lower = more focused |
| **Top-P** | 0.0-1.0 | Nucleus sampling - limits tokens by probability mass |
| **Top-K** | Integer | Limits consideration to K most likely tokens |
| **Min-P** | 0.0-1.0 | Minimum probability threshold for token consideration |

### Environment Overrides

Override automatic parameter selection:

```bash
export LLM_TEMPERATURE="0.8"  # Override temperature for all workers
export LLM_TOP_P="0.9"        # Override Top-P 
export LLM_TOP_K="40"         # Override Top-K
export LLM_MIN_P="0.05"       # Override Min-P
```

## Worker Configuration

### Decision Workers

Handle initial article relevance determination:

```bash
# Ollama configuration
export DECISION_OLLAMA_CONFIGS="localhost|11434|llama3.1:70b;localhost|11434|qwen2.5:7b/no_think"

# OpenAI configuration  
export DECISION_OPENAI_CONFIGS="sk-xxxxx|gpt-4;sk-yyyyy|gpt-3.5-turbo"
```

### Analysis Workers

Perform deep content analysis with optional fallback:

```bash
# Ollama with fallback
export ANALYSIS_OLLAMA_CONFIGS="localhost|11434|llama3.1:70b||localhost|11435|qwen2.5:7b/no_think"

# OpenAI with fallback
export ANALYSIS_OPENAI_CONFIGS="sk-xxxxx|gpt-4||sk-yyyyy|gpt-3.5-turbo"
```

### Configuration Format

#### **Ollama Format**
- Basic: `host|port|model[/no_think]`
- With Fallback: `main_host|main_port|main_model||fallback_host|fallback_port|fallback_model`

#### **OpenAI Format**
- Basic: `api_key|model`
- With Fallback: `main_api_key|main_model||fallback_api_key|fallback_model`

## Environment Variables

### Core Configuration

| Variable | Required | Description |
|----------|----------|-------------|
| `SLACK_TOKEN` | ✅ | OAuth token for Slack app |
| `SLACK_CHANNEL` | ✅ | Slack channel ID for notifications |
| `URLS` | ✅ | Semicolon-separated RSS feed URLs |
| `TOPICS` | ✅ | Topic definitions for monitoring |

### Worker Configuration

| Variable | Description |
|----------|-------------|
| `DECISION_OLLAMA_CONFIGS` | Ollama instances for decision workers |
| `ANALYSIS_OLLAMA_CONFIGS` | Ollama instances for analysis workers |
| `DECISION_OPENAI_CONFIGS` | OpenAI configurations for decision workers |
| `ANALYSIS_OPENAI_CONFIGS` | OpenAI configurations for analysis workers |

### LLM Parameter Overrides

| Variable | Range | Default Behavior |
|----------|-------|------------------|
| `LLM_TEMPERATURE` | 0.0-2.0 | Auto: 0.6 (thinking), 0.7 (non-thinking) |
| `LLM_TOP_P` | 0.0-1.0 | Auto: 0.95 (thinking), 0.8 (non-thinking) |
| `LLM_TOP_K` | Integer | Auto: 20 (both modes) |
| `LLM_MIN_P` | 0.0-1.0 | Auto: 0.0 (both modes) |

### Optional Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `DATABASE_PATH` | `argus.db` | SQLite database path |
| `PLACES_JSON_PATH` | None | Geographic analysis configuration |
| `RUST_LOG` | `info` | Logging level (trace, debug, info, warn, error) |

## Place-Specific Analysis

Enable geographic relevance detection by creating a `places.json` file:

### 1. Create Configuration File
```bash
cp places.json.template places.json
export PLACES_JSON_PATH="places.json"
```

### 2. Structure
```json
{
  "Continent": {
    "Country": [
      "First, Last, City, Country, Timezone, SlackID"
    ]
  }
}
```

### 3. Example Configuration
```json
{
  "Europe": {
    "Italy": [
      "Marco, Rossi, Florence, Italy, CET (UTC+1), marco.r"
    ],
    "France": [
      "Marie, Dubois, Paris, France, CET (UTC+1), marie.d"
    ]
  },
  "Asia": {
    "Japan": [
      "Yuki, Tanaka, Tokyo, Japan, JST (UTC+9), yuki.t"
    ]
  }
}
```

### 4. How It Works
When enabled, Argus analyzes each article for geographic relevance:
1. **Continent Check**: "Does this affect people in [Continent]?"
2. **Country Check**: "Does this affect people in [Country]?"  
3. **City Check**: "Does this affect people in/near [City]?"
4. **Notification**: Affected individuals are tagged in Slack notifications

## Installation & Setup

### Prerequisites
- **Rust**: Latest stable version
- **SQLite**: For data persistence
- **Ollama** (optional): For local model deployment
- **OpenAI API Key** (optional): For cloud models

### 1. Clone and Build
```bash
git clone <repository_url>
cd argus
cargo build --release
```

### 2. Environment Configuration
```bash
cp env.template .env
# Edit .env with your configuration
source .env
```

### 3. First Run
```bash
cargo run --release
```

## Usage Examples

### Basic Setup (Single Worker)
```bash
export DECISION_OLLAMA_CONFIGS="localhost|11434|llama3.1:8b"
export ANALYSIS_OLLAMA_CONFIGS="localhost|11434|llama3.1:8b"
```

### Advanced Setup (Multi-Worker with Fallback)
```bash
export DECISION_OLLAMA_CONFIGS="localhost|11434|llama3.1:70b;localhost|11434|qwen2.5:7b/no_think"
export ANALYSIS_OLLAMA_CONFIGS="localhost|11434|llama3.1:70b||localhost|11435|qwen2.5:7b/no_think"
```

### Mixed Providers
```bash
export DECISION_OPENAI_CONFIGS="sk-xxxxx|gpt-4"
export ANALYSIS_OLLAMA_CONFIGS="localhost|11434|llama3.1:70b"
```

### Parameter Tuning
```bash
export LLM_TEMPERATURE="0.8"
export LLM_TOP_P="0.9"
export LLM_TOP_K="40"
```

## Logging & Monitoring

Argus uses structured logging with multiple targets:

### Log Targets
- **stdout**: General application logs (info level)
- **file**: Detailed logs including LLM requests (debug level)
- **web_request**: RSS and API activity
- **llm_request**: Model interaction details

### File Logging
Logs are stored in `logs/app.log` with daily rotation:
```
logs/
├── app.log          # Current day
├── app.log.2025-05-29
└── app.log.2025-05-28
```

### Monitoring Worker Activity
```bash
# Watch live logs
tail -f logs/app.log

# Filter for specific worker
tail -f logs/app.log | grep "Decision Worker 0"

# Monitor parameter usage
tail -f logs/app.log | grep "Using.*mode"
```

## Troubleshooting

### Connection Issues

#### **Ollama Connection Failed**
```bash
# Check Ollama status
curl http://localhost:11434/api/tags

# Verify model availability
ollama list

# Test specific model
ollama run llama3.1:8b "Hello"
```

#### **OpenAI API Errors**
- Verify API key validity
- Check rate limits and quotas
- Ensure model access permissions

### Worker Problems

#### **Workers Not Starting**
1. Check environment variable syntax
2. Verify model availability
3. Review logs for specific error messages

#### **Performance Issues**
- Monitor system resources (RAM/CPU)
- Consider adjusting worker counts
- Use lighter models for high-throughput scenarios

### Configuration Errors

#### **Invalid Model Configuration**
```
Error: Invalid Ollama configuration format
```
**Solution**: Check format: `host|port|model[/no_think]`

#### **Missing Environment Variables**
```
Error: SLACK_TOKEN environment variable required
```
**Solution**: Ensure all required variables are set in `.env`

### Parameter Tuning Issues

#### **Poor Response Quality**
- Increase temperature for more creativity
- Adjust Top-P for different response styles
- Use thinking mode for complex analysis

#### **Slow Performance**
- Use non-thinking models (`/no_think` suffix)
- Reduce Top-K values
- Consider lighter models

## Advanced Topics

### Performance Optimization

#### **Worker Scaling**
- 1-2 Decision workers typically sufficient
- Scale Analysis workers based on article volume
- Monitor CPU/memory usage to find optimal counts

#### **Model Selection Strategy**
- Use large models (70B+) for complex analysis
- Use smaller models (7B-13B) with `/no_think` for decisions
- Mix model sizes based on task complexity

### Custom Prompts
Argus generates specialized prompts for different tasks:
- **Relevance Detection**: Topic matching prompts
- **Entity Extraction**: Named entity recognition
- **Geographic Analysis**: Location-based relevance
- **Summarization**: Context-aware article summaries

### Database Management

#### **SQLite Maintenance**
```bash
# Backup database
cp argus.db argus.db.backup

# Vacuum database (reclaim space)
sqlite3 argus.db "VACUUM;"

# Check database size
ls -lh argus.db
```

#### **Schema Updates**
Migration scripts are provided in `migrations/`:
```bash
cargo run --bin migrate_cluster_schema
cargo run --bin migrate_cluster_merge_schema
```

### Scaling Considerations

#### **Multiple Instances**
- Use separate databases per instance
- Coordinate RSS feed parsing
- Share Slack notifications carefully

#### **Load Balancing**
- Distribute RSS feeds across instances
- Use different Ollama instances per worker type
- Implement external load balancing for high availability

## Dependencies

### Core Dependencies
- `ollama_rs`: Ollama API integration
- `async_openai`: OpenAI API integration  
- `readability`: Content extraction
- `rss`: RSS feed parsing
- `sqlx`: Database operations
- `reqwest`: HTTP client
- `tokio`: Async runtime
- `tracing`: Structured logging
- `serde_json`: JSON processing

### Development Dependencies
- `clap`: CLI argument parsing
- `anyhow`: Error handling
- `chrono`: Date/time operations

## Contributing

Contributions are welcome! Please:

1. Fork the repository
2. Create a feature branch
3. Add tests for new functionality
4. Ensure all tests pass
5. Submit a pull request

### Development Setup
```bash
# Run tests
cargo test

# Run specific test binaries
cargo run --bin test_thinking_model
cargo run --bin test_ollama_endpoints

# Format code
cargo fmt

# Lint code
cargo clippy
```

## License

This project is licensed under the MIT License. See LICENSE.txt for details.
