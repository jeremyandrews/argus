# Argus

Argus is a Rust-based program designed to read RSS feeds, summarize articles based on specified topics, and post these summaries to a Slack channel via webhook.

<img src="https://github.com/jeremyandrews/argus/blob/main/assets/argus-logo.png" alt="Argus Logo" width="200"/>

The name "Argus" is inspired by Argus Panoptes, the all-seeing giant in Greek mythology, reflecting the program's ability to monitor and analyze numerous information sources.

## Features

- **RSS Feed Parsing:** Reads articles from specified RSS feed URLs.
- **Content Extraction:** Extracts readable content from article URLs.
- **Topic Matching:** Uses a language model to match articles with specified topics.
- **Summary and Analysis:** Generates concise summaries and analyses of matching articles.
- **Slack Integration:** Posts summaries to a Slack channel using a webhook.

## Environment Variables

Configure the program using environment variables. Copy `env.template` to `.env` and edit it as necessary. You will need to source the `.env` file to make the variables available to your shell.

### Environment Variables in `env.template`:

- `SLACK_TOKEN`: The OAuth token of the Slack App to send news notifications.
- `SLACK_CHANNEL`: The Slack channel ID to send the notifications to.
- `URLS`: A list of RSS URLs to scrape. Use feeds without access restrictions.
- `TOPICS`: A list of topics to search for and report on.
- `OLLAMA_PORT`: Optionally specify a custom port for the Ollama API.
- `OLLAMA_HOST`: Optionally specify a custom hostname for the Ollama API.
- `OLLAMA_MODEL`: Optionally specify an Ollama model to use.
- `DATABASE_PATH`: Optionally specify a custom path to the SQLite database file. Default is `argus.db`.
- `LLM_TEMPERATURE`: Optionally specify a temperature for the language model. Default is `0.0`.
- `RUST_LOG`: Logging level for the application. Possible values are: `trace`, `debug`, `info`, `warn`, `error`. Default is `info`.

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
3. Source the environment variables:
    ```sh
    source .env
    ```
4. Build and run the program:
    ```sh
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
- `tracing` for logging.
- `tracing-subscriber` for initializing logging.

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
- **Logging Level:** Adjust the `RUST_LOG` environment variable to control the verbosity of logs (`trace`, `debug`, `info`, `warn`, `error`).

## Contributing

Contributions are welcome! Open an issue or submit a pull request on GitHub.

## License

This project is licensed under the MIT License.
