# Active Development Context

## Current Focus: JSON Mode Formatting Bug Fix

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

## Previous Focus: Entity Normalizer Improvements

We've fixed issues in the entity normalizer module that were causing test failures. The primary focus was on improving the organization acronym matching logic to handle common patterns like "NASA" matching "NASA Goddard Space Flight Center", while preventing incorrect matches like "Space" and "SpaceX". 

### Key Improvements

1. **Enhanced Organization Acronym Handling**:
   - Improved detection of acronyms in organization names using both normalized and original formats
   - Added explicit support for acronyms at the beginning of longer names (e.g., "NASA Goddard Space Flight Center")
   - Implemented handling for acronyms that represent the initials of the longer name
   - Fixed false positive matches between partial organization names (e.g., "Space" vs "SpaceX")

2. **Testing and Quality Assurance**:
   - Fixed failing tests in test_substring_matching, test_stemming, and test_levenshtein_distance
   - Ensured all test cases properly handle specialized entity matching rules
   - Added special cases to prevent plurals matching singular forms for person entities
   - Maintained the distinction between product names from the same manufacturer (e.g., "Microsoft Windows" vs "Microsoft Office")

3. **Process Improvements**:
   - Added a firm development rule to always run `cargo check` and `cargo test` after making changes
   - Updated the memory-bank's .clinerules with this requirement to prevent future regressions
   - Enforced the practice of fixing failing tests immediately before continuing development

The implementation takes a more general approach to entity matching rules rather than special-casing specific entity names, which will better scale to the wide variety of entities the system encounters.

## Previous Focus: Article Clustering Fixes & Enhancements

We've implemented and fixed a robust article clustering system that automatically groups related articles to provide better context and enable more meaningful analysis. This enhancement allows the system to identify and present articles discussing the same topics or events.

Recent improvements include:
1. Fixed Ollama client initialization in vector module to use the new API requiring separate host and port parameters
2. Added static MODEL and TOKENIZER variables for consistent embedding model access
3. Fixed manage_clusters CLI tool with proper table formatting and command-line argument handling
4. Updated logging functions to properly handle unused parameters

### Implementation Details

1. **Core Clustering Functionality**:
   - Added automatic cluster assignment in the analysis worker pipeline
   - Implemented entity-based similarity for more accurate clustering
   - Created cluster merging capabilities to consolidate related clusters
   - Added cluster summary generation using the LLM
   - Implemented importance scoring for clusters based on recency, article count, and quality

2. **Database Schema Updates**:
   - Added `article_clusters` table for storing cluster metadata
   - Added `article_cluster_mappings` for many-to-many relationships
   - Added `cluster_merge_history` to track cluster merges
   - Implemented `cluster_schema.sql` and `cluster_merge_schema.sql` migrations

3. **API Integration**:
   - Added `/clusters/sync` endpoint for client synchronization
   - Implemented delta updates to minimize data transfer
   - Created structured response format for cluster data
   - Added support for tracking cluster changes (updates, merges, deletions)

4. **CLI Management Tool**:
   - Created `manage_clusters` CLI utility for administrative operations
   - Implemented commands for listing, showing, finding merge candidates, and merging clusters
   - Added summary regeneration functionality
   - Provided detailed cluster information views with entity and article data

5. **Architecture Improvements**:
   - Modularized database functions in `db/cluster.rs`
   - Separated clustering logic from DB operations
   - Added helper function in vector module for LLM client access
   - Maintained separation of concerns between API and data layers

### Using the Clustering System

Clustering happens automatically during article processing. The system:

1. Extracts entities from each article
2. Finds the best matching cluster based on entity overlap
3. Creates a new cluster if no good match is found
4. Generates a summary for the cluster using the LLM
5. Calculates an importance score to prioritize clusters
6. Periodically checks for similar clusters that should be merged

The system supports both automatic and manual cluster management:

- Automatic clustering during normal processing
- Automatic detection and merging of similar clusters
- Manual review and management through the CLI tool
- Client synchronization through the API endpoint

Example CLI usage:
```bash
# List recent clusters
cargo run --bin manage_clusters -- list

# Show details for a specific cluster
cargo run --bin manage_clusters -- show 123 --articles

# Find clusters that could be merged
cargo run --bin manage_clusters -- find-merge-candidates --threshold 0.7

# Merge multiple clusters
cargo run --bin manage_clusters -- merge 123 456 --reason "Same event coverage"

# Regenerate a cluster summary
cargo run --bin manage_clusters -- regenerate-summary 123
```

## Previous Focus: Thinking Model for Analysis Workers

We've implemented a new feature that allows one analysis worker to use a thinking/reasoning model. This enhancement enables more detailed and transparent analysis of articles by using a model that shows its reasoning process before providing a final answer.

### Implementation Details

1. **Model Configuration**: We're using the `qwen3:30b-a3b-fp16` model with specific generation parameters:
   - Temperature = 0.6
   - TopP = 0.95
   - TopK = 20
   - MinP = 0.0 (not supported in the current ollama-rs version but included for future compatibility)

2. **LLM Integration Updates**:
   - Added a `ThinkingModelConfig` struct to `lib.rs` to support reasoning models
   - Enhanced `LLMParams` with a `thinking_config` field to indicate when thinking mode is active
   - Implemented regex-based functionality to strip `<think>...</think>` tags from responses
   - Updated the Ollama and OpenAI LLM clients to handle thinking model parameters

