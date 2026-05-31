{%- if downstream == "redis" %}
//! Key/value store backed by Redis, with an in-memory fallback for tests and local runs.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use redis::AsyncCommands;
use redis::aio::ConnectionManager;

use crate::metrics::ErrorKind;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("redis error")]
    Redis(#[from] redis::RedisError),
}

impl StoreError {
    pub fn kind(&self) -> ErrorKind {
        ErrorKind::Downstream
    }
}

/// `ConnectionManager` is a single multiplexed connection that reconnects on its own
/// (not a pool). The in-memory variant keeps the same interface so tests need no server.
#[derive(Clone)]
pub enum Store {
    Redis(ConnectionManager),
    InMemory(Arc<Mutex<HashMap<String, String>>>),
}

impl Store {
    pub async fn connect(url: &str) -> Result<Self, StoreError> {
        let client = redis::Client::open(url)?;
        Ok(Store::Redis(ConnectionManager::new(client).await?))
    }

    pub fn in_memory() -> Self {
        Store::InMemory(Default::default())
    }

    pub async fn get(&self, key: &str) -> Result<Option<String>, StoreError> {
        match self {
            Store::Redis(manager) => Ok(manager.clone().get(key).await?),
            // Lock spans only the map access, never an `.await`.
            Store::InMemory(map) => Ok(map.lock().unwrap().get(key).cloned()),
        }
    }

    pub async fn set(&self, key: &str, value: &str) -> Result<(), StoreError> {
        match self {
            Store::Redis(manager) => {
                let _: () = manager.clone().set(key, value).await?;
                Ok(())
            }
            Store::InMemory(map) => {
                map.lock().unwrap().insert(key.to_string(), value.to_string());
                Ok(())
            }
        }
    }
}
{%- elif downstream == "http-service" %}
//! Client for the downstream HTTP service: timeouts, reqwest's native retry budget{% if circuit_breaker %}, and a circuit breaker{% endif %}.

use std::time::Duration;

use http::Method;
use http::StatusCode;

use crate::metrics::ErrorKind;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("downstream request failed")]
    Request(#[from] reqwest::Error),
    {%- if circuit_breaker %}
    #[error("circuit breaker is open")]
    CircuitOpen,
    {%- endif %}
}

impl StoreError {
    pub fn kind(&self) -> ErrorKind {
        match self {
            StoreError::Request(e) if e.is_timeout() => ErrorKind::Timeout,
            _ => ErrorKind::Downstream,
        }
    }
}

{%- if circuit_breaker %}
/// Breaker policy: open after 5 consecutive failures, then probe on an exponential backoff.
type Breaker = failsafe::StateMachine<
    failsafe::failure_policy::ConsecutiveFailures<failsafe::backoff::Exponential>,
    (),
>;
{%- endif %}

#[derive(Clone)]
pub struct Store {
    client: reqwest::Client,
    base_url: std::sync::Arc<str>,
    {%- if circuit_breaker %}
    breaker: std::sync::Arc<Breaker>,
    {%- endif %}
}

impl Store {
    pub fn connect(base_url: &str, timeout: Duration) -> Result<Self, StoreError> {
        let host = base_url
            .split("://")
            .nth(1)
            .and_then(|h| h.split('/').next())
            .and_then(|h| h.split(':').next())
            .unwrap_or(base_url)
            .to_string();
        let client = reqwest::Client::builder()
            // Overall per-request deadline: covers connect, send, and receiving the body.
            .timeout(timeout)
            .pool_max_idle_per_host(16)
            .pool_idle_timeout(Duration::from_secs(30))
            // Native retry: budget defaults to 20% extra load; retry idempotent 503s.
            .retry(
                reqwest::retry::for_host(host)
                    .max_retries_per_request(3)
                    .classify_fn(|rr| match (rr.method(), rr.status()) {
                        (&Method::GET, Some(StatusCode::SERVICE_UNAVAILABLE)) => rr.retryable(),
                        _ => rr.success(),
                    }),
            )
            .build()?;
        Ok(Store {
            client,
            base_url: base_url.into(),
            {%- if circuit_breaker %}
            breaker: std::sync::Arc::new(
                failsafe::Config::new()
                    .failure_policy(failsafe::failure_policy::consecutive_failures(
                        5,
                        failsafe::backoff::exponential(
                            Duration::from_secs(1),
                            Duration::from_secs(30),
                        ),
                    ))
                    .build(),
            ),
            {%- endif %}
        })
    }

    pub async fn get(&self, key: &str) -> Result<Option<String>, StoreError> {
        {%- if circuit_breaker %}
        use failsafe::futures::CircuitBreaker;
        let call = async {
            let resp = self
                .client
                .get(format!("{}/{key}", self.base_url))
                .send()
                .await?;
            if resp.status() == StatusCode::NOT_FOUND {
                return Ok::<_, reqwest::Error>(None);
            }
            Ok(Some(resp.error_for_status()?.text().await?))
        };
        self.breaker
            .call(call)
            .await
            .map_err(|e| match e {
                failsafe::Error::Inner(e) => StoreError::Request(e),
                failsafe::Error::Rejected => StoreError::CircuitOpen,
            })
        {%- else %}
        let resp = self
            .client
            .get(format!("{}/{key}", self.base_url))
            .send()
            .await?;
        if resp.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        Ok(Some(resp.error_for_status()?.text().await?))
        {%- endif %}
    }

    pub async fn set(&self, key: &str, value: &str) -> Result<(), StoreError> {
        self.client
            .put(format!("{}/{key}", self.base_url))
            .body(value.to_string())
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }
}
{%- endif %}
