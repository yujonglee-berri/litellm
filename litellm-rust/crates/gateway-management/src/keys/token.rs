use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use litellm_gateway_auth::hash_token;
use rand::RngCore;

use crate::error::Error;

pub const GENERATED_KEY_BYTES: usize = 16;
pub const MINIMUM_CUSTOM_KEY_LENGTH: usize = 16;
pub const MAXIMUM_KEY_LENGTH: usize = 100;

pub fn generate_plaintext_key() -> String {
    let mut bytes = [0u8; GENERATED_KEY_BYTES];
    rand::thread_rng().fill_bytes(&mut bytes);
    format!("sk-{}", URL_SAFE_NO_PAD.encode(bytes))
}

pub fn abbreviate_api_key(api_key: &str) -> String {
    if api_key.len() < MINIMUM_CUSTOM_KEY_LENGTH {
        return "sk-...".to_string();
    }
    format!("sk-...{}", &api_key[api_key.len() - 4..])
}

pub fn hash_if_needed(key: &str) -> String {
    if key.starts_with("sk-") {
        hash_token(key)
    } else {
        key.to_string()
    }
}

pub fn validate_custom_key(key: &str) -> Result<(), Error> {
    if !key.starts_with("sk-") {
        let masked = if key.len() > 8 {
            format!("{}****{}", &key[..4], &key[key.len() - 4..])
        } else {
            "****".to_string()
        };
        return Err(Error::bad_request(
            format!(
                "Invalid key format. LiteLLM Virtual Key must start with 'sk-'. Received: {masked}"
            ),
            Some("key"),
        ));
    }
    if key.len() < MINIMUM_CUSTOM_KEY_LENGTH {
        return Err(Error::bad_request(
            format!(
                "Invalid key format. LiteLLM Virtual Key must be at least {MINIMUM_CUSTOM_KEY_LENGTH} characters long."
            ),
            Some("key"),
        ));
    }
    if key.len() > MAXIMUM_KEY_LENGTH || !is_sk_charset(key) {
        return Err(Error::bad_request("Invalid key format.", Some("key")));
    }
    Ok(())
}

pub fn is_key_identifier(key: &str) -> bool {
    is_sk_charset(key) || is_token_hash(key)
}

fn is_sk_charset(key: &str) -> bool {
    let Some(rest) = key.strip_prefix("sk-") else {
        return false;
    };
    !rest.is_empty()
        && rest.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '_' || character == '-'
        })
}

fn is_token_hash(key: &str) -> bool {
    key.len() == 64 && key.chars().all(|character| character.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::{abbreviate_api_key, generate_plaintext_key, validate_custom_key};
    use litellm_gateway_auth::hash_token;

    #[test]
    fn generated_keys_are_sk_secrets_that_hash() {
        let key = generate_plaintext_key();
        assert!(key.starts_with("sk-"));
        assert!(key.len() >= 16);
        validate_custom_key(&key).expect("generated key is valid");
        let hash = hash_token(&key);
        assert_eq!(hash.len(), 64);
        assert_ne!(hash, key);
    }

    #[test]
    fn abbreviation_keeps_only_the_last_four() {
        assert_eq!(abbreviate_api_key("sk-abcdefghijklmnop"), "sk-...mnop");
        assert_eq!(abbreviate_api_key("short"), "sk-...");
    }

    #[test]
    fn custom_keys_must_be_sk_and_long_enough() {
        assert!(validate_custom_key("not-a-key").is_err());
        assert!(validate_custom_key("sk-short").is_err());
        assert!(validate_custom_key("sk-abcdefghijklmnop").is_ok());
    }
}
