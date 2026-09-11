use litellm_operation::{Operation, StreamingOperation};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ChatCompletions;

impl Operation for ChatCompletions {
    type Request<'a> = ChatCompletionsRequest<'a>;
    type Response = ChatCompletionsResponse;

    const NAME: &'static str = "chat_completions";
}

impl StreamingOperation for ChatCompletions {
    type StreamEvent = ChatCompletionsStreamEvent;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChatCompletionsRequest<'a> {
    pub model: &'a str,
    pub messages: &'a [ChatMessage<'a>],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChatMessage<'a> {
    pub role: Role,
    pub content: &'a str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChatCompletionsResponse {
    pub model: String,
    pub content: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ChatCompletionsStreamEvent {
    TextDelta(String),
    Finished,
}
