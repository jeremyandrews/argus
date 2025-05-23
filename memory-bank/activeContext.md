# Active Development Context

## Current Focus: Enhanced ELI5 Prompt for Language Consistency and Foreign Content

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

## Previous Focus: Added ELI5 Explanation to Article Analysis

We've added a new "Explain Like I'm 5" (ELI5) section to the JSON that is written to R2 for each analyzed article. This feature provides simplified explanations of complex topics in plain language that is accessible to readers with no background knowledge.

### Implementation Details

1. **New Prompt Creation**:
   - Added `eli5_prompt` function in `src/prompt/summarization.rs`
   - Designed the prompt to generate explanations at approximately a US 4th-5th grade reading level (ages 9-11)
   - Included comprehensive guidelines for writing style, content structure, and example formats
   - Added appropriate source attribution handling based on source type ([OFFICIAL], [NEWS], [RUMOR/LEAK], [ANALYSIS])

2. **Module Updates**:
   - Exported the new prompt function in `src/prompt/mod.rs`
   - Updated the `process_analysis` function in `src/workers/analysis/quality.rs` to generate the ELI5 explanation
   - Modified the function's return type to include the new ELI5 field
   - Created the ELI5 explanation when an article has a valid summary

3. **JSON Integration**:
   - Updated both `process_matched_topic_item` and `process_life_safety_item` functions in `src/workers/analysis/processing.rs`
   - Added the ELI5 field to the JSON response for both types of articles
   - Ensured the ELI5 explanation is written to R2 storage along with other analysis fields

4. **Testing**:
   - Created `test_eli5_prompt.rs` for testing the prompt generation and response
   - Created `test_full_analysis.rs` for validating the entire analysis pipeline including ELI5
   - Both tests verify that the ELI5 field is correctly generated and included in analysis results

This enhancement ensures that all processed articles now include a simplified explanation that makes complex news and technical content more accessible to a wider audience.

## Previous Focus: Brotli Dependency Upgrade

We've upgraded the brotli compression library from version 3.4 to 8.0. This dependency is used in the RSS module for decompressing brotli-compressed content from web feeds.

### Implementation Details

1. **Dependency Update**:
   - Updated the brotli crate version in Cargo.toml from "3.4" to "8.0"
   - Verified that the API remained compatible with our usage pattern

2. **Key Components Updated**:
   - The decompression code in `src/rss/fetcher.rs` and `src/rss/test.rs` continues to use the same API:
     ```rust
     let mut reader = brotli::Decompressor::new(&bytes[..], 4096);
     if reader.read_to_end(&mut decoded).is_ok() && decoded.len() > 0 {
         // Process decompressed content
     }
     ```
   - No code changes were required as the API has remained stable

3. **Testing and Verification**:
   - Verified the project builds successfully with `cargo check`
   - Ran the full test suite to ensure compatibility with `cargo test`
   - Updated the Memory Bank documentation to reflect the upgrade

This upgrade ensures we're using the latest version of the brotli library, which may include performance improvements, bug fixes, and security patches.

## Previous Focus: JSON Mode Formatting Bug Fix

We've resolved an issue with JSON mode formatting being incorrectly applied to article sections like tiny titles and summaries. The problem manifested as error messages appearing in the article output:

```
{"error": "The request could not be processed. Please try again later."}
```

### Root Cause Analysis

The issue was caused by JSON mode not being properly reset between LLM calls. We were using a single mutable `LLMParams` object that retained its `json_format` setting across different calls. When entity extraction or threat location detection (which require JSON mode) ran before generating summaries or titles (which require plain text), the JSON mode was still active, causing the LLM to return JSON error responses.

### Implementation of a Type-Driven Solution

1. **Type-Safe Parameter System**:
   - Created specialized parameter types that encode the format in the type itself:
     - `TextLLMParams`: For plain text responses
     - `JsonLLMParams`: For JSON formatted responses with schema
   - Added new API functions:
     - `generate_text_response`: Always uses plain text mode
     - `generate_json_response`: Always uses JSON mode with the specified schema
   - Added conversion functions for backward compatibility

2. **Updated Key Components**:
   - `src/prompt/summarization.rs`: Now receives clean parameters that never use JSON mode
   - `src/entity/extraction.rs`: Uses explicit JSON mode via `JsonLLMParams`
   - `src/workers/decision/threat.rs`: Uses explicit JSON mode for threat location
   - All functions now create fresh parameter objects rather than modifying a shared one

3. **Compiler Enforcement**:
   - The type system now enforces correct format usage throughout the code
   - Impossible to accidentally leave JSON mode enabled between calls
   - Clear documentation of intent in the type signatures

This architecture change prevents JSON format settings from leaking between different LLM calls, providing robust protection against this class of bug in the future.

## Previous Focus: Tiny Title Prompt Redesign

