# Entity Matching Diagnostic Tools

This document provides comprehensive instructions for using the entity matching diagnostic tools we've developed to analyze and improve the entity matching system.

## Overview

We've created a comprehensive suite of diagnostic tools to identify why approximately 80% of valid entity matches are being missed while maintaining the current high precision. These tools help identify patterns, specific issues, and provide data for systematic improvement of the matching algorithm.

## Available Tools

### 1. API Analysis Endpoint

The `/articles/analyze-match` endpoint provides detailed diagnostic information about why two articles do or don't match.

#### Usage

```bash
curl -X POST https://api.argus.com/articles/analyze-match \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"source_article_id": 12345, "target_article_id": 67890}'
```

#### Response Format

```json
{
  "match_status": false,
  "combined_score": 0.62,
  "threshold": 0.75,
  "vector_similarity": {
    "score": 0.76,
    "weighted_contribution": 0.456,
    "weight": 0.6
  },
  "entity_similarity": {
    "score": 0.42,
    "weighted_contribution": 0.168,
    "weight": 0.4,
    "entity_overlap_count": 2,
    "primary_overlap_count": 1,
    "person_overlap": 0.75,
    "organization_overlap": 0.25,
    "location_overlap": 0.0,
    "event_overlap": 0.0
  },
  "shared_entities": [
    {
      "name": "Joe Biden",
      "entity_type": "PERSON",
      "source_importance": "PRIMARY",
      "target_importance": "SECONDARY"
    },
    {
      "name": "White House",
      "entity_type": "ORGANIZATION",
      "source_importance": "SECONDARY",
      "target_importance": "MENTIONED"
    }
  ],
  "missing_score": 0.13,
  "reason": "Combined score below threshold (needs 0.13 more)"
}
```

#### Key Fields

- `match_status`: Whether the articles match according to current criteria
- `combined_score`: The final similarity score (0.0-1.0)
- `threshold`: The minimum score required for a match (currently 0.75)
- `vector_similarity`: Vector embedding similarity metrics
- `entity_similarity`: Entity-based similarity metrics
- `shared_entities`: List of entities that appear in both articles
- `missing_score`: How much more score would be needed to reach threshold
- `reason`: Human-readable explanation for the match decision

### 2. Single Pair Analysis Tool (analyze_matches)

The `analyze_matches` command-line tool provides detailed analysis of why two specific articles do or don't match.

#### Usage

```bash
cargo run --bin analyze_matches 12345 67890
```

Where `12345` and `67890` are the article IDs you want to compare.

#### Output

The tool outputs detailed information about:

1. Vector similarity score (0.0-1.0)
2. Entity overlap metrics:
   - Total shared entities
   - Primary entities shared
   - Breakdown by entity type (person, organization, location, event)
3. Combined score calculation
4. Clear explanation of why articles do or don't match
5. Entity-by-entity comparison with importance levels

#### Example

```
Analyzing match between articles 12345 and 67890:

Vector Similarity: 0.76 (weighted: 0.456, weight: 0.6)

Entity Similarity: 0.42 (weighted: 0.168, weight: 0.4)
  - Shared entities: 2
  - Primary entities shared: 1
  - Person overlap: 0.75
  - Organization overlap: 0.25
  - Location overlap: 0.0
  - Event overlap: 0.0

Shared Entities:
  - Joe Biden (PERSON)
    Source importance: PRIMARY
    Target importance: SECONDARY
  - White House (ORGANIZATION)
    Source importance: SECONDARY
    Target importance: MENTIONED

Combined Score: 0.62
Threshold: 0.75
Match Result: NO MATCH (needs 0.13 more)

Reason: Combined score below threshold (needs 0.13 more)
```

### 3. Batch Analysis Tool (batch_analyze)

The `batch_analyze` tool processes multiple article pairs to identify patterns in matching success and failure.

#### Usage

```bash
cargo run --bin batch_analyze INPUT_CSV [OUTPUT_CSV]
```

Where:
- `INPUT_CSV` is a CSV file with article ID pairs
- `OUTPUT_CSV` (optional) is the file to write detailed results to (defaults to batch_results.csv)

#### Input CSV Format

```csv
source_id,target_id,[expected_match]
12345,67890,true
45678,23456,false
```

The `expected_match` column is optional. Including it allows the tool to compare expected vs. actual matches.

#### Output

1. A detailed CSV file with:
   - Source and target article IDs
   - Match status
   - Combined score
   - Vector score
   - Entity score
   - Number of shared entities
   - Number of primary shared entities
   - Entity type overlap scores
   - Reason for match/non-match
   - Expected match status (if provided)
   - Whether expected matches actual

2. Console summary statistics:
   - Total articles analyzed
   - Match rate
   - Most common reasons for non-matches
   - Score distributions for matches vs. non-matches
   - Precision and recall metrics (if expected match data provided)

