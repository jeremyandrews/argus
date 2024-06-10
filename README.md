# Argus

This Rust program reads RSS feeds, extracts and summarizes articles based on specified topics, and posts the summarized content to a Slack channel using a webhook. It utilizes several crates for RSS parsing, readability extraction, and integration with the Ollama language model for text generation.

## Features

- **RSS Feed Parsing:** Reads articles from specified RSS feed URLs.
- **Content Extraction:** Extracts readable content from article URLs.
- **Topic Matching:** Uses a language model to determine if an article matches specified topics.
- **Summary and Analysis:** Generates concise summaries and analyses of matching articles.
- **Slack Integration:** Posts summaries and analyses to a Slack channel using a webhook.

## Environment Variables

The program relies on several environment variables for configuration:

- `URLS`: A semicolon-separated list of RSS feed URLs.
- `OLLAMA_HOST`: The hostname for the Ollama service (default: `localhost`).
- `OLLAMA_PORT`: The port for the Ollama service (default: `11434`).
- `OLLAMA_MODEL`: The model name for the Ollama language model (default: `llama2`).
- `TOPICS`: A semicolon-separated list of topics to match against the articles.
- `SLACK_WEBHOOK_URL`: The webhook URL for posting messages to Slack.

## Installation

1. **Clone the repository:**
    ```sh
    git clone <repository_url>
    cd <repository_directory>
    ```

2. **Set up environment variables:**
    Create a `.env` file in the root directory and add the necessary environment variables:
    ```sh
    URLS="https://example.com/rss1;https://example.com/rss2"
    OLLAMA_HOST="localhost"
    OLLAMA_PORT="11434"
    OLLAMA_MODEL="llama2"
    TOPICS="topic1;topic2"
    SLACK_WEBHOOK_URL="https://hooks.slack.com/services/..."
    ```

3. **Run the program:**
    ```sh
    cargo run --release
    ```

## Usage

Upon running, the program will:

1. Parse the RSS feeds from the specified URLs.
2. Extract and summarize the content of each article.
3. Match the article content against the specified topics.
4. Post the summaries and analyses of matching articles to Slack.

## Dependencies

- `ollama_rs`: For integrating with the Ollama language model.
- `readability`: For extracting readable content from web pages.
- `rss`: For parsing RSS feeds.
- `serde_json`: For handling JSON data.
- `reqwest`: For making HTTP requests.

## Troubleshooting

- **RSS Feed Errors:** Ensure the RSS feed URLs are correct and accessible.
- **Ollama Service Connection:** Check that the Ollama service is running and accessible at the specified host and port.
- **Slack Webhook Errors:** Verify that the Slack webhook URL is correct and that your Slack app has the necessary permissions.

## Contributing

Contributions are welcome! Please open an issue or submit a pull request on GitHub.

## License

This project is licensed under the MIT License.
