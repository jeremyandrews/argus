# Active Development Context

## Current Focus: Thinking Model for Analysis Workers

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
