//! Provider-independent concepts used to plan a LiteLLM operation

pub mod batch;

/// The semantic operation requested by the LiteLLM caller
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperationKind {
    ChatCompletions,
    Ocr,
    BatchSubmit,
    BatchRetrieve,
    BatchCancel,
    BatchResults,
}

/// How the caller receives the result of an operation
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DeliveryMode {
    #[default]
    Complete,
    Stream,
}

/// Whether a provider or model is known to support a requested feature
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapabilitySupport {
    Supported,
    Unsupported,
    Unknown,
}

/// How faithfully a transformation implements the public operation over a wire API
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fidelity {
    Exact,
    Emulated,
    Lossy,
}

/// A top-level LiteLLM operation and its typed public contract
pub trait Operation: Send + Sync + 'static {
    type Request<'a>;
    type Response;

    const KIND: OperationKind;
}

/// An operation whose result can be delivered incrementally
pub trait StreamingOperation: Operation {
    type StreamEvent;
}

/// A provider-native API used on the wire
pub trait WireOperation: Clone + Copy + Send + Sync + 'static {
    fn name(self) -> &'static str;
}

/// A provider identity, independent of the wire API it hosts
pub trait Provider: Clone + Copy + Send + Sync + 'static {
    fn name(self) -> &'static str;
}

/// Metadata shared by every transformation of an operation to one wire API
pub trait OperationTransformation<O: Operation>: Send + Sync {
    type WireOperation: WireOperation;

    fn wire_operation(&self) -> Self::WireOperation;

    fn fidelity(&self) -> Fidelity;
}

/// Converts the canonical request of an operation into a typed wire request
pub trait RequestTransformation<O: Operation>: OperationTransformation<O> {
    type Input;
    type Output;
    type Error;

    fn transform_request(&self, input: Self::Input) -> Result<Self::Output, Self::Error>;
}

/// Separates operation parameters from routing and internal call configuration
pub trait ParameterTransformation<O: Operation>: OperationTransformation<O> {
    type Input;
    type Output;
    type Error;

    fn transform_parameters(&self, input: Self::Input) -> Result<Self::Output, Self::Error>;
}

/// Converts a typed wire response into the canonical response of an operation
pub trait ResponseTransformation<O: Operation>: OperationTransformation<O> {
    type Input;
    type Error;

    fn transform_response(&self, input: Self::Input) -> Result<O::Response, Self::Error>;
}

/// Stateful conversion of wire events into canonical operation events
pub trait StreamTransformation<O: StreamingOperation>: OperationTransformation<O> {
    type State: Default;
    type Input;
    type Error;

    fn transform_stream_event(
        &self,
        state: &mut Self::State,
        input: Self::Input,
    ) -> Result<Vec<O::StreamEvent>, Self::Error>;
}

/// A runtime transformation selection and the wire operation it targets
pub trait TransformationKind: Clone + Copy + Send + Sync + 'static {
    type WireOperation: WireOperation;

    fn wire_operation(self) -> Self::WireOperation;
}

/// The fully selected components of an operation before execution
pub trait OperationPlan: Clone + Copy + Send + Sync + 'static {
    type Operation: Operation;
    type Provider: Provider;
    type WireOperation: WireOperation;
    type Transformation: TransformationKind<WireOperation = Self::WireOperation>;

    fn provider(self) -> Self::Provider;

    fn transformation(self) -> Self::Transformation;

    fn wire_operation(self) -> Self::WireOperation {
        self.transformation().wire_operation()
    }

    fn delivery_support(self, delivery: DeliveryMode) -> CapabilitySupport {
        if delivery == DeliveryMode::Complete {
            CapabilitySupport::Supported
        } else {
            CapabilitySupport::Unsupported
        }
    }
}
