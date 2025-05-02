# Progress: Argus

## Current Status

Argus is currently in active development with all major components implemented and functioning. The system has matured with sophisticated entity-based article matching capabilities and enhanced diagnostic tools. The project follows an iterative development approach with continuous refinement of existing features alongside the addition of new capabilities.

## What Works

### Core Infrastructure
- ✅ **Async Processing Framework**: Tokio-based concurrency model for efficient execution
- ✅ **Database Integration**: SQLite with SQLx for persistent storage and queues
- ✅ **Logging System**: Comprehensive tracing setup with file and console outputs
- ✅ **Configuration System**: Environment variable-based configuration

### Content Processing
- ✅ **RSS Feed Parsing**: Reliable fetching and parsing of feeds in multiple formats
- ✅ **RSS Diagnostic Tool**: Specialized `test_rss_feed` binary for troubleshooting feed loading issues
- ✅ **Multi-Format Compression Support**: Support for Brotli, gzip, zlib, and deflate compressed content
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

### Database-Driven Entity Alias System
- ✅ **Database Schema Enhancement**: Implemented comprehensive database schema for entity aliases
  - Added dedicated tables for entity aliases (`entity_aliases`) and negative matches (`entity_negative_matches`)
  - Created indexing for efficient alias lookups with multiple access patterns
  - Implemented statistics tracking via `alias_pattern_stats` table for pattern effectiveness
  - Added review batch infrastructure with `alias_review_batches` and `alias_review_items` tables

- ✅ **Multi-Tier Matching Strategy**:
  - Implemented direct matching after basic normalization (fastest)
  - Created database-driven alias lookup for known entity variations (reliable)
  - Enhanced fuzzy matching with configurable thresholds (flexible)
  - Added pattern-based matching for specific entity types (specialized)

- ✅ **Pattern & LLM-Based Alias Discovery**: Built intelligent alias detection systems
  - Implemented regex-based pattern extraction for common alias formats (e.g., "also known as", "formerly")
  - Added validation and confidence scoring system for potential aliases
  - Created extraction functions to identify potential aliases from article text
  - Built configurable pattern system for easy extension

- ✅ **Performance Optimization**: Enhanced alias system performance
  - Implemented thread-safe caching layer with time-based expiration for frequently accessed aliases
  - Optimized alias lookups with three-tier approach: exact match → cache → database → fuzzy matching
  - Added eviction policies to maintain cache size and performance over time

- ✅ **Admin Tools & Management**: Developed utilities for alias management
  - Created comprehensive CLI tool `manage_aliases` with commands for:
    * Migrating static aliases to database
    * Adding new alias pairs
    * Testing entity name matching
    * Creating and reviewing batches of alias suggestions
    * Viewing system statistics
  - Implemented approval and rejection workflows with reason tracking
  - Added pattern performance analysis with statistics reporting
  
- ✅ **Negative Learning Mechanism**: Implemented system to learn from mistakes
  - Created infrastructure to track rejected matches in `entity_negative_matches` table
  - Built tools for managing negative matches
  - Implemented persistence to prevent repeated false positives
  - Added automatic negative match creation from rejected aliases

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

### Parameter Optimization for Entity Matching
- ✅ **Initial Parameter Tuning**: Changed similarity threshold from 75% to 70% to increase match recall
- 🔄 **Systematic Threshold Testing**: Testing different similarity thresholds and weights
  - Creating comprehensive test datasets with known matches/non-matches
  - Developing statistical analysis for different parameter configurations
  - Measuring precision, recall, and F1 scores for various threshold combinations
  - Building visualization tools for parameter performance analysis
- 🔄 **Entity Type-Specific Parameters**: Developing specialized parameters by entity type
  - Evaluating optimal weights for person entities vs. organizations vs. locations
  - Testing different importance levels for PRIMARY vs. SECONDARY entities
  - Analyzing entity count impact on appropriate threshold values
  - Developing entity type correlation metrics to understand matching patterns
- 🔄 **Adaptive Thresholds**: Implementing dynamic thresholds based on article characteristics
  - Creating algorithms that adjust thresholds based on entity count and distribution
  - Developing intelligence for temporal-aware threshold adjustments
  - Implementing confidence-weighted scoring for more reliable matches
  - Building feedback mechanisms to refine thresholds over time
