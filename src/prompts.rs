const DONT_TELL_ME: &str =
    "Do not tell me what you're doing, do not explain that you're writing in American English.";
const FORMAT_INSTRUCTIONS: &str =
    "Format your answer for easy and clear readability in text. Use _italic_ for italicized text,
     *bold* for bold text, ~strike~ for strikethrough text, > for block quotes, `code` for code
     formatting, and \\n for newlines.";
const WRITE_IN_CLEAR_ENGLISH: &str = "Write in accessible and clear American English.";

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
        "{summary} | Please summarize down to 200 characters or less.

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

pub fn relation_to_topic_prompt(article_text: &str, topic_name: &str) -> String {
    format!( "{article} |
Briefly explain in one or two sentences how this relates to {topic}, starting with 'This relates to {topic}.'.

{write_in_clear_english}

{dont_tell_me}

{format_instructions}",
        article = article_text,
        topic = topic_name,
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
