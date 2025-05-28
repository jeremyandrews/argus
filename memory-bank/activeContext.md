# Active Development Context

## Current Focus: Analysis Worker Freeze Protection

We've implemented timeout protections and enhanced monitoring for the analysis worker to prevent it from freezing indefinitely. This addresses an issue where the analysis workers would stop processing news articles after running for extended periods (12+ hours) without any visible panics.

### Key Improvements

1. **Database Operation Timeouts**:
   - Added 30-second timeouts to queue fetch operations (`fetch_and_delete_from_life_safety_queue`, `fetch_and_delete_from_matched_topics_queue`)
   - Added 10-second timeouts to hash existence checks (`has_hash`, `has_title_domain_hash`)
   - Added 30-second timeouts to article save operations (`add_article`)
   - Added 30-second timeout to entity processing (`process_entity_extraction`)
   - All timeouts use standard `tokio::time::timeout` for consistency

2. **Enhanced Error Handling**:
   - Timeout errors are now logged with detailed worker information
   - Database errors are caught and logged separately from timeout errors
   - Operations continue gracefully after timeout failures instead of hanging

3. **Worker Health Monitoring**:
   - Added heartbeat logging every 30 seconds showing:
     - Worker ID and model
     - Time since last activity
     - Current operating mode (Analysis or FallbackDecision)
   - Added tracing spans around major operations for better observability

4. **Standardized Timeout Error Messages**:
   - All timeout errors now follow the pattern: `[TIMEOUT] <operation_type> timed out after <duration>s: <specific_function>`
   - This makes it easy to search logs for `[TIMEOUT]` to find all timeout occurrences
   - Each message includes the exact operation, timeout duration, and function name for precise debugging

5. **Complete List of Timeout Messages**:
   - Database operations: `fetch_and_delete_from_life_safety_queue` (30s), `fetch_and_delete_from_matched_topics_queue` (30s), `has_hash` (10s), `has_title_domain_hash` (10s), `add_article` (30s), `process_entity_extraction` (30s)
   - LLM operations: `ollama.generate` (120s), `openai.completions.create` (120s)

### Implementation Details

The changes primarily affect:
- `src/workers/analysis/processing.rs`: Added timeouts to all database operations
- `src/workers/analysis/worker_loop.rs`: Added heartbeat logging and tracing spans
- `src/workers/analysis/similarity.rs`: Added timeout to entity extraction processing
- `src/llm.rs`: Standardized timeout error messages for LLM operations

### Thinking Tag Processing Status

The thinking tag functionality (including `/no_think` mode) is properly protected:
- LLM calls have 120-second timeouts (already existed before these changes)
- Tag stripping is just regex-based string processing (no network/blocking operations)
- No infinite loops or blocking operations in the thinking tag handling

These improvements ensure that:
- No database operation can hang indefinitely
- Worker health is continuously monitored
- Issues are clearly logged with context
- The system degrades gracefully rather than freezing
- Timeout errors can be easily searched and analyzed

## Previous Focus: Enhanced ELI5 Prompt for Language Consistency and Foreign Content

We've improved the ELI5 (Explain Like I'm 5) prompt to ensure consistent use of American English and proper handling of foreign language content. This enhancement addresses an issue where the ELI5 section was sometimes being written in a foreign language despite the presence of language standards in the common prompt helpers.

### Key Improvements

1. **Added Dedicated "Language Requirements" Section**:
   - Added explicit instructions to ALWAYS write the entire explanation in clear American English
   - Included specific guidelines for handling non-English text in articles
   - Added instructions to mentally translate foreign language content before creating the explanation
   - Specified proper handling of direct quotes from foreign languages (include original with translation)
   - Added clear examples of proper foreign language handling

2. **Strengthened Language Instructions**:
   - Added "ALWAYS write your explanation in clear American English" to the top-level instructions
   - Added "NEVER write your explanation in any language other than English" as a critical requirement
   - Included guidance on using American spelling and grammar conventions throughout
   - Added instructions for handling measurements (include both metric and imperial units)
   - Specified to avoid region-specific idioms or expressions

3. **Added Foreign Language Examples to Avoid**:
   - Example 9: Foreign Language Response - "El nuevo descubrimiento científico permite editar genes con mayor precisión..." (showing a completely non-English response)
   - Example 10: Mixed Language - "Scientists discovered a new way to edit genes that's molto preciso (very precise) and will help cure diseases." (showing inappropriate mixing of languages)

4. **Enhanced Content Structure Guidelines**:
   - Added specific instruction for articles in languages other than English: "For articles in languages other than English, maintain the same structure but base your explanation on your translation"
   - Ensured all existing ELI5 functionality is preserved while adding the language requirements

These enhancements ensure that all ELI5 explanations are consistently written in American English, regardless of the source article's language, while maintaining proper attribution and handling of any foreign language content that needs to be directly quoted.

## Previous Focus: Enhanced ELI5 Prompt for Sensitive Political Topics

We've improved the ELI5 (Explain Like I'm 5) prompt to better handle sensitive political topics, particularly those related to policies that affect human rights, civil liberties, or vulnerable populations. This enhancement addresses an issue where the simplification process could inadvertently downplay the seriousness of certain actions or policies.

### Key Improvements

1. **Added "Handling Sensitive Topics" Section**:
   - Added specific guidelines for explaining policies that affect human rights, civil liberties, or vulnerable populations
   - Included instructions to maintain appropriate moral framing even in simplified language
   - Emphasized never minimizing the real-world impact of policies on affected people
   - Added guidance to explain consequences in concrete terms without downplaying severity
   - Instructed to avoid euphemisms that obscure the nature of harmful policies

2. **Added Political Examples**:
   - Added Example 4: Immigration Enforcement Policy - demonstrates how to explain family separation policies with appropriate context and moral framing
   - Added Example 5: Executive Order on Civil Liberties - shows how to explain surveillance policies while presenting both rationale and concerns

3. **Added Unsuccessful Examples to Avoid**:
   - Example 6: Minimizing Impact - "The President made a rule that some people can't come into the country anymore. Some people were sad about it, but the President said it would keep everyone safer."
   - Example 7: False Equivalence - "Some people think the policy is good, and some think it's bad. Both sides have good points, so it's just a matter of opinion."
   - Example 8: Euphemistic Language - "The government decided to relocate certain individuals to specialized facilities while their cases were being processed." (instead of clearly explaining detention or deportation)

These enhancements ensure that when simplifying complex political topics, the ELI5 explanations maintain appropriate moral framing, don't minimize impacts, present multiple perspectives accurately, and use precise language that doesn't obscure the nature of controversial policies.
