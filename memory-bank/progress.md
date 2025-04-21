# Progress: Argus

## Current Status

Argus is currently in active development with major components implemented and functioning. The system follows an iterative development approach with continuous refinement of existing features alongside the addition of new capabilities.

## What Works

### Core Infrastructure
- ✅ **Async Processing Framework**: Tokio-based concurrency model for efficient execution
- ✅ **Database Integration**: SQLite with SQLx for persistent storage and queues
- ✅ **Logging System**: Comprehensive tracing setup with file and console outputs
- ✅ **Configuration System**: Environment variable-based configuration

### Content Processing
- ✅ **RSS Feed Parsing**: Reliable fetching and parsing of feeds in multiple formats
- ✅ **Content Extraction**: Article extraction from web pages using readability
- ✅ **Duplicate Detection**: Content deduplication through hash-based comparison
- ✅ **Queue Management**: Database-backed queues for reliable content processing

### LLM Integration
- ✅ **Ollama Support**: Integration with local Ollama LLM instances
- ✅ **OpenAI Support**: Integration with cloud-based OpenAI models
- ✅ **Prompt Templates**: Structured prompt system for consistent LLM interactions
- ✅ **Model Configuration**: Flexible configuration of different models for different tasks

### Analysis & Relevance
- ✅ **Topic Matching**: Two-stage matching for improved relevance determination
- ✅ **Content Summarization**: Extraction of key points from articles
- ✅ **Critical Analysis**: Assessment of source credibility and content quality
- ✅ **Logical Fallacy Detection**: Identification of reasoning flaws in content
- ✅ **Geographic Impact Assessment**: Detection of geographical relevance for safety issues
- ✅ **Action Recommendations**: Providing practical, actionable steps based on article content
- ✅ **Talking Points**: Generating discussion-worthy topics for sharing and engagement
- ✅ **Basic Vector Similarity**: Initial implementation of article similarity using E5 embeddings

### Entity-Based Article Matching
- ✅ **Relational Entity Model**: Implemented comprehensive database schema for entity storage and relationships
- ✅ **Entity Extraction**: Built structured LLM prompts for reliable entity recognition
- ✅ **Entity Repository**: Created efficient storage and retrieval patterns for entities
- ✅ **Entity Importance Classification**: System for ranking entity relevance (PRIMARY/SECONDARY/MENTIONED)
- ✅ **Event Date Tracking**: Added temporal information extraction for event correlation
- ✅ **Analysis Integration**: Incorporated entity extraction into the analysis workflow
- ✅ **Entity Module Architecture**: Established specialized module with types, extraction, matching, and repository components
- ✅ **Multi-Factor Similarity**: Implemented algorithms combining vector similarity with entity overlap
- ✅ **Vector Integration**: Enhanced vector payloads with entity IDs and temporal data
- ✅ **Similarity Algorithms**: Created enhanced match detection with weighted scoring across multiple dimensions

### Notification Systems
- ✅ **Slack Integration**: Formatting and delivery of notifications to Slack channels
- ✅ **iOS Push Notifications**: Integration with Apple Push Notification service
- ✅ **R2 Content Storage**: Content upload to Cloudflare R2 for app access

### API & Mobile Integration
- ✅ **Authentication System**: JWT-based authentication for mobile clients
- ✅ **Topic Subscription**: User subscription management for topics
- ✅ **Seen Article Syncing**: Tracking of read/unread article status
- ✅ **Device Management**: Device token registration and tracking

## In Progress

### Entity Extraction and Storage
- ✅ **JSON Schema System**: Implemented JsonSchemaType enum to handle different LLM response formats
- ✅ **Schema Definitions**: Added proper schema definitions for entity extraction responses
- ✅ **Entity Extraction Fix**: Fixed entity extraction to properly use JSON mode with LLMs
- ✅ **LLM Parameter Enhancement**: Extended LLMParams to specify which JSON schema to use
- ✅ **Analysis Worker Integration**: Replaced direct LLM calls with proper extract_entities function 
- ✅ **Error Handling**: Added better error handling and logging for entity extraction
- ✅ **Reprocessing Utility**: Created process_entities.rs utility for existing articles
- ✅ **Testing Utility**: Added test_entity_extraction.rs for verifying functionality
- ✅ **Serialization Fix**: Fixed field name mismatch between LLM output and database processing
- ✅ **Ollama Connectivity Fix**: Fixed URL handling for Ollama endpoints in utility programs
- ✅ **Documentation**: Added documentation to new utilities and updated memory bank
- 🔄 **Extraction Quality**: Monitoring and adjusting entity extraction prompts
- 🔄 **Entity Normalization**: Enhancing normalization for better cross-article matching

