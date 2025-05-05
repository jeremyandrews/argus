# Argus Technical Context

## Technology Stack

### Core Technologies
- **Language**: Rust (stable channel)
- **Database**: SQLite
- **Vector Database**: Integrated SQLite-based vector storage with Qdrant client
- **Web Framework**: Axum for API endpoints
- **UI Clients**: 
  - Slack integration for notifications
  - Mobile application (React Native)

### External Services
- **LLM Integration**: 
  - OpenAI API for cloud-based models
  - Local models via Ollama, with special support for:
    - Qwen models with `/no_think` mode (disables thinking tags for direct responses)
- **Cloud Storage**: AWS S3/Cloudflare R2 for content storage
- **Logging**: Tracing library with structured logging
- **Metrics**: Prometheus-compatible metrics collection
- **Notification**: Slack API for alerts

### Development Tooling
- **Build System**: Cargo
- **Package Management**: Cargo packages and dependencies
- **Testing**: Rust test framework and integration tests
- **CI/CD**: GitHub Actions for continuous integration
- **Documentation**: Cargo doc and Markdown documentation

## System Components

### RSS Fetcher
- **Purpose**: Retrieves content from configured RSS feeds
- **Technologies**: reqwest for HTTP, feed-rs for RSS parsing
- **Behavior**: Periodically polls feeds, normalizes URLs, deduplicates entries

### Decision Workers
- **Purpose**: Determine if content is relevant to configured topics
- **Technologies**: LLM integration for content classification
- **Behavior**: Process RSS queue items, classify content, route to appropriate queues

### Analysis Workers
- **Purpose**: Perform deep analysis of relevant content
- **Technologies**: LLM integration for analysis, vector operations for similarity
- **Behavior**: Generate summaries, extract entities, evaluate quality, store embeddings

### API Server
- **Purpose**: Provide data access and integrations
- **Technologies**: Axum web framework, JWT for authentication
- **Behavior**: Serve article data, handle notifications, manage user subscriptions

### Vector Store
- **Purpose**: Enable semantic similarity search of content
- **Technologies**: SQLite-based vector operations
- **Behavior**: Store and query high-dimensional embeddings, calculate similarities

### Entity Store
- **Purpose**: Track and relate named entities across articles
- **Technologies**: SQLite with specialized schema
- **Behavior**: Store entities with normalization, track relationships, support entity queries

## Technical Constraints

### Performance Requirements
- **RSS Fetching**: Process 1000+ feeds every 15 minutes
- **Decision Making**: Classify content within 60 seconds of fetching
- **Analysis**: Complete full analysis within 5 minutes of classification
- **API Response**: 95th percentile response time under 200ms

### Scalability Considerations
- **Worker Scaling**: Support for multiple concurrent workers of each type
- **Database Performance**: Optimized indexes for common query patterns
- **Memory Efficiency**: Careful management of memory usage for vector operations
- **Connection Pooling**: Database connection pooling for concurrent access

### Security Requirements
- **API Authentication**: JWT-based authentication for all API endpoints
- **Rate Limiting**: Protection against excessive API usage
- **Input Validation**: Strict validation of all external inputs
- **Dependency Management**: Regular updates for security patches

### Deployment Considerations
- **Containerization**: Docker support for consistent deployment
- **Configuration**: Environment variable-based configuration
- **Monitoring**: Health checks and metrics endpoints
- **Logging**: Structured logging with configurable verbosity levels

## Technical Debt & Constraints

### Current Limitations
- **Single Database**: Currently uses a single SQLite database instance
- **Model Flexibility**: Limited support for swapping between different LLM providers
- **Test Coverage**: Integration tests need expansion
- **Documentation**: API documentation needs improvement

### Planned Technical Improvements
- **Entity Extraction Enhancement**: Improve detection and normalization of entities
- **Temporal Awareness**: Better date extraction and event correlation
- **Cross-Language Support**: Improved handling of non-English content
- **Distributed Database**: Support for database sharding or replication

## Development Practices

### Code Structure
- **Module Organization**: Modular code with clear separation of concerns
- **Error Handling**: Consistent error propagation and handling
- **Configuration**: Centralized configuration management
- **Component Communication**: Message passing via database queues

### Testing Approach
- **Unit Testing**: Critical components have unit test coverage
- **Integration Testing**: End-to-end testing of worker pipelines
- **Performance Testing**: Benchmarks for critical operations
- **LLM Testing**: Validation of model outputs against expectations

### Documentation Standards
- **Code Documentation**: All public functions and types have documentation
- **Architecture Documentation**: System design and patterns are documented
- **API Documentation**: OpenAPI/Swagger documentation for REST endpoints
- **Operational Documentation**: Deployment and monitoring guides

## Libraries & Components

### Data Processing & Storage
- **qdrant-client**: Vector database client for semantic search operations
- **sqlx**: SQLite database with async support
- **aws-sdk-s3**: AWS S3 integration for R2 content storage
- **feed-rs**: RSS feed parsing and processing
- **readability**: HTML content extraction and cleaning

### Text Processing
- **tokenizers**: Text tokenization for LLM processing
- **strsim**: String similarity calculations
- **rust-stemmers**: Word stemming for normalization
- **unicode-normalization**: Unicode text normalization
- **unicode-segmentation**: Text segmentation utilities
- **whatlang**: Language detection for multilingual support
- **urlnorm**: URL normalization

### Machine Learning
- **candle-core/nn/transformers**: Machine learning operations framework
- **async-openai**: OpenAI API client
- **ollama-rs**: Ollama API integration for local LLMs

### Web & API
- **axum**: Web framework for API endpoints
- **jsonwebtoken**: JWT authentication
- **reqwest**: HTTP client for external API interactions

### Security & Cryptography
- **ring**: Cryptographic operations
- **sha2**: Hashing functions
- **base64**: Encoding and decoding
