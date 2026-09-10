use crate::Error;
use crate::providers::anthropic::messages::transformation::complete_anthropic_url;

use super::super::types::ResolvedChatCompletionsRequest;

pub(super) fn resolve(request: &ResolvedChatCompletionsRequest<'_>) -> Result<String, Error> {
    let env_lookup = |key: &str| std::env::var(key).ok();
    Ok(complete_anthropic_url(request.api_base, &env_lookup))
}
