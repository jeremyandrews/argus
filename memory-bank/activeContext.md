# Active Context: Argus

## Current Status

Argus is a fully operational AI agent system for information monitoring and analysis with dual notification paths (Slack and iOS app). The system has evolved into a mature architecture with well-defined components and enhanced entity-based article matching capabilities:

- RSS feed parsing and queue management
- Decision workers for content relevance determination
- Analysis workers for in-depth content processing
- API endpoints for iOS app integration
- Multiple notification channels (Slack and iOS push notifications)

The codebase demonstrates a sophisticated use of Rust with asynchronous processing, database integration, and multiple language model interfaces, suggesting active ongoing development.

## Recent Development Focus

Based on code review, recent development appears focused on:

1. **RSS Feed Diagnostic Tool and Brotli Compression Support**:
   - **Created RSS Diagnostic Tool**: Built `test_rss_feed` binary for troubleshooting feed loading issues
   - **Enhanced Compression Support**: Added Brotli decompression capability to RSS module
   - **Fixed Content Loading**: Resolved issues with feeds using `content-encoding: br` 
   - **Detailed Diagnostics**: Tool displays HTTP headers, content previews, and hex dumps for debugging
   - **Header Analysis**: Improved header inspection to identify compression methods in use
   - **Unified Error Reporting**: Standardized feed error reporting and diagnostics

2. **Article Matching Threshold Adjustment**: Changed the article similarity threshold from 75% to 70%:
   - **Increased Match Recall**: Lowered the minimum combined similarity score for article matches
   - **More Lenient Matching**: Now requires less strict vector and entity similarity
   - **Broader Clustering**: Will create more inclusive article clusters with related content
   - **Balance Adjustment**: Maintains the 60% vector similarity / 40% entity similarity weighting

Based on code review, other recent development appears focused on:

1. **Improved Summary Generation**: Enhanced tiny summaries with two key improvements:
   - **Lead-in Variety**: Modified prompts to avoid always starting with "In April 2025..." unless dates are critical to understanding
   - **Temporal Accuracy**: Added explicit instructions to maintain proper tense for events based on when they occur relative to the current date
   - **Diverse Opening Styles**: Added examples showing different approaches to starting summaries (action-focused, discovery-focused, announcement-focused)
   - **Date Definition**: Added clear definition of "TODAY" to avoid treating future events as if they've already happened
   - **Examples Update**: Replaced date-centric examples with more varied alternatives that demonstrate effective non-date openings

2. **Flexible Worker Configuration**: Supporting multiple LLM backends (Ollama, OpenAI) with detailed configuration options
3. **Dynamic Worker Role Switching**: Enabling Analysis Workers to switch to Decision Worker roles when needed
3. **Enhanced Content Analysis**: Implementing critical analysis, logical fallacy detection, and additional insights
4. **iOS App Integration**: Building out API endpoints and push notification capabilities
5. **Quality Metrics**: Generating source quality and argument quality scores
6. **Life Safety Alerts**: Implementing geographical impact detection for safety-critical information
7. **Enhanced LLM Error Logging**: Improved connection identification in error logs to pinpoint failing LLM instances
8. **Article ID Exposure**: Added article IDs to R2 JSON files for enabling matching feedback from iOS app

## Current Enhancement Project: Parameter Optimization for Entity Matching

After successfully implementing the database-driven entity alias system, we're now focusing on parameter optimization to improve article clustering accuracy. This represents the next phase in our entity matching improvement plan, which aims to fine-tune the matching algorithm for better recall while maintaining precision.

### Implementation Status

1. ✅ **Database-Driven Entity Alias System** (Completed):
   - Added dedicated tables for entity aliases (`entity_aliases`) and negative matches (`entity_negative_matches`)
   - Implemented comprehensive indexing for efficient alias lookups
   - Added tracking for alias performance and confidence levels via `alias_pattern_stats` table
   - Created batch review infrastructure with `alias_review_batches` and `alias_review_items` tables
   - Implemented pattern-based alias discovery with configurable regex patterns
   - Created comprehensive CLI tools for alias management
   - Built negative learning system to prevent repeated false positives

