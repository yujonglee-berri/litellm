use litellm_operation::{ServerStreamingContract, StreamTransformation};

struct ChatCompletions;
struct AnthropicMessagesApi;
struct AnthropicChatAdapter;

impl ServerStreamingContract for ChatCompletions {
    type Request = String;
    type Event = String;
}

impl ServerStreamingContract for AnthropicMessagesApi {
    type Request = Vec<u8>;
    type Event = Vec<u8>;
}

impl StreamTransformation for AnthropicChatAdapter {
    type Caller = ChatCompletions;
    type Upstream = AnthropicMessagesApi;
    type Context = ();
    type Error = ();

    fn transform_stream(
        &self,
        _context: (),
        _upstream: litellm_operation::EventStream<Vec<u8>, ()>,
    ) -> litellm_operation::EventStream<String, ()> {
        Box::pin(futures_util::stream::empty())
    }
}

fn configure<T>(_adapter: T)
where
    T: StreamTransformation<Caller = ChatCompletions, Upstream = AnthropicMessagesApi>,
{
}

#[test]
fn transformation_names_both_streaming_contracts() {
    configure(AnthropicChatAdapter);
}
