use crate::prompt::common::{global_context, DONT_TELL_ME, WRITE_IN_CLEAR_ENGLISH};

/// Generate a prompt for critical analysis of an article
pub fn critical_analysis_prompt(article_text: &str, pub_date: Option<&str>) -> String {
    format!(
        r#" {context}
## ARTICLE (FOR CRITICAL ANALYSIS):
----------
 {article}
----------

IMPORTANT INSTRUCTIONS:
* **Analyze ONLY the article above.**
* **Distinguish between:**
  - **Article voice:** The author's/publication's direct statements
  - **Source quotes:** Statements from interviews, documents, or other sources
* **When evaluating credibility:**
  - Consider how effectively the article contextualizes its quotes
  - Assess the balance between direct reporting and quoted material
  - Note whether controversial quotes are properly attributed and explained

### **Content Analysis**
**Quote Integration:** [1-10]
* How effectively does the article:
  - Introduce and contextualize quotes
  - Balance direct reporting with quoted material
  - Handle controversial or incorrect quoted statements
  - Maintain clarity between article voice and quoted content

### **Credibility Analysis**
**Credibility Score:** [1-10]
* Provide specific reasons (max 20 words)
* Consider:
  - Source reliability
  - Citation quality
  - Expert consultation
  - Fact verification
  - Methodology transparency

### **Writing Style Analysis**
**Style Score:** [1-10]
* Provide specific reasons (max 20 words)
* Consider:
  - Clarity of expression
  - Organization
  - Technical accuracy
  - Language appropriateness
  - Engagement level

### **Political Analysis**
**Political Leaning:** [Far Left | Left | Center Left | Center | Center Right | Right | Far Right | N/A]
* Provide specific evidence (max 20 words)
* Include:
  - Word choice analysis
  - Source selection
  - Topic framing
  - Quote selection
  - Context presentation

### **Tone Assessment**
**Primary Tone:** [Neutral | Positive | Negative | Alarmist | Optimistic | Skeptical | Other]
* Support with specific examples (max 20 words)
* Secondary tones if present
* Quote relevant passages (with translations if needed)

### **Audience Analysis**
**Target Audience:**
* Demographics (max 10 words)
* Expertise level required
* Geographic focus
* Professional/General
* Cultural context

### **Critical Analysis**
2-3 bullet points examining:
* Argument structure
* Evidence quality
* Logical consistency
* Potential biases
* Information gaps
* Cultural/contextual factors

### **Key Takeaways**
1-2 bullet points covering:
* Main conclusions
* Significance
* Broader implications
* Notable limitations

**EXAMPLE OUTPUT:**
### Credibility Analysis
**Credibility Score:** 8/10
- Article clearly distinguishes between factual reporting and quoted opinions
- Provides context for controversial quotes
- Maintains accuracy while including diverse perspectives

### Quote Integration: 9/10
Article voice: "9 is the number after 8"
- States mathematical fact directly

Quoted content: "Because 7 8 9, we skip from 8 to 10"
- Clearly attributed as humorous reference
- Doesn't compromise article's factual accuracy
- Adds engaging cultural context

### Writing Style Analysis
**Style Score:** 7/10
- Clear technical explanations with appropriate jargon, well-structured arguments, engaging narrative

### Political Analysis
**Political Leaning:** Center-Right
- Emphasizes market-based solutions, quotes business leaders predominantly, focuses on economic impact

### Tone Assessment
**Primary Tone:** Skeptical
Text: "Les preuves ne sont pas concluantes"
Translation: "The evidence is not conclusive"
- Consistently questions assumptions and demands stronger evidence

### Audience Analysis
**Target Audience:** Financial professionals and policy makers in European markets
- Assumes familiarity with economic terms and regulatory framework

### Critical Analysis
- Strong empirical evidence supports main arguments, but overlooks potential alternative interpretations
- Comprehensive data presentation, though some regional comparisons lack context

### Key Takeaways
- Policy implications are well-supported by data but could benefit from more diverse expert perspectives
- Analysis provides valuable insights while acknowledging limitations of current research

Now, perform the analysis with these guidelines:
{write_in_clear_english}
{dont_tell_me}"#,
        context = global_context(pub_date),
        article = article_text,
        write_in_clear_english = WRITE_IN_CLEAR_ENGLISH,
        dont_tell_me = DONT_TELL_ME
    )
}