2. ✅ **Initial Parameter Tuning** (Completed): 
   - Lowered the article similarity threshold from 75% to 70% to increase match recall
   - Maintained the weighting ratio of 60% vector similarity / 40% entity-based similarity
   - Verified improved matching with test datasets while monitoring false positive rates

3. 🔄 **Advanced Parameter Optimization** (In Progress):
   - **Systematic Threshold Testing**: Evaluating different threshold combinations against test datasets
   - **Entity Type Weighting**: Developing entity type-specific parameters (person, organization, location, event)
   - **Adaptive Thresholds**: Creating algorithms that adjust thresholds based on article characteristics
   - **Confidence-Based Scoring**: Implementing weighted matches based on confidence levels

### Core Components

- **Diagnostic Tooling**: Using our comprehensive diagnostic tools to measure effectiveness:
  ```rust
  // analyze_matches: Detailed single pair analyzer
  // batch_analyze: Statistical analysis of multiple article pairs
  // create_match_pairs: Test dataset generator
  ```

- **Similarity Calculation**: Enhancement of the multi-factor similarity calculation:
  ```rust
  pub async fn calculate_entity_similarity_async(
      db: &Database,
      source_entities: &ExtractedEntities,
      target_entities: &ExtractedEntities,
      source_date: Option<&str>,
      target_date: Option<&str>,
  ) -> anyhow::Result<EntitySimilarityMetrics>
  ```

- **Matching Algorithm**: Refined threshold and weighting system:
  ```rust
  // Entity-specific weighting
  metrics.person_overlap = calculate_type_similarity_async(
      db,
      source_entities,
      target_entities,
      EntityType::Person,
      &normalizer,
  ).await?;

  // Combined score calculation with weighted components
  metrics.calculate_combined_score();
  ```

### Implementation Plan

1. **Analysis of Current Performance**:
   - Using diagnostic tools to generate baseline statistics on article matching
   - Identifying patterns in false negatives (missed matches) and false positives
   - Creating targeted test datasets for systematic evaluation

2. **Entity Type-Specific Optimization**:
   - Evaluating different weights for each entity type (person, organization, location, event)
   - Determining optimal similarity thresholds for each entity type
   - Creating separate parameters for primary versus secondary entity importance

3. **Adaptive Threshold Implementation**:
   - Developing algorithms that adjust similarity thresholds based on:
     * Total number of entities in source and target articles
     * Distribution of entity types
     * Presence of high-confidence entities
     * Temporal proximity between articles

4. **Parameter Validation and Tuning**:
   - Systematic testing with large datasets (1000+ article pairs)
   - Measuring precision and recall across different parameter configurations
   - Optimizing for maximum F1 score (balanced precision and recall)
   - Implementing the optimal parameter set based on validation results

### Expected Benefits

- **Improved Match Recall**: Better identification of related articles through optimized thresholds
- **Reduced False Positives**: More precise matching to avoid incorrect connections
- **Content-Adaptive Matching**: Tailored parameters based on article and entity characteristics
- **Measurable Performance**: Clear metrics for matching accuracy and tuning effectiveness
- **Transparent Decision-Making**: Detailed explanations for why articles do or don't match

## Previous Enhancement Project: Database-Driven Entity Alias System

We've implemented a comprehensive database-driven entity alias system to improve entity name matching and article clustering. This represents a significant architectural enhancement that moves from static, hard-coded aliases to a dynamic, database-backed approach that can learn and improve over time.

### Core Components

- **Database Schema**: Added tables and functions in `db.rs` for alias management:
  ```rust
  pub async fn add_entity_alias(...) -> Result<i64, sqlx::Error>
  pub async fn are_names_equivalent(...) -> Result<bool, sqlx::Error>
  pub async fn get_canonical_name(...) -> Result<Option<String>, sqlx::Error>
  pub async fn add_negative_match(...) -> Result<i64, sqlx::Error>
  pub async fn is_negative_match(...) -> Result<bool, sqlx::Error>
  pub async fn migrate_static_aliases(...) -> Result<usize, sqlx::Error>
  ```

