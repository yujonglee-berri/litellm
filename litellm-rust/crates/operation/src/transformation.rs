use std::pin::Pin;

use futures_util::Stream;

use crate::{DeliveryMode, Operation, SessionOperation, WireOperation};

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

pub type EventStream<T, E> = Pin<Box<dyn Stream<Item = Result<T, E>> + Send + 'static>>;

pub trait ServerStreamingContract {
    type Request: Send;
    type Event: Send + 'static;
}

pub trait StreamTransformation {
    type Caller: ServerStreamingContract;
    type Upstream: ServerStreamingContract;
    type Context: Send + 'static;
    type Error: Send + 'static;

    fn transform_stream(
        &self,
        context: Self::Context,
        upstream: EventStream<<Self::Upstream as ServerStreamingContract>::Event, Self::Error>,
    ) -> EventStream<<Self::Caller as ServerStreamingContract>::Event, Self::Error>;
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
