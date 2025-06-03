# Argus Article JSON Schema

This document defines the complete JSON schema for article analysis results returned by the Argus system. This JSON is stored in S3 and referenced via the `r2_url` field.

## Overview

Each analyzed article produces a comprehensive JSON object containing analysis results, entity data, cluster information, and similar articles. The JSON structure is designed to provide complete information for frontend applications while maintaining backward compatibility.

## Complete JSON Schema

```json
{
  // Core Article Information
  "topic": "Alert: Direct",
  "title": "Apple Announces iPhone 15 with USB-C",
  "url": "https://example.com/article",
  "pub_date": "2023-09-12T18:00:00Z",
  "hash": "abc123def456",
  "title_domain_hash": "xyz789uvw012",
  
  // Analysis Results
  "summary": "Apple officially announced the iPhone 15 series featuring USB-C connectivity, marking the end of the Lightning port era. The change comes in response to EU regulations requiring universal charging standards.",
  
  "tiny_summary": "Apple announced iPhone 15 with USB-C, replacing Lightning ports due to EU regulations",
  
  "relevance": "This announcement significantly impacts Apple's ecosystem and represents compliance with new EU charging standards. Relevant for technology adoption, regulatory compliance, and consumer electronics markets.",
  
  "eli5": "Apple is changing the charging port on their phones from their special Lightning port to USB-C, which is what most other devices use. They're doing this because Europe made a rule saying all phones should use the same type of charger so people don't need so many different cables.",
  
  "talking_points": [
    "First major iPhone design change since Lightning port introduction in 2012",
    "Compliance with EU's Digital Markets Act requirements",
    "Potential impact on Apple's MFi (Made for iPhone) accessory ecosystem",
    "Consumer benefits: reduced cable clutter and universal charging"
  ],
  
  "action_recommendations": [
    "Monitor accessory market response and new USB-C iPhone accessories",
    "Track EU regulatory compliance timelines for other Apple products",
    "Assess impact on Apple's services revenue from Lightning accessory licensing"
  ],
  
  // Quality and Scoring
  "sources_quality": 3,
  "argument_quality": 3,
  "logical_fallacies": [],
  
  // Analysis Sections
  "source_analysis": "Article sources include Apple's official press release, verified Apple executive statements, and regulatory documentation from the European Commission. All sources are primary and authoritative.",
  
  "critical_analysis": "The announcement represents a significant strategic shift for Apple, balancing regulatory compliance with ecosystem control. The timing suggests Apple prioritized EU market access over maintaining proprietary standards.",
  
  "argus_speaks": "This transition marks Apple's adaptation to regulatory pressure while potentially opening new revenue streams through enhanced compatibility. The move signals increasing influence of regulatory frameworks on Big Tech product decisions.",
  
  // Entity Extraction (NEW)
  "entities": [
    {
      "name": "Apple Inc.",
      "normalized_name": "apple inc",
      "type": "ORGANIZATION",
      "importance": "PRIMARY"
    },
    {
      "name": "iPhone 15",
      "normalized_name": "iphone 15",
      "type": "PRODUCT",
      "importance": "PRIMARY"
    },
    {
      "name": "European Union",
      "normalized_name": "european union",
      "type": "ORGANIZATION",
      "importance": "SECONDARY"
    },
    {
      "name": "USB-C",
      "normalized_name": "usb-c",
      "type": "PRODUCT",
      "importance": "PRIMARY"
    },
    {
      "name": "Lightning",
      "normalized_name": "lightning",
      "type": "PRODUCT",
      "importance": "SECONDARY"
    }
  ],
  
  // Cluster Summary (NEW)
  "cluster_summary": "**TL;DR:** Apple announced the iPhone 15 series with USB-C ports, ending over a decade of Lightning connector usage due to EU regulatory requirements.\n\n**Full Summary:**\nAccording to Apple's official announcement and multiple high-quality sources, the iPhone 15 series represents a significant shift in Apple's hardware strategy with the adoption of USB-C connectivity. This change affects the entire iPhone lineup and marks the end of Lightning ports that have been standard since 2012.\n\nThe transition is primarily driven by the European Union's Digital Markets Act, which requires universal charging standards for mobile devices sold in EU markets. Apple's compliance with these regulations demonstrates the growing influence of regulatory frameworks on major technology decisions.\n\n**References:**\nHigh Quality Sources:\n- Apple Official Press Release (Sept 12, 2023)\n- Apple Event Livestream (Sept 12, 2023)\n- EU Digital Markets Act Documentation (2022)\n\nMedium Quality Sources:\n- The Verge: iPhone 15 Analysis (Sept 12, 2023)\n- TechCrunch: Market Impact Assessment (Sept 12, 2023)",
  
  // Similar Articles
  "similar_articles": [
    {
      "id": 12345,
      "json_url": "https://s3.example.com/analysis/article-12345.json",
      "title": "EU Mandates Universal Phone Chargers",
      "tiny_summary": "European Union finalizes legislation requiring USB-C for all mobile devices",
      "category": "Technology",
      "published_date": "2023-09-10T14:30:00Z",
      "quality_score": 3,
      "similarity_score": 0.87,
      
      // Vector Quality Metrics
      "vector_score": 0.85,
      "vector_active_dimensions": 1024,
      "vector_magnitude": 2.34,
      
      // Entity Similarity Metrics
      "entity_overlap_count": 3,
      "primary_overlap_count": 2,
      "person_overlap": 0.0,
      "org_overlap": 0.8,
      "location_overlap": 0.6,
      "event_overlap": 0.9,
      "temporal_proximity": 0.95,
      
      "similarity_formula": "Combined vector (0.85) + entity overlap (3 entities) + temporal proximity (0.95) = 0.87"
    }
  ]
}
```