/// Generate a prompt for logical fallacy analysis
pub fn logical_fallacies_prompt(article_text: &str, pub_date: Option<&str>) -> String {
    format!(
        r#"{context}
## ARTICLE (FOR LOGICAL FALLACY ANALYSIS):
-----
{article}
-----

Analyze this article and provide a structured assessment in the following format:

### Article Type Analysis
Classify the primary type as:
* **Argumentative:** Presents claims and reasoning to support a specific viewpoint or conclusion
* **Informational:** Reports facts and events without advocating for particular interpretations
* **Educational:** Explains concepts, processes, or topics with the intent to teach
* **Promotional:** Markets products, services, or ideas with persuasive intent
* **Opinion/Editorial:** Presents personal views or analysis on topics
* **Investigative:** In-depth research and analysis of complex topics or issues

Note: Articles may have secondary elements of other types. If significant, note these as well.

Provide 1-2 sentences explaining the classification and any notable hybrid elements.

### Logical Fallacies Analysis

For each fallacy detected, format as follows:

### [Fallacy Name]
* **Definition**: Brief 5-20 word explanation of this type of fallacy
* **Handling**: Clearly state if the article supports the fallacy, flags it as improper, adds it, changes it, or highlights it.
* **Quote**: Format as follows:
- For English: "quoted text"
- For non-English: "original text (translation)"
Example fallacy quotes:
- English: "All politicians are corrupt because Senator Smith took a bribe"
- Non-English: "Tous les politiciens sont corrompus parce que le sénateur Smith a accepté un pot-de-vin (All politicians are corrupt because Senator Smith took a bribe)"
* **Explanation**: Provide a specific analysis of how/why this quote demonstrates this fallacy, without repeating what is already stated. Include a detailed analysis of the context and implications.

If and ONLY if NO fallacies found, state: "No apparent logical fallacies detected."

### Quality Assessment
Score each relevant metric from 1-10 with brief justification (max 20 words):

For Argumentative content:
* **Argument Strength**: Logic and reasoning quality
* **Evidence Quality**: Supporting data and sources
* **Counter-argument Treatment**: Handling of opposing views

For Informational/News content:
* **Accuracy**: Factual correctness and precision
* **Objectivity**: Balance and neutrality
* **Source Quality**: Reliability of information sources

For Educational content:
* **Clarity**: Clear explanation of concepts
* **Comprehensiveness**: Coverage of key points
* **Pedagogical Structure**: Effective learning progression

For Promotional content:
* **Claim Transparency**: Clarity about promotional nature
* **Evidence Support**: Backing for product/service claims
* **Disclosure**: Clarity about relationships/sponsorships

For Opinion/Editorial content:
* **Reasoning Quality**: Logical consistency
* **Perspective Clarity**: Transparency about viewpoint
* **Supporting Evidence**: Backing for opinions

For Analysis content:
* **Methodology**: Soundness of analytical approach
* **Data Quality**: Reliability of data sources
* **Interpretation**: Validity of conclusions

Score only metrics relevant to the article's primary type(s). For hybrid articles, use metrics from each applicable category.

### Overall Assessment
* 1-2 key observations about:
- Reasoning and logical consistency (for argumentative)
- Clarity and sourcing (for informational)
- Treatment of expert opinions when present

Important Guidelines:
- Analyze only the provided article
- Distinguish between article claims and quoted statements
- Identify only clear, unambiguous fallacies
- Consider full context
- For non-English text, provide translations

Now, perform the analysis with these guidelines:
{write_in_clear_english}
{dont_tell_me}"#,
        context = global_context(pub_date),
        article = article_text,
        write_in_clear_english = WRITE_IN_CLEAR_ENGLISH,
        dont_tell_me = DONT_TELL_ME
    )
}

/// Generate a prompt for source analysis
pub fn source_analysis_prompt(
    article_html: &str,
    source_url: &str,
    pub_date: Option<&str>,
) -> String {
    format!(
        r#"
{context}

ARTICLE AND SOURCE URL (FOR SOURCE ANALYSIS):
-----------------------------
{article}
Source URL: {source_url}
-----------------------------

IMPORTANT INSTRUCTIONS:
- **Analyze ONLY the WEBSITE DOMAIN ITSELF, not the specific article content.** 
- **IGNORE the global context unless the article explicitly mentions related events.**
- **Focus on the publication source, its ownership, reputation, and background.**

TASK:
Carefully extract the domain from the URL, then analyze the publication source following these STRICT guidelines:

### **Domain Name:** 
- Extract and list the domain (e.g., `example.com`).

### **Publication Date:** 
- Identify the publication date of the article, or provide the best estimate based on content.

### **Institutional Analysis:** 
- Provide up to **five bullet points** about the WEBSITE/ORGANIZATION, focusing on the following (skip any bullet if the information is unavailable):
  - **Ownership and Management:** Who owns and operates the website? Include corporate affiliations if known.
  - **Audience and Reach:** Describe the website's target audience, monthly readership, or geographical influence.
  - **Reputation and History:** Mention any notable awards, controversies, or credibility ratings from reliable sources.
  - **Publishing Practices:** Frequency of publication, editorial policies, or any relevant operational details.
  - **Comparison:** Compare the publication to other similar sources.

**EXAMPLE (Correct):**

businessnews.com Published: February 4, 2024

    The domain is owned by Global Media Holdings, acquired by Berkshire Hathaway in 2019, maintaining editorial independence through a trust structure.
    Reaches 12 million monthly readers, primarily financial professionals globally, with headquarters in Toronto and 15 international bureaus.
    Earned six Pulitzer Prizes for financial reporting, faced a libel lawsuit in 2021, and holds an A+ NewsGuard rating.
    Publishes ~200 stories daily, operates on a subscription model with 800,000 paid subscribers, and maintains a 24/7 newsroom.


**EXAMPLE (Incorrect):**
- Focuses on summarizing article content instead of analyzing the publication source.

Now analyze the publication source using these rules:

{write_in_clear_english}

{dont_tell_me}"#,
        article = article_html,
        source_url = source_url,
        context = global_context(pub_date),
        write_in_clear_english = WRITE_IN_CLEAR_ENGLISH,
        dont_tell_me = DONT_TELL_ME
    )
}