### Enhanced Clustering with Entity-Based Matching
- ✅ **Multi-Factor Similarity**: Implemented algorithms that combine vector similarity with entity relationships
- ✅ **Entity Repository**: Added `get_articles_by_entities` method in db.rs for centralized entity queries 
- ✅ **Dual-Query Approach**: Implemented combined vector and entity-based search strategy with proper error handling
- ✅ **Compilation Issues**: Fixed type annotation and Qdrant client compatibility issues in vector.rs
- ✅ **Similarity Transparency**: Enhanced article matching with detailed similarity metrics exposure
  - Added vector quality fields (vector_score, active_dimensions, magnitude)
  - Added entity-specific metrics (overlap counts, type-specific similarity scores)
  - Added similarity formula explanations in the JSON output
  - Verified implementation with `cargo check --bin argus` (no warnings)
- ✅ **Similarity Scoring Consistency**: Fixed inconsistency in similarity scoring that allowed articles with no entity overlap to appear in results
  - Modified scoring to apply consistent 60% weighting to vector similarity when no entity data exists
  - Updated similarity formula descriptions to accurately reflect the actual calculation
  - Ensured articles need to share at least some entity overlap to reach the minimum threshold (0.75)
  - Verified the fix ensures more semantically relevant "similar articles" results
- ✅ **Enhanced Diagnostic Logging**: Added comprehensive logging throughout the entity matching process
  - Added detailed logging of entity extraction, entity retrieval, and entity matching steps
  - Added pre-filter and post-filter logging to show exactly which articles are being filtered out and why
  - Added verification of entity overlap in final results with error reporting if an article without entity overlap passes filtering
  - Improved transparency for debugging the entity-based article matching system
- ✅ **Entity Retrieval Fix**: Fixed critical issue in the source entity retrieval process
  - Enhanced `get_similar_articles_with_entities` function to track source article ID
  - Added database verification to compare entity counts in different systems
  - Fixed incomplete function calls in analysis_worker.rs to ensure all parameters are passed
  - Added warning generation when entity IDs are missing or inconsistent
  - Implemented comprehensive source entity tracing through the matching pipeline
- ✅ **Enhanced Debug Diagnostics**: Implemented comprehensive diagnostic enhancements to troubleshoot entity matching issues
  - Added detailed entity-by-entity comparison logs in the matching process
  - Enhanced entity retrieval with importance level and entity type breakdowns
  - Added SQL query diagnostics to detect date filtering issues
  - Implemented critical error detection for date filtering problems
  - Added sample data logging for filtered articles to identify format issues
- ✅ **Date Comparison Fix**: Fixed critical issue with date filtering in SQL queries
  - Identified that string comparison of RFC3339 formatted dates was failing
  - Initial attempt to use SQLite's `datetime()` functions did not fully resolve the issue
  - Modified SQL query to use substring comparison: `substr(a.pub_date, 1, 10) >= substr(?, 1, 10)`
  - This extracts only the date portion (YYYY-MM-DD) from both dates, avoiding timezone complexities
  - Verified fix with direct SQL testing showing proper date-only matching
  - Enhanced diagnostics confirmed the root cause and solution effectiveness
- ✅ **Date Window Approach for Related Articles**: Implemented a more robust date filtering method
  - Changed from fixed threshold date to a dynamic date window around each article's own publication date
  - Window spans from 14 days before to 1 day after the article's publication date
  - Updated `get_articles_by_entities_with_date` in db.rs to use this window approach
  - Modified `get_similar_articles_with_entities` and `get_articles_by_entities` in vector.rs to pass source article ID
  - Added code to retrieve the source article's publication date for date window calculation
  - Enhanced logging to monitor effectiveness of the date window approach
  - Identified future cleanup needs for articles with unrealistic dates

- ✅ **NULL Handling and Date Filtering Fixes**: Fixed critical bugs in data storage and retrieval
  - Fixed `store_embedding` in vector.rs to properly handle NULL values using `Option<&str>` parameters
  - Enhanced SQL query with COALESCE to consider both event_date and pub_date when filtering
  - Added index on articles(pub_date) for improved query performance
  - Modified query strategy to skip date filtering when no source date exists
  - Fixed "unknown" string literals being stored instead of proper NULL values
  - Implemented proper SQL substring date extraction to handle RFC3339 formatted dates consistently
- 🔄 **Entity-Aware Clustering**: Implementing cluster tracking based on shared entities
- 🔄 **Qdrant Integration**: Extending vector database integration with entity data
- 🔄 **Entity Filtering**: Implementing search and filtering by entity

