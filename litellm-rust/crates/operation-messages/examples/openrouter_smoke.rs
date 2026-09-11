use std::process::ExitCode;
use std::time::Instant;

use futures_util::StreamExt;
use litellm_auth::{ExistingHeaderBehavior, HeaderAuth, SecretValue};
use litellm_operation::{HttpTarget, JsonExecution, NoHooks, Pipeline, SseExecution};
use litellm_operation_messages::{
    AnthropicMessagesWire, CallMessage, MessagesCall, MessagesStreamEvent,
};
use litellm_transport::ReqwestTransport;

#[tokio::main]
async fn main() -> ExitCode {
    let Ok(api_key) = std::env::var("OPENROUTER_API_KEY") else {
        eprintln!("OPENROUTER_API_KEY is not set");
        return ExitCode::FAILURE;
    };
    let model = std::env::var("OPENROUTER_MODEL")
        .unwrap_or_else(|_| "anthropic/claude-haiku-4.5".to_string());
    let endpoint = std::env::var("OPENROUTER_MESSAGES_URL")
        .unwrap_or_else(|_| "https://openrouter.ai/api/v1/messages".to_string());
    let streaming = std::env::var("OPENROUTER_STREAM").map(|value| value == "1") == Ok(true);

    let Ok(endpoint) = endpoint.parse() else {
        eprintln!("OPENROUTER_MESSAGES_URL is not a valid URL");
        return ExitCode::FAILURE;
    };
    let Ok(authenticator) =
        HeaderAuth::bearer(&SecretValue::new(api_key), ExistingHeaderBehavior::Preserve)
    else {
        eprintln!("failed to construct OpenRouter bearer auth");
        return ExitCode::FAILURE;
    };

    let mut parameters = serde_json::Map::new();
    parameters.insert("max_tokens".into(), serde_json::json!(48));
    let call = MessagesCall {
        model: model.clone(),
        messages: vec![CallMessage {
            role: "user".into(),
            content: "Count from one to five, separating each number with a comma.".into(),
        }],
        system: Some("You are a smoke test. Follow the instruction literally.".into()),
        parameters,
    };
    let target = HttpTarget {
        endpoint,
        authenticator,
    };
    let transport = ReqwestTransport::new(reqwest::Client::new());

    if streaming {
        let start = Instant::now();
        let pipeline = Pipeline::for_target(
            target,
            AnthropicMessagesWire::new("2023-06-01"),
            SseExecution::new(transport),
            NoHooks,
        );
        match pipeline.stream(call, &()).await {
            Ok(mut stream) => {
                let mut deltas = 0;
                while let Some(event) = stream.next().await {
                    match event.expect("event decodes") {
                        MessagesStreamEvent::ContentDelta(text) => {
                            deltas += 1;
                            print!("[{}ms:{} deltas] ", start.elapsed().as_millis(), deltas);
                            println!("{text}");
                        }
                        MessagesStreamEvent::Finished => {
                            println!(
                                "finished in {}ms after {deltas} deltas",
                                start.elapsed().as_millis()
                            );
                        }
                    }
                }
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("stream failed: {error:?}");
                ExitCode::FAILURE
            }
        }
    } else {
        let pipeline = Pipeline::for_target(
            target,
            AnthropicMessagesWire::new("2023-06-01"),
            JsonExecution::new(transport),
            NoHooks,
        );
        match pipeline.handle(call, &()).await {
            Ok(response) => {
                println!("model: {model}");
                println!(
                    "stop_reason: {}",
                    response.stop_reason.as_deref().unwrap_or("?")
                );
                println!("content: {}", response.content);
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("complete failed: {error:?}");
                ExitCode::FAILURE
            }
        }
    }
}
