use anyhow::Context;
use sqlx::PgPool;
use uuid::Uuid;

/// How often the worker wakes to scan for due delivery jobs.
const POLL_INTERVAL_SECS: u64 = 30;
/// How many jobs are claimed per wakeup cycle.
const BATCH_SIZE: i64 = 10;

#[derive(sqlx::FromRow)]
struct DeliveryJob {
    id: Uuid,
    activity_id: Uuid,
    target_inbox_url: String,
    attempt_count: i32,
    max_attempts: i32,
}

/// Spawn the delivery worker as a background tokio task.
///
/// The worker reads `HMAC_SECRET` from the environment to decrypt actor
/// private keys for HTTP Signature signing.
pub fn start_delivery_worker(pool: PgPool) {
    let app_secret: Vec<u8> = std::env::var("HMAC_SECRET")
        .map(|s| s.into_bytes())
        .unwrap_or_default();

    tokio::spawn(async move {
        run_delivery_loop(pool, app_secret).await;
    });
}

async fn run_delivery_loop(pool: PgPool, app_secret: Vec<u8>) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(POLL_INTERVAL_SECS));

    loop {
        interval.tick().await;
        if let Err(e) = process_due_jobs(&pool, &app_secret).await {
            tracing::error!("federation delivery worker: {}", e);
        }
    }
}

async fn process_due_jobs(pool: &PgPool, app_secret: &[u8]) -> anyhow::Result<()> {
    let jobs: Vec<DeliveryJob> = sqlx::query_as(
        r#"
        SELECT id, activity_id, target_inbox_url, attempt_count, max_attempts
        FROM federation_delivery_jobs
        WHERE status = 'queued' AND next_attempt_at <= NOW()
        ORDER BY next_attempt_at ASC
        LIMIT $1
        FOR UPDATE SKIP LOCKED
        "#,
    )
    .bind(BATCH_SIZE)
    .fetch_all(pool)
    .await
    .context("delivery job query failed")?;

    for job in jobs {
        // Skip delivery to blocked/suspended domains without consuming retry budget.
        let target_domain = job
            .target_inbox_url
            .strip_prefix("https://")
            .or_else(|| job.target_inbox_url.strip_prefix("http://"))
            .and_then(|rest| rest.split('/').next())
            .unwrap_or("");

        if !target_domain.is_empty()
            && crate::federation::moderation::is_domain_blocked(pool, target_domain).await
        {
            tracing::info!(
                job_id = %job.id,
                target = %job.target_inbox_url,
                "delivery skipped: domain {} is blocked/suspended",
                target_domain
            );
            sqlx::query(
                "UPDATE federation_delivery_jobs \
                 SET status = 'failed', last_error = 'domain blocked', updated_at = NOW() \
                 WHERE id = $1",
            )
            .bind(job.id)
            .execute(pool)
            .await?;
            continue;
        }

        // Claim the job
        sqlx::query("UPDATE federation_delivery_jobs SET status = 'processing' WHERE id = $1")
            .bind(job.id)
            .execute(pool)
            .await?;

        let job_id = job.id;
        let activity_id = job.activity_id;
        let attempt = job.attempt_count;
        let max_attempts = job.max_attempts;

        match deliver_job(pool, &job, app_secret).await {
            Ok(()) => {
                sqlx::query(
                    "UPDATE federation_delivery_jobs \
                     SET status = 'delivered', delivered_at = NOW(), updated_at = NOW() \
                     WHERE id = $1",
                )
                .bind(job_id)
                .execute(pool)
                .await?;

                sqlx::query(
                    "UPDATE federation_activities \
                     SET processing_status = 'processed', processed_at = NOW() \
                     WHERE id = $1",
                )
                .bind(activity_id)
                .execute(pool)
                .await?;

                tracing::debug!(
                    job_id = %job_id,
                    "activity delivered"
                );
            }
            Err(e) => {
                let next_attempt_count = attempt + 1;
                let exhausted = next_attempt_count >= max_attempts;
                let new_status = if exhausted { "failed" } else { "queued" };
                let next_at = if exhausted {
                    chrono::Utc::now()
                } else {
                    compute_next_retry(next_attempt_count)
                };

                tracing::warn!(
                    job_id = %job_id,
                    attempt = next_attempt_count,
                    exhausted,
                    "delivery failed: {}",
                    e
                );

                sqlx::query(
                    "UPDATE federation_delivery_jobs \
                     SET status = $2, attempt_count = $3, last_error = $4, \
                         next_attempt_at = $5, updated_at = NOW() \
                     WHERE id = $1",
                )
                .bind(job_id)
                .bind(new_status)
                .bind(next_attempt_count)
                .bind(e.to_string())
                .bind(next_at)
                .execute(pool)
                .await?;

                if exhausted {
                    sqlx::query(
                        "UPDATE federation_activities \
                         SET processing_status = 'failed' \
                         WHERE id = $1",
                    )
                    .bind(activity_id)
                    .execute(pool)
                    .await?;
                }
            }
        }
    }

    Ok(())
}

