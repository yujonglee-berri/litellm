use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use aws_credential_types::Credentials;

use crate::constants::AWS_PROFILE_NAME;
use crate::credentials::{
    AMBIENT_CREDENTIALS_TTL, STATIC_CREDENTIALS_TTL, cache_key, credential_cache_ttl,
    get_cached_credentials, same_role_arns, set_cached_credentials,
};
use crate::*;

fn no_env(_: &str) -> Option<String> {
    None
}

fn parity_inputs() -> (String, Vec<u8>, BTreeMap<String, String>) {
    (
        "https://bedrock-runtime.us-east-1.amazonaws.com/model/amazon.titan-text-express-v1/invoke"
            .to_string(),
        br#"{"input":"hello"}"#.to_vec(),
        BTreeMap::from([("Content-Type".to_string(), "application/json".to_string())]),
    )
}

#[test]
fn reads_the_region_field_of_a_model_arn_not_the_account_id() {
    let (_, region) = bedrock_model_id_and_region(
        "bedrock/arn:aws:bedrock:us-west-2:123456789012:foundation-model/anthropic.claude-v2",
    );
    assert_eq!(region.as_deref(), Some("us-west-2"));
}

#[test]
fn classification_preserves_python_precedence() {
    let config = AwsAuthConfig {
        access_key_id: Some("ak".into()),
        secret_access_key: Some("sk".into()),
        session_token: Some("token".into()),
        region_name: Some("us-east-1".into()),
        session_name: Some("session".into()),
        profile_name: Some("profile".into()),
        role_name: Some("role".into()),
        web_identity_token: Some("oidc".into()),
        ..Default::default()
    };
    assert!(matches!(
        classify_auth(config, &no_env),
        AwsAuthFlow::WebIdentity { .. }
    ));
}

#[test]
fn debug_redacts_every_credential_value() {
    let config = AwsAuthConfig {
        access_key_id: Some("access-id".into()),
        secret_access_key: Some("secret-key".into()),
        session_token: Some("session-token".into()),
        web_identity_token: Some("web-token".into()),
        external_id: Some("external-id".into()),
        ..Default::default()
    };
    let flow = classify_auth(config.clone(), &no_env);
    let debug = format!("{config:?} {flow:?}");

    for secret in [
        "access-id",
        "secret-key",
        "session-token",
        "web-token",
        "external-id",
    ] {
        assert!(!debug.contains(secret));
    }
}

#[test]
fn cache_identity_still_includes_redacted_credentials() {
    let first = AwsAuthConfig {
        access_key_id: Some("first".into()),
        secret_access_key: Some("secret".into()),
        region_name: Some("us-east-1".into()),
        ..Default::default()
    };
    let second = AwsAuthConfig {
        access_key_id: Some("second".into()),
        ..first.clone()
    };

    let first_flow = classify_auth(first.clone(), &no_env);
    let second_flow = classify_auth(second.clone(), &no_env);
    assert_ne!(
        cache_key(&first, &first_flow),
        cache_key(&second, &second_flow)
    );
}

#[test]
fn classification_covers_fallthroughs() {
    let env = |key: &str| match key {
        AWS_PROFILE_NAME => Some("profile".into()),
        _ => None,
    };
    assert!(matches!(
        classify_auth(AwsAuthConfig::default(), &env),
        AwsAuthFlow::Profile { .. }
    ));
    assert!(matches!(
        classify_auth(
            AwsAuthConfig {
                access_key_id: Some("ak".into()),
                secret_access_key: Some("sk".into()),
                session_token: Some("token".into()),
                ..Default::default()
            },
            &no_env
        ),
        AwsAuthFlow::SessionToken { .. }
    ));
    assert!(matches!(
        classify_auth(
            AwsAuthConfig {
                access_key_id: Some("ak".into()),
                secret_access_key: Some("sk".into()),
                region_name: Some("us-east-1".into()),
                ..Default::default()
            },
            &no_env
        ),
        AwsAuthFlow::StaticKeys { .. }
    ));
    assert_eq!(
        classify_auth(AwsAuthConfig::default(), &no_env),
        AwsAuthFlow::DefaultChain
    );
}

