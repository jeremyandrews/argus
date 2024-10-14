// prompts.rs

pub fn summary_prompt(article_text: &str) -> String {
    format!(
        "{} | Carefully read and thoroughly understand the provided text. Create a comprehensive summary
in bullet points in American English that cover all the main ideas and key points from the
entire text, maintains the original text's structure and flow, and uses clear and concise
language. For really short texts (up to 25 words): simply quote the text, for short texts (up to
100 words): 2-4 bullet points, for medium-length texts (501-1000 words): 3-5 bullet points, for
long texts (1001-2000 words): 4-8 bullet points, and for very long texts (over 2000 words): 6-10
bullet points.

Do not tell me what you're doing, do not explain that you're summarizing in American English.

Format your answer for easy and clear readability in text, _italic_ will produce italicized text,
*bold* will produce bold text, ~strike~ will produce strikethrough text, > at the beginning of one
or more lines will generate a block quote, `surrounding text` will format it as code, and \\n will
generate a newline.",
        article_text
    )
}

pub fn tiny_summary_prompt(summary_response: &str) -> String {
    format!(
        "{} | Please summarize down to 200 characters or less. Write in American English.

Do not tell me what you're doing, do not explain that you're limiting yourself to 200 characters or
that you're writing in American English.",
        summary_response
    )
}

pub fn critical_analysis_prompt(article_text: &str) -> String {
    format!(
        "{} | Carefully read and thoroughly understand the provided text.

Please provide a credibility score from 1 to 10, where 1 represents highly biased or fallacious
content, and 10 represents unbiased, logically sound content. On the next line explain the score
in no more than 10 words.

Then please provide a style score from 1 to 10, where 1 represents very poorly written text, and
10 represents eloquent and understandable text. On the next line explain the score in no more than
10 words.

Then please provide a political weight that is either Left, Center Left, Center, Center Right,
Right, or not applicable. On the next line explain the score in no more than 10 words.

Then please provide a concise two to three sentence critical analysis of the text in American English.

Do not tell me what you're doing, do not explain that you're writing in American English.

Format your answer for easy and clear readability in text, _italic_ will produce italicized text,
*bold* will produce bold text, ~strike~ will produce strikethrough text, > at the beginning of one
or more lines will generate a block quote, `surrounding text` will format it as code, and \\n will
generate a newline.",
        article_text
    )
}

pub fn logical_fallacies_prompt(article_text: &str) -> String {
    format!(
        "{} | Carefully read and thoroughly understand the provided text. If there are biases (e.g.,
confirmation bias, selection bias), logical fallacies (e.g., ad hominem, straw man, false
dichotomy), please explain in one or two short sentences. If there are none, state that in no
more than five words.

Then, with a maximum of one or two short sentences, identify the strength of arguments and
evidence presented.

Write in accessible and clear American English.

Do not tell me what you're doing, do not explain that you're writing in American English.

Format your answer for easy and clear readability in text, _italic_ will produce italicized text,
*bold* will produce bold text, ~strike~ will produce strikethrough text, > at the beginning of one
or more lines will generate a block quote, `surrounding text` will format it as code, and \\n will
generate a newline.",
        article_text
    )
}

pub fn relation_to_topic_prompt(article_text: &str, topic_name: &str) -> String {
    format!(
        "{} | Briefly explain in one or two short sentences how this relates to {}, starting with the
words 'This relates to {}.'

Write in accessible and clear American English.

Do not tell me what you're doing, do not explain that you're writing in American English.

Format your answer for easy and clear readability in text, _italic_ will produce italicized text,
*bold* will produce bold text, ~strike~ will produce strikethrough text, > at the beginning of one
or more lines will generate a block quote, `surrounding text` will format it as code, and \\n will
generate a newline.",
        article_text, topic_name, topic_name
    )
}

pub fn confirm_prompt(summary_response: &str, topic_name: &str) -> String {
    format!(
        "{} | Is this article really about {} and not a promotion or advertisement? Answer yes or no.",
        summary_response, topic_name
    )
}

pub fn yes_no_prompt(article_text: &str, topic_name: &str) -> String {
    format!(
        "{} | Is this article specifically about {}? Answer yes or no.",
        article_text, topic_name
    )
}

pub fn threat_prompt(article_text: &str) -> String {
    format!(
        "{} | Is this article about any ongoing or imminent and potentially life-threatening event or situation? Answer yes or no.",
        article_text
    )
}

pub fn how_does_it_affect_prompt(article_text: &str, affected_places: &str) -> String {
    format!(
        "{} | How does this article affect the life and safety of people living in the following
places: {}? Answer in no more than two sentences in American English.

Do not tell me what you're doing, do not explain that you're writing in American English.

Format your answer for easy and clear readability in text, _italic_ will produce italicized text,
*bold* will produce bold text, ~strike~ will produce strikethrough text, > at the beginning of one
or more lines will generate a block quote, `surrounding text` will format it as code, and \\n will
generate a newline.",
        article_text, affected_places
    )
}

pub fn why_not_affect_prompt(article_text: &str, non_affected_places: &str) -> String {
    format!(
        "{} | Why does this article not affect the life and safety of people living in the following places: {}? Answer in a few sentences.",
        article_text, non_affected_places
    )
}
