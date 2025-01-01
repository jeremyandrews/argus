use chrono::Local;

const DONT_TELL_ME: &str =
    "Do not tell me what you're doing, do not explain that you're writing in American English.";
/*
"To ensure easy readability, let's use the following formatting options:

* _Italic_ for emphasis or special usage, like a _foreign word_.
* *Bold* for importance, such as *key takeaways*.
* ~Strike~ for indicating something is no longer relevant, like a ~deleted
option~.
* > for block quotes, like this:
> This sets apart a large section of text or quotes someone.
* `Code` for technical terms, commands, or code, such as a `programming
language`.

By using these formats, we can make our conversation more engaging and
easy to follow.";
*/
const FORMAT_INSTRUCTIONS: &str =
    "To ensure that our conversation is easy to follow and understand, use the 
following formatting options when it make the text more readable:

When appropriate, add emphasis or indicate that a word or phrase is being 
used in a special way by using _italic_ formatting, such as when referring to 
a _foreign word_ or a _technical term_. Use *bold* text to draw attention 
to important information, like *key takeaways* or *main points*. If you 
need to indicate that something is no longer relevant or has been 
corrected, use ~strike~ formatting, such as when showing a ~deleted 
option~ or a ~revised estimate~. Only do so when it makes the text easier
to understand.

For quoting someone or setting apart a large section of text when it makes
your writing more reasable, use the > block quote formatting, like this:
> This is an example of a block quote, which can be used to set apart a 
> large section of text or to quote someone or to quote the article. Use
> this format to make it clear that you are referencing someone else's words.
> Only do this if you need to quote someone to make your writing more
> accessible.

Finally, when specifically referencing code, commands, or specific
 technical terms, use `code` formatting, such as when talking about a
`programming language` or a `specific software feature` or `function`.
Only use if it makes your writing more readable.

Use these formatting styles to make your text more readable and engaging. Only
use special formatting when it makes the text more readable.";
const WRITE_IN_CLEAR_ENGLISH: &str = "Write in accessible and clear American English.";

const CONTEXT_2024: &str = "
- In January 2024, BRICS expanded, adding five nations and shifting global economic power. Record global temperatures highlighted the escalating climate crisis. A Supreme Court ruling on Trump’s immunity shaped U.S. legal precedents. Japan faced devastation from a 7.6 earthquake, and a U.S. winter storm exposed energy vulnerabilities.
- In February 2024, significant events included devastating wildfires in Chile, the resignation of Hungary's president amid controversy, a deadly Israeli airstrike in Rafah, a severe U.S. winter storm disrupting energy production, and U.S. airstrikes targeting Iranian facilities in Iraq and Syria.
- In March 2024, France enshrined abortion rights in its constitution, Sweden joined NATO, the U.S. Supreme Court ruled on ballot access for federal candidates, LeBron James reached 40,000 NBA points, and a Massachusetts Air National Guard member pleaded guilty to leaking security secrets.
- In April 2024, a total solar eclipse united North America in awe. Concurrently, severe weather events highlighted the pressing issue of climate change. U.S. campuses saw pro-Palestinian protests, igniting debates on free speech. Donald Trump's criminal trial marked a historic legal moment, while UK local elections influenced political dynamics.
- In May 2024, Vladimir Putin began his fifth term as Russian President; Chinese Premier Xi Jinping visited Europe, marking a significant diplomatic engagement; Israeli forces seized Gaza's Rafah crossing; Stormy Daniels testified in a trial involving Donald Trump; and severe floods in Brazil's Rio Grande do Sul state resulted in substantial loss of life and property.
- In June 2024, the world faced record-breaking heatwaves, underscoring the urgent need for climate action. The 80th anniversary of D-Day was commemorated, honoring World War II sacrifices. The US Supreme Court's decision on homelessness influenced national policies. Apple announced a partnership with OpenAI to integrate generative AI into its devices, marking a significant technological advancement. G7 leaders committed $50 billion to support Ukraine, reflecting global geopolitical dynamics.
- In July 2024, the Paris Olympics united nations in athletic competition; record-breaking heatwaves highlighted the climate crisis; President Biden withdrew from the US presidential race, endorsing Kamala Harris; the UK experienced a political shift with Labour's victory; and Israeli airstrikes in Gaza escalated regional tensions.
- In August 2024, Earth experienced its hottest month on record, highlighting the urgent climate crisis. The Paris Olympics showcased global athletic talent, fostering unity. Financial markets faced volatility, with the IMF cautioning about future instability. The WHO declared mpox a public health emergency, urging international action. Escalating conflict in Gaza raised humanitarian and geopolitical concerns.
- In September 2024, the UN's Summit of the Future led to a pivotal 'Pact for the Future.' A groundbreaking whole-eye and face transplant was declared successful. Hurricane Helene's unprecedented impact highlighted climate challenges. The Paris Olympics concluded, celebrated for inclusivity. Apple unveiled new products, influencing global technology trends.
- In October 2024, the U.S. presidential campaign intensified ahead of the November election. Globally, the anniversary of the October 7 Hamas attack prompted widespread commemorations. Claudia Sheinbaum was inaugurated as Mexico's first female president. The BRICS summit in Russia focused on economic cooperation among emerging economies. Domestically, Hurricane Helene caused significant damage along the U.S. Gulf Coast, leading to extensive federal disaster response efforts.
- In November 2024, Donald Trump's re-election and the Republican Party's regained House majority signaled potential policy shifts. Global markets reacted with strengthened U.S. stocks and dollar values. North Korea's deployment of over 10,000 troops to support Russia in Ukraine escalated the conflict. Additionally, 2024 is projected to be the hottest year on record, exceeding a 1.5°C temperature rise since pre-industrial times, underscoring the urgent need to address climate change.";

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

pub fn source_analysis_prompt(article_html: &str, source_url: &str) -> String {
    // Get today's date
    let today = Local::now();
    let day = today.format("%-d").to_string(); // Day without leading zero
    let month = today.format("%B").to_string(); // Full month name
    let year = today.format("%Y").to_string(); // Full year

    format!(
        " A small sampling of events in 2024 since your knowledge cutoff: `{context}` |
Article to review: `{article}` | Source URL: `{source_url}` |
In 2-4 sentences, provide an analysis of the source of this content which includes any background information you know on the source, including details such as ownership, general purpose and goals, awards, scandals, and other relevant information.  Today is {month} {day} {year}, is the content likely recent or does it indicate otherwise?

{write_in_clear_english}

{dont_tell_me}

{format_instructions}",
        context = CONTEXT_2024,
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
