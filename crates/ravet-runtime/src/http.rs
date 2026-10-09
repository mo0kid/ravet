//! Bounded, synchronous HTTPS POST for worker threads. No implicit activation.
use std::{fmt, time::Duration};
use ureq::{
    Agent,
    tls::{RootCerts, TlsConfig},
};

/// Explicit HTTP limits; constructing an options value performs no I/O.
#[derive(Clone, Debug)]
pub struct HttpOptions {
    pub timeout: Duration,
    pub maximum_response_bytes: usize,
    pub user_agent: String,
}
impl Default for HttpOptions {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(15),
            maximum_response_bytes: 65_536,
            user_agent: "RAVET".into(),
        }
    }
}
/// HTTP 4xx/5xx are successful transport results, with their status and body.
#[derive(Debug)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HttpError {
    InvalidRequest,
    Timeout,
    TooLarge,
    Transport,
}
impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidRequest => "invalid HTTPS request or limits",
            Self::Timeout => "HTTPS request timed out",
            Self::TooLarge => "HTTPS response exceeds caller's byte limit",
            Self::Transport => "HTTPS transport failed",
        })
    }
}
impl std::error::Error for HttpError {}
fn map_error(error: ureq::Error) -> HttpError {
    match error {
        ureq::Error::Timeout(_) => HttpError::Timeout,
        ureq::Error::BodyExceedsLimit(_) => HttpError::TooLarge,
        _ => HttpError::Transport,
    }
}
fn validate(url: &str, options: &HttpOptions) -> Result<(), HttpError> {
    let uri = url
        .parse::<ureq::http::Uri>()
        .map_err(|_| HttpError::InvalidRequest)?;
    if uri.scheme_str() != Some("https")
        || uri.host().is_none()
        || uri.authority().is_some_and(|a| a.as_str().contains('@'))
        || url.contains('#')
        || options.timeout.is_zero()
        || options.timeout > Duration::from_secs(300)
        || options.maximum_response_bytes > 16 * 1024 * 1024
        || options.user_agent.len() > 1024
        || ureq::http::HeaderValue::from_str(&options.user_agent).is_err()
    {
        return Err(HttpError::InvalidRequest);
    }
    Ok(())
}
/// HTTPS JSON POST with normal platform certificate validation. Redirects are
/// disabled, no cookie jar or credentials are attached, and environment proxy
/// selection is disabled. The deadline includes connection and body reading.
/// Reads bounded binary response data; callers choose their own text/JSON codec.
/// The body is limited to 16 MiB and response limits to 0..=16 MiB.
pub fn post_json(url: &str, body: &[u8], options: &HttpOptions) -> Result<HttpResponse, HttpError> {
    validate(url, options)?;
    let agent: Agent = Agent::config_builder()
        .https_only(true)
        .http_status_as_error(false)
        .max_redirects(0)
        .proxy(None)
        .timeout_global(Some(options.timeout))
        .tls_config(
            TlsConfig::builder()
                .root_certs(RootCerts::PlatformVerifier)
                .build(),
        )
        .build()
        .into();
    send(&agent, url, body, options)
}
fn send(
    agent: &Agent,
    url: &str,
    body: &[u8],
    options: &HttpOptions,
) -> Result<HttpResponse, HttpError> {
    if body.len() > 16 * 1024 * 1024 {
        return Err(HttpError::InvalidRequest);
    }
    let mut response = agent
        .post(url)
        .header("Content-Type", "application/json")
        .header("User-Agent", &options.user_agent)
        .send(body)
        .map_err(map_error)?;
    let status = response.status().as_u16();
    let bytes = response
        .body_mut()
        .with_config()
        .limit(options.maximum_response_bytes as u64)
        .read_to_vec()
        .map_err(map_error)?;
    Ok(HttpResponse {
        status,
        body: bytes,
    })
}

#[cfg(test)]
mod tests;
