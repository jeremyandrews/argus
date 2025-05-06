# System Patterns and Architecture

## Core Architecture

Argus is a modular Rust application built around several key components:

1. **RSS Fetching**: Gather news and content from configured RSS feeds
2. **Content Processing**: Extract and analyze article content
3. **Decision Making**: Determine relevance and priority of articles
4. **Analysis**: Perform in-depth analysis of relevant articles
5. **Database Storage**: Store articles, analysis results, and metadata
6. **API Interface**: Expose results and controls to users

## RSS Module Architecture

The RSS module is organized in a modular fashion to handle feed fetching, parsing, and processing:

```
src/rss/
├── mod.rs           # Main module exports
├── types.rs         # RSS data types and constants
├── client.rs        # HTTP client functionality
├── parser.rs        # Feed content parsing
├── fetcher.rs       # RSS feed fetching loop
├── test.rs          # Testing and diagnostic tools
└── util.rs          # Helper functions
```

## Worker System Architecture

The worker system is a crucial component, operating with a modular pattern:

```mermaid
flowchart TD
    subgraph RSS[RSS Processing]
        RSS_Feed[RSS Feed Parsing] --> RSS_Queue[RSS Queue]
    end
    
    subgraph Workers[Worker System]
        subgraph Decision[Decision Worker]
            Worker_Loop[Worker Loop] --> Text_Extraction[Text Extraction]
            Worker_Loop --> Threat_Assessment[Threat Assessment]
            Worker_Loop --> Topic_Processing[Topic Processing]
        end
        
        subgraph Analysis[Analysis Worker]
            Analysis_Loop[Worker Loop] --> Processing[Processing]
            Processing --> Quality[Quality Assessment]
            Processing --> Entity[Entity Handling]
            Processing --> Similarity[Similarity Analysis]
        end
    end
    
    subgraph Storage[Storage]
        DB[Database] --> Articles[Articles]
        DB --> Entities[Entities]
        DB --> Clusters[Clusters]
    end
    
    RSS_Queue --> Decision
    Decision --> Storage
    Decision --> Analysis
    Analysis --> Storage
```

### Worker Module Organization

The worker system is now organized in a modular fashion:

```
src/workers/
├── mod.rs                 # Main module export
├── common.rs              # Shared functionality between workers
├── analysis/              # Analysis worker components
│   ├── mod.rs             # Module exports
│   ├── worker_loop.rs     # Main analysis worker loop
│   ├── processing.rs      # Processing logic
│   ├── quality.rs         # Quality assessment functionality
│   ├── similarity.rs      # Similarity calculation
│   └── entity_handling.rs # Entity extraction and processing
└── decision/              # Decision worker components
    ├── mod.rs             # Module exports
    ├── worker_loop.rs     # Main decision worker loop
    ├── processing.rs      # Processing logic
    ├── extraction.rs      # Article text extraction
    └── threat.rs          # Threat assessment functionality
```

#### Decision Worker Flow

```mermaid
flowchart TD
    start[Start] --> loop[Main Loop]
    loop --> rss_queue[Fetch from RSS Queue]
    rss_queue --> check_age[Check Article Age]
    check_age --> |Too Old| skip[Skip Article]
    check_age --> |Recent| extract[Extract Article Text]
    extract --> |Failed| handle_error[Record Access Error]
    extract --> |Success| threat_check[Check for Threats]
    threat_check --> |Is Threat| location_check[Determine Threat Location]
    location_check --> |Specific Location| safety_queue[Add to Life Safety Queue]
    location_check --> |No Specific Location| topic_process[Process for Topics]
    threat_check --> |Not Threat| topic_process
    topic_process --> promo_check[Check if Promotional]
    promo_check --> |Is Promotional| skip_promo[Skip as Non-Relevant]
    promo_check --> |Not Promotional| topics_loop[Loop Through Topics]
    topics_loop --> relevance_check[Check Relevance to Topic]
    relevance_check --> |Not Relevant| next_topic[Try Next Topic]
    relevance_check --> |Relevant| matched_queue[Add to Matched Topics Queue]
    topics_loop --> |No Matches| record_non_relevant[Record as Non-Relevant]
    skip --> loop
    handle_error --> loop
    safety_queue --> loop
    skip_promo --> loop
    matched_queue --> loop
    next_topic --> topics_loop
    record_non_relevant --> loop
```