- **Alias Discovery**: Added pattern-based detection in `aliases.rs`:
  ```rust
  pub fn extract_potential_aliases(...) -> Vec<(String, String, EntityType, f64)>
  ```

- **EntityNormalizer Enhancement**: Updated to use database aliases in `normalizer.rs`:
  ```rust
  pub async fn async_names_match(...) -> anyhow::Result<bool>
  ```

- **Entity Matching**: Enhanced with database-backed similarity calculation in `matching.rs`:
  ```rust
  pub async fn calculate_entity_similarity_async(...) -> anyhow::Result<EntitySimilarityMetrics>
  ```

- **Admin CLI**: Created comprehensive command-line interface in `manage_aliases.rs`

### Benefits Delivered

- **Enhanced Matching Accuracy**: Better identification of related entities through centralized alias management
- **Learning Capability**: System improves over time as new aliases are discovered and validated
- **Centralized Management**: Single source of truth for all entity aliases with comprehensive review tools
- **Pattern Tracking**: Pattern effectiveness statistics for optimizing future alias discovery
- **Backward Compatibility**: Ensures consistent behavior while transitioning to the new system

## Entity-Based Article Clustering

We've implemented a comprehensive entity-based system to improve article clustering and similarity matching with the following approach:

### Implementation Strategy
1. **Relational Entity Model**: Creating a complete relational database schema for entity management:
   - Dedicated tables for entities, entity-article relationships, and entity details
   - Proper indexing for efficient query patterns
   - Support for entity hierarchies, especially for locations
   - Cluster tracking and management

2. **Entity Extraction**: Adding Named Entity Recognition (NER) capability using structured LLM prompts to extract:
   - People with roles and affiliations
   - Organizations with types and industries
   - Locations with hierarchical relationships
   - Temporal information and events
   - Primary vs. secondary entity importance

3. **Multi-Factor Similarity**: Enhancing the existing vector-based matching with:
   - Entity overlap detection (shared people, organizations, locations)
   - Temporal proximity scoring (using publication and event dates)
   - Hierarchical location matching (handling geographic containment)
   - Refined similarity calculation that combines multiple factors

4. **Database Enhancements**:
   - New entity tables with proper relationships
   - Extended article schema with event_date and cluster_id
   - Comprehensive indexing strategy for performance
   - Cluster management tables

5. **Qdrant Integration**:
   - Extending payload schema to include entity data
   - Enhancing vector search with entity-aware filtering
   - Improving result ranking with multi-factor scoring

### Most Recent Fix: Vector Similarity Calculation Fix

We've identified and fixed a critical issue in the vector similarity calculation that was causing articles to incorrectly fail to match:

**Results**
- Articles now match correctly when they have both high vector similarity and sufficient entity overlap
- Testing with sample articles shows the issue is resolved:
  - Vector similarity between sample articles is now correctly calculated as 0.92 (92%) instead of 0.0
  - Combined with entity similarity of 0.515, the final score is 0.759 (above the 0.75 threshold)
  - Articles are now correctly identified as matches

- This fix significantly improves the accuracy of the article matching system by ensuring:
  1. Proper vector similarity calculation in all code paths
  2. Consistent handling of both vector and entity components of the similarity score
  3. Better error reporting when vector calculations fail

## Active Work Areas

### Parameter Optimization
- **Systematic Testing**: Evaluating different threshold combinations against test datasets
- **Adaptive Thresholds**: Creating algorithms that adjust thresholds based on article characteristics
- **Entity Type Weighting**: Fine-tuning the importance of different entity types in matching
- **Confidence-Based Scoring**: Weighting matches by confidence level
- **Performance Metrics**: Tracking precision and recall statistics for different parameter sets

### LLM Integration Enhancements
- **Fallback Handling**: Implementing graceful fallback between different LLM providers
- **Model Configuration**: Fine-tuning prompt templates for different analysis types
- **Temperature Settings**: Optimizing temperature settings for different tasks

