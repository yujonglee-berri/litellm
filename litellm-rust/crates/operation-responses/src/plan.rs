use litellm_operation::{
    Complete, Fidelity, OperationPlan, OperationTransformation, Provider, Session, Stream,
    SupportsWire, TransformationForDelivery, TransformationKind, WireOperation,
};

use crate::Responses;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OpenAI;

impl Provider for OpenAI {
    const NAME: &'static str = "openai";
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OpenAIResponses;

impl WireOperation for OpenAIResponses {
    const NAME: &'static str = "openai.responses";
}

impl SupportsWire<OpenAIResponses, Complete> for OpenAI {}
impl SupportsWire<OpenAIResponses, Stream> for OpenAI {}
impl SupportsWire<OpenAIResponses, Session> for OpenAI {}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OpenAIResponsesTransformation;

impl OperationTransformation<Responses, OpenAIResponses> for OpenAIResponsesTransformation {
    const FIDELITY: Fidelity = Fidelity::Exact;
}

impl TransformationKind<Responses, OpenAIResponses> for OpenAIResponsesTransformation {
    const NAME: &'static str = "responses.to_openai";
}

impl TransformationForDelivery<Responses, OpenAIResponses, Complete>
    for OpenAIResponsesTransformation
{
}

impl TransformationForDelivery<Responses, OpenAIResponses, Stream>
    for OpenAIResponsesTransformation
{
}

impl TransformationForDelivery<Responses, OpenAIResponses, Session>
    for OpenAIResponsesTransformation
{
}

pub type OpenAICompletePlan =
    OperationPlan<Responses, OpenAI, OpenAIResponses, Complete, OpenAIResponsesTransformation>;
pub type OpenAIStreamPlan =
    OperationPlan<Responses, OpenAI, OpenAIResponses, Stream, OpenAIResponsesTransformation>;
pub type OpenAISessionPlan =
    OperationPlan<Responses, OpenAI, OpenAIResponses, Session, OpenAIResponsesTransformation>;

pub const fn openai_complete() -> OpenAICompletePlan {
    OperationPlan::new(OpenAI, OpenAIResponses, OpenAIResponsesTransformation)
}

pub const fn openai_stream() -> OpenAIStreamPlan {
    OperationPlan::new(OpenAI, OpenAIResponses, OpenAIResponsesTransformation)
}

pub const fn openai_session() -> OpenAISessionPlan {
    OperationPlan::new(OpenAI, OpenAIResponses, OpenAIResponsesTransformation)
}

#[cfg(test)]
mod tests {
    use litellm_operation::{Delivery, Provider, WireOperation};

    use super::*;

    #[test]
    fn openai_responses_keeps_the_three_deliveries_distinct() {
        let complete = openai_complete();
        let stream = openai_stream();
        let session = openai_session();

        fn accepts_complete(_: OpenAICompletePlan) {}
        fn accepts_stream(_: OpenAIStreamPlan) {}
        fn accepts_session(_: OpenAISessionPlan) {}

        accepts_complete(complete);
        accepts_stream(stream);
        accepts_session(session);

        assert_eq!(OpenAI::NAME, "openai");
        assert_eq!(OpenAIResponses::NAME, "openai.responses");
        assert_eq!(complete.delivery(), Delivery::Complete);
        assert_eq!(stream.delivery(), Delivery::Stream);
        assert_eq!(session.delivery(), Delivery::Session);
    }
}
