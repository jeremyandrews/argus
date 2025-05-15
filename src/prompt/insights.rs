use crate::prompt::common::{global_context, DONT_TELL_ME, WRITE_IN_CLEAR_ENGLISH};

/// Generate a prompt for creating action recommendations based on an article
pub fn action_recommendations_prompt(article_text: &str, pub_date: Option<&str>) -> String {
    format!(
        r#"{context}
## ARTICLE (FOR ACTION RECOMMENDATIONS):
----------
{article}
----------

IMPORTANT INSTRUCTIONS:
* **Analyze ONLY the article above.**
* **IGNORE the global context unless explicitly mentioned in article.**
* **For non-English text, include translations of relevant quotes.**

### **Action Recommendations**
Create a list of 3-5 clear, practical, and actionable recommendations based directly on the article's content. These should be things readers could reasonably do in response to the information provided.

### Recommendation Guidelines
* Focus on actions that are:
  - **Practical:** Can be implemented by the average reader
  - **Specific:** Clear and concrete, not vague suggestions
  - **Relevant:** Directly related to the article's content
  - **Diverse:** Cover different types of actions when possible
  - **Balanced:** Represent different perspectives when appropriate

* For different article types, consider:
  - **News Events:** How to prepare for, respond to, or learn more about the situation
  - **Technology:** How to utilize, evaluate, or adapt to the technology
  - **Policy Changes:** How to comply with, benefit from, or engage with the policy
  - **Research Findings:** How to apply findings to personal or professional contexts
  - **Market Developments:** How to adjust strategies or make informed decisions

### Response Format
* Start each recommendation with a strong action verb
* Keep each point to 1-2 sentences (25-40 words)
* Use bullet points (-)
* Include concrete details from the article
* Maintain factual accuracy
* Avoid generic advice that would apply to any article

**EXAMPLE (Technology Article):**
- **Download the security patch** released by Microsoft immediately, as it addresses the critical Windows vulnerability that has already compromised over 100,000 systems worldwide.
- **Enable two-factor authentication** on all cloud services mentioned in the article, particularly those handling sensitive data like financial or healthcare information.
- **Review your organization's response plan** for ransomware attacks, ensuring it addresses the specific threats detailed by the security researchers at Black Hat 2024.
- **Sign up for the free webinar** on November 15th featuring cybersecurity experts from the article who will demonstrate practical prevention techniques.

**EXAMPLE (Political Development):**
- **Contact your representative** about the infrastructure bill discussed in the article, especially if you live in one of the five states explicitly mentioned as receiving priority funding.
- **Attend the public hearing** scheduled for October 7th where officials will answer questions about how the new regulations affect homeowners in coastal regions.
- **Apply for the tax credit** before the December 31st deadline, as the article indicates this opportunity will not be extended into the next fiscal year.
- **Review the official guidelines** published on the government website referenced in the article to determine your eligibility for the expanded program.

**EXAMPLE (Health News):**
- **Schedule a consultation** with your healthcare provider about the new treatment option, particularly if you have the specific condition discussed in the research findings.
- **Verify insurance coverage** for the newly approved medication, as the article notes that several major providers already include it in their formularies.
- **Download the symptom-tracking app** developed by the research team, which is available for free during the first month after release.
- **Join the patient advocacy group** mentioned in the article that is working to improve access to the treatment in underserved communities.

**POOR EXAMPLES (Avoid):**
- "Learn more about this topic" (too vague)
- "Stay informed about developments" (not specific enough)
- "Consider how this affects you" (not actionable)
- "Share this information with others" (generic)

Now create 3-5 specific, actionable recommendations based on this article:
{write_in_clear_english}
{dont_tell_me}"#,
        context = global_context(pub_date),
        article = article_text,
        write_in_clear_english = WRITE_IN_CLEAR_ENGLISH,
        dont_tell_me = DONT_TELL_ME
    )
}

