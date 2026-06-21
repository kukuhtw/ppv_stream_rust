use sqlx::PgPool;

pub async fn is_blocked_by_creator(
    pool: &PgPool,
    creator_user_id: &str,
    blocked_user_id: &str,
) -> anyhow::Result<bool> {
    let count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM creator_blocked_users
        WHERE creator_user_id = $1
          AND blocked_user_id = $2
          AND (expires_at IS NULL OR expires_at > NOW())
        "#,
    )
    .bind(creator_user_id)
    .bind(blocked_user_id)
    .fetch_one(pool)
    .await?;

    Ok(count > 0)
}