#### Example Console Output

```
Summary Statistics:
-------------------
Total article pairs analyzed: 100
Matched pairs: 22 (22.0%)
Non-matched pairs: 78 (78.0%)

Prediction Accuracy (for 45 pairs with expectations):
Correct predictions: 35 (77.8%)
Incorrect predictions: 10 (22.2%)
False positives: 3 (6.7% of non-matches)
False negatives: 7 (15.6% of expected matches)

Reasons for Non-Matches:
No shared entities: 42 (53.8% of non-matches)
Low vector similarity: 18 (23.1% of non-matches)
Weak entity similarity: 12 (15.4% of non-matches)
Combined score below threshold: 6 (7.7% of non-matches)

Score Distributions:
Matched pairs - Avg combined score: 0.82, Avg vector: 0.86, Avg entity: 0.75, Avg shared entities: 3.2
Non-matched pairs - Avg combined score: 0.48, Avg vector: 0.65, Avg entity: 0.23, Avg shared entities: 0.9
```

### 4. Test Data Generator (create_match_pairs)

The `create_match_pairs` tool generates test datasets of article pairs for analysis.

#### Usage

```bash
cargo run --bin create_match_pairs OUTPUT_CSV [NUM_PAIRS] [DAYS_BACK]
```

Where:
- `OUTPUT_CSV` is the file to write the article pairs to
- `NUM_PAIRS` (optional) is the number of pairs to generate (default: 100)
- `DAYS_BACK` (optional) is how many days back to look for articles (default: 7)

#### Output

A CSV file with article ID pairs in the format:

```csv
source_id,target_id
12345,67890
45678,23456
```

This file is ready to use with the `batch_analyze` tool.

#### How It Works

1. Finds articles with extracted entities from the last N days
2. Creates pairs of articles from the same day (likely to be related)
3. Adds random pairs until reaching the target number
4. Ensures no duplicate pairs

## Diagnostic Workflow

### For Individual Match Investigation

When you have a specific pair of articles that should match but don't:

1. Use the single pair analyzer to get detailed diagnostics:
   ```bash
   cargo run --bin analyze_matches 12345 67890
   ```

2. Look for specific issues:
   - No shared entities despite semantic similarity (extraction issue)
   - Low vector similarity despite entity overlap (embedding issue)
   - Entity importance misalignment (importance classification issue)
   - Date filtering eliminating valid matches (temporal issue)

3. Check the entity extraction for each article to confirm entities are being extracted correctly

### For Systematic Improvement

To identify patterns and prioritize improvements:

1. Generate a test dataset:
   ```bash
   cargo run --bin create_match_pairs test_pairs.csv 200 14
   ```

2. Run batch analysis:
   ```bash
   cargo run --bin batch_analyze test_pairs.csv results.csv
   ```

3. Analyze the statistics to identify the most common reasons for missed matches

4. Prioritize improvements based on frequency and impact

5. After implementing changes, re-run the analysis with the same dataset to measure improvement

## Common Match Failure Patterns

Based on our analysis, these are the most common reasons for missed matches:

1. **No Shared Entities**: The LLM entity extraction is missing entities in one or both articles
   - Improve entity extraction prompts
   - Add entity alias/normalization system

2. **Low Vector Similarity**: The embedding model doesn't recognize the semantic similarity
   - Evaluate alternative embedding models
   - Fine-tune embeddings for news domain

3. **Entity Importance Misclassification**: Entities are extracted but with inconsistent importance levels
   - Refine importance classification in extraction prompts
   - Add importance normalization logic

4. **Date Filtering Issues**: Valid matches eliminated by date constraints
   - Widen date window for potential matches
   - Improve date extraction from articles

5. **Single Entity Type Dominance**: Matching relies too heavily on one entity type
   - Balance weights between entity types
   - Ensure proper extraction across all entity types

## Performance Considerations

- The `batch_analyze` tool can process hundreds of article pairs, but performance depends on database size
- For large datasets, consider breaking analysis into smaller batches
- If analyzing thousands of pairs, run as an overnight job

## Best Practices

1. **Start with representative samples**: Include known matches and non-matches in your test datasets
2. **Focus on one issue at a time**: Identify the most common failure pattern and address it first
3. **Track improvements**: Re-run analysis after each change to measure impact
4. **Use consistent test datasets**: Maintain reference datasets for benchmarking
5. **Look for edge cases**: Special attention to articles with unusual characteristics
6. **Document findings**: Record patterns and insights in the memory bank

## Future Enhancements

Planned improvements to the diagnostic tools:

1. **Expected matches file**: Support for a separate file of known matches to test against
2. **Entity extraction diagnostics**: Direct comparison of extracted entities between articles
3. **Interactive mode**: Command-line tool for exploring match decisions interactively
4. **Visualization**: Statistical visualization of match patterns
5. **Configuration testing**: Testing different thresholds and weights
