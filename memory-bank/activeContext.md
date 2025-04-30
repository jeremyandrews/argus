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

1. **Article Matching Threshold Adjustment**: Changed the article similarity threshold from 75% to 70%:
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

## Current Enhancement Project: Database-Driven Entity Alias System

We've implemented a comprehensive database-driven entity alias system to improve entity name matching and article clustering. This represents a significant architectural enhancement that moves from static, hard-coded aliases to a dynamic, database-backed approach that can learn and improve over time.

### Implementation Progress

1. ✅ **Database Schema Enhancement**:
   - Added dedicated tables for entity aliases (`entity_aliases`) and negative matches (`entity_negative_matches`)
   - Implemented comprehensive indexing for efficient alias lookups
   - Added tracking for alias performance and confidence levels via `alias_pattern_stats` table
   - Created schema for review batches with `alias_review_batches` and `alias_review_items` tables

2. ✅ **Pattern & LLM-Based Alias Discovery**:
   - Implemented regex-based pattern extraction for common alias formats with configurable patterns:
     ```rust
     r#"(?i)(?P<canonical>.+?),?\s+(?:also\s+)?(?:known|called)\s+as\s+["']?(?P<alias>.+?)["']?[,\.)]"#
     ```
   - Added pattern extraction function for automatic alias detection in articles
   - Built pattern statistics tracking to measure effectiveness
   - Implemented confidence scoring for discovered aliases

3. ✅ **Admin Tools & Management**:
   - Created comprehensive CLI tool `manage_aliases` with subcommands:
     - `migrate`: Move static aliases to database
     - `add`: Add new alias pairs
     - `test`: Test if two entity names match
     - `create-review-batch`: Create review batches
     - `review-batch`: Interactive review of suggested aliases
     - `stats`: View alias system statistics
   - Implemented approval and rejection workflows with reason tracking

4. ✅ **Static-to-Dynamic Migration**:
   - Implemented migration utility to move existing static aliases to database
   - Created backward compatibility layer to maintain existing functionality
   - Built fallback mechanism when database is unavailable
   - Preserved static aliases during transition period

5. ✅ **Negative Learning Mechanism**:
   - Added infrastructure to track rejected matches in `entity_negative_matches` table
   - Implemented persistence to prevent repeated false positives
   - Created automatic negative match creation from rejected aliases
   - Incorporated negative match checking in alias matching algorithm

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

### Recent Fixes

1. ✅ **Pattern Statistics Collection**:
   - Fixed unused `increment_pattern_stat` function in `db.rs` by integrating it into both approval and rejection workflows
   - Updated `approve_alias_suggestion` to retrieve the source pattern and track successful matches
   - Updated `reject_alias_suggestion` to also track pattern rejection rates
   - This enables performance tracking to identify which patterns generate the best alias suggestions

2. ✅ **CLI Tool Argument Conflict Resolution**:
   - Fixed command line argument conflict in the `Test` command where both `name1` and `name2` used the same `-n` flag
   - Changed to use `-1` for the first name and `-2` for the second name to avoid conflict
   - Improved usability of the command line interface

3. ✅ **Static Alias Migration Verification**:
   - Confirmed that the low migration count (2 static aliases) is expected behavior
   - Many entries in the static alias maps are canonical self-references (e.g., "jeff bezos" → "jeff bezos")
   - The migration correctly skips pairs where normalized forms are identical
   - Only two entries had distinct normalized forms that needed migration to the database

### Benefits Delivered

- **Enhanced Matching Accuracy**: Better identification of related entities through centralized alias management
- **Learning Capability**: System improves over time as new aliases are discovered and validated
- **Centralized Management**: Single source of truth for all entity aliases with comprehensive review tools
- **Pattern Tracking**: Pattern effectiveness statistics for optimizing future alias discovery
- **Backward Compatibility**: Ensures consistent behavior while transitioning to the new system

## Previous Enhancement Project: Entity-Based Article Clustering

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

### Core Components
- **Entity Module**: New component for entity extraction, storage, and matching
- **Relational Schema**: Properly normalized database design for entity relationships
- **Enhanced Similarity Algorithm**: Combined vector and entity-based matching
- **Cluster Management**: Tools for creating and maintaining article clusters