/// Generate a prompt for creating talking points based on an article
pub fn talking_points_prompt(article_text: &str, pub_date: Option<&str>) -> String {
    format!(
        r#"{context}
## ARTICLE (FOR TALKING POINTS):
----------
{article}
----------

IMPORTANT INSTRUCTIONS:
* **Analyze ONLY the article above.**
* **IGNORE the global context unless explicitly mentioned in article.**
* **For non-English text, include translations of relevant quotes.**

### **Talking Points**
Create 3-5 insightful talking points that could drive meaningful discussion about this article. These should help someone engage others in conversation about the topic, whether in casual discussions, social media, professional settings, or formal debates.

### Talking Point Guidelines
* Create points that are:
  - **Thought-provoking:** Stimulate deeper thinking and conversation
  - **Balanced:** Acknowledge different perspectives when appropriate
  - **Evidence-based:** Grounded in specific facts from the article
  - **Substantive:** Focus on significant aspects, not trivial details
  - **Diverse:** Cover different dimensions of the topic

* For different article types, consider:
  - **News Events:** Implications, historical parallels, future impact
  - **Technology:** Ethical considerations, practical applications, societal effects
  - **Policy Changes:** Beneficiaries, challenges, alternative approaches
  - **Research Findings:** Limitations, applications, surprising elements
  - **Market Developments:** Winners/losers, underlying trends, future scenarios

### Response Format
* Format each point as a discussion-starter question OR a bold statement + follow-up question
* Keep each talking point to 30-50 words
* Use bullet points (-)
* Include specific references to article content
* Ensure factual accuracy
* Avoid basic summary points that don't promote discussion

**EXAMPLE (Technology Article):**
- **How might the facial recognition limitations** described in the article affect different demographic groups unequally, given the researchers found a 35% higher error rate for certain populations?
- **The article suggests that companies are rushing AI deployment before adequate testing.** How should we balance innovation speed with safety in emerging technologies?
- **Is the 5-year timeline for quantum computing breakthroughs** realistic given the technical challenges outlined by the MIT researchers, or are the commercial predictions overly optimistic?

**EXAMPLE (Economic News):**
- **The shift toward remote work has created "winner and loser" cities.** How might the 15% population outflow from major urban centers reshape housing markets and tax bases in the coming decade?
- **Despite record corporate profits mentioned in the article,** wage growth remains stagnant at 2.3%. What explains this disconnection between company success and worker compensation?
- **How significant is the Central Bank's strategy shift** toward inflation tolerance, and who stands to benefit most from the new approach outlined by Chairperson Rodriguez?

**EXAMPLE (Health Research):**
- **The article reports a surprising 40% reduction in symptoms,** yet the sample size was relatively small. How should patients balance hope with scientific caution when evaluating breakthrough treatments?
- **Could the accessibility issues highlighted in the study** lead to wider health disparities, especially considering the $6,000 monthly cost is only partially covered by insurance?
- **The researchers prioritized quality of life over longevity.** Is this shift in medical research priorities reflective of changing societal values around healthcare?

**POOR EXAMPLES (Avoid):**
- "What do you think about this issue?" (too vague)
- "The article talks about new technology." (mere summary)
- "Is this development good or bad?" (overly simplistic)
- "The CEO made some interesting points." (lacks substance)

Now create 3-5 engaging talking points based on this article:
{write_in_clear_english}
{dont_tell_me}"#,
        context = global_context(pub_date),
        article = article_text,
        write_in_clear_english = WRITE_IN_CLEAR_ENGLISH,
        dont_tell_me = DONT_TELL_ME
    )
}

