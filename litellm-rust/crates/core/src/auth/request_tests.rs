use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use reqwest::header::{AUTHORIZATION, HeaderName, HeaderValue};
use reqwest::{Method, Request, Url};

use super::*;
use crate::auth::{ResolvedCredential, TokenFuture, TokenProvider};

fn request(path: &str) -> Request {
    let mut request = Request::new(
        Method::POST,
        Url::parse(&format!("https://example.com/{path}?version=1")).unwrap(),
    );
    *request.body_mut() = Some("payload".into());
    request
        .headers_mut()
        .insert("x-trace", HeaderValue::from_static("trace"));
    request
}

#[tokio::test]
async fn shared_auth_preserves_request_across_routes_and_redacts_credentials() {
    let auth: Box<dyn Auth> = Box::new(
        HeaderAuth::bearer(
            &SecretValue::new("private-token"),
            ExistingHeaderBehavior::Reject,
        )
        .unwrap(),
    );
    for path in ["ocr", "chat/completions", "embeddings"] {
        let authenticated = auth.authenticate(request(path)).await.unwrap();
        assert_eq!(authenticated.method(), Method::POST);
        assert_eq!(authenticated.url().path(), format!("/{path}"));
        assert_eq!(authenticated.url().query(), Some("version=1"));
        assert_eq!(
            authenticated.body().unwrap().as_bytes(),
            Some(b"payload".as_slice())
        );
        assert_eq!(authenticated.headers()["x-trace"], "trace");
        assert_eq!(
            authenticated.headers()[AUTHORIZATION],
            "Bearer private-token"
        );
        assert!(authenticated.headers()[AUTHORIZATION].is_sensitive());
        assert!(!format!("{authenticated:?}").contains("private-token"));
    }
}

#[test]
fn invalid_credentials_are_rejected_without_exposing_values() {
    for secret in ["", "  ", "private\r\ninjected: value"] {
        let error = HeaderAuth::bearer(&SecretValue::new(secret), ExistingHeaderBehavior::Reject)
            .unwrap_err();
        assert!(!error.to_string().contains("private"));
    }
    let auth = HeaderAuth::new(
        [(
            HeaderName::from_static("x-api-key"),
            SecretValue::new("private-key"),
        )],
        ExistingHeaderBehavior::Reject,
    )
    .unwrap();
    assert!(!format!("{auth:?}").contains("private-key"));
}

#[test]
fn header_conflicts_are_case_insensitive_and_preserve_redacts_every_value() {
    let mut request = request("ocr");
    request
        .headers_mut()
        .append(AUTHORIZATION, HeaderValue::from_static("caller-one"));
    request
        .headers_mut()
        .append(AUTHORIZATION, HeaderValue::from_static("caller-two"));
    let preserved = HeaderAuth::bearer(
        &SecretValue::new("configured"),
        ExistingHeaderBehavior::Preserve,
    )
    .unwrap()
    .apply(request)
    .unwrap();
    let values = preserved
        .headers()
        .get_all(AUTHORIZATION)
        .iter()
        .collect::<Vec<_>>();
    assert_eq!(values.len(), 2);
    assert_eq!(values[0], "caller-one");
    assert_eq!(values[1], "caller-two");
    assert!(values.iter().all(|value| value.is_sensitive()));
    let rejected = HeaderAuth::new(
        [(
            HeaderName::from_bytes(b"AUTHORIZATION").unwrap(),
            SecretValue::new("configured"),
        )],
        ExistingHeaderBehavior::Reject,
    )
    .unwrap()
    .apply(preserved)
    .unwrap_err();
    assert_eq!(
        rejected,
        AuthError::Configuration(AuthConfigurationError::ExistingCredentialHeader)
    );
}

#[derive(Debug)]
struct RotatingToken(AtomicUsize);

impl TokenProvider for RotatingToken {
    fn acquire(&self) -> TokenFuture<'_> {
        Box::pin(async move {
            let count = self.0.fetch_add(1, Ordering::SeqCst);
            Ok(ResolvedCredential::Static(SecretValue::new(format!(
                "token-{count}"
            ))))
        })
    }
}

#[tokio::test]
async fn token_auth_reacquires_for_each_attempt_and_skips_existing_auth() {
    let provider = Arc::new(RotatingToken(AtomicUsize::new(0)));
    let auth = BearerTokenAuth::new(
        TokenProviderHandle::new(provider.clone()),
        ExistingHeaderBehavior::Preserve,
    );
    for count in 0..2 {
        let authenticated = auth.authenticate(request("messages")).await.unwrap();
        assert_eq!(
            authenticated.headers()[AUTHORIZATION],
            format!("Bearer token-{count}")
        );
    }
    let mut request = request("messages");
    request
        .headers_mut()
        .insert(AUTHORIZATION, HeaderValue::from_static("caller"));
    let authenticated = auth.authenticate(request).await.unwrap();
    assert_eq!(authenticated.headers()[AUTHORIZATION], "caller");
    assert!(authenticated.headers()[AUTHORIZATION].is_sensitive());
    assert_eq!(provider.0.load(Ordering::SeqCst), 2);
}

#[derive(Debug)]
struct FailedToken;

impl TokenProvider for FailedToken {
    fn acquire(&self) -> TokenFuture<'_> {
        Box::pin(async { Err(AuthError::EmptyCallerCredential) })
    }
}

#[tokio::test]
async fn failed_acquisition_does_not_return_an_unauthenticated_request() {
    let auth = BearerTokenAuth::new(
        TokenProviderHandle::new(Arc::new(FailedToken)),
        ExistingHeaderBehavior::Reject,
    );
    assert_eq!(
        auth.authenticate(request("ocr")).await.unwrap_err(),
        AuthError::EmptyCallerCredential
    );
}
