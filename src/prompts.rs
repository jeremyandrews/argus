use chrono::Local;

const DONT_TELL_ME: &str =
    "Do not tell me what you're doing, do not explain that you're writing in American English.";
const FORMAT_INSTRUCTIONS: &str =
    "To ensure our conversation is easy to follow and understand, use the
following formatting options when they make the text more readable:

### Emphasis and Special Words
Use **bold** text to draw attention to important information, like **key takeaways** or **main points**.
Use _italic_ formatting to indicate a word or phrase is being used in a special way, such as when referring to a _foreign word_ or a _technical term_.

### Quotes and Block Quotes
Use the > block quote formatting to set apart a large section of text or to quote someone, like this:
> This is an example of a block quote, which can be used to set apart a
> large section of text or to quote someone or to quote the article.
Use this format to make it clear that you are referencing someone else's words.

### Code and Technical Terms
Use `code` formatting when specifically referencing code, commands, or specific technical terms, such as when talking about a `programming language` or a `specific software feature` or `function`.

### General Guidelines
Only use special formatting when it makes the text more readable. Avoid excessive formatting, as it can make the text harder to understand. Use these formatting styles to make your text more readable and engaging.

By following these guidelines, you'll be able to create clear and concise text that is easy to understand and engaging to read.";
const WRITE_IN_CLEAR_ENGLISH: &str = "Write in accessible and clear American English.";

const CONTEXT: &str = "
In Q1 2024, BRICS expanded, shifting global economic power, while record temperatures highlighted climate concerns. Japan's 7.6 earthquake and U.S. winter storms exposed vulnerabilities. France enshrined abortion rights, Sweden joined NATO, and the U.S. Supreme Court ruled on key legal precedents. Major wildfires and geopolitical tensions added to global challenges.
In Q2 2024, a solar eclipse captivated North America as record heatwaves and severe floods underscored climate urgency. Trump’s trial and free speech protests stirred U.S. discourse. Putin’s fifth term, Xi's European visit, and G7's $50B Ukraine aid shaped geopolitics. Apple’s AI integration marked tech innovation.
In Q3 2024, the Paris Olympics fostered unity amidst record-breaking heatwaves and escalating Gaza tensions. Biden withdrew from the presidential race, endorsing Kamala Harris. The UN's 'Pact for the Future' and a historic face transplant marked milestones. Hurricane Helene and mpox emphasized urgent global challenges.
In Q4 2024, Trump’s re-election and U.S. economic growth highlighted domestic shifts. Hurricane Helene devastated the Gulf Coast, while 2024 set a record as the hottest year. South Korea’s political turmoil and Assad’s overthrow reshaped global dynamics. The Notre-Dame reopening symbolized cultural resilience.
- In January 2025, Donald Trump is inaugurated as the 47th U.S. President, signaling a major political shift. Los Angeles faces its most destructive wildfires, causing significant damage and loss of life. A European report confirms 2024 as the hottest year on record, emphasizing climate change urgency. Ukraine halts Russian gas transit, affecting European energy dynamics. Canadian Prime Minister Justin Trudeau announces his resignation, indicating impending leadership changes.";

pub fn summary_prompt(article_text: &str) -> String {
    format!(
        "{article} |
Carefully read and thoroughly understand the provided text. Create a comprehensive summary
in bullet points that cover all the main ideas and key points from the entire text, maintains the
original text's structure and flow, and uses clear and concise language. For really short texts
(up to 25 words): simply quote the text, for short texts (up to 100 words): 2-4 bullet points, for
medium-length texts (501-1000 words): 3-5 bullet points, for long texts (1001-2000 words): 4-8
bullet points, and for very long texts (over 2000 words): 6-10 bullet points.

{write_in_clear_english}

{dont_tell_me}

{format_instructions}",
        article = article_text,
        write_in_clear_english = WRITE_IN_CLEAR_ENGLISH,
        dont_tell_me = DONT_TELL_ME,
        format_instructions = FORMAT_INSTRUCTIONS
    )
}

pub fn tiny_summary_prompt(summary_response: &str) -> String {
    format!(
        "{summary} | Please summarize down to 400 characters or less.

{write_in_clear_english}

{dont_tell_me}",
        summary = summary_response,
        write_in_clear_english = WRITE_IN_CLEAR_ENGLISH,
        dont_tell_me = DONT_TELL_ME
    )
}

pub fn tiny_title_prompt(summary_response: &str) -> String {
    format!(
        "{summary} | Please write an informational and accurate 3 to 5 word title for this text.

{write_in_clear_english}

{dont_tell_me}",
        summary = summary_response,
        write_in_clear_english = WRITE_IN_CLEAR_ENGLISH,
        dont_tell_me = DONT_TELL_ME
    )
}

pub fn critical_analysis_prompt(article_text: &str) -> String {
    format!(
        "{article} |
Carefully read and thoroughly understand the provided text.

Provide a credibility score from 1 to 10, where 1 represents highly biased content and 10
represents unbiased content. Explain the score in no more than 15 words.

Provide a style score from 1 to 10, where 1 represents poorly written text and 10 represents
eloquent text. Explain the score in no more than 15 words.

Provide a political weight (Left, Center Left, Center, Center Right, Right, or not applicable).
Explain in no more than 15 words.

Provide a concise two to three sentence critical analysis.

{write_in_clear_english}

{dont_tell_me}

{format_instructions}",
        article = article_text,
        write_in_clear_english = WRITE_IN_CLEAR_ENGLISH,
        dont_tell_me = DONT_TELL_ME,
        format_instructions = FORMAT_INSTRUCTIONS
    )
}

