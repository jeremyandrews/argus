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

### Current Focus: Entity Extraction Integration

We've fixed and enhanced the entity extraction system to properly populate entity tables, which are essential for entity-based article matching.

Recently identified and fixed:
- ✅ Fixed entity extraction in analysis_worker.rs to properly use JSON mode with the LLM
- ✅ Replaced direct LLM calls with proper calls to the extract_entities function
- ✅ Added better error handling and logging for entity extraction
- ✅ Created process_entities.rs utility to reprocess existing articles for entity data
- ✅ Created test_entity_extraction.rs utility to verify entity extraction functionality

Current implementation status:
- ✅ Entity extraction now correctly extracts entities from article text
- ✅ Entity data is properly stored in the database tables
- ✅ Entity-based article matching and clustering now has the data it needs
- ✅ Both new and existing articles can have their entities extracted
- ✅ Detailed logging provides visibility into the extraction process

Next steps:
- Monitor entity extraction quality and adjust prompts as needed
- Analyze entity distribution patterns to optimize matching algorithms
- Enhance entity normalization for better cross-article matching
- Consider UI enhancements to visualize entity relationships
- Explore entity-based search functionality

### Previous Focus: Enhanced Article Matching Transparency

We've implemented a dual-query approach for enhanced article matching that combines:

- Vector similarity (60% weight)
- Entity overlap (30% weight) 
- Temporal proximity (10% weight)

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

Implementation status:
- ✅ Added `get_articles_by_entities` method in db.rs to centralize database queries
- ✅ Implemented `get_similar_articles_with_entities` in vector.rs 
- ✅ Designed and implemented dual-query approach that combines vector and entity-based search results
- ✅ Enhanced transparency of similarity scoring with detailed metrics
- ✅ Added formula explanation showing the weighted contributions

This enhancement builds upon the current "similar articles" functionality, making it more transparent and helping diagnose quality issues in related content matching.

## Active Work Areas

### LLM Integration Enhancements
- **Fallback Handling**: Implementing graceful fallback between different LLM providers
- **Model Configuration**: Fine-tuning prompt templates for different analysis types
- **Temperature Settings**: Optimizing temperature settings for different tasks

### iOS App Integration
- **Push Notification System**: Refining the notification delivery system
- **R2 Content Storage**: Storing and serving content through Cloudflare R2
- **Subscription Management**: Managing user topic subscriptions and preferences

### Performance Optimization
- **Resource Allocation**: Balancing workloads between decision and analysis tasks
- **Queue Management**: Optimizing database queue processing
- **Parallel Processing**: Ensuring efficient utilization of multiple workers

### Critical Analysis Features
- **Source Quality Assessment**: Evaluating source credibility and reliability
- **Argument Quality Evaluation**: Detecting logical fallacies and reasoning strength
- **Additional Insights**: Generating deeper context and connections

## Next Development Steps

Based on codebase analysis, likely next steps include:

1. **Vector Database Integration**: Exploring deeper integration with Qdrant for semantic search
2. **Enhanced Metrics**: Improving system health and performance monitoring
3. **Testing Infrastructure**: Expanding automated testing for reliability
4. **Topic Management**: Building tools for better topic configuration and management
5. **User Preference Refinement**: Further customization of notification preferences
6. **Content Filtering Improvements**: Refining relevance detection and quality assessment

## Active Decisions

### Architecture Decisions
- **Database Scaling**: Evaluating whether SQLite will remain sufficient or if migration to PostgreSQL will be needed
- **Deployment Model**: Determining the optimal deployment strategy for production environments
- **Worker Distribution**: Finalizing approach to worker allocation and load balancing

### Feature Decisions
- **Topic Configuration**: Determining the best approach for managing and updating topic definitions
- **Quality Thresholds**: Setting appropriate thresholds for content quality assessment
- **Notification Frequency**: Balancing notification volume to avoid overwhelming users

### Technical Decisions
- **LLM Provider Strategy**: Evaluating cost/performance tradeoffs between different LLM providers
- **Caching Strategy**: Determining what and how to cache for performance optimization
- **Error Handling**: Standardizing approach to error recovery and system resilience

## Integration Requirements

### iOS Application
- **Authentication Flow**: JWT-based authentication system for secure access
- **Push Notification Handling**: Proper setup of APNs certificates and device token management
- **Content Synchronization**: Tracking read/unread status across devices

### Slack Integration
- **Message Formatting**: Standardized formatting for different content types
- **Channel Management**: Appropriate routing of different content categories
- **Interaction Handling**: Potential for interactive elements in Slack messages

### LLM Services
- **API Access**: Proper API key management for OpenAI and other providers
- **Ollama Configuration**: Setup and maintenance of local Ollama instances
- **Provider Failover**: Seamless switching between providers when needed
- **Interaction Handling**: Potential for interactive elements in Slack messages

### LLM Services
- **API Access**: Proper API key management for OpenAI and other providers
- **Ollama Configuration**: Setup and maintenance of local Ollama instances
- **Provider Failover**: Seamless switching between providers when needed
