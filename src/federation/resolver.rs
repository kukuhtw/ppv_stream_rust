use std::net::IpAddr;

/// Maximum allowed response body size when fetching remote ActivityPub objects.
const MAX_BODY_BYTES: usize = 128 * 1024; // 128 KB

/// Returns true when FEDERATION_DEV_HTTP_BYPASS is enabled.
/// Only for local dev / integration-test Docker environments.
fn dev_bypass_enabled() -> bool {
    std::env::var("FEDERATION_DEV_HTTP_BYPASS")
        .map(|v| matches!(v.trim(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
}

/// Returns `true` when the IP address is safe to contact from a server
/// context (i.e. not a private, loopback, link-local, or reserved range).
pub fn is_safe_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            !v4.is_private()
                && !v4.is_loopback()
                && !v4.is_link_local()
                && !v4.is_broadcast()
                && !v4.is_unspecified()
                && !v4.is_documentation()
        }
        IpAddr::V6(v6) => !v6.is_loopback() && !v6.is_multicast() && !v6.is_unspecified(),
    }
}

/// Extract the hostname and port from an `https://` URL.
fn extract_host_port(url: &str) -> Option<(String, u16)> {
    let rest = url.strip_prefix("https://")?;
    let end = rest.find('/').unwrap_or(rest.len());
    let hostport = &rest[..end];
    if let Some(colon) = hostport.rfind(':') {
        // Guard against IPv6 addresses: only treat colon as port separator
        // when what follows is a valid port number.
        if let Ok(port) = hostport[colon + 1..].parse::<u16>() {
            return Some((hostport[..colon].to_string(), port));
        }
    }
    Some((hostport.to_string(), 443))
}

/// Check that `url` is HTTPS and resolves only to publicly routable addresses.
///
/// When `FEDERATION_DEV_HTTP_BYPASS=1` all checks are skipped so that two
/// Docker containers (with private 172.x IPs) can federate during integration
/// testing.  Never set this in production.
async fn assert_safe_url(url: &str) -> Result<(), String> {
    if dev_bypass_enabled() {
        if url.is_empty() {
            return Err("URL is empty".into());
        }
        return Ok(());
    }

    if !url.starts_with("https://") {
        return Err("remote URL must use HTTPS".into());
    }

    let (host, port) =
        extract_host_port(url).ok_or_else(|| "failed to parse URL host".to_string())?;

    if host == "localhost" || host == "127.0.0.1" || host == "::1" {
        return Err("remote URL must not target localhost".into());
    }

    let lookup = tokio::net::lookup_host(format!("{}:{}", host, port))
        .await
        .map_err(|e| format!("DNS resolution failed for {}: {}", host, e))?;

    for addr in lookup {
        if !is_safe_ip(addr.ip()) {
            return Err(format!(
                "URL {} resolves to private/reserved address {}",
                url,
                addr.ip()
            ));
        }
    }

    Ok(())
}

/// Fetch an ActivityPub JSON-LD document from a remote URL.
///
/// Performs SSRF protection (HTTPS only, no private IPs) and enforces a
/// 128 KB body limit and a 10-second timeout.  When
/// `FEDERATION_DEV_HTTP_BYPASS=1` the SSRF check is skipped so that
/// integration-test Docker containers can reach each other via private IPs.
pub async fn fetch_remote_object(url: &str) -> Result<serde_json::Value, String> {
    assert_safe_url(url).await?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::limited(3))
        .danger_accept_invalid_certs(dev_bypass_enabled()) // allow self-signed in dev
        .build()
        .map_err(|e| format!("HTTP client build error: {e}"))?;

    let response = client
        .get(url)
        .header(
            reqwest::header::ACCEPT,
            "application/activity+json, application/ld+json; profile=\"https://www.w3.org/ns/activitystreams\"",
        )
        .send()
        .await
        .map_err(|e| format!("request to {} failed: {e}", url))?;

    if !response.status().is_success() {
        return Err(format!(
            "remote {} returned HTTP {}",
            url,
            response.status()
        ));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("failed to read remote body: {e}"))?;

    if bytes.len() > MAX_BODY_BYTES {
        return Err(format!(
            "remote body exceeds {} byte limit ({} bytes)",
            MAX_BODY_BYTES,
            bytes.len()
        ));
    }

    serde_json::from_slice(&bytes).map_err(|e| format!("remote body is not valid JSON: {e}"))
}

/// Fetch and validate a remote actor document, returning its public key PEM if present.
pub async fn fetch_remote_actor_key(actor_url: &str) -> Result<(String, String), String> {
    let doc = fetch_remote_object(actor_url).await?;

    // Validate that this is an Actor type we recognise
    let actor_type = doc.get("type").and_then(|t| t.as_str()).unwrap_or_default();
    if !matches!(
        actor_type,
        "Person" | "Organization" | "Application" | "Service" | "Group"
    ) {
        return Err(format!(
            "remote document at {} has unsupported type: {}",
            actor_url, actor_type
        ));
    }

    let public_key = doc
        .get("publicKey")
        .ok_or_else(|| format!("actor at {} has no publicKey", actor_url))?;

    let key_id = public_key
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "actor publicKey has no id".to_string())?
        .to_string();

    let pem = public_key
        .get("publicKeyPem")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "actor publicKey has no publicKeyPem".to_string())?
        .to_string();

    Ok((key_id, pem))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn private_ipv4_is_blocked() {
        assert!(!is_safe_ip(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
        assert!(!is_safe_ip(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
        assert!(!is_safe_ip(IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1))));
        assert!(!is_safe_ip(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))));
        assert!(!is_safe_ip(IpAddr::V4(Ipv4Addr::new(169, 254, 0, 1))));
    }

    #[test]
    fn loopback_ipv6_is_blocked() {
        assert!(!is_safe_ip(IpAddr::V6(Ipv6Addr::LOCALHOST)));
        assert!(!is_safe_ip(IpAddr::V6(Ipv6Addr::UNSPECIFIED)));
    }

    #[test]
    fn public_ip_is_allowed() {
        // 1.1.1.1 (Cloudflare)
        assert!(is_safe_ip(IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1))));
        // 8.8.8.8 (Google)
        assert!(is_safe_ip(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))));
    }

    #[test]
    fn http_url_is_rejected() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(assert_safe_url("http://example.com/actor"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("HTTPS"));
    }

    #[test]
    fn localhost_url_is_rejected() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(assert_safe_url("https://localhost/actor"));
        assert!(result.is_err());
    }

    #[test]
    fn host_port_extraction() {
        assert_eq!(
            extract_host_port("https://example.com/users/alice"),
            Some(("example.com".into(), 443))
        );
        assert_eq!(
            extract_host_port("https://example.com:8443/inbox"),
            Some(("example.com".into(), 8443))
        );
        assert_eq!(
            extract_host_port("http://example.com"),
            None // http:// is not accepted
        );
    }
}
