# No-Think Mode for Qwen Models

## Overview

This document describes the "no-think" mode feature implemented for Qwen models in the Argus system. This feature allows Qwen models to bypass their internal reasoning process (displayed in `<think></think>` tags) and provide direct responses.

## Background

Qwen models (particularly Qwen3) have a built-in thinking mode where they show their reasoning process enclosed in `<think></think>` tags before providing a final answer. While this is valuable for understanding the model's reasoning, in some scenarios it may be preferable to get direct responses without the intermediate thinking steps.

## Implementation

The no-think mode is implemented by appending `/no_think` to prompts sent to Qwen models. This is a special instruction recognized by Qwen models, instructing them to skip the thinking tags and provide direct responses.

### Key Components

1. **LLMParams Structure**: Added a `no_think` boolean field to the `LLMParams` struct to control this behavior

2. **Workers Integration**: 
   - Both analysis and decision workers fully support this parameter
   - Common code path for handling thinking configuration through `ProcessItemParams`

3. **Configuration System**: Added support for specifying no_think mode in Ollama configuration strings

4. **Test Utilities**: Enhanced test tools to support testing with no_think mode

### Usage

#### Environment Configuration

In the Ollama configuration strings, you can now specify no_think mode:

```
DECISION_OLLAMA_CONFIGS="localhost|11434|qwen3:32b-a3b-fp16|true"
ANALYSIS_OLLAMA_CONFIGS="localhost|11434|qwen3:32b-a3b-fp16|true"
```

The format is: `host|port|model|no_think`

#### Testing

You can test the no_think mode using the included test utility:

```bash
# Test with no_think mode enabled
cargo run --bin test_thinking_model -- --no-think -P "Your test prompt"

# Compare with normal thinking mode
cargo run --bin test_thinking_model -- -P "Your test prompt"
```

## Behavior with Different Models

- **Qwen Models**: 
  - Will disable the thinking process and return direct responses
  - May return empty `<think></think>` tags in the response, which is expected behavior
  - The system automatically strips these empty tags from the final response
  - Empty tags are logged for debugging but not treated as errors
- **Other Models**: The `/no_think` suffix will be ignored or treated as part of the prompt
  - The system safely handles this by only applying the suffix to models whose names start with "qwen"

## Technical Details

1. When `no_think` is `true`, the system appends `/no_think` to the prompt text before sending it to the LLM

2. Implementation in `test_thinking_model.rs`:
   ```rust
   if args.no_think {
       info!("No-think mode enabled - appending /no_think to prompt");
       request.prompt = format!("{} /no_think", request.prompt);
   }
   ```

3. The system continues to create a `ThinkingModelConfig` even in no_think mode for compatibility, but the model will not use it when the `/no_think` suffix is present

## Compatibility Considerations

- Always check if a model supports the `/no_think` directive before enabling this feature
- Currently, this feature is specific to Qwen models and should only be used with them
- For OpenAI models, the `no_think` parameter is ignored

## Diagnostics

When troubleshooting issues with this feature:

1. Check if the model name starts with "qwen" (case-insensitive)
2. Verify that `/no_think` is correctly appended to the prompt
3. Use the `test_thinking_model` utility with the `--no-think` flag and the `-r` (raw output) flag to see the exact model response
4. Empty `<think></think>` tags are automatically stripped from responses, so you shouldn't see them in the final output

## Future Enhancements

Potential improvements to the no_think feature:

1. Add model-specific handling for other models that might support similar directives
2. Create a global configuration setting for default thinking behavior
3. Implement automatic detection of models that support thinking/no-thinking modes
