
// src/email.rs

use tracing::info;

/// Demo email sender: logs reset links to console.
pub async fn send_reset(email: &str, token: &str) {
    info!(%email, %token, "Password reset link (demo): /auth/reset?token={token}");
}
