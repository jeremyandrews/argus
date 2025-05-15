use sha2::{Digest, Sha256};
use tracing::{debug, error, info, warn};
// No need for sleep import, we use weighted_sleep instead
use url::Url;

// No need to import Database, we use it through params
use crate::llm::generate_llm_response;
use crate::prompt;
use crate::util::weighted_sleep;
use crate::workers::common::{extract_llm_params, FeedItem, ProcessItemParams};
use crate::{WorkerDetail, TARGET_DB, TARGET_LLM_REQUEST};

use super::extraction::{extract_article_text, handle_access_denied};
use super::threat::{article_is_relevant, check_if_threat_at_all, determine_threat_location};

/// Processes a single feed item, determining if it's a threat or matches any topics.
pub async fn process_item(
    item: FeedItem,
    params: &mut ProcessItemParams<'_>,
    worker_detail: &WorkerDetail,
) {
    debug!(target: TARGET_LLM_REQUEST, "[{} {} {}]: reviewing {} ({}).", worker_detail.name, worker_detail.id, worker_detail.model, item.title.clone().unwrap_or_default(), item.url
    );
    let article_url = item.url;
    let article_title = item.title.unwrap_or_default();

    // Compute title_domain_hash
    let parsed_url = match Url::parse(&article_url) {
        Ok(parsed) => parsed,
        Err(e) => {
            error!(
                target: TARGET_LLM_REQUEST,
                "Failed to parse article URL: {}: {}",
                article_url,
                e
            );
            return;
        }
    };
    let base_domain = parsed_url.domain().unwrap_or("");
    let title_domain_concat = format!("{}{}", base_domain, article_title);

    let mut hasher = Sha256::new();
    hasher.update(title_domain_concat.as_bytes());
    let title_domain_hash = format!("{:x}", hasher.finalize());

    // Check if this hash already exists in the database
    if params
        .db
        .has_title_domain_hash(&title_domain_hash)
        .await
        .unwrap_or(false)
    {
        info!(
            target: TARGET_LLM_REQUEST,
            "Article with title_domain_hash {} already processed, skipping.",
            title_domain_hash
        );
        return;
    }

    match extract_article_text(&article_url, worker_detail).await {
        Ok((article_text, article_html)) => {
            // Skip articles with no meaningful content
            if article_text.trim().is_empty() || article_text.trim().len() < 100 {
                warn!(
                    target: TARGET_LLM_REQUEST,
                    "[{} {} {}]: Article '{}' has insufficient content, skipping.",
                    worker_detail.name, worker_detail.id, worker_detail.model, article_url
                );
                return;
            }

            let mut hasher = Sha256::new();
            hasher.update(article_text.as_bytes());
            let article_hash = format!("{:x}", hasher.finalize());

            // Check if the hash already exists in the database
            if params.db.has_hash(&article_hash).await.unwrap_or(false) {
                info!(target: TARGET_LLM_REQUEST, "Article with hash {} already processed, skipping.", article_hash);
                return;
            }

            let places = params.places.clone();

            // First check if it's a threat - this takes priority
            if check_if_threat_at_all(&article_text, params, &worker_detail).await {
                let threat =
                    determine_threat_location(&article_text, places, params, &worker_detail).await;

                if !threat.is_empty() {
                    // Add to life safety queue if it's a threat
                    params
                        .db
                        .add_to_life_safety_queue(
                            &threat,
                            &article_url,
                            &article_title,
                            &article_text,
                            &article_html,
                            &article_hash,
                            &title_domain_hash,
                            item.pub_date.as_deref(),
                        )
                        .await
                        .unwrap_or_else(|e| {
                            error!(
                                target: TARGET_DB,
                                "Failed to add article to life safety queue: {:?}", e
                            )
                        });
                } else {
                    // If not a valid threat, process normally for topics
                    process_topics(
                        &article_text,
                        &article_url,
                        &article_title,
                        &article_hash,
                        &title_domain_hash,
                        &article_html,
                        item.pub_date.as_deref(),
                        params,
                        worker_detail,
                    )
                    .await;
                }
            } else {
                // Not a threat, process for topics
                process_topics(
                    &article_text,
                    &article_url,
                    &article_title,
                    &article_hash,
                    &title_domain_hash,
                    &article_html,
                    item.pub_date.as_deref(),
                    params,
                    worker_detail,
                )
                .await;
            }
            weighted_sleep().await;
        }
        Err(access_denied) => {
            handle_access_denied(
                access_denied,
                &article_url,
                &article_title,
                &title_domain_hash,
                item.pub_date.as_deref(),
                params.db,
                worker_detail,
            )
            .await;
        }
    }
}

