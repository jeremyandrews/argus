# Argus System Patterns

## Architectural Overview
Argus employs a multi-worker processing pipeline architecture that maximizes throughput and resilience. The system comprises several specialized worker types that operate concurrently to process different stages of the content lifecycle.

```mermaid
flowchart TD
    RSS[RSS Fetcher] --> Queue[(RSS Queue)]
    Queue --> DW1[Decision Worker 1]
    Queue --> DW2[Decision Worker 2]
    Queue --> DWn[Decision Worker n]
    
    DW1 --> LifeSafety[(Life Safety Queue)]
    DW1 --> MatchedTopics[(Matched Topics Queue)]
    DW2 --> LifeSafety
    DW2 --> MatchedTopics
    DWn --> LifeSafety
    DWn --> MatchedTopics
    
    LifeSafety --> AW1[Analysis Worker 1]
    MatchedTopics --> AW1
    LifeSafety --> AW2[Analysis Worker 2]
    MatchedTopics --> AW2
    
    AW1 --> VS[(Vector Store)]
    AW1 --> DB[(SQLite Database)]
    AW1 --> ES[(Entity Store)]
    AW2 --> VS
    AW2 --> DB
    AW2 --> ES
    
    DB --> API[API Server]
    VS --> API
    ES --> API
    
    API --> Slack[Slack Notifications]
    API --> Mobile[Mobile App]
```

## Core Design Patterns

### 1. Worker Pipeline Pattern
- **Workers**: Specialized processes for specific tasks (RSS fetching, decision making, analysis)
- **Message Queues**: Database tables functioning as work queues
- **Concurrency**: Multiple workers processing items simultaneously
- **Load Distribution**: Random and prioritized queue item selection

### 2. Content Processing Pipeline
```mermaid
flowchart LR
    Fetch[RSS Fetch] --> Extract[Content Extraction]
    Extract --> Decide[Relevance Decision]
    Decide --> Queue[Topic/Safety Queuing]
    Queue --> Analyze[Deep Analysis]
    Analyze --> Store[Database Storage]
    Store --> EntityExtract[Entity Extraction]
    EntityExtract --> Vector[Vector Embedding]
    Vector --> Similar[Similarity Matching]
    Similar --> Notify[Notification]
```

### 3. Database Patterns
- **Central SQLite Database**: Persistent storage with structured schema
- **Queue Tables**: RSS, Matched Topics, and Life Safety queues
- **Article Storage**: Complete content with analysis metadata
- **Entity Storage**: Named entity extraction with relationships
- **Index Optimization**: Strategic indexing for query performance

### 4. Content Matching Patterns
1. **Vector Similarity Matching**
   - Embeds article summaries into vector space
   - Calculates cosine similarity between embeddings
   - Identifies semantically similar content
   
2. **Entity-Based Matching**
   - Extracts named entities (people, organizations, locations, events)
   - Normalizes entity names for consistent matching
   - Tracks entity importance (PRIMARY, SECONDARY, MENTIONED)
   - Links articles sharing significant entities
   
3. **Temporal Correlation**
   - Tracks publication dates and event dates
   - Groups content related to the same timeframe
   - Enables chronological event tracking

### 5. Analysis Patterns
- **Multi-Stage Analysis**: Progressive refinement of content understanding
- **Quality Scoring**: Source quality and argument quality metrics
- **Fallback Mechanism**: Adaptive worker behavior during idle periods

### 6. Notification Patterns
- **Topic-Based Filtering**: User subscription to specific topics
- **Priority-Based Delivery**: Life safety alerts receive highest priority
- **Multi-Channel Distribution**: Slack and mobile application delivery
- **Rich Content Display**: Formatted analysis with embedded metadata

## Key Implementation Patterns

### Worker Management
- **Startup Sequence**: Orderly initialization of system components
- **Worker Configuration**: Environment-based configuration
- **Error Handling**: Graceful failure recovery with logging
- **Retry Logic**: Exponential backoff for transient failures

### Content Processing 
- **URL Normalization**: Consistent handling of URLs to prevent duplicates
- **Hash-Based Deduplication**: Content-based duplicate detection
- **HTML Parsing**: Robust extraction of article content
- **Quality Thresholds**: Minimum requirements for processing

### Entity Extraction
- **Entity Recognition**: LLM-based identification of named entities
- **Entity Categorization**: Classification by type (PERSON, ORGANIZATION, etc.)
- **Importance Ranking**: Determination of entity significance to article
- **Normalization**: Standardization of entity names for matching

### Data Persistence
- **Transaction Management**: ACID compliance for critical operations
- **Concurrent Access**: Safe multi-worker database operations
- **Query Optimization**: Performance-tuned database interactions
- **Schema Evolution**: Forward-compatible database design
