// src/email.rs
//
// SMTP email sender using lettre.
// Config is loaded from the `smtp_settings` table (single row, id=1).
// Falls back to a no-op log if SMTP is disabled or misconfigured.
//
// Email subjects and message bodies are configurable through environment
// variables. Built-in defaults are written in English.

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
// Email template helpers
// ---------------------------------------------------------------------------

fn env_template(key: &str, default_value: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default_value.to_string())
}

fn render_template(template: &str, values: &[(&str, &str)]) -> String {
    values.iter().fold(template.to_string(), |acc, (key, value)| {
        acc.replace(&format!("{{{{{key}}}}}"), value)
    })
}

// ---------------------------------------------------------------------------
// Email templates
// ---------------------------------------------------------------------------

/// Send a password reset link for the forgot-password flow.
pub async fn send_reset(pool: &PgPool, to_email: &str, token: &str, base_url: &str) {
    let cfg = SmtpConfig::load(pool).await;
    let reset_url = format!("{base_url}/public/auth/reset_password.html?token={token}");

    let subject = env_template(
        "EMAIL_RESET_PASSWORD_SUBJECT",
        "Reset your PPV Stream password",
    );

    let template = env_template(
        "EMAIL_RESET_PASSWORD_HTML",
        r#"
<p>You are receiving this email because a password reset was requested for your PPV Stream account.</p>
<p><a href="{{reset_url}}" style="background:#dc3545;color:#fff;padding:10px 20px;text-decoration:none;border-radius:5px;display:inline-block;">Reset Password</a></p>
<p>This link is valid for 2 hours. If you did not request this, you can safely ignore this email.</p>
<p>You can also copy and paste this link into your browser:<br><code>{{reset_url}}</code></p>
"#,
    );

    let html = render_template(&template, &[("reset_url", &reset_url)]);

    if let Err(e) = send(&cfg, to_email, "", &subject, &html).await {
        warn!("send_reset failed: {e}");
    }
}

/// Send a notification that the account password was changed successfully.
pub async fn send_password_changed(pool: &PgPool, to_email: &str, username: &str) {
    let cfg = SmtpConfig::load(pool).await;

    let subject = env_template(
        "EMAIL_PASSWORD_CHANGED_SUBJECT",
        "Your PPV Stream password was changed",
    );

    let template = env_template(
        "EMAIL_PASSWORD_CHANGED_HTML",
        r#"
<p>Hello <b>{{username}}</b>,</p>
<p>Your PPV Stream account password was changed successfully.</p>
<p>If you did not make this change, please contact the administrator immediately or use the forgot-password feature to secure your account.</p>
<p>This is an automated email. Please do not reply.</p>
"#,
    );

    let html = render_template(&template, &[("username", username)]);

    if let Err(e) = send(&cfg, to_email, username, &subject, &html).await {
        warn!("send_password_changed failed: {e}");
    }
}

/// Send a test email from the admin SMTP settings page.
pub async fn send_test(cfg: &SmtpConfig, to_email: &str) -> Result<()> {
    let subject = env_template(
        "EMAIL_TEST_SUBJECT",
        "PPV Stream test email",
    );

    let html = env_template(
        "EMAIL_TEST_HTML",
        "<p>This is a test email from <b>PPV Stream</b>. Your SMTP configuration is working successfully.</p>",
    );

    send(cfg, to_email, "Admin", &subject, &html).await
}