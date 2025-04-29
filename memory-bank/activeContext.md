# Active Context: Argus

## Current Status

Argus is a functioning AI agent system for information monitoring and analysis with dual notification paths (Slack and iOS app). The system has a mature architecture with well-defined components:

- RSS feed parsing and queue management
- Decision workers for content relevance determination
- Analysis workers for in-depth content processing
- API endpoints for iOS app integration
- Multiple notification channels (Slack and iOS push notifications)

The codebase demonstrates a sophisticated use of Rust with asynchronous processing, database integration, and multiple language model interfaces, suggesting active ongoing development.

## Recent Development Focus

Based on code review, recent development appears focused on:

1. **Flexible Worker Configuration**: Supporting multiple LLM backends (Ollama, OpenAI) with detailed configuration options
2. **Dynamic Worker Role Switching**: Enabling Analysis Workers to switch to Decision Worker roles when needed
3. **Enhanced Content Analysis**: Implementing critical analysis, logical fallacy detection, and additional insights
4. **iOS App Integration**: Building out API endpoints and push notification capabilities
5. **Quality Metrics**: Generating source quality and argument quality scores
6. **Life Safety Alerts**: Implementing geographical impact detection for safety-critical information
7. **Enhanced LLM Error Logging**: Improved connection identification in error logs to pinpoint failing LLM instances
8. **Article ID Exposure**: Added article IDs to R2 JSON files for enabling matching feedback from iOS app

## Current Enhancement Project: Entity-Based Article Clustering

We're implementing a comprehensive entity-based system to improve article clustering and similarity matching with the following approach:

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

### Current Focus: Vector Similarity Calculation Fix

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

### Previous Focus: Entity Matching Diagnostic & Improvement Tools

While our entity-based article matching system works correctly for the matches it does find (high precision), we're addressing the issue of missed matches (low recall). Our analysis indicates we're missing approximately 80% of valid matches. To systematically improve this, we've implemented comprehensive diagnostic tools as part of our multi-phase improvement plan.

#### Implemented Diagnostic Tools

We've created a set of tools to help systematically identify why matches are being missed:

1. **API Diagnostic Endpoint**
   - ✅ Added `/articles/analyze-match` endpoint for analyzing specific article pairs
   - ✅ Provides detailed vector similarity, entity similarity, and combined score metrics
   - ✅ Shows entity-by-entity comparison with importance levels
   - ✅ Explains reasons why matches fail with specific thresholds and required scores
   - ✅ Includes NearMissMatch detection for articles that almost matched

2. **Command-line Analysis Tools**
   - ✅ Created `analyze_matches` utility for detailed analysis of a single article pair
   - ✅ Implemented `batch_analyze` tool to process multiple article pairs and generate statistics
   - ✅ Built `create_match_pairs` utility to generate test datasets from recent articles
   - ✅ Added detailed reporting on common pattern failures and match statistics

3. **Enhanced Database Support**
   - ✅ Added `find_articles_with_entities` method to retrieve articles with entity data
   - ✅ Improved entity-based matching to work with IDs or article objects
   - ✅ Added helper methods to support diagnostic tools

4. **iOS Feedback Enablement**
   - ✅ Added article IDs to the R2 JSON files to enable matching feedback from the iOS app
   - ✅ Modified the analysis_worker.rs to include article IDs in both life safety and matched topics JSON
   - ✅ Ensured article IDs are available in all interfaces where feedback might be needed
   - ✅ Created comprehensive documentation for using the diagnostic tools in memory-bank/entity_matching_tools.md

#### Using the Entity Matching Diagnostic Tools

**API Endpoint Usage:**
```bash
# Test a specific article pair with curl
curl -X POST https://api.argus.com/articles/analyze-match \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"source_article_id": 12345, "target_article_id": 67890}'
```

**Single Article Pair Analysis:**
```bash
# Run detailed analysis of why two articles do or don't match
cargo run --bin analyze_matches 12345 67890
```
This will output detailed metrics about vector similarity, entity overlap, scores for each entity type (person, organization, location, event), temporal proximity, and a clear explanation of why the articles do or don't match.

**Batch Analysis Tool:**
```bash
# Process a CSV file of article pairs and generate statistical report
cargo run --bin batch_analyze article_pairs.csv results.csv
```
This analyzes multiple article pairs and produces:
- Detailed CSV with scoring information for each pair
- Summary statistics about match success rates
- Most common reasons for match failures
- Average scores for matches vs. non-matches
- Entity overlap analysis

**Test Data Generation:**
```bash
# Create a CSV file with article pairs for testing
cargo run --bin create_match_pairs test_pairs.csv 100 7
```
This generates a test dataset by:
- Finding articles with entity data from the last N days (7 in this example)
- Creating pairs of articles likely to be related (same-day publication)
- Adding random pairs to reach the target count (100 in this example)
- Outputting a CSV file ready for use with the batch analysis tool

**Quick Vector Analysis:**
```bash
# Quickly check if article vectors are valid
./no_match.sh 12345 67890
```
This shell script provides a simple way to check if two articles should match and why they might not be matching, with a focus on vector similarity issues.

