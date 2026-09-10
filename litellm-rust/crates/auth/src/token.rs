use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use tokio::sync::Mutex;
use veil::Redact;

use crate::AuthError;

use super::secret::SecretValue;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolvedCredential {
    Static(SecretValue),
    AccessToken {
        token: SecretValue,
        expires_on: Option<SystemTime>,
    },
}

impl ResolvedCredential {
    pub fn secret(&self) -> &SecretValue {
        match self {
            Self::Static(secret) | Self::AccessToken { token: secret, .. } => secret,
        }
    }
}

pub type TokenFuture<'a> =
    Pin<Box<dyn Future<Output = Result<ResolvedCredential, AuthError>> + Send + 'a>>;

pub trait TokenProvider: std::fmt::Debug + Send + Sync {
    fn acquire(&self) -> TokenFuture<'_>;
}

#[derive(Clone, Redact)]
pub struct TokenProviderHandle(#[redact(with = "[REDACTED]")] Arc<dyn TokenProvider>);

impl TokenProviderHandle {
    pub fn new(caller: Arc<dyn TokenProvider>) -> Self {
        Self(caller)
    }

    pub async fn acquire(&self) -> Result<ResolvedCredential, AuthError> {
        self.0.acquire().await
    }
}

#[derive(Clone, Debug)]
pub struct CachedTokenProvider {
    provider: TokenProviderHandle,
    state: Arc<Mutex<Option<ResolvedCredential>>>,
    refresh_before: Duration,
}

impl CachedTokenProvider {
    pub fn new(provider: TokenProviderHandle, refresh_before: Duration) -> Self {
        Self {
            provider,
            state: Arc::new(Mutex::new(None)),
            refresh_before,
        }
    }

    fn reusable(&self, credential: &ResolvedCredential, now: SystemTime) -> bool {
        match credential {
            ResolvedCredential::Static(_) => true,
            ResolvedCredential::AccessToken {
                expires_on: Some(expires_on),
                ..
            } => now
                .checked_add(self.refresh_before)
                .is_some_and(|refresh_at| *expires_on > refresh_at),
            ResolvedCredential::AccessToken {
                expires_on: None, ..
            } => false,
        }
    }
}

impl TokenProvider for CachedTokenProvider {
    fn acquire(&self) -> TokenFuture<'_> {
        Box::pin(async move {
            let mut state = self.state.lock().await;
            let now = SystemTime::now();
            if let Some(credential) = state.as_ref()
                && self.reusable(credential, now)
            {
                return Ok(credential.clone());
            }
            let credential = self.provider.acquire().await?;
            if self.reusable(&credential, now) {
                *state = Some(credential.clone());
            } else {
                *state = None;
            }
            Ok(credential)
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, SystemTime};

    use super::*;

    #[derive(Debug)]
    struct CountingProvider(AtomicUsize);

    impl TokenProvider for CountingProvider {
        fn acquire(&self) -> TokenFuture<'_> {
            Box::pin(async move {
                let count = self.0.fetch_add(1, Ordering::SeqCst);
                Ok(ResolvedCredential::AccessToken {
                    token: SecretValue::new(format!("token-{count}")),
                    expires_on: Some(SystemTime::now() + Duration::from_secs(600)),
                })
            })
        }
    }

    #[tokio::test]
    async fn valid_token_is_cached_across_acquisitions() {
        let source = Arc::new(CountingProvider(AtomicUsize::new(0)));
        let cached = CachedTokenProvider::new(
            TokenProviderHandle::new(source.clone()),
            Duration::from_secs(30),
        );

        let first = cached.acquire().await.unwrap();
        let second = cached.acquire().await.unwrap();

        assert_eq!(first, second);
        assert_eq!(source.0.load(Ordering::SeqCst), 1);
    }
}