## Field Definitions

### Core Article Fields

| Field | Type | Description |
|-------|------|-------------|
| `topic` | string | Relevance classification (e.g., "Alert: Direct", "Alert: Indirect", "Not Relevant") |
| `title` | string | Article title |
| `url` | string | Original article URL |
| `pub_date` | string | Publication date in ISO 8601 format |
| `hash` | string | Content hash for deduplication |
| `title_domain_hash` | string | Combined title and domain hash |

### Analysis Fields

| Field | Type | Description |
|-------|------|-------------|
| `summary` | string | Comprehensive article summary (200-400 words) |
| `tiny_summary` | string | Brief summary for quick scanning (1-2 sentences) |
| `relevance` | string | Detailed relevance explanation |
| `eli5` | string | "Explain Like I'm 5" - Simple explanation for general audiences |
| `talking_points` | array[string] | Key discussion points and highlights |
| `action_recommendations` | array[string] | Suggested actions or follow-ups |
| `argus_speaks` | string | AI perspective and additional context |

### Quality and Analysis

| Field | Type | Description |
|-------|------|-------------|
| `sources_quality` | integer | Source quality score (1-3, where 3=highest quality) |
| `argument_quality` | integer | Argument logic quality score (1-3) |
| `logical_fallacies` | array[string] | Identified logical fallacies, if any |
| `source_analysis` | string | Detailed source credibility analysis |
| `critical_analysis` | string | Critical evaluation of claims and evidence |

### Entity Data (NEW)

| Field | Type | Description |
|-------|------|-------------|
| `entities` | array[object] | Extracted entities from the article |
| `entities[].name` | string | Original entity name as found in text |
| `entities[].normalized_name` | string | Normalized name for matching |
| `entities[].type` | string | Entity type: "PERSON", "ORGANIZATION", "LOCATION", "EVENT", "PRODUCT", "DATE", "OTHER" |
| `entities[].importance` | string | Importance level: "PRIMARY", "SECONDARY" |

### Cluster Summary (NEW)

| Field | Type | Description |
|-------|------|-------------|
| `cluster_summary` | string | Executive summary combining multiple related articles with TL;DR, full summary, quality notes, and references |

### Similar Articles

| Field | Type | Description |
|-------|------|-------------|
| `similar_articles` | array[object] | Articles with similar content or entities |
| `similar_articles[].id` | integer | Article database ID |
| `similar_articles[].json_url` | string | URL to similar article's analysis JSON |
| `similar_articles[].title` | string | Similar article title |
| `similar_articles[].tiny_summary` | string | Brief summary of similar article |
| `similar_articles[].similarity_score` | number | Overall similarity score (0.0-1.0) |
| `similar_articles[].quality_score` | integer | Quality score (1-3) |

#### Vector Similarity Metrics

| Field | Type | Description |
|-------|------|-------------|
| `vector_score` | number | Pure vector similarity score |
| `vector_active_dimensions` | integer | Number of active dimensions in embedding |
| `vector_magnitude` | number | Vector magnitude |

#### Entity Similarity Metrics

| Field | Type | Description |
|-------|------|-------------|
| `entity_overlap_count` | integer | Total overlapping entities |
| `primary_overlap_count` | integer | Primary entities in common |
| `person_overlap` | number | Person entity overlap score |
| `org_overlap` | number | Organization entity overlap score |
| `location_overlap` | number | Location entity overlap score |
| `event_overlap` | number | Event entity overlap score |
| `temporal_proximity` | number | Time-based proximity score |
| `similarity_formula` | string | Human-readable explanation of similarity calculation |

## Entity Types

The system recognizes the following entity types:

- **PERSON**: Individuals (e.g., "Tim Cook", "Elon Musk")
- **ORGANIZATION**: Companies, governments, institutions (e.g., "Apple Inc.", "European Union")
- **LOCATION**: Geographic locations (e.g., "California", "European Union")
- **EVENT**: Named events or incidents (e.g., "WWDC 2023", "iPhone 15 Launch")
- **PRODUCT**: Products, services, technologies (e.g., "iPhone 15", "USB-C")
- **DATE**: Specific dates or time periods (e.g., "September 2023")
- **OTHER**: Other named entities not fitting above categories

## Entity Importance Levels

- **PRIMARY**: Central to the article's main story
- **SECONDARY**: Supporting or contextual entities

## Quality Scoring

Quality scores use a 1-3 scale:

- **3 (High)**: Reliable sources, strong evidence, clear reasoning
- **2 (Medium)**: Generally trustworthy with some minor concerns
- **1 (Low)**: Significant quality or credibility issues

## Version History

- **v1.0**: Initial schema with basic analysis fields
- **v1.1**: Added entity extraction and cluster summaries
- **v1.2**: Enhanced similarity metrics with vector and entity details

## Usage Examples

### Frontend Filtering by Entity Type
```javascript
// Filter articles about specific organizations
articles.filter(article => 
  article.entities.some(entity => 
    entity.type === 'ORGANIZATION' && 
    entity.normalized_name === 'apple inc'
  )
)
```

### Quality-Based Display
```javascript
// Show quality indicator
const getQualityLabel = (score) => {
  switch(score) {
    case 3: return "High Quality";
    case 2: return "Medium Quality"; 
    case 1: return "Low Quality";
    default: return "Unknown Quality";
  }
}
```

### Cluster Summary Integration
```javascript
// Use cluster summary as primary content when available
const displayContent = article.cluster_summary || article.summary;
```

This schema provides complete information for building sophisticated news analysis interfaces while maintaining compatibility with existing implementations.
