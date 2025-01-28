# Argus

Argus is a Rust-based artificial intelligence (AI) agent designed to monitor and analyze numerous information sources. As an "AI agent", Argus performs tasks autonomously, making decisions based on the data it receives.

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
- `PLACES_JSON_PATH`: Optionally specify the path to a JSON file with place information.
- `RUST_LOG`: Logging level for the application. Possible values are: `trace`, `debug`, `info`, `warn`, `error`. Default is `info`.

## Place-Specific Analysis

To enable place-specific analysis, you need to create a JSON file that includes information about the continents, countries, and cities you want to monitor. Here’s how to set it up:

1. **Copy the Template:**
    ```sh
    cp places.json.template places.json
    ```

2. **Edit `places.json`:** Update the JSON file with your data. The structure should be as follows:
    ```json
    {
        "Continent": {
            "Country": [
                "First name, Last name, City, Country, Time Zone, Slack ID"
            ],
            "Another Country": [
                "First name, Last name, City, Another Country, Time Zone, Slack ID"
            ]
        },
        "Another Continent": {
            "Country": [
                "First name, Last name, City, Country, Time Zone, Slack ID"
            ]
        }
    }
    ```

3. **Example `places.json`:**
    ```json
        {
        "Africa": {
            "Nigeria": [
                "Chinwe, Okoro, Lagos, Nigeria, WAT (UTC+1), Chinwe O"
            ],
            "Egypt": [
                "Omar, Farouk, Cairo, Egypt, EET (UTC+2), Omar F"
            ]
        },
        "Asia": {
            "Japan": [
                "Yuki, Nakamura, Tokyo, Japan, JST (UTC+9), YukiN"
            ],
            "Vietnam": [
                "Linh, Tran, Hanoi, Vietnam, ICT (UTC+7), Linh Tran"
            ]
        },
        "Europe": {
            "Spain": [
                "Carlos, Ruiz, Madrid, Spain, CET (UTC+1), carlosr"
            ],
            "France": [
                "Marie, Dubois, Paris, France, CET (UTC+1), maried"
            ]
        },
        "North America": {
            "Canada": [
                "Alex, Johnson, Toronto, Canada, EST (UTC-5), AlexJ",
                "Sarah, Wong, Vancouver, Canada, PST (UTC-8), SarahW"
            ]
        }
    }
    ```

4. **Set the Environment Variable:**
    ```sh
    export PLACES_JSON_PATH="places.json"
    ```

### How It Works

When the `PLACES_JSON_PATH` environment variable is set, the program will:

1. Load the JSON structure from the specified file into memory.
2. For each relevant continent, ask the language model, "Is this a current event directly affecting people living on the continent of <CONTINENT>? Answer yes or no."
3. If the answer is "yes," it will loop through the countries in that continent and ask, "Is this a current event directly affecting people living in the country of <COUNTRY> on <CONTINENT>? Answer yes or no."
4. If the answer is "yes," it will loop through the cities in that country and ask, "Is this a current event directly affecting people living in or near the city of <CITY> in the country of <COUNTRY> on <CONTINENT>? Answer yes or no."
5. If the answer is "yes," it will add all people in that city to a list of affected individuals.
6. After checking all relevant places, the program will post the article to Slack and include a summary of all affected people in the format: "This article affects: FIRST LAST (SLACK HANDLE), FIRST LAST (SLACK HANDLE), ..."

By setting up and configuring the `places.json` file, you can tailor the analysis to focus on specific regions and ensure that relevant individuals are notified about articles that directly impact them.

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

## Logging

Argus uses the `tracing` crate for logging with two log layers: one for stdout and one for log files.

- **Stdout Log Layer:**
  - Logs at the `info` level and above.
  - Excludes `llm_request` debug logs.

- **File Log Layer:**
  - Logs at the `debug` level for `llm_request`.
  - Logs at the `info` level for other logs.
  - Logs are stored in `logs/app.log`.

### Example Logging Configuration in `main.rs`

```rust
let stdout_log = fmt::layer()
    .with_writer(io::stdout)
    .with_filter(EnvFilter::new("info,llm_request=off"));

let file_appender = rolling::daily("logs", "app.log");
let file_log = fmt::layer()
    .with_writer(file_appender)
    .with_filter(EnvFilter::new("web_request=info,llm_request=debug,info"));

Registry::default().with(stdout_log).with(file_log).init();
```

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