### iOS App Integration
- **Push Notification System**: Refining the notification delivery system
- **R2 Content Storage**: Storing and serving content through Cloudflare R2
- **Subscription Management**: Managing user topic subscriptions and preferences
- **Article ID Exposure**: Added article IDs to the JSON data stored in R2 for enabling match feedback from users

### Performance Optimization
- **Resource Allocation**: Balancing workloads between decision and analysis tasks
- **Queue Management**: Optimizing database queue processing
- **Parallel Processing**: Ensuring efficient utilization of multiple workers

### Critical Analysis Features
- **Source Quality Assessment**: Evaluating source credibility and reliability
- **Argument Quality Evaluation**: Detecting logical fallacies and reasoning strength
- **Additional Insights**: Generating deeper context and connections
- **Action Recommendations**: Providing practical, actionable steps based on article content
- **Talking Points**: Generating discussion-worthy topics related to article content

## Next Development Steps

Based on codebase analysis, likely next steps include:

1. **Complete Parameter Optimization**: Finish systematic testing and implement optimal parameter configuration for entity matching.
   - Implement adaptive thresholds based on article characteristics
   - Develop entity type-specific weights for enhanced matching
   - Create and validate test datasets for comprehensive evaluation

2. **Entity-Aware Clustering**: Move beyond pairwise matching to true clustering.
   - Implement cluster tracking based on shared entities
   - Develop hierarchical relationships between connected entities
   - Create algorithms for cluster management and maintenance

3. **Vector Database Integration**: Exploring deeper integration with Qdrant for semantic search
4. **Enhanced Metrics**: Improving system health and performance monitoring
5. **Testing Infrastructure**: Expanding automated testing for reliability
6. **Topic Management**: Building tools for better topic configuration and management
7. **User Preference Refinement**: Further customization of notification preferences

## Active Decisions

### Architecture Decisions
- **Database Scaling**: Evaluating whether SQLite will remain sufficient or if migration to PostgreSQL will be needed
- **Deployment Model**: Determining the optimal deployment strategy for production environments
- **Worker Distribution**: Finalizing approach to worker allocation and load balancing
- **Parameter Optimization Strategy**: Deciding between manual tuning and algorithmic optimization approaches
- **Adaptive Threshold Implementation**: Determining the factors that should influence dynamic thresholds

### Feature Decisions
- **Entity Type Importance**: Determining optimal weights for different entity types in matching
- **Quality Thresholds**: Setting appropriate thresholds for content quality assessment
- **Notification Frequency**: Balancing notification volume to avoid overwhelming users
- **Entity Matching Feedback**: Establishing a process for incorporating user feedback on missed matches
- **Alias Pattern Selection**: Determining which patterns are most effective for alias discovery

### Technical Decisions
- **LLM Provider Strategy**: Evaluating cost/performance tradeoffs between different LLM providers
- **Caching Strategy**: Determining what and how to cache for performance optimization
- **Error Handling**: Standardizing approach to error recovery and system resilience
- **Vector Similarity Calculation**: Refining methods for calculating and comparing vector embeddings
- **Parameter Storage**: Deciding how to store and manage matching parameters (config files vs. database)

## Integration Requirements

### iOS Application
- **Authentication Flow**: JWT-based authentication system for secure access
- **Push Notification Handling**: Proper setup of APNs certificates and device token management
- **Content Synchronization**: Tracking read/unread status across devices
- **Entity Match Feedback**: Using article IDs to report missed matches for algorithm improvement

### Slack Integration
- **Message Formatting**: Standardized formatting for different content types
- **Channel Management**: Appropriate routing of different content categories
- **Interaction Handling**: Potential for interactive elements in Slack messages

### LLM Services
- **API Access**: Proper API key management for OpenAI and other providers
- **Ollama Configuration**: Setup and maintenance of local Ollama instances
- **Provider Failover**: Seamless switching between providers when needed