#### Ongoing Multi-Phase Approach

We're implementing a four-phase plan to improve entity matching while maintaining precision:

1. **Measurement & Feedback System** (Current Phase - Tools Implemented)
   - ✅ Exposed article IDs in all interfaces for feedback collection
   - ✅ Created diagnostic endpoints to explain matching decisions
   - ✅ Implemented logging of potential matches that fall below threshold
   - ✅ Built analysis tools to identify patterns in missed matches

2. **Enhanced Normalization & Fuzzy Matching** (Planned)
   - Implement fuzzy matching for entity names using Levenshtein distance and phonetic algorithms
   - Add acronym detection and expansion (e.g., "FBI" ↔ "Federal Bureau of Investigation")
   - Implement word stemming for variations (e.g., "President Biden" ↔ "Biden's presidency")
   - Create an entity alias system to track known variations

3. **Parameter Optimization** (Planned)
   - Experiment with different similarity thresholds and weight distributions
   - Implement adaptive thresholds based on article characteristics
   - Develop entity type-specific matching parameters

4. **Advanced Relationship Modeling** (Planned)
   - Build hierarchical entity relationships
   - Implement relationship-aware matching
   - Add contextual understanding of entity importance

### Previous Focus: Enhanced Diagnostics for Entity Matching Issues

We've implemented comprehensive diagnostics to identify why entity-based related articles weren't appearing properly in the iOS app:

**Root Cause Analysis**
- Entity extraction and storage are working correctly:
  - Logs show successful extraction: `Successfully extracted 12 entities from article text`
  - Entities are properly stored in the database: `Successfully processed entity extraction for article 19134669 with 12 entities`
- However, when retrieving similar articles, the entity relationship information isn't being correctly used:
  - Logs show: `Building source entities from 0 entity IDs: []`
  - No logs found for "entities for article", indicating `get_article_entities` isn't being called

**Previous Fix Attempts**
- Enhanced `build_entities_from_ids` function to:
  - Try to determine source article ID from entity IDs via database lookup
  - Use `db.get_article_entities()` to get complete entity-article relationship data
  - Preserve proper importance levels from the database instead of assuming all are PRIMARY
  - Add better fallback mechanisms when direct lookup fails
- Improved `get_similar_articles_with_entities` function to:
  - Add direct entity retrieval from source article as a fallback
  - Include multiple recovery paths for entity data retrieval
  - Provide better logging for diagnostic purposes

**Enhanced Diagnostic Implementation**
- ✅ **Entity Matching Process** (in `matching.rs`):
  - Added detailed logging for entity similarity calculations with entity-by-entity comparison tracking
  - Added type-by-type breakdowns of entity comparisons (Person, Organization, Location, Event) 
  - Added critical error detection for when overlapping entities produce zero scores
  
- ✅ **Entity Retrieval Process** (in `vector.rs`):
  - Enhanced the `build_entities_from_ids` function with more detailed entity tracing
  - Added importance level and entity type breakdowns for easier troubleshooting
  - Added entity-by-entity logging with type and importance data
  - Added critical error detection when we fail to retrieve entities despite having valid entity IDs
  - Added tracking of entity-based versus vector-based matches
  
- ✅ **Database Entity Search** (in `db.rs`):
  - Added a preliminary query to count matching articles without date filtering
  - Added critical error reporting when date filtering eliminates all potential matches
  - Added sample output of articles being filtered by date to identify format issues
  - Enhanced logging of SQL execution with parameter values

**Root Cause Found and Fixed**
- ✅ Issue identified: Date filtering in `get_articles_by_entities_with_date` was eliminating all potential matches
- ✅ The problem was in the SQL date comparison: `AND a.pub_date > ?` wasn't properly comparing RFC3339 formatted dates
- ✅ Our enhanced diagnostics found that articles with matching entities were being found (hundreds of matches) but all were being eliminated by the date filter
- ✅ Direct SQL tests showed the issue: even SQLite's `datetime()` function wasn't handling the RFC3339 formatted dates consistently

**Fix Implemented**
- ✅ Modified the SQL query in `db.rs` to use a simpler date comparison approach:
  ```sql
  AND substr(a.pub_date, 1, 10) >= substr(?, 1, 10)
  ```
- ✅ This extracts just the date portion (YYYY-MM-DD) from both the article date and threshold date
- ✅ By comparing only the date portions, we avoid timezone and format complications
- ✅ This approach ensures proper date comparison regardless of RFC3339 specific format variations

