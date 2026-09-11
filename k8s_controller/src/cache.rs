use anyhow::{anyhow, Context};
use reqwest::Client;
use tracing::{info, warn};

fn bot_cache_key(bot_name: &str) -> String {
    format!("bot-code/{bot_name}")
}

fn cache_download_url(base_url: &str) -> String {
    format!("{}/download", base_url.trim_end_matches('/'))
}

async fn verify_artifact_and_get_etag(client: &Client, artifact_url: &str, bot_name: &str) -> anyhow::Result<Option<String>> {
    let response = client
        .get(artifact_url)
        .send()
        .await
        .with_context(|| format!("Failed to verify bot artifact for {bot_name}"))?;

    let status = response.status();
    if !status.is_success() {
        return Err(anyhow!("Bot artifact for {bot_name} returned HTTP {status}"));
    }

    let etag = response
        .headers()
        .get(reqwest::header::ETAG)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    // We only need the response headers. Dropping the response avoids intentionally
    // downloading the artifact body into the K8S controller process.
    drop(response);

    Ok(etag)
}

async fn warm_cache(client: &Client, caching_server_url: &str, unique_key: &str, artifact_url: &str, etag: &str) -> anyhow::Result<()> {
    let body = serde_json::json!({
        "uniqueKey": unique_key,
        "url": artifact_url,
        "md5hash": etag,
    });

    let response = client
        .post(cache_download_url(caching_server_url))
        .json(&body)
        .send()
        .await
        .with_context(|| format!("Failed to prefetch {unique_key} through the shared cache"))?;

    let status = response.status();
    if !status.is_success() {
        return Err(anyhow!("Shared cache returned HTTP {status} while prefetching {unique_key}"));
    }

    // A successful response means the caching server has already fetched and stored
    // the requested artifact. The body is the artifact itself, which the controller
    // does not need, so drop it rather than buffering a second copy here.
    drop(response);

    Ok(())
}

/// Verify that the current bot ZIP is reachable, then warm the shared cache before
/// allocating a Kubernetes Job for the match.
///
/// Source-artifact failures are returned to the caller so the Job is not created.
/// Cache-only failures are logged and treated as non-fatal because the match
/// controller already supports downloading directly from the artifact store.
pub async fn prefetch_bot_zip(caching_server_url: &str, bot_name: &str, artifact_url: &str) -> anyhow::Result<()> {
    let client = Client::new();
    let unique_key = bot_cache_key(bot_name);

    let Some(etag) = verify_artifact_and_get_etag(&client, artifact_url, bot_name).await? else {
        warn!("Bot artifact {:?} is reachable but has no ETag; skipping cache prefetch", bot_name);
        return Ok(());
    };

    if caching_server_url.is_empty() {
        info!("Shared cache URL is empty; verified bot artifact {:?} but skipped prefetch", bot_name);
        return Ok(());
    }

    match warm_cache(&client, caching_server_url, &unique_key, artifact_url, &etag).await {
        Ok(()) => {
            info!("Prefetched bot artifact {:?} into the shared cache with ETag {:?}", bot_name, etag);
        }
        Err(e) => {
            warn!(
                "Shared cache prefetch failed for bot {:?}: {:?}. The artifact source is reachable, so the Kubernetes Job may fall back to the existing direct-download path.",
                bot_name, e
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{bot_cache_key, cache_download_url};

    #[test]
    fn bot_cache_key_matches_match_controller_key() {
        assert_eq!(bot_cache_key("ExampleBot"), "bot-code/ExampleBot");
    }

    #[test]
    fn cache_download_url_uses_download_endpoint() {
        assert_eq!(
            cache_download_url("http://aiarena-caching-nodeport-service/"),
            "http://aiarena-caching-nodeport-service/download"
        );
    }
}