#[tokio::test]
async fn static_credentials_do_not_use_network() {
    let credentials = resolve_credentials(
        AwsAuthConfig {
            access_key_id: Some("ak".into()),
            secret_access_key: Some("sk".into()),
            region_name: Some("us-east-1".into()),
            ..Default::default()
        },
        &no_env,
    )
    .await
    .expect("static credentials");
    assert_eq!(credentials.access_key_id(), "ak");
    assert_eq!(credentials.session_token(), None);
}

#[test]
fn cache_policy_matches_python_flows() {
    assert_eq!(
        credential_cache_ttl(&AwsAuthFlow::StaticKeys {
            access_key_id: "ak".into(),
            secret_access_key: "sk".into(),
            region_name: "us-east-1".into(),
        }),
        Some(STATIC_CREDENTIALS_TTL)
    );
    assert_eq!(
        credential_cache_ttl(&AwsAuthFlow::DefaultChain),
        Some(AMBIENT_CREDENTIALS_TTL)
    );
    assert_eq!(
        credential_cache_ttl(&AwsAuthFlow::SessionToken {
            access_key_id: "ak".into(),
            secret_access_key: "sk".into(),
            session_token: "token".into(),
        }),
        None
    );
    assert_eq!(
        credential_cache_ttl(&AwsAuthFlow::Profile {
            name: "profile".into()
        }),
        None
    );
    assert_eq!(
        credential_cache_ttl(&AwsAuthFlow::AssumeRole {
            role: "arn:aws:iam::123456789012:role/demo".into(),
            session_name: None,
        }),
        None
    );
    assert_eq!(
        credential_cache_ttl(&AwsAuthFlow::WebIdentity {
            token: "token".into(),
            role: "arn:aws:iam::123456789012:role/demo".into(),
            session_name: "session".into(),
        }),
        None
    );
}

#[test]
fn cache_round_trip_preserves_credentials() {
    let key = format!("cache-test-{}", std::process::id());
    let credentials = Credentials::new("cache-ak", "cache-sk", None, None, "test");
    set_cached_credentials(key.clone(), credentials, STATIC_CREDENTIALS_TTL);
    assert_eq!(
        get_cached_credentials(&key).map(|value| value.access_key_id().to_string()),
        Some("cache-ak".to_string())
    );
}

#[test]
fn same_role_comparison_matches_partition_account_and_role() {
    assert!(same_role_arns(
        "arn:aws:iam::123456789012:role/path/demo",
        "arn:aws:sts::123456789012:assumed-role/demo/session"
    ));
    assert!(!same_role_arns(
        "arn:aws:iam::123456789012:role/demo",
        "arn:aws:iam::999999999999:role/demo"
    ));
    assert!(!same_role_arns(
        "arn:aws:iam::123456789012:role/demo",
        "arn:aws-cn:iam::123456789012:role/demo"
    ));
    assert!(!same_role_arns(
        "arn:aws:iam::123456789012:user/demo",
        "arn:aws:iam::123456789012:role/demo"
    ));
}

#[test]
fn a_forwarded_client_header_is_not_folded_into_the_signature() {
    let (url, body, mut headers) = parity_inputs();
    headers.insert("x-request-id".to_string(), "abc-123".to_string());
    headers.insert("Accept-Encoding".to_string(), "gzip".to_string());
    headers.insert("x-amzn-trace-id".to_string(), "Root=1-abc".to_string());
    let signable = aws_signature_headers(&headers);

    assert!(!signable.contains_key("x-request-id"));
    assert!(!signable.contains_key("Accept-Encoding"));
    assert!(signable.contains_key("x-amzn-trace-id"));
    assert!(signable.contains_key("Content-Type"));

    let credentials = Credentials::new(
        "AKIDEXAMPLE",
        "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
        None,
        None,
        "test",
    );
    let signed = sign_bedrock_post(
        &url,
        &body,
        &signable,
        "us-east-1",
        &credentials,
        SystemTime::UNIX_EPOCH,
    )
    .expect("signs");
    let authorization = signed
        .get("Authorization")
        .expect("carries an authorization header");
    assert!(!authorization.contains("x-request-id"));
    assert!(!authorization.contains("accept-encoding"));
}