**Date Window Approach and Database Fixes**
- ✅ Changed from fixed date threshold to a dynamic date window (14 days before to 1 day after article's publication date)
- ✅ Updated code in `db.rs` and `vector.rs` to use article's own publication date as a reference point
- ✅ Added comprehensive logging to identify issues with date filtering
- ✅ Fixed critical bugs with date filtering and NULL handling:
  - **NULL Handling Fix**: Modified `store_embedding` to properly handle NULL values for dates by using `Option<&str>` parameters instead of defaulting to "unknown" string literals
  - **Date Window Enhancement**: Updated SQL query to use COALESCE to check both event_date and pub_date when filtering: `COALESCE(date(substr(a.event_date,1,10)), date(substr(a.pub_date,1,10)))`
  - **Performance Optimization**: Added index on `articles(pub_date)` to improve query performance
  - **Proper NULL Semantics**: Only include the date filter when a source date exists, ensuring proper SQL behavior with NULL values

### Previous Focus: Fixing Inconsistent Entity-Based Article Matching

We've resolved multiple issues with the article similarity scoring system that were causing articles with no entity overlap to sometimes appear in "similar articles" results:

**First fix: Similarity weighting inconsistency**
- We implemented a dual-query approach for enhanced article matching that combines:
  - Vector similarity (60% weight)
  - Entity overlap (30% weight) 
  - Temporal proximity (10% weight)
- However, there was an inconsistency in the code:
  - For articles with entity data, we correctly applied the formula: `0.6 * vector_score + 0.4 * entity_similarity`
  - But for articles without entity data, we were using 100% of the vector score, bypassing our weighting system
  - This allowed articles with zero entity overlap to appear in results if their vector similarity was high enough (≥0.75)

**Second fix: Missing consistent weighting in other code paths**
- After fixing the first issue, we discovered a second inconsistency in the `get_similar_articles_with_entities` function:
  - When articles without entities were found through vector-based searching, we correctly applied the 60% weighting in some code paths
  - But another code path for the same condition was using 100% of the vector score
  - This inconsistency allowed articles with no entity overlap to still appear in results

Previously completed fixes:
- ✅ Modified all code paths that handle articles without entity data to use `final_score = 0.6 * article.score`
- ✅ Updated all similarity formula descriptions to consistently say "60% vector similarity"
- ✅ Added detailed diagnostic logging throughout the entity extraction and matching process
- ✅ Added verification logging that confirms all final matches have entity overlap
- ✅ Added detailed pre-filter and post-filter logging to show exactly which articles are being filtered out and why

Impact of these changes:
- Articles without entity overlap will score a maximum of 0.6 (60% of perfect vector similarity)
- Since the threshold is 0.75, articles must have some entity overlap to appear in results
- The system now properly enforces the requirement that similar articles should share at least one entity
- Users will see more relevant, semantically connected content in similar articles sections
- Developers have much better visibility into the matching process through enhanced logging

### Previous Focus: Enhanced Article Matching Transparency

Recently completed:
- ✅ Enhanced `ArticleMatch` struct to expose detailed similarity metrics:
  - Added vector quality metrics (vector_score, vector_active_dimensions, vector_magnitude)
  - Added entity overlap metrics (entity_overlap_count, primary_overlap_count)
  - Added entity type-specific metrics (person_overlap, org_overlap, location_overlap, event_overlap)
  - Added temporal_proximity to show date-based similarity
  - Added similarity_formula field to explain the calculation methodology
- ✅ Updated all JSON outputs to include the new fields in similar articles sections
- ✅ Ensured backward compatibility with all existing functionality
- ✅ Verified implementation with `cargo check --bin argus` (no warnings)
- ✅ Added `get_articles_by_entities` method in db.rs to centralize database queries
- ✅ Implemented `get_similar_articles_with_entities` in vector.rs 
- ✅ Designed and implemented dual-query approach that combines vector and entity-based search results
- ✅ Enhanced transparency of similarity scoring with detailed metrics
- ✅ Added formula explanation showing the weighted contributions

## Active Work Areas

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

1. **Vector Database Integration**: Exploring deeper integration with Qdrant for semantic search
2. **Enhanced Metrics**: Improving system health and performance monitoring
3. **Testing Infrastructure**: Expanding automated testing for reliability
4. **Topic Management**: Building tools for better topic configuration and management
5. **User Preference Refinement**: Further customization of notification preferences
6. **Content Filtering Improvements**: Refining relevance detection and quality assessment
7. **Entity Matching Feedback**: Analyzing user feedback on missed matches to improve the algorithm
8. **Vector Similarity Enhancements**: Continue refining vector similarity calculations for edge cases

## Active Decisions

### Architecture Decisions
- **Database Scaling**: Evaluating whether SQLite will remain sufficient or if migration to PostgreSQL will be needed
- **Deployment Model**: Determining the optimal deployment strategy for production environments
- **Worker Distribution**: Finalizing approach to worker allocation and load balancing

### Feature Decisions
- **Topic Configuration**: Determining the best approach for managing and updating topic definitions
- **Quality Thresholds**: Setting appropriate thresholds for content quality assessment
- **Notification Frequency**: Balancing notification volume to avoid overwhelming users
- **Entity Matching Feedback**: Establishing a process for incorporating user feedback on missed matches

### Technical Decisions
- **LLM Provider Strategy**: Evaluating cost/performance tradeoffs between different LLM providers
- **Caching Strategy**: Determining what and how to cache for performance optimization
- **Error Handling**: Standardizing approach to error recovery and system resilience
- **Vector Similarity Calculation**: Refining methods for calculating and comparing vector embeddings

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