pub fn logical_fallacies_prompt(article_text: &str) -> String {
    format!(
        "{article} |
Carefully read and thoroughly understand the provided text. Explain any biases or logical fallacies
in one or two sentences. If there are none, state that in no more than five words.

Identify the strength of arguments and evidence presented in one or two sentences.

{write_in_clear_english}

{dont_tell_me}

{format_instructions}",
        article = article_text,
        write_in_clear_english = WRITE_IN_CLEAR_ENGLISH,
        dont_tell_me = DONT_TELL_ME,
        format_instructions = FORMAT_INSTRUCTIONS
    )
}

pub fn relation_to_topic_prompt(article_text: &str, topic_prompt: &str) -> String {
    format!( "{article} |
Briefly explain in one or two sentences how this relates to {topic}, starting with 'This relates to {topic}.'.

{write_in_clear_english}

{dont_tell_me}

{format_instructions}",
        article = article_text,
        topic = topic_prompt,
        write_in_clear_english = WRITE_IN_CLEAR_ENGLISH,
        dont_tell_me = DONT_TELL_ME,
        format_instructions = FORMAT_INSTRUCTIONS
    )
}

pub fn how_does_it_affect_prompt(article_text: &str, affected_places: &str) -> String {
    format!(
        "{article} |
How does this article affect the life and safety of people in the following places: {places}?
Answer in no more than two sentences.

{write_in_clear_english}

{dont_tell_me}

{format_instructions}",
        article = article_text,
        places = affected_places,
        write_in_clear_english = WRITE_IN_CLEAR_ENGLISH,
        dont_tell_me = DONT_TELL_ME,
        format_instructions = FORMAT_INSTRUCTIONS
    )
}

pub fn why_not_affect_prompt(article_text: &str, non_affected_places: &str) -> String {
    format!(
        "{article} |
Why does this article not affect the life and safety of people in the following places:
{places}? Answer in no more than two sentences.

{write_in_clear_english}

{dont_tell_me}

{format_instructions}",
        article = article_text,
        places = non_affected_places,
        write_in_clear_english = WRITE_IN_CLEAR_ENGLISH,
        dont_tell_me = DONT_TELL_ME,
        format_instructions = FORMAT_INSTRUCTIONS
    )
}

pub fn source_analysis_prompt(article_html: &str, source_url: &str) -> String {
    // Get today's date
    let today = Local::now();
    let day = today.format("%-d").to_string(); // Day without leading zero
    let month = today.format("%B").to_string(); // Full month name
    let year = today.format("%Y").to_string(); // Full year

    format!(
        " A small sampling of events since your knowledge cutoff in 2022: `{context}` |
Article to review: `{article}` | Source URL: `{source_url}` |
Analyze the source of the article, including if possible its background, ownership, purpose, and notable achievements or controversies. Consider factors such as awards, scandals, and any relevant historical context. Given the current date is {month} {day} {year}, assess whether the content appears to be recent or if there are indications it may be outdated. Please provide your analysis in 2-4 sentences.

{write_in_clear_english}

{dont_tell_me}

{format_instructions}",
        context = CONTEXT,
        article = article_html,
        source_url = source_url,
        day = day,
        month = month,
        year = year,
        write_in_clear_english = WRITE_IN_CLEAR_ENGLISH,
        dont_tell_me = DONT_TELL_ME,
        format_instructions = FORMAT_INSTRUCTIONS
    )
}

/** The following prompts expect a 'yes' or 'no' answer. */

pub fn threat_prompt(article_text: &str) -> String {
    format!(
        "{article} |
Is this article about any ongoing or imminent life-threatening event or situation? Answer yes or no.",
        article = article_text
    )
}

pub fn continent_threat_prompt(article_text: &str, continent: &str) -> String {
    format!("{article} |
Is this article about an ongoing or imminent life-threatening event affecting people on the continent
 of {continent}? Answer yes or no.",
        article = article_text,
        continent = continent
    )
}

pub fn country_threat_prompt(article_text: &str, country: &str, continent: &str) -> String {
    format!(
        "{article} |
Is this article about an ongoing or imminent life-threatening event affecting people in {country} on
the continent of {continent}? Answer yes or no.",
        article = article_text,
        country = country,
        continent = continent
    )
}

pub fn region_threat_prompt(
    article_text: &str,
    region: &str,
    country: &str,
    continent: &str,
) -> String {
    format!("{article} |
Is this article about an ongoing or imminent life-threatening event affecting people in the region of
{region}, {country}, {continent}? Answer yes or no.",
        article = article_text,
        region = region,
        country = country,
        continent = continent
    )
}

pub fn city_threat_prompt(
    article_text: &str,
    city_name: &str,
    region: &str,
    country: &str,
    continent: &str,
) -> String {
    format!(
        "{article} |
Is this article about an ongoing or imminent life-threatening event affecting people in or near the
city of {city}, {region}, {country}, {continent}? Answer yes or no.",
        article = article_text,
        city = city_name,
        region = region,
        country = country,
        continent = continent
    )
}

pub fn confirm_prompt(summary_response: &str, topic_name: &str) -> String {
    format!(
        "{summary} |
Is this article really about {topic} with enough content to analyze, and not a promotion or advertisement? Answer yes or no.",
        summary = summary_response,
        topic = topic_name
    )
}

pub fn is_this_about(article_text: &str, topic_name: &str) -> String {
    format!(
        "{article} |
Is this article specifically about {topic} with enough content to analyze? Answer yes or no.",
        article = article_text,
        topic = topic_name
    )
}