/// Generate a prompt for creating additional insights about an article
pub fn additional_insights_prompt(article_text: &str, pub_date: Option<&str>) -> String {
    format!(
        r#"# Analysis Framework
## Global Context (Reference Only)
{context}
_Note: Only reference global context if directly relevant to analyzing the article_
## Source Material
----------
{article}
----------

## Core Requirements
- Analyze the article's content, not the global context
- Reveal deeper context and connections beyond the article's surface details
- Illuminate cultural nuances and regional perspectives specific to the article
- Draw unexpected parallels and insights that enhance understanding of the article
- Ground claims in concrete examples from or related to the article's content
- After completing the primary analysis, always include a Devil's Advocate perspective to challenge key assumptions
- Ensure the Devil's Advocate analysis is specific to the content and insights provided, not generic counterpoints

## Analysis Requirements
- Choose at least **one category from each of the following groups**:
- **Systemic Analysis:** (Technical Depth, Global Patterns, Hidden Dimensions)
- **Human & Cultural Impact:** (Cultural Lens, Ripple Effects, Character Studies)
- **Creative & Alternative Perspectives:** (That's Ironic, Pattern Recognition, Unexpected Angles, Popular Culture, There's A Word For That, It All Started With, Logical Conclusion)
- Do **not** overuse Cultural Lens, Ripple Effects, or Character Studies unless uniquely fitting.
- If the article discusses **technology, business, economics, or science**, prioritize at least one **Systemic Analysis** category.
- If the article covers **a person or event**, consider Character Studies but balance it with a **Systemic or Historical** category.
- Avoid overly broad or generic insights—focus on **specific, well-supported claims**.
- When discussing cultural, economic, or societal impact, **avoid sweeping generalizations** and instead provide **concrete, precise examples**.
- If an insight applies to nearly any article, reconsider its relevance to this one.
- Provide **2-4 key insights** per chosen category.
- Each insight should be **15-30 words**, balancing brevity with substance.
- **After completing all other analyses, ALWAYS include the Devil's Advocate category
- The Devil's Advocate section should:
- Challenge at least one key insight from each previous category used
- Provide specific, concrete alternatives to the assumptions made
- Maintain the same level of analytical rigor as the primary analysis
- Focus on substantive critiques rather than superficial contradictions
- Ground counter-arguments in evidence where possible
- Devil's Advocate insights should follow the same 15-30 word format and quantity requirements as other categories

### 🌍 Cultural Lens
- Power dynamics and hierarchies
- Local traditions and values
- Historical patterns
- Social structures and relationships
- Language and communication styles

### 📊 Technical Depth
- Core principles and mechanisms
- Hidden complexities
- System interactions
- Engineering challenges
- Implementation details

### 💡 Global Patterns
- Cross-cultural parallels
- Regional adaptations
- Universal principles
- Contrasting approaches
- Historical echoes

### 📈 Ripple Effects
- Industry transformations
- Societal shifts
- Economic impacts
- Political implications
- Cultural evolution

### 🤔 Hidden Dimensions
- Unspoken assumptions
- Alternative frameworks
- Overlooked factors
- Competing narratives
- Cultural blind spots

### 📅 Time's Echo
- Events on this date
- Cyclical patterns
- Historical rhymes
- Forgotten precedents
- Anniversary insights

### 👁️ Unexpected Angles
- Nature's perspective
- Future archaeologists' view
- Children's understanding
- Alien anthropologist's report
- Ordinary objects' stories

### 🎭 Character Studies
- Key personalities
- Hidden influencers
- Unlikely heroes
- Silent catalysts
- Generational contrasts

### 🎨 Creative Connections
- Art world parallels
- Literary echoes
- Musical metaphors
- Architectural analogies
- Gaming dynamics

### 🎬 Scene Shifts
- Behind the curtain
- Alternative endings
- Untold beginnings
- Parallel universes
- What-if scenarios

### 🌱 Seeds of Change
- Small triggers
- Butterfly effects
- Hidden catalysts
- Quiet revolutions
- Gradual transformations

### 🎯 Precision Focus
- Crucial details
- Pivotal moments
- Key decisions
- Critical junctures
- Defining elements

### 📚 Popular Culture
- Reactions in popular media
- Popular books or movies that parallel real-world events
- Cultural references and their significance

### 🤔 That's Ironic
- Historical coincidences
- Unexpected parallels
- Amusing contradictions
- Role reversals
- Cosmic timing

### 🧩 Pattern Recognition
- Mathematical symmetries
- Natural world parallels
- Social physics
- Economic rhythms
- Evolutionary echoes

### 🔤 There's A Word For That
- Cross-cultural terminology
- Specialized jargon
- Untranslatable concepts
- Etymology insights
- Linguistic precision

### 🌱 It All Started With
- Origin stories
- Historical foundations
- Initial catalysts
- Foundational influences
- Evolutionary beginnings

### 🔮 Logical Conclusion
- Future trajectories
- Ultimate implications
- Potential outcomes
- Natural endpoints
- Evolutionary destinations

### Example Output:
### 📊 Technical Depth
- The quantum chip's new architecture integrates superconducting circuits with traditional silicon, enabling unprecedented coherence times while maintaining scalability for commercial applications
- Error correction protocols now handle environmental noise through a distributed network of sensors, reducing decoherence by 60% compared to previous generations
- Novel gate designs incorporate machine learning optimization, allowing quantum operations to execute 40% faster while maintaining high fidelity

### 💡 Global Patterns
- As quantum systems approach practical advantage in specific domains, pharmaceutical companies are already developing hybrid classical-quantum workflows for drug discovery
- The convergence of quantum computing with AI could revolutionize financial modeling by 2026, though regulatory frameworks remain uncertain

### 🎭 Character Studies
- The CEO's decision to prioritize sustainability reflects a broader industry trend towards corporate social responsibility

### 📚 Popular Culture
- The film "Invasion of the Body Snatchers" parallels real-world concerns about political manipulation and loss of individuality

### 🤔 That's Ironic
- The rapid advancement of quantum computing ironically highlights the limitations of classical computing in solving certain problems
- While quantum computers excel at modeling uncertainty, their own development has been remarkably predictable, following Moore's Law-like scaling

### 🧩 Pattern Recognition
- Like the transition from vacuum tubes to transistors, this quantum breakthrough combines materials innovation with clever engineering workarounds to solve scaling limitations
- The industry's collaborative approach to error correction mirrors early classical computing, where shared standards accelerated development across competing platforms
- Current quantum scaling challenges echo semiconductor manufacturing hurdles of the 1970s, suggesting similar solutions might apply
- The emergence of quantum-specific programming languages parallels the evolution from machine code to high-level languages in classical computing

### 🔤 There's A Word For That
- "Zukunftsangst" (German: fear of the future) characterizes the industry's cautious optimism, balancing excitement for quantum possibilities against concerns about implementation challenges
- "Kaizen" (Japanese: continuous improvement) describes the incremental approach to quantum error correction that's proving more effective than revolutionary methods

### 🌱 It All Started With
- Today's quantum computing revolution traces back to Richard Feynman's 1981 suggestion that quantum systems might be needed to efficiently simulate quantum physics
- The breakthrough builds upon Bell's inequality experiments from the 1960s that first demonstrated quantum entanglement as a real, exploitable phenomenon

### 🔮 Logical Conclusion
- Quantum computing may ultimately lead to a bifurcated computing landscape where specialized quantum processors handle specific tasks while classical systems manage everyday computing needs
- The natural endpoint could be quantum networks connecting distributed quantum processors, creating a fundamentally new computing paradigm beyond today's internet architecture

### 😈 Devil's Advocate
- Critically examine assumptions and claims made in previous sections
- Challenge conventional interpretations
- Explore counter-narratives and alternative explanations
- Question methodology and evidence
- Consider unintended consequences

Example Devil's Advocate points:
- The quantum breakthrough may actually slow industry progress by focusing resources on a suboptimal approach
- Collaborative standards could stifle innovation by prematurely narrowing the solution space
- The classical computing parallels might mislead us - quantum computing may follow fundamentally different development patterns

{write_in_clear_english}
{dont_tell_me}"#,
        context = global_context(pub_date),
        article = article_text,
        write_in_clear_english = WRITE_IN_CLEAR_ENGLISH,
        dont_tell_me = DONT_TELL_ME
    )
}