We've completely redesigned the `tiny_title_prompt` function in `src/prompt/summarization.rs` to solve ongoing issues with title accuracy, particularly around the handling of rumors vs. confirmed information. The previous approach was overly complex and still produced incorrect titles in some cases.

### Key Improvements

1. **Complete Prompt Redesign**: 
   - Simplified the prompt structure with clearer, more direct instructions
   - Organized by core principles and practical examples
   - Added explicit format patterns for each source type
   - Emphasized present tense usage for all titles (the western tradition for headlines)

2. **Dual Summary Context**:
   - Modified the function to accept both the tiny summary AND the original summary
   - Gives the title generator access to the explicit source type labels ([OFFICIAL], [NEWS], etc.) from the original summary
   - Clear instruction to base the title primarily on the tiny summary, using the original summary only for context and certainty determination

3. **Simpler Source Type Handling**:
   - Created specific title patterns for each source type:
     - For [OFFICIAL]: "[Entity] [Action Verb] [Object]" (e.g., "Apple Launches New iPad")
     - For [NEWS]: "[Entity] [Action Verb] [Object]" or "[Source] Reports [Event]" (e.g., "WSJ Reports Tesla Layoffs")
     - For [RUMOR/LEAK]: Limited to specific patterns like "Rumored [Subject]" or "[Subject] Reportedly [Verb]"
     - For [ANALYSIS]: Analysis-indicating patterns like "Analysts Predict [Outcome]"

4. **Clear Examples for Each Type**:
   - Added parallel examples showing the transformation from original summary to tiny summary to title
   - Grouped examples by source type for easier reference
   - Included common mistakes to avoid with explicit before/after examples

5. **Code Updates**:
   - Updated function signature: `pub fn tiny_title_prompt(tiny_summary: &str, original_summary: &str)`
   - Modified the calling code in `src/workers/analysis/quality.rs` to pass both summaries

This redesign focuses on capturing what's truly important about a title: summarizing the core event in present tense while clearly indicating the certainty level based on the source type. The new approach should significantly improve title accuracy and consistency.

## Previous Focus: RSS Module Refactoring

We've modularized the `src/rss.rs` file, which had grown too large, into a well-organized directory structure. The module has been split into smaller, focused files to improve maintainability and code organization, following the project's pattern of keeping files under 500 lines.

### Key Improvements

1. **Modular Structure Creation**: 
   - Created a hierarchical directory structure with logical component organization
   - Split functionality into specialized files organized by responsibility
   - Implemented clean separation of concerns between different RSS operations

2. **New Module Organization**:
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

3. **Backward Compatibility**:
   - Updated `lib.rs` to re-export key RSS functions for backward compatibility
   - Preserved all existing APIs so dependent code continues to work
   - Maintained function signatures to ensure consistent behavior

4. **Code Quality Improvements**:
   - Removed unused imports and variables
   - Fixed warning for unused variables
   - Added proper log summary of processed articles
   - Ensured consistent error handling across modules

5. **Testing and Quality Assurance**:
   - Verified all functionality with `cargo test --all`
   - Ensured all tests pass with the new modular structure
   - Confirmed the RSS feed testing tool works correctly
   - Validated real-world functionality

This refactoring follows the same pattern previously applied to workers, prompts, vector, and clustering modules, maintaining a consistent code organization approach throughout the codebase.

## Previous Focus: Clustering Module Refactoring

We've refactored the clustering.rs module, which had grown too large, into a well-organized directory structure. The module has been split into smaller, focused files to improve maintainability and code organization, following the project's pattern of keeping files under 500 words.

### Key Improvements

1. **Modular Structure Creation**: 
   - Created a hierarchical directory structure with logical component organization
   - Split functionality into specialized files (assignment.rs, entities.rs, summary.rs, etc.)
   - Created a nested merging/ directory for all merging-related functionality

2. **Architectural Improvements**:
   - Moved database operations to `db/cluster.rs` following the system's architectural pattern
   - Created proper delegation from clustering modules to database operations
   - Added function stubs in code that will need implementation

3. **Testing and Quality Assurance**:
   - Added simple test module (clustering/tests.rs) to verify exports work correctly
   - Run full test suite to ensure no regressions were introduced
   - Ensured all code compiles without errors or warnings

4. **Backward Compatibility**:
   - Maintained re-exports in clustering/mod.rs for all key functions
   - Preserved all existing APIs so dependent code continues to work
   - Used the same constants and types to ensure consistent behavior

This refactoring makes the codebase more maintainable for future development and follows the same modular pattern already established for the workers and vector modules.

## Next Steps

Future work should continue using the modular approach:

1. When adding new functionality to workers, place it in the appropriate subdirectory
2. Keep files under 500 lines for maintainability
3. Put shared functionality in the common module
4. Consider if other large modules could benefit from similar reorganization
