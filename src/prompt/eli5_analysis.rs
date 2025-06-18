/// Comprehensive ELI5 Guidelines and Examples
/// This contains all the detailed instructions for creating high-quality ELI5 explanations

pub const ARTICLE_ANALYSIS_GUIDELINES: &str = r#"
### Handling Sensitive Topics
* When explaining policies that affect human rights, civil liberties, or vulnerable populations:
  - Maintain appropriate moral framing even in simplified language
  - Never minimize the real-world impact of policies on affected people
  - Explain consequences in concrete, relatable terms without downplaying severity
  - Use age-appropriate but accurate descriptions of controversial actions
  - Avoid euphemisms that obscure the nature of harmful policies
* For political topics:
  - Explain both the stated rationale AND the criticism/concerns
  - Present multiple perspectives while maintaining factual accuracy
  - Use neutral but precise language to describe controversial actions
  - Ensure simplification doesn't inadvertently normalize harmful policies

### Writing Approach
* Explain as if to someone intelligent but with no specialized knowledge in this field
* Focus on the "why" and "how" for better understanding
* Use specific examples over vague generalizations
* Connect to familiar experiences when possible
* Maintain appropriate source attribution when information comes from specific sources
* Balance thoroughness with simplicity - explain fully but with the simplest possible concepts

### **SUCCESSFUL EXAMPLES:**

**Example 1: Technology Article (Original topic: Advanced AI Model Release)**
"OpenAI just made a new AI helper called GPT-4 that's much better at understanding both text and pictures. Think of AI as a computer brain that can learn to do tasks by looking at lots of examples.

This new AI is like upgrading from a toy car to a real one. The old version (GPT-3) was already pretty smart - it could write stories, answer questions, and explain complicated topics in simple ways. But it sometimes made silly mistakes or got confused easily.

The new GPT-4 is much better at solving tricky problems. Imagine if your calculator could not just add numbers but also help with your homework, write your book reports, and explain why the sky is blue - all just by typing questions to it.

One of the biggest improvements is that GPT-4 can now understand pictures. If you show it a photo of what's in your refrigerator, it could suggest recipes you can make with those ingredients. This is a big deal because earlier versions could only work with text.

This matters because AI tools like GPT-4 are becoming part of everyday life - they help doctors diagnose illnesses, assist customer service agents, create art, and even help students learn. As these tools get better, they'll change how we work, learn, and solve problems in the future. This article was written in English for general readers, with a professional tone, and appears to be informing people about recent technology developments."

**Example 2: Business News (Original topic: Semiconductor Manufacturing Supply Chain)**
"Computer chips (also called semiconductors) are like the brains inside all our electronic devices - phones, laptops, cars, and even refrigerators. Most of these chips are made in just a few places in the world, especially Taiwan and South Korea.

The article explains that there's a big problem happening right now because not enough chips are being made to meet everyone's needs. It's like if there was suddenly not enough bread for everyone who wants sandwiches.

This chip shortage happened for several reasons. First, when COVID-19 hit, companies thought people would buy fewer electronics, so they ordered fewer chips. But the opposite happened - people stuck at home bought MORE computers and gadgets, not less!

At the same time, chip factories (called 'fabs') are extremely complicated to build. Imagine the most advanced factory you can think of, then multiply that by 100. They cost billions of dollars and take years to construct. So manufacturers can't just quickly make more chips when demand increases.

The shortage affects many things we buy. Car companies have had to stop making some vehicles because they can't get the chips that run everything from engines to entertainment systems. That's why some car prices have gone up and why it's harder to find certain models.

This matters to everyday people because it means electronics might cost more or be harder to find in stores. It also shows how connected our global economy is - problems in one part of the world can affect products everywhere else. This article was written in English for general readers, with an informative tone, and aims to help people understand current supply chain challenges."

**Example 3: Scientific Research (Original topic: CRISPR Gene Editing Breakthrough)**
"Scientists have found a better way to use a tool called CRISPR, which lets them change the instruction manual inside living cells. Every living thing has DNA, which is like a cookbook with recipes that tell cells how to grow and work.

Sometimes, there are mistakes in this cookbook that can cause diseases. CRISPR works like a very tiny pair of scissors combined with a search function - it can find specific recipes (genes) and make precise changes to fix problems.

The big news in this article is that scientists made CRISPR much more accurate. Earlier versions sometimes made changes in the wrong places - imagine trying to fix a typo in a cookbook but accidentally changing instructions on a different page too! The improved method reduces these mistakes by about 80%.

To understand how impressive this is, think about performing surgery with a butter knife versus a precise scalpel. Both can cut, but the scalpel lets you be much more careful and exact. This new CRISPR technique is like upgrading from an okay tool to an excellent one.

In their experiments, scientists successfully corrected a genetic mutation that causes a blood disease called sickle cell anemia. They took cells from patients, fixed the genetic mistake, then put the healthy cells back - and the cells started making proper blood components.

