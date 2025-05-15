use crate::prompt::common::DONT_TELL_ME;

/// Generate a prompt for scoring the source quality based on critical analysis
pub fn sources_quality_prompt(critical_analysis: &str) -> String {
    format!(
        r#"Based on this critical analysis:
~~~
{analysis}
~~~

Return a single number (1, 2, or 3) representing the overall quality of the source:
1 = Poor (red) - Major credibility issues, unreliable source, or significant bias
2 = Moderate (yellow) - Some concerns but generally acceptable
3 = Excellent (green) - Highly credible, reliable source with minimal bias

Base your assessment primarily on:
- Credibility Score
- Source reputation and reliability
- Use of reliable sources
- Professional standards
- Editorial oversight

Return ONLY the number 1, 2, or 3.
{dont_tell_me}"#,
        analysis = critical_analysis,
        dont_tell_me = DONT_TELL_ME
    )
}

/// Generate a prompt for scoring argument quality based on logical fallacy analysis
pub fn argument_quality_prompt(logical_fallacies: &str) -> String {
    format!(
        r#"Based on this logical fallacy and article quality analysis:
~~~
{analysis}
~~~
Return a single number (1, 2, or 3) representing the overall quality of the article:
1 = Poor (red) - Multiple serious fallacies, weak arguments, or unreliable information
2 = Moderate (yellow) - Some fallacies, weaknesses in reasoning, or minor factual issues
3 = Excellent (green) - Strong arguments OR well-sourced, factually reliable information

Base your assessment on:
- If the article is **Argumentative**:
  - Number and severity of logical fallacies
  - Argument strength score
  - Evidence quality score
  - Overall logical consistency
- If the article is **Informational**:
  - Clarity & coherence score
  - Factual reliability score
  - Overall professionalism and objectivity

Return ONLY the number 1, 2, or 3.
{dont_tell_me}"#,
        analysis = logical_fallacies,
        dont_tell_me = DONT_TELL_ME
    )
}

/// Generate a prompt for determining the source type based on source analysis
pub fn source_type_prompt(source_analysis: &str, article_url: &str) -> String {
    format!(
        r#"Based on this source analysis and URL:
~~~
{analysis}
URL: {url}
~~~

Return a single word describing the source type if it matches one of these categories, otherwise return "none":

"official" - Government websites (.gov), recognized international organizations, or primary sources for their domain (e.g., drupal.org for Drupal news)
"academic" - University or research institution websites (.edu, established research centers)
"questionable" - Known disinformation sources, sites with severe credibility issues, or extreme bias
"corporate" - Official company websites for relevant industry news
"nonprofit" - Recognized nonprofit or NGO websites
"press" - Established press organizations with professional standards

Return ONLY one of these exact words: official, academic, questionable, corporate, nonprofit, press, none
{dont_tell_me}"#,
        analysis = source_analysis,
        url = article_url,
        dont_tell_me = DONT_TELL_ME
    )
}
