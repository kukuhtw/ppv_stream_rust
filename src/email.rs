// src/email.rs
//
// SMTP email sender using lettre.
// Config is loaded from the `smtp_settings` table (single row, id=1).
// Falls back to a no-op log if SMTP is disabled or misconfigured.

use anyhow::{anyhow, Result};
use lettre::{
    message::{header::ContentType, Mailbox},
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};
use sqlx::PgPool;
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// Config struct loaded from DB
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct SmtpConfig {
    pub host:       String,
    pub port:       u16,
    pub username:   String,
    pub password:   String,
    pub from_email: String,
    pub from_name:  String,
    pub use_tls:    bool,
    pub enabled:    bool,
}

impl SmtpConfig {
    /// Load SMTP config from `smtp_settings` row id=1.
    pub async fn load(pool: &PgPool) -> Self {
        let row = sqlx::query(
            "SELECT host, port, username, password, from_email, from_name, use_tls, enabled
             FROM smtp_settings WHERE id = 1"
        )
        .fetch_optional(pool)
        .await;

        match row {
            Ok(Some(r)) => {
                use sqlx::Row;
                Self {
                    host:       r.try_get("host").unwrap_or_default(),
                    port:       r.try_get::<i32, _>("port").unwrap_or(587) as u16,
                    username:   r.try_get("username").unwrap_or_default(),
                    password:   r.try_get("password").unwrap_or_default(),
                    from_email: r.try_get("from_email").unwrap_or_default(),
                    from_name:  r.try_get("from_name").unwrap_or_else(|_| "PPV Stream".into()),
                    use_tls:    r.try_get("use_tls").unwrap_or(true),
                    enabled:    r.try_get("enabled").unwrap_or(false),
                }
            }
            _ => Self::default(),
        }
    }

    fn is_ready(&self) -> bool {
        self.enabled
            && !self.host.is_empty()
            && !self.from_email.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Core send function
// ---------------------------------------------------------------------------

async fn send(cfg: &SmtpConfig, to_email: &str, to_name: &str, subject: &str, html: &str) -> Result<()> {
    if !cfg.is_ready() {
        info!(to = to_email, subject, "SMTP disabled — email not sent (logged only)");
        return Ok(());
    }

    let from_mailbox: Mailbox = format!("{} <{}>", cfg.from_name, cfg.from_email)
        .parse()
        .map_err(|e| anyhow!("invalid from address: {e}"))?;

    let to_mailbox: Mailbox = if to_name.is_empty() {
        to_email.parse().map_err(|e| anyhow!("invalid to address: {e}"))?
    } else {
        format!("{to_name} <{to_email}>")
            .parse()
            .map_err(|e| anyhow!("invalid to address: {e}"))?
    };

    let email = Message::builder()
        .from(from_mailbox)
        .to(to_mailbox)
        .subject(subject)
        .header(ContentType::TEXT_HTML)
        .body(html.to_string())
        .map_err(|e| anyhow!("build email: {e}"))?;

    let creds = Credentials::new(cfg.username.clone(), cfg.password.clone());

    let transport: AsyncSmtpTransport<Tokio1Executor> = if cfg.use_tls {
        AsyncSmtpTransport::<Tokio1Executor>::relay(&cfg.host)
            .map_err(|e| anyhow!("smtp relay: {e}"))?
            .port(cfg.port)
            .credentials(creds)
            .build()
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&cfg.host)
            .map_err(|e| anyhow!("smtp starttls: {e}"))?
            .port(cfg.port)
            .credentials(creds)
            .build()
    };

    transport
        .send(email)
        .await
        .map_err(|e| anyhow!("smtp send: {e}"))?;

    info!(to = to_email, subject, "Email sent");
    Ok(())
}

// ---------------------------------------------------------------------------
// Email templates
// ---------------------------------------------------------------------------

/// Kirim link reset password (digunakan oleh forgot-password flow).
pub async fn send_reset(pool: &PgPool, to_email: &str, token: &str, base_url: &str) {
    let cfg = SmtpConfig::load(pool).await;
    let reset_url = format!("{base_url}/public/auth/reset_password.html?token={token}");
    let html = format!(r#"
<p>Anda menerima email ini karena ada permintaan reset password untuk akun Anda.</p>
<p><a href="{reset_url}" style="background:#dc3545;color:#fff;padding:10px 20px;text-decoration:none;border-radius:5px;display:inline-block;">Reset Password</a></p>
<p>Link berlaku 2 jam. Jika bukan Anda yang meminta, abaikan email ini.</p>
<p>Atau salin link berikut ke browser:<br><code>{reset_url}</code></p>
"#);
    if let Err(e) = send(&cfg, to_email, "", "Reset Password Akun Anda", &html).await {
        warn!("send_reset failed: {e}");
    }
}

/// Notifikasi bahwa password telah berhasil diubah.
pub async fn send_password_changed(pool: &PgPool, to_email: &str, username: &str) {
    let cfg = SmtpConfig::load(pool).await;
    let html = format!(r#"
<p>Halo <b>{username}</b>,</p>
<p>Password akun Anda di <b>PPV Stream</b> baru saja berhasil diubah.</p>
<p>Jika Anda tidak melakukan perubahan ini, segera hubungi admin atau gunakan fitur <b>Lupa Password</b> untuk mengamankan akun Anda.</p>
<p>Email ini dikirim otomatis, mohon tidak membalas.</p>
"#);
    if let Err(e) = send(&cfg, to_email, username, "Password Anda Telah Diubah", &html).await {
        warn!("send_password_changed failed: {e}");
    }
}

/// Kirim test email dari admin settings.
pub async fn send_test(cfg: &SmtpConfig, to_email: &str) -> Result<()> {
    let html = "<p>Ini adalah test email dari <b>PPV Stream</b>. Konfigurasi SMTP Anda berhasil!</p>";
    send(cfg, to_email, "Admin", "Test Email PPV Stream", html).await
}
