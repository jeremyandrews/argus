# Argus

Argus is a Rust-based program designed to read RSS feeds, summarize articles based on specified topics, and post these summaries to a Slack channel via webhook.

## Features

- **RSS Feed Parsing:** Reads articles from specified RSS feed URLs.
- **Content Extraction:** Extracts readable content from article URLs.
- **Topic Matching:** Uses a language model to match articles with specified topics.
- **Summary and Analysis:** Generates concise summaries and analyses of matching articles.
- **Slack Integration:** Posts summaries to a Slack channel using a webhook.

## Environment Variables

Configure the program using environment variables. Copy `env.template` to `.env` and edit it as necessary.

## Installation

1. Clone the repository:
    ```sh
    git clone <repository_url>
    cd <repository_directory>
    ```
2. Set up environment variables:
    ```sh
    cp env.template .env
    ```
3. Build and run the program:
    ```sh
    cargo build --release
    cargo run --release
    ```

## Usage

When run, the program will:

1. Parse RSS feeds from specified URLs.
2. Extract and summarize article content.
3. Match content against specified topics.
4. Post summaries and analyses to Slack.

## Dependencies

- `ollama_rs` for integrating with the Ollama language model.
- `readability` for extracting readable content.
- `rss` for parsing RSS feeds.
- `serde_json` for handling JSON data.
- `reqwest` for making HTTP requests.

## Example

To run the program:

1. Set up environment variables in the `.env` file.
2. Execute the program:
    ```sh
    cargo run --release
    ```

You will see output indicating the progress of loading RSS feeds, extracting articles, matching topics, and posting to Slack.

## Troubleshooting

- **RSS Feed Errors:** Ensure RSS feed URLs are correct and accessible.
- **Ollama Service Connection:** Confirm the Ollama service is running and accessible at the specified host and port.
- **Slack Webhook Errors:** Verify the Slack webhook URL is correct and your Slack app has the necessary permissions.

## Contributing

Contributions are welcome! Open an issue or submit a pull request on GitHub.

## License

This project is licensed under the MIT License.