async fn deliver_job(pool: &PgPool, job: &DeliveryJob, app_secret: &[u8]) -> anyhow::Result<()> {
    // Load the activity payload and the sending actor's URI
    let (actor_uri, payload): (String, serde_json::Value) = sqlx::query_as(
        "SELECT actor_uri, payload FROM federation_activities WHERE id = $1 LIMIT 1",
    )
    .bind(job.activity_id)
    .fetch_one(pool)
    .await
    .context("activity payload lookup failed")?;

    // Load the local actor's DB record to get the user_id for key decryption
    let local_user_id: Option<String> = sqlx::query_scalar(
        "SELECT local_user_id FROM federation_actors \
         WHERE actor_uri = $1 AND is_local = TRUE LIMIT 1",
    )
    .bind(&actor_uri)
    .fetch_optional(pool)
    .await
    .context("actor lookup failed")?
    .flatten();

    let user_id =
        local_user_id.ok_or_else(|| anyhow::anyhow!("no local actor record for {}", actor_uri))?;

    let (private_key_pem, key_id) =
        crate::federation::keys::load_actor_private_key(pool, &user_id, app_secret)
            .await
            .context("actor key load failed")?;

    // Serialise the payload
    let body = serde_json::to_string(&payload).context("payload serialisation failed")?;
    let digest = crate::federation::signatures::build_digest(body.as_bytes());

    // Extract host + path from the target inbox URL
    let (host, path) = parse_url_host_path(&job.target_inbox_url)?;

    let date = chrono::Utc::now()
        .format("%a, %d %b %Y %H:%M:%S GMT")
        .to_string();

    let signature_header = crate::federation::signatures::create_signature(
        &crate::federation::signatures::SignatureInput {
            key_id: &key_id,
            private_key_pem: &private_key_pem,
            method: "POST",
            path_and_query: &path,
            host: &host,
            date: &date,
            digest: Some(&digest),
        },
    )
    .context("HTTP Signature creation failed")?;

    // POST the signed activity
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .context("HTTP client build failed")?;

    let response = client
        .post(&job.target_inbox_url)
        .header("Content-Type", "application/activity+json")
        .header("Host", &host)
        .header("Date", &date)
        .header("Digest", &digest)
        .header("Signature", &signature_header)
        .body(body)
        .send()
        .await
        .context("HTTP POST failed")?;

    let status = response.status();
    if !status.is_success() {
        let body_preview = response
            .text()
            .await
            .unwrap_or_default()
            .chars()
            .take(300)
            .collect::<String>();
        anyhow::bail!(
            "remote {} returned {}: {}",
            job.target_inbox_url,
            status,
            body_preview
        );
    }

    Ok(())
}

// ── Utilities ──────────────────────────────────────────────────────────────

/// Exponential backoff: 2^attempt seconds + up to 30 s jitter, capped at 1 h.
fn compute_next_retry(attempt: i32) -> chrono::DateTime<chrono::Utc> {
    use rand::Rng;
    let base: u64 = compute_retry_delay_seconds(attempt);
    let jitter: u64 = rand::thread_rng().gen_range(0..=30);
    chrono::Utc::now() + chrono::Duration::seconds((base + jitter) as i64)
}

fn compute_retry_delay_seconds(attempt: i32) -> u64 {
    (2u64).saturating_pow(attempt as u32).min(3600)
}

fn parse_url_host_path(url: &str) -> anyhow::Result<(String, String)> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .ok_or_else(|| anyhow::anyhow!("unsupported URL scheme: {}", url))?;

    let slash = rest.find('/').unwrap_or(rest.len());
    let host = rest[..slash].to_string();
    let path = if slash < rest.len() {
        rest[slash..].to_string()
    } else {
        "/".to_string()
    };

    Ok((host, path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_url_host_path_standard() {
        let (host, path) = parse_url_host_path("https://example.com/users/alice/inbox").unwrap();
        assert_eq!(host, "example.com");
        assert_eq!(path, "/users/alice/inbox");
    }

    #[test]
    fn parse_url_host_path_no_trailing_slash() {
        let (host, path) = parse_url_host_path("https://example.com").unwrap();
        assert_eq!(host, "example.com");
        assert_eq!(path, "/");
    }

    #[test]
    fn parse_url_host_path_with_port() {
        let (host, path) = parse_url_host_path("https://example.com:8443/inbox").unwrap();
        assert_eq!(host, "example.com:8443");
        assert_eq!(path, "/inbox");
    }

    #[test]
    fn next_retry_is_in_the_future() {
        let next = compute_next_retry(1);
        assert!(next > chrono::Utc::now());
    }

    #[test]
    fn next_retry_grows_with_attempt() {
        assert!(compute_retry_delay_seconds(3) > compute_retry_delay_seconds(1));
        assert_eq!(compute_retry_delay_seconds(0), 1);
        assert_eq!(compute_retry_delay_seconds(12), 3600);
    }
}