This matters because many diseases are caused by problems in our DNA. Better gene editing tools could eventually help treat or cure conditions like cystic fibrosis, certain types of blindness, and even some cancers. However, there are still many steps before these treatments would be widely available to patients. This article was written in English for college-educated readers, with a scientific but accessible tone, and appears to be explaining recent medical research."

**Example 4: Political Policy (Original topic: Immigration Enforcement Policy)**
"The government recently changed how it handles people who come to the country without permission. This change means that some families are being separated - parents go to one place while their children go to another.

The people who made this rule say it's important because they want to discourage other families from trying to enter the country without going through the proper process. They believe this will help protect the country's borders and make sure everyone follows the immigration laws.

However, many doctors, lawyers, and human rights experts are very concerned because separating children from their parents can cause serious emotional harm to the children. They point out that many of these families are running away from dangerous situations in their home countries and are asking for protection (called asylum).

This matters because how we treat people who come to our country - especially children - reflects our values as a society. The debate is about finding the right balance between enforcing immigration laws and treating people humanely, especially those who may be fleeing from danger. This article was written in English for general readers, with a serious tone, and appears to be explaining a controversial government policy."

**Example 5: Government Action (Original topic: Executive Order on Civil Liberties)**
"The President signed an official paper (called an executive order) that changes some rules about how the government can monitor or collect information about people living in the country.

Under these new rules, government agencies can more easily look at certain records or communications without getting permission from a judge first. The President says this is necessary to help catch dangerous people who might want to harm others.

Many civil rights lawyers and privacy experts are worried about these changes. They say the new rules might allow the government to collect too much information about ordinary people who haven't done anything wrong. They're concerned this could violate people's right to privacy, which is protected by our Constitution.

This matters to everyone because it's about finding the right balance between keeping people safe and protecting their rights and freedoms. Throughout history, societies have had to figure out this balance, and it's something we continue to debate today. This article was written in English for general readers, with an analytical tone, and appears to be explaining the implications of new government policies."

### **UNSUCCESSFUL EXAMPLES (AVOID):**

1. **Too Technical:** "The CRISPR-Cas9 system's off-target effects were mitigated through modification of the guide RNA scaffold, resulting in an 80% reduction in non-specific endonuclease activity as measured by genome-wide sequencing."

2. **Too Vague:** "Scientists made a thing that edits genes better. It's more accurate now and helps with diseases. This is important for medicine."

3. **Too Condescending:** "Imagine DNA is like a book, but a really really complicated book that most people wouldn't understand. The scientists, who are very smart, figured out how to change words in this book!"

4. **Not Factually Accurate:** "The new CRISPR technique can now cure all genetic diseases with no risks, and doctors will start using it in all hospitals next month."

5. **Too Abstract:** "The paradigm of genetic intervention has shifted toward a more deterministic methodology, centralizing accuracy over throughput in the evolving narrative of biomedical applications."

6. **Minimizing Impact:** "The President made a rule that some people can't come into the country anymore. Some people were sad about it, but the President said it would keep everyone safer."

7. **False Equivalence:** "Some people think the policy is good, and some think it's bad. Both sides have good points, so it's just a matter of opinion."

8. **Euphemistic Language:** "The government decided to relocate certain individuals to specialized facilities while their cases were being processed." (instead of clearly explaining detention or deportation)

9. **Foreign Language Response:** "El nuevo descubrimiento científico permite editar genes con mayor precisión..." (WRONG - explanation must be in American English)

10. **Mixed Language:** "Scientists discovered a new way to edit genes that's molto preciso (very precise) and will help cure diseases." (WRONG - only include foreign text for direct quotes)

### Article Analysis Guidelines (MANDATORY)
Your final sentence MUST analyze the original article's characteristics using these guidelines:

**Required Elements:**
- Language the article was written in
- Education level it assumes from readers
- The tone/style the author used
- The likely purpose of the article

**Analysis Categories (use any appropriate terms, not limited to these examples):**

**Education Level Examples:**
- General Public, High School, College, Professional, Expert, or any other appropriate description

**Tone Examples:**
- Professional, Casual, Academic, Opinion, Urgent, Analytical, Formal, Conversational, or any other fitting description

**Purpose Examples:**
- Informing, Explaining, Persuading, Alerting, Analyzing, Arguing, Educating, Entertaining, or any other accurate purpose

**Format as a natural, conversational sentence** that helps readers understand what they just read. Use whatever terms most accurately describe the article, even if not listed in the examples above.

**Example formats:**
- "This article was written in English for general readers, with a professional tone, and appears to be informing people about recent business developments."
- "This article was written in Spanish for college-educated readers, using an argumentative tone to persuade people about environmental policy."
- "This article was written in English for professionals in the tech industry, with an analytical tone, and aims to educate readers about new security vulnerabilities."
"#;
