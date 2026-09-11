use crate::{DeliveryMode, Operation, SessionOperation, StreamingOperation, WireOperation};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Fidelity {
    Exact,
    Emulated,
    Lossy,
}

pub trait OperationTransformation<O: Operation, W: WireOperation>: Send + Sync + 'static {
    const FIDELITY: Fidelity;
}

pub trait TransformationKind<O: Operation, W: WireOperation>:
    OperationTransformation<O, W>
{
    const NAME: &'static str;
}

pub trait TransformationForDelivery<O, W, D>: TransformationKind<O, W>
where
    O: Operation,
    W: WireOperation,
    D: DeliveryMode,
{
}

pub trait RequestTransformation<O: Operation, W: WireOperation>:
    OperationTransformation<O, W>
{
    type WireRequest;
    type Error;

    fn transform_request(&self, request: O::Request<'_>) -> Result<Self::WireRequest, Self::Error>;
}

pub trait ParameterPolicy {
    fn rule(&self, parameter: &str) -> ParameterRule;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParameterRule {
    PassThrough,
    Rename(&'static str),
    Drop,
    Reject,
}

pub trait ParameterTransformation<O: Operation, W: WireOperation>:
    OperationTransformation<O, W> + ParameterPolicy
{
    type Parameters;
    type WireParameters;
    type Error;

    fn transform_parameters(
        &self,
        parameters: Self::Parameters,
    ) -> Result<Self::WireParameters, Self::Error>;
}

pub trait ResponseTransformation<O: Operation, W: WireOperation>:
    OperationTransformation<O, W>
{
    type WireResponse;
    type Error;

    fn transform_response(&self, response: Self::WireResponse) -> Result<O::Response, Self::Error>;
}

pub trait StreamTransformation<O: StreamingOperation, W: WireOperation>:
    OperationTransformation<O, W>
{
    type State: Default;
    type WireEvent;
    type Events: IntoIterator<Item = O::StreamEvent>;
    type Error;

    fn transform_stream_event(
        &self,
        state: &mut Self::State,
        event: Self::WireEvent,
    ) -> Result<Self::Events, Self::Error>;
}

pub trait SessionTransformation<O: SessionOperation, W: WireOperation>:
    OperationTransformation<O, W>
{
    type State: Default;
    type WireClientEvent;
    type WireServerEvent;
    type ClientFrames: IntoIterator<Item = Self::WireClientEvent>;
    type ServerEvents: IntoIterator<Item = O::ServerEvent>;
    type Error;

    fn transform_client_event(
        &self,
        state: &mut Self::State,
        event: &O::ClientEvent,
    ) -> Result<Self::ClientFrames, Self::Error>;

    fn transform_server_event(
        &self,
        state: &mut Self::State,
        event: Self::WireServerEvent,
    ) -> Result<Self::ServerEvents, Self::Error>;
}