### Implementation Progress
1. ✅ Database schema extensions for entity storage - Added all required tables and indexes in db.rs
2. ✅ Entity extraction using LLM with structured prompts - Implemented entity_extraction_prompt in prompts.rs
3. ✅ Entity normalization and relationship mapping - Created entity management functions in db.rs
4. ✅ Integration with analysis pipeline - Added entity extraction to the analysis workflow in analysis_worker.rs
5. ✅ Support for event dates - Added event_date field to articles table for temporal matching
6. ✅ Entity importance classification - Implemented PRIMARY/SECONDARY/MENTIONED ranking
7. ✅ Entity module organization - Created specialized module with types, extraction, matching, and repository components
8. ✅ Vector database integration - Added entity IDs and event dates to vector embeddings
9. ✅ Multi-dimensional similarity - Implemented algorithms combining vector similarity, entity overlap, and temporal proximity

### Most Recent Fix: Vector Similarity Calculation Fix

We've identified and fixed a critical issue in the vector similarity calculation that was causing articles to incorrectly fail to match:

**Root Cause Analysis**
- Vector similarity was being calculated incorrectly in three different parts of the codebase:
  - In diagnostic tools (`analyze_matches.rs` and `batch_analyze.rs`): Using a dummy zero vector instead of retrieving the source article's actual vector
  - In API code (`app/api.rs`): Same issue - using a dummy vector `&vec![0.0; 1024]` for calculations
  - In the entity-based search code path in `vector.rs`: Not properly handling self-comparisons

- The impact of these issues:
  - Vector similarity was coming back as 0.0 rather than the actual value (which was often 0.9+ for related articles)
  - Since vector similarity is weighted at 60% in the final score, this prevented matches even when entity similarity was high
  - The error was consistent across both diagnostic and production code
  - Error logs showed: `ERROR vector: Failed to calculate vector similarity for article 21235787`

**Fix Implementation**
1. ✅ **Fixed src/vector.rs**:
   - Updated the `get_similar_articles_with_entities` function to properly handle self-comparisons and explicitly set vector_score and score for self-comparisons
   - Modified calculation of vector similarity for entity-matched articles to use proper vector retrieval and direct comparison
   - Enhanced error handling to provide better diagnostics when vector calculations fail

2. ✅ **Fixed src/app/api.rs**:
   - Replaced the dummy vector approach with proper retrieval of both article vectors
   - Implemented direct vector comparison using the common `calculate_direct_similarity` function
   - Added proper error handling with specific error messages for each failure mode

3. ✅ **Leveraged Common Code Path**:
   - Utilized re-exported functions in lib.rs to ensure consistent vector handling across the codebase:
   ```rust
   pub use vector::{calculate_direct_similarity, get_article_vector_from_qdrant};
   ```
   - This ensures all code paths use the same vector retrieval and comparison logic

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

### Entity Alias System Enhancement
- **Database Schema Design**: Creating comprehensive schema for alias management
- **Pattern Extraction**: Building pattern-based alias discovery systems
- **LLM Integration**: Leveraging language models for alias detection
- **Admin Tooling**: Developing CLI utilities for alias management
- **Static-to-Dynamic Migration**: Moving from hardcoded to database-driven approach

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

1. **Complete Entity Alias System**: Finish implementing the database-driven alias system
2. **Vector Database Integration**: Exploring deeper integration with Qdrant for semantic search
3. **Enhanced Metrics**: Improving system health and performance monitoring
4. **Testing Infrastructure**: Expanding automated testing for reliability
5. **Topic Management**: Building tools for better topic configuration and management
6. **User Preference Refinement**: Further customization of notification preferences
7. **Content Filtering Improvements**: Refining relevance detection and quality assessment
8. **Entity Matching Feedback**: Analyzing user feedback on missed matches to improve the algorithm

## Active Decisions

### Architecture Decisions
- **Database Scaling**: Evaluating whether SQLite will remain sufficient or if migration to PostgreSQL will be needed
- **Deployment Model**: Determining the optimal deployment strategy for production environments
- **Worker Distribution**: Finalizing approach to worker allocation and load balancing
- **Entity Alias Architecture**: Deciding on the best approach for transitioning to database-driven aliases

### Feature Decisions
- **Topic Configuration**: Determining the best approach for managing and updating topic definitions
- **Quality Thresholds**: Setting appropriate thresholds for content quality assessment
- **Notification Frequency**: Balancing notification volume to avoid overwhelming users
- **Entity Matching Feedback**: Establishing a process for incorporating user feedback on missed matches
- **Alias Pattern Selection**: Determining which patterns are most effective for alias discovery

### Technical Decisions
- **LLM Provider Strategy**: Evaluating cost/performance tradeoffs between different LLM providers
- **Caching Strategy**: Determining what and how to cache for performance optimization
- **Error Handling**: Standardizing approach to error recovery and system resilience
- **Vector Similarity Calculation**: Refining methods for calculating and comparing vector embeddings
- **Alias Cache Design**: Implementing effective caching for database aliases to maintain performance

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