#[test]
fn signing_matches_botocore_golden_vector() {
    let (url, body, headers) = parity_inputs();
    let credentials = Credentials::new(
        "AKIDEXAMPLE",
        "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
        Some("session-token".to_string()),
        None,
        "test",
    );
    let signed = sign_bedrock_post(
        &url,
        &body,
        &headers,
        "us-east-1",
        &credentials,
        UNIX_EPOCH + std::time::Duration::from_secs(1_704_164_645),
    )
    .expect("golden signature");
    assert_eq!(
        signed.get("X-Amz-Date").map(String::as_str),
        Some("20240102T030405Z")
    );
    assert_eq!(
        signed.get("X-Amz-Security-Token").map(String::as_str),
        Some("session-token")
    );
    assert_eq!(
        signed.get("Authorization").map(String::as_str),
        Some(
            "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20240102/us-east-1/bedrock/aws4_request, SignedHeaders=content-type;host;x-amz-date;x-amz-security-token, Signature=55c027ef47527d3ad63f1735f9d099efdbc99f296ff914bd94e727e24ec0e464"
        )
    );
}

#[test]
fn signing_without_session_token_omits_security_header() {
    let (url, body, headers) = parity_inputs();
    let credentials = Credentials::new(
        "AKIDEXAMPLE",
        "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
        None,
        None,
        "test",
    );
    let signed = sign_bedrock_post(
        &url,
        &body,
        &headers,
        "us-east-1",
        &credentials,
        UNIX_EPOCH + std::time::Duration::from_secs(1_704_164_645),
    )
    .expect("signature");
    assert!(!signed.contains_key("X-Amz-Security-Token"));
}

#[ignore]
#[tokio::test]
async fn live_bedrock_invoke_model_returns_200() -> Result<(), Box<dyn std::error::Error>> {
    let access_key_id = std::env::var("AWS_BEDROCK_TEST_ACCESS_KEY_ID")?;
    let secret_access_key = std::env::var("AWS_BEDROCK_TEST_SECRET_ACCESS_KEY")?;
    let body = br#"{"anthropic_version":"bedrock-2023-05-31","max_tokens":1,"messages":[{"role":"user","content":[{"type":"text","text":"ping"}]}]}"#.to_vec();
    let headers = BTreeMap::from([("Content-Type".to_string(), "application/json".to_string())]);
    let credentials = resolve_credentials(
        AwsAuthConfig {
            access_key_id: Some(access_key_id),
            secret_access_key: Some(secret_access_key),
            region_name: Some("us-west-2".to_string()),
            ..Default::default()
        },
        &no_env,
    )
    .await?;
    let client = reqwest::Client::new();
    let mut failures = Vec::new();

    for region in ["us-west-2", "us-east-1"] {
        let url = format!(
            "https://bedrock-runtime.{region}.amazonaws.com/model/us.anthropic.claude-opus-4-8/invoke"
        );
        let signed_headers = sign_bedrock_post(
            &url,
            &body,
            &headers,
            region,
            &credentials,
            SystemTime::now(),
        )?;
        let mut request = client.post(&url).body(body.clone());
        for (name, value) in &headers {
            request = request.header(name, value);
        }
        for (name, value) in signed_headers {
            request = request.header(name, value);
        }
        let response = request.send().await?;
        let status = response.status();
        let response_body = response.text().await?;
        let snippet: String = response_body.chars().take(240).collect();
        println!("region={region} status={status} response={snippet}");
        if status == reqwest::StatusCode::OK {
            return Ok(());
        }
        failures.push(format!("{region}: {status} {snippet}"));
    }

    panic!(
        "no Bedrock region returned HTTP 200: {}",
        failures.join("; ")
    );
}