- 🔄 **Parameter Storage & Configuration**: Creating system for parameter management
  - Designing flexible configuration system for threshold parameters
  - Implementing parameter versioning for testing and rollback
  - Creating diagnostic tools to evaluate parameter effectiveness
  - Building documentation system to track parameter changes and impacts

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
  - Ensured articles need to share at least some entity overlap to reach the minimum threshold
  - Verified the fix ensures more semantically relevant "similar articles" results
- ✅ **Enhanced Diagnostic Logging**: Added comprehensive logging throughout the entity matching process
  - Added detailed logging of entity extraction, entity retrieval, and entity matching steps
  - Added pre-filter and post-filter logging to show exactly which articles are being filtered out and why
  - Added verification of entity overlap in final results with error reporting if an article without entity overlap passes filtering
  - Improved transparency for debugging the entity-based article matching system
- ✅ **Date Window Approach for Related Articles**: Implemented a more robust date filtering method
  - Changed from fixed threshold date to a dynamic date window around each article's own publication date
  - Window spans from 14 days before to 1 day after the article's publication date
  - Updated `get_articles_by_entities_with_date` in db.rs to use this window approach
  - Modified `get_similar_articles_with_entities` and `get_articles_by_entities` in vector.rs to pass source article ID
  - Added code to retrieve the source article's publication date for date window calculation
  - Enhanced logging to monitor effectiveness of the date window approach
  - Identified future cleanup needs for articles with unrealistic dates
- ✅ **Vector Similarity Calculation Fix**: Fixed critical issue with vector similarity calculation
  - Fixed vector retrieval in multiple code paths to ensure consistent vector similarity results
  - Implemented common code paths for vector operations to maintain consistency
  - Enhanced error reporting for vector calculation failures
  - Verified fix with test cases showing proper matching based on both vector and entity similarity
- 🔄 **Entity-Aware Clustering**: Implementing cluster tracking based on shared entities
- 🔄 **Advanced Relationship Modeling**:
  - Building hierarchical relationships between connected entities
  - Implementing relationship-aware matching in matching algorithm
  - Creating contextual importance understanding for entities

### Feature Refinement
- ✅ **Summary Generation Improvements**: Enhanced tiny summary prompts with two key improvements:
  - Modified prompts to avoid date-centric lead-ins unless dates are critical
  - Added explicit instructions for temporal accuracy to ensure proper tense for events
  - Provided clear definition of "TODAY" to prevent reporting future events as if already happened
  - Added diverse example summaries with varied opening styles
  - Verified changes via successful compilation and deployment
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

### Entity Matching Enhancement (Next Phase)
- 📋 **Entity-Aware Clustering**: Move beyond pairwise matching to true clustering
  - Implementation of cluster tracking based on shared entities
  - Development of hierarchical relationships between connected entities
  - Creation of algorithms for cluster management and maintenance
  - Support for multi-hop relationships between articles

- 📋 **Advanced Relationship Modeling**: Create deeper entity relationship models
  - Build hierarchical relationships between entities (company subsidiaries, geographic containment)
  - Develop temporal awareness for entity relationships (acquisitions, name changes)
  - Implement transitive matching (if A matches B and B matches C, consider A and C related)
  - Create entity relationship graphs for visualization and analysis

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
- ✓ Fixed vector similarity calculation for proper article matching
- ✓ Implemented comprehensive database-driven entity alias system
- ✓ Fixed pattern statistics collection to track pattern effectiveness
- ✓ Enhanced CLI tool with improved interface and argument handling
- ✓ Initial parameter tuning (lowered threshold from 75% to 70%)

### In Progress
- 🔄 Parameter optimization for entity matching
- 🔄 Entity type-specific weighting and threshold development
- 🔄 Adaptive threshold algorithm implementation
- 🔄 Qdrant integration for entity-based vector search
- 🔄 Refined article relationship detection
- 🔄 Parameter performance measurement and optimization

### Next Targets
- 🎯 Complete parameter optimization for entity matching
- 🎯 Implement entity-aware clustering with shared entity tracking
- 🎯 Optimize entity extraction quality and performance
- 🎯 Implement entity-based search functionality
- 🎯 Enhance error handling and system resilience
- 🎯 Expand testing infrastructure
- 🎯 Deployment automation improvements