3. **Worker Updates**:
   - Modified `analysis_loop` in `workers/analysis/worker_loop.rs` to accept a `thinking_config` parameter
   - Updated main.rs to configure the first analysis worker to use the thinking model
   - Added appropriate logging for thinking model operations

4. **Testing Infrastructure**:
   - Created a new `test_thinking_model.rs` binary for testing the thinking model capabilities
   - Added comprehensive error logging for thinking model testing

### Using the Thinking Model

Previously, only the first analysis worker (ID = 1) used the thinking model. We've now implemented a global switch to enable reasoning mode for all analysis workers:

- Set the `USE_REASONING_MODELS` environment variable to `true` to enable thinking mode for ALL analysis workers
- Set it to `false` or leave it unset to disable thinking mode for ALL analysis workers

When enabled, all analysis workers will use these recommended parameters:
- Temperature = 0.6
- TopP = 0.95
- TopK = 20
- MinP = 0.0

These settings follow the recommended configuration for thinking models and avoid greedy decoding, which can lead to performance degradation and repetitions.

The system strips out the thinking process (content in `<think>...</think>` tags) before using the response, and thinking mode is not used in fallback mode (when an analysis worker acts as a decision worker).

Example usage:
```bash
USE_REASONING_MODELS=true ./run.sh
```

#### Implementation Details

The reasoning model feature is implemented with consistent behavior across all workers:

1. The `USE_REASONING_MODELS` environment variable is checked at startup
2. When enabled, each analysis worker initializes a `ThinkingModelConfig` with the recommended parameters
3. Each worker uses the same temperature (0.6) and thinking parameters
4. Appropriate logging indicates when a worker is using reasoning mode

### Test Utility

Use the new test utility to verify the thinking model functionality:

```bash
# Standard usage (shows only the processed response)
cargo run --bin test_thinking_model -- -H localhost -p 11434 -m qwen3:30b-a3b-fp16 -P "Your test prompt" -T 0.6

# View the raw response with thinking tags and the processed response for comparison
cargo run --bin test_thinking_model -- -H localhost -p 11434 -m qwen3:30b-a3b-fp16 -P "Your test prompt" -T 0.6 -r

# Test with JSON formatting (simple generic JSON)
cargo run --bin test_thinking_model -- -H localhost -p 11434 -m qwen3:30b-a3b-fp16 -j -P "Extract people and organizations from this text: 'Apple CEO Tim Cook spoke at the event in San Francisco.'"

# Test with specific JSON schema (entity extraction)
cargo run --bin test_thinking_model -- -H localhost -p 11434 -m qwen3:30b-a3b-fp16 -j -s entity -P "Extract all entities from this article: 'Apple announced its new product in Cupertino yesterday.'"

# Test with threat location schema
cargo run --bin test_thinking_model -- -H localhost -p 11434 -m qwen3:30b-a3b-fp16 -j -s threat -P "Analyze this article for impacted regions: 'The hurricane warning affects coastal areas in Florida.'"

# Combine raw mode with JSON (to see the thinking process for structured outputs)
cargo run --bin test_thinking_model -- -H localhost -p 11434 -m qwen3:30b-a3b-fp16 -j -s entity -r -P "Extract all entities from this text: 'Microsoft and Google announced a partnership.'"
```

Note the usage of `-H` for host, `-p` for port, `-P` for prompt, and `-T` for temperature to avoid command-line argument conflicts. The optional `-r` flag will show the raw response with all thinking tags intact, followed by the processed response with tags stripped.

This will run a simple sentiment analysis test with the thinking model and show the result after stripping thinking tags.

## Previous Focus: Vector Module Reorganization

We've just completed modularizing the `vector.rs` file, which was over 1,200 lines. Following the pattern established with the workers module, we've created a directory-based module structure to improve maintainability and readability.

### New Structure

The vector module is now organized as follows:

```
src/vector/
├── mod.rs           # Main exports, shared constants, and globals
├── config.rs        # Configuration and model initialization
├── embedding.rs     # Text to vector conversion
├── storage.rs       # Vector storage in Qdrant
├── similarity.rs    # Similarity calculation between vectors
├── search.rs        # Article search functions
└── types.rs         # Shared data structures
```

Each file now has a focused responsibility and stays under 500 lines for better maintainability.

## Previous Focus: Workers Module Reorganization

We previously completed a significant reorganization of the worker system, creating a modular structure that's more maintainable and easier to navigate. This reorganization was driven by the need to keep individual files under 500 lines for better maintainability and code organization.

### New Structure

The workers are now organized as follows:

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

### Key Changes

1. **Modular Design**: Both workers are now split into specialized modules with clear responsibilities
2. **Backward Compatibility**: The lib.rs file re-exports the worker modules to maintain backward compatibility with existing code
3. **Common Functionality**: Shared code between workers is now in a central `common.rs` file
4. **Size Reduction**: Each file is now smaller and focused on a specific task, making code easier to understand and maintain

### Using the New Structure

- For new code, import directly from the new module structure: `use argus::workers::...`
- Existing code continues to work through re-exports in lib.rs: `use argus::analysis_worker`
- The main worker loop functions remain the entry points for spawning workers

### Related Changes

- Updated the prompt module structure to support the new worker organization
- Fixed minor issues and warnings throughout the codebase
- Ensured all binary tools continue to work with the new structure

## Next Steps

Future work should continue using the modular approach:

1. When adding new functionality to workers, place it in the appropriate subdirectory
2. Keep files under 500 lines for maintainability
3. Put shared functionality in the common module
4. Consider if other large modules could benefit from similar reorganization
