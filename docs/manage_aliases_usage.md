# Basic Usage of `manage_aliases`

`manage_aliases.sh` is a wrapper script that provides an interface to Argus's entity alias management system. This documentation covers the essential operations for working with aliases.

## Overview

The entity alias system helps Argus recognize when different names refer to the same entity. For example, "Tim Cook" and "Timothy Cook" should be recognized as the same person. The system uses a combination of:

- Pattern-based matching
- Manual corrections ("fix" source)
- Static predefined aliases

## Checking Alias Statistics

To view current alias system statistics:

```bash
./manage_aliases.sh stats
```

This command shows:
- Alias counts by source (STATIC, fix, pattern)
- Pattern statistics with approval/rejection rates
- Total approved/pending/rejected aliases
- Top rejected alias pairs (if any)

Example output:
```json
{
  "by_source": {
    "STATIC": 2,
    "fix": 8,
    "pattern": 1845
  },
  "negative_matches": 0,
  "pattern_stats": [
    {
      "approved": 37,
      "enabled": true,
      "pattern_id": "pattern",
      "pattern_type": "OTHER",
      "rejected": 0,
      "total": 37
    }
  ],
  "top_rejected_pairs": [],
  "total_approved": 1852,
  "total_pending": 3,
  "total_rejected": 0
}
```

## Reviewing New Aliases

The alias review process follows these steps:

1. **Create a review batch**:
   ```bash
   ./manage_aliases.sh create-review-batch
   ```
   This command generates a batch of pending aliases for review.

2. **Review a specific batch**:
   ```bash
   ./manage_aliases.sh review-batch --batch-id [BATCH_ID]
   ```
   Where `[BATCH_ID]` is the identifier for the batch you want to review.

3. **During the review process**:
   - Each potential alias pair is presented with both text entities shown
   - The source of the match is displayed (e.g., "pattern")
   - A confidence score is shown (e.g., "0.80")
   - You are prompted to:
     - Approve (a): Confirm these are aliases of the same entity
     - Reject (r): Indicate these are different entities
     - Skip (s): Postpone decision for later review

## Testing Entity Matching

To test if two names would match as aliases:

```bash
./manage_aliases.sh test --name1 "Apple Inc." --name2 "Apple Corporation" --entity-type "organization"
```

This command shows whether the two entities would be considered aliases under current matching rules. It displays:
- In-memory alias match result
- Database-backed match result
- The normalized form of each name

## Adding Manual Alias Corrections

To add a manual alias correction:

```bash
./manage_aliases.sh add --canonical "Tim Cook" --alias "Timothy D. Cook" --entity-type "person" --source "fix"
```

This creates a direct alias between the canonical name and the alias, marking it as a "fix" source type. Available entity types are:
- person
- organization (or org)
- product
- location
- event

## Troubleshooting Improper Aliases

If you encounter improper aliases in review batches (such as article content being incorrectly treated as entities), you can:

1. **Always reject improper matches** - Use "r" to reject matches that contain full paragraphs or article content

2. **Check patterns in the alias extraction system** - The system now validates potential aliases to filter out:
   - Entities longer than 100 characters
   - Entities with more than 10 words
   - Text containing common sentence indicators
   - Sentence-like structures with periods

3. **Add manual corrections** - For critical entities that consistently fail to match properly:
   ```bash
   ./manage_aliases.sh add --canonical "Proper Form" --alias "Variant" --entity-type "type" --source "fix"
   ```

## Available Commands

```
migrate              Migrate static aliases to the database
add                  Add a new alias to the system
test                 Test if two entity names match
create-review-batch  Create a batch of aliases for review
review-batch         Review a specific batch of aliases
stats                Display alias system statistics
help                 Print help information
```

## Best Practices

1. **Regular Review Batches**: Process review batches regularly to maintain system accuracy

2. **Careful Evaluation**: Read both entity texts completely before approving to ensure they truly refer to the same entity

3. **Track Statistics**: Monitor the stats command output to identify potential issues with matching algorithms  

4. **Add Manual Corrections**: When important entities consistently fail to match, add manual aliases with the `add` command

5. **Consistency**: Be consistent in your approval/rejection decisions to help train the system
