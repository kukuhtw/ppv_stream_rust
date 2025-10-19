// src/sessions.rs
use tower_cookies::{Cookie, Cookies};
use sqlx::PgPool;
use chrono::{Duration, Utc};
use uuid::Uuid;

const COOKIE_NAME: &str = "ppv_session";

pub async fn create_session(
    pool: &PgPool,
    user_id: &str,
    is_admin: bool,
    cookies: &Cookies,
) -> sqlx::Result<()> {
    let sid = Uuid::new_v4().to_string();
    let now = Utc::now();
    let exp = now + Duration::days(7);

    sqlx::query!(
        r#"
        INSERT INTO sessions (id, user_id, is_admin, created_at, expires_at)
        VALUES ($1, $2, $3, $4, $5)
        "#,
        sid,
        user_id,
        if is_admin { 1 } else { 0 },
        now.to_rfc3339(),
        exp.to_rfc3339()
    )
    .execute(pool)
    .await?;

    let mut c = Cookie::new(COOKIE_NAME, sid);
    c.set_http_only(true);
    c.set_path("/");
    cookies.add(c);
    Ok(())
}

pub async fn destroy_session(pool: &PgPool, cookies: &Cookies) -> sqlx::Result<()> {
    if let Some(c) = cookies.get(COOKIE_NAME) {
        let sid = c.value().to_string();
        let _ = sqlx::query!(r#"DELETE FROM sessions WHERE id = $1"#, sid)
            .execute(pool)
            .await;
        cookies.remove(Cookie::from(COOKIE_NAME));
    }
    Ok(())
}

/// Returns Some((user_id, is_admin)) if cookie is valid & not expired.
pub async fn current_user_id(pool: &PgPool, cookies: &Cookies) -> Option<(String, bool)> {
    let sid = cookies.get(COOKIE_NAME)?.value().to_string();
    let row = sqlx::query!(
        r#"SELECT user_id, is_admin, expires_at FROM sessions WHERE id = $1 LIMIT 1"#,
        sid
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()?;

    // expires_at disimpan sebagai TEXT (RFC3339)
    let exp = chrono::DateTime::parse_from_rfc3339(&row.expires_at)
        .ok()?
        .with_timezone(&Utc);

    if exp < Utc::now() {
        // cleanup jika expired
        let _ = sqlx::query!(r#"DELETE FROM sessions WHERE id = $1"#, sid)
            .execute(pool)
            .await;
        return None;
    }

    Some((row.user_id, row.is_admin != 0))
}