#### Analysis Worker Flow

```mermaid
flowchart TD
    start[Start] --> loop[Main Loop]
    loop --> check_mode[Check Current Mode]
    check_mode --> |Analysis Mode| try_safety[Try Life Safety Queue]
    try_safety --> |Found Item| process_safety[Process Safety Item]
    try_safety --> |Empty| try_topics[Try Matched Topics Queue]
    try_topics --> |Found Item| process_topics[Process Topic Item]
    try_topics --> |Empty| sleep[Sleep and Continue]
    check_mode --> |Fallback Decision Mode| fetch_rss[Fetch from RSS Queue]
    fetch_rss --> process_feed[Process Feed Item as Decision Worker]
    process_feed --> check_duration[Check Fallback Duration]
    check_duration --> |Expired| switch_back[Switch to Analysis Mode]
    check_duration --> |Not Expired| loop
    process_safety --> loop
    process_topics --> loop
    sleep --> check_idle[Check Idle Time]
    check_idle --> |Idle Too Long| has_fallback[Check for Fallback Config]
    check_idle --> |Not Idle| loop
    has_fallback --> |Fallback Available| switch_mode[Switch to Fallback Decision Mode] 
    has_fallback --> |No Fallback| loop
    switch_mode --> loop
    switch_back --> loop
```

### Prompt System Architecture

The prompt system is organized in a modular fashion to match the workers system:

```
src/prompt/
├── mod.rs               # Main module with exports
├── analysis.rs          # Analysis-related prompts
├── common.rs            # Common prompt utilities
├── decisions.rs         # Decision-making prompts
├── entity.rs            # Entity-related prompts
├── insights.rs          # Insight generation prompts
├── relevance.rs         # Relevance assessment prompts
├── scoring.rs           # Quality scoring prompts
└── summarization.rs     # Summary generation prompts
```

## Database Architecture

The database layer is organized as follows:

```
src/db/
├── mod.rs              # Main module exports
├── core.rs             # Core database functionality
├── article.rs          # Article-related operations
├── cluster.rs          # Clustering functionality
├── device.rs           # Device management
├── queue.rs            # Queue operations
├── schema.rs           # Database schema
└── entity/             # Entity subsystem
    ├── mod.rs          # Entity module exports
    ├── core.rs         # Core entity functionality
    ├── alias.rs        # Entity alias handling
    └── relation.rs     # Entity relationship handling
```

## Entity System

The entity system handles the extraction, normalization, and relationship management of entities:

```
src/entity/
├── mod.rs              # Main module exports
├── aliases.rs          # Alias management
├── extraction.rs       # Entity extraction
├── matching.rs         # Entity matching
├── normalizer.rs       # Entity normalization
├── repository.rs       # Entity storage
└── types.rs            # Entity type definitions
```

## Worker Communication Pattern

Workers communicate through the database using specialized queues:

1. **RSS Queue**: Contains URLs to be processed by Decision Workers
2. **Life Safety Queue**: Urgent items about threats requiring immediate analysis
3. **Matched Topics Queue**: Articles matching configured topics requiring analysis

## Fallback Pattern

The analysis worker implements a fallback pattern that allows it to:

1. Monitor its idle time
2. Switch to act as a Decision worker if idle too long
3. Process RSS queue items directly during fallback mode
4. Switch back to Analysis mode after a predetermined time

This enables efficient resource utilization when there are no analysis tasks pending.
