//! Item store. With no downstream URL it keeps items in memory, so the service runs with no
//! dependencies; with a URL it forwards to `{url}/items/{key}` over a shared, reused client.
//! See the connection-management skill for why the client is built once and shared.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Context;
use http::{Method, StatusCode};

use crate::metrics::ErrorKind;

/// Per-request timeout for each downstream call. Kept short so a slow downstream surfaces as a
/// fast 502 rather than consuming our own request budget.
const DOWNSTREAM_TIMEOUT: Duration = Duration::from_secs(5);

/// Retry budget per request for idempotent downstream calls (GET/PUT on a 503).
const DOWNSTREAM_MAX_RETRIES: u32 = 2;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("downstream request failed")]
    Request(#[from] reqwest::Error),
}

impl StoreError {
    pub fn kind(&self) -> ErrorKind {
        match self {
            StoreError::Request(e) if e.is_timeout() => ErrorKind::Timeout,
            _ => ErrorKind::Downstream,
        }
    }
}

/// Cheaply clonable handle to our backend.
#[derive(Clone)]
pub enum Store {
    InMemory(Arc<Mutex<HashMap<String, String>>>),
    Http {
        client: reqwest::Client,
        base_url: Arc<str>,
    },
}

impl Store {
    /// In-memory when `downstream_url` is `None`, otherwise an HTTP forwarder.
    pub fn new(downstream_url: Option<&str>) -> anyhow::Result<Self> {
        Ok(match downstream_url {
            Some(url) => Store::Http {
                client: build_client(url)?,
                base_url: url.trim_end_matches('/').into(),
            },
            None => Store::InMemory(Default::default()),
        })
    }

    /// One-shot startup reachability check. `None` for the in-memory store (nothing to probe).
    pub async fn probe(&self) -> Option<Result<(), StoreError>> {
        match self {
            Store::InMemory(_) => None,
            Store::Http { client, base_url } => Some(
                client
                    .get(format!("{base_url}/health"))
                    .send()
                    .await
                    .and_then(|resp| resp.error_for_status())
                    .map(|_| ())
                    .map_err(StoreError::from),
            ),
        }
    }

    pub async fn get(&self, key: &str) -> Result<Option<String>, StoreError> {
        match self {
            // Lock spans only the map access, never an `.await`: holding a std Mutex across a
            // yield point would block the worker thread.
            Store::InMemory(map) => Ok(map.lock().unwrap_or_else(|e| e.into_inner()).get(key).cloned()),
            Store::Http { client, base_url } => {
                let resp = client.get(format!("{base_url}/items/{key}")).send().await?;
                if resp.status() == StatusCode::NOT_FOUND {
                    return Ok(None);
                }
                Ok(Some(resp.error_for_status()?.text().await?))
            }
        }
    }

    pub async fn set(&self, key: &str, value: &str) -> Result<(), StoreError> {
        match self {
            Store::InMemory(map) => {
                map.lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(key.to_string(), value.to_string());
                Ok(())
            }
            Store::Http { client, base_url } => {
                client
                    .put(format!("{base_url}/items/{key}"))
                    .body(value.to_string())
                    .send()
                    .await?
                    .error_for_status()?;
                Ok(())
            }
        }
    }
}

/// Builds the shared client once. Reusing it keeps the connection pool and TLS sessions warm; the
/// retry budget caps amplification at ~20% and retries idempotent GET/PUT 503s.
fn build_client(downstream_url: &str) -> anyhow::Result<reqwest::Client> {
    // reqwest::Url handles ports, userinfo, and IPv6 literals that naive string splitting mangles.
    let host = reqwest::Url::parse(downstream_url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_string))
        .unwrap_or_else(|| downstream_url.to_string());
    reqwest::Client::builder()
        .timeout(DOWNSTREAM_TIMEOUT)
        .pool_max_idle_per_host(16)
        .pool_idle_timeout(Duration::from_secs(30))
        .retry(
            reqwest::retry::for_host(host)
                .max_retries_per_request(DOWNSTREAM_MAX_RETRIES)
                .classify_fn(|rr| match (rr.method(), rr.status()) {
                    (&Method::GET | &Method::PUT, Some(StatusCode::SERVICE_UNAVAILABLE)) => {
                        rr.retryable()
                    }
                    _ => rr.success(),
                }),
        )
        .build()
        .context("build downstream HTTP client")
}