/// Processes the article to see if it matches any of the specified topics.
async fn process_topics(
    article_text: &str,
    article_url: &str,
    article_title: &str,
    article_hash: &str,
    title_domain_hash: &str,
    article_html: &str,
    pub_date: Option<&str>,
    params: &mut ProcessItemParams<'_>,
    worker_detail: &WorkerDetail,
) {
    // Early check to filter promotional content
    let promo_check_prompt = prompt::filter_promotional_content(article_text);
    let llm_params = extract_llm_params(params);
    if let Some(promo_response) =
        generate_llm_response(&promo_check_prompt, &llm_params, worker_detail).await
    {
        if promo_response.trim().to_lowercase().starts_with("yes") {
            // This is a promotional article, skip further processing
            debug!(target: TARGET_LLM_REQUEST, "[{} {} {}]: article is primarily promotional (sales/discounts), skipping.", 
                   worker_detail.name, worker_detail.id, worker_detail.model);

            // Add to database as non-relevant
            let _ = params
                .db
                .add_article(
                    article_url,
                    false,
                    None,
                    None,
                    None,
                    Some(article_hash),
                    Some(title_domain_hash),
                    None,
                    pub_date,
                    None, // event_date
                )
                .await;

            return;
        }
    }

    let mut article_relevant = false;

    for topic in params.topics {
        let parts: Vec<_> = topic.trim().split(':').collect();
        if parts.len() < 2 {
            continue;
        }
        let topic_name = parts[0].trim();
        let topic_prompt = parts[1].trim();

        if topic_name.is_empty() || topic_prompt.is_empty() {
            continue;
        }

        debug!(target: TARGET_LLM_REQUEST, "[{} {} {}]: asking if about {}: {}.", worker_detail.name, worker_detail.id, worker_detail.model, topic_name, topic_prompt);

        let yes_no_prompt = prompt::is_this_about(article_text, topic_prompt);
        let mut llm_params = extract_llm_params(params);
        if let Some(yes_no_response) =
            generate_llm_response(&yes_no_prompt, &llm_params, worker_detail).await
        {
            if yes_no_response.trim().to_lowercase().starts_with("yes") {
                // Article is relevant to the topic
                article_relevant = true;

                // Perform a secondary check before posting to Slack
                if params.db.has_hash(article_hash).await.unwrap_or(false) {
                    info!(
                        target: TARGET_LLM_REQUEST,
                        "Article with hash {} was already processed (second check), skipping topic '{}'.",
                        article_hash,
                        topic_name
                    );
                    continue; // Skip to the next topic
                }

                if article_is_relevant(
                    article_text,
                    topic_prompt,
                    pub_date,
                    &mut llm_params,
                    worker_detail,
                )
                .await
                {
                    // Add to matched topics queue
                    if let Err(e) = params
                        .db
                        .add_to_matched_topics_queue(
                            article_text,
                            article_html,
                            article_url,
                            article_title,
                            article_hash,
                            title_domain_hash,
                            topic_name,
                            pub_date,
                        )
                        .await
                    {
                        error!(target: TARGET_LLM_REQUEST, "[{} {} {}]: failed to add to Matched Topics queue: {}: [{:?}].", worker_detail.name, worker_detail.id, worker_detail.model, topic_name, e);
                    } else {
                        debug!(target: TARGET_LLM_REQUEST, "[{} {} {}]: added to Matched Topics queue: {}.", worker_detail.name, worker_detail.id, worker_detail.model, topic_name);
                    }

                    return; // No need to continue checking other topics
                } else {
                    debug!(target: TARGET_LLM_REQUEST, "[{} {} {}]: not about '{}' or is promotional.", worker_detail.name, worker_detail.id, worker_detail.model, topic_name);
                    weighted_sleep().await;
                }
            } else {
                debug!(target: TARGET_LLM_REQUEST, "[{} {} {}]: not about '{}': {}.", worker_detail.name, worker_detail.id, worker_detail.model, topic_name, yes_no_response.trim());
                weighted_sleep().await;
            }
        }
    }

    // If no relevant topic was found, add the URL to the database as a non-relevant article
    if !article_relevant {
        match params
            .db
            .add_article(
                article_url,
                false,
                None,
                None,
                None,
                Some(article_hash),
                Some(title_domain_hash),
                None,
                pub_date,
                None, // event_date
            )
            .await
        {
            Ok(_) => {
                debug!(target: TARGET_DB, "[{} {} {}]: added non-relevant article to database.", worker_detail.name, worker_detail.id, worker_detail.model);
            }
            Err(e) => {
                debug!(target: TARGET_DB, "[{} {} {}]: failed to add non-relevant article to database: {:?}", worker_detail.name, worker_detail.id, worker_detail.model, e);
            }
        }
    }
}