### Feature Refinement
- 🔄 **Worker Role Switching**: Optimizing the dynamic allocation between decision and analysis tasks
- 🔄 **Quality Scoring Algorithm**: Refining source and argument quality assessment
- 🔄 **Article Categorization**: Enhancing topic matching precision for edge cases
- 🔄 **Geographical Impact Precision**: Improving accuracy of location-based relevance

### Performance Optimization
- 🔄 **Queue Processing Efficiency**: Reducing database contention in queue operations
- 🔄 **Content Extraction Speed**: Optimizing the article extraction process
- 🔄 **LLM Prompt Efficiency**: Refining prompts for faster and more consistent results
- 🔄 **Memory Optimization**: Reducing memory usage during content processing

### Infrastructure Improvements
- 🔄 **Error Recovery**: Enhancing system resilience to external service failures
- 🔄 **Metrics Collection**: Expanding monitoring capabilities
- 🔄 **Deployment Automation**: Streamlining production deployment

## Planned Features

### Content Enhancement
- 📋 **Vector Database Advanced Features**: Utilizing Qdrant filters and advanced query capabilities
- 📋 **Trending Detection**: Identification of emerging topics and trends
- 📋 **Multi-language Support**: Expansion to non-English content sources
- 📋 **Cross-cluster Analysis**: Identifying relationships between different clusters

### User Experience
- 📋 **Notification Customization**: More granular control over notification preferences
- 📋 **Interactive Feedback**: User feedback mechanisms for relevance improvement
- 📋 **Content Rating System**: User-driven quality assessment
- 📋 **Cluster-based Notifications**: Enabling notification preferences based on article clusters

### System Capabilities
- 📋 **Event Detection**: Identifying emerging events across multiple sources
- 📋 **Cross-reference Analysis**: Comparing information across multiple articles
- 📋 **Historical Context**: Providing historical background on current topics
- 📋 **Cluster Visualization**: Tools for visualizing article relationships and clusters

## Known Issues

### Content Processing
- 🐛 **JavaScript-Heavy Sites**: Difficulty extracting content from sites with complex JS rendering
- 🐛 **Paywalled Content**: Inconsistent handling of sites with paywalls or access restrictions
- 🐛 **Rate Limiting**: Occasional blocking by sites with aggressive anti-scraping measures

### LLM Interaction
- 🐛 **Prompt Consistency**: Occasional inconsistency in LLM outputs for similar inputs
- 🐛 **Context Length Limitations**: Handling of very long articles that exceed model context windows
- 🐛 **Specialized Content**: Reduced accuracy when analyzing highly technical or domain-specific content
- 🐛 **Entity Extraction Reliability**: Inconsistent NER quality depending on article complexity

### Performance
- 🐛 **Database Contention**: Occasional slowdowns due to SQLite's single-writer limitation
- 🐛 **Memory Usage Spikes**: High memory usage when processing multiple large articles concurrently
- 🐛 **LLM Latency**: Variable processing times dependent on LLM provider performance
- 🐛 **Vector Search Scaling**: Performance impacts as the vector database grows

### Mobile Integration
- 🐛 **Notification Delivery**: Occasional delays in push notification delivery
- 🐛 **Token Expiration Handling**: Edge cases in JWT token refresh flow
- 🐛 **Content Synchronization**: Issues with seen/unseen status across multiple devices

## Milestones

### Completed
- ✓ Core RSS processing infrastructure
- ✓ Initial LLM integration (Ollama and OpenAI)
- ✓ Basic topic matching and relevance determination
- ✓ Slack notification system
- ✓ iOS app API endpoints
- ✓ Critical and logical analysis features
- ✓ Geographic impact assessment
- ✓ Device subscription management
- ✓ Basic vector similarity search
- ✓ Entity database schema implementation
- ✓ Entity extraction and storage
- ✓ Enhanced LLM error logging with connection information
- ✓ Dual-query article similarity with entity and vector matching
- ✓ Fixed entity extraction failures in utility programs

### In Progress
- 🔄 Qdrant integration for entity-based vector search
- 🔄 Refined article relationship detection
- 🔄 Article clustering based on entity relationships
- 🔄 Domain-specific entity extraction refinement

### Next Targets
- 🎯 Complete integration of entity-based clustering
- 🎯 Optimize entity extraction quality and performance
- 🎯 Implement entity-based search functionality
- 🎯 Enhanced error handling and system resilience
- 🎯 Expanded testing infrastructure
- 🎯 Deployment automation improvements
