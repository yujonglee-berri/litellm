use std::marker::PhantomData;

use crate::{Operation, SessionOperation, StreamingOperation};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Delivery {
    Complete,
    Stream,
    Session,
}

pub trait DeliveryMode: Send + Sync + 'static {
    const DELIVERY: Delivery;
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Complete;

impl DeliveryMode for Complete {
    const DELIVERY: Delivery = Delivery::Complete;
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Stream;

impl DeliveryMode for Stream {
    const DELIVERY: Delivery = Delivery::Stream;
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Session;

impl DeliveryMode for Session {
    const DELIVERY: Delivery = Delivery::Session;
}

pub trait DeliveryFor<O: Operation>: DeliveryMode {}

impl<O: Operation> DeliveryFor<O> for Complete {}
impl<O: StreamingOperation> DeliveryFor<O> for Stream {}
impl<O: SessionOperation> DeliveryFor<O> for Session {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapabilitySupport {
    Supported,
    Unsupported,
    Unknown,
}

pub trait Capability: Send + Sync + 'static {
    const NAME: &'static str;
}

pub trait SupportsCapability<O: Operation, C: Capability>: Provider {
    const SUPPORT: CapabilitySupport;
}

pub trait Provider: Clone + Copy + Send + Sync + 'static {
    const NAME: &'static str;
}

pub trait WireOperation: Clone + Copy + Send + Sync + 'static {
    const NAME: &'static str;
}

pub trait SupportsWire<W: WireOperation, D: DeliveryMode>: Provider {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OperationPlan<O, P, W, D, T>
where
    O: Operation,
    P: Provider + SupportsWire<W, D>,
    W: WireOperation,
    D: DeliveryFor<O>,
    T: crate::TransformationForDelivery<O, W, D>,
{
    provider: P,
    wire_operation: W,
    transformation: T,
    marker: PhantomData<fn() -> (O, D)>,
}

impl<O, P, W, D, T> OperationPlan<O, P, W, D, T>
where
    O: Operation,
    P: Provider + SupportsWire<W, D>,
    W: WireOperation,
    D: DeliveryFor<O>,
    T: crate::TransformationForDelivery<O, W, D>,
{
    pub const fn new(provider: P, wire_operation: W, transformation: T) -> Self {
        Self {
            provider,
            wire_operation,
            transformation,
            marker: PhantomData,
        }
    }

    pub const fn provider(&self) -> &P {
        &self.provider
    }

    pub const fn wire_operation(&self) -> &W {
        &self.wire_operation
    }

    pub const fn transformation(&self) -> &T {
        &self.transformation
    }

    pub const fn delivery(&self) -> Delivery {
        D::DELIVERY
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Fidelity, OperationTransformation, SessionOperation, TransformationForDelivery,
        TransformationKind,
    };

    struct CompleteOnly;

    impl Operation for CompleteOnly {
        type Request<'a> = &'a str;
        type Response = String;

        const NAME: &'static str = "complete_only";
    }

    struct Duplex;

    impl Operation for Duplex {
        type Request<'a> = &'a str;
        type Response = String;

        const NAME: &'static str = "duplex";
    }

    impl SessionOperation for Duplex {
        type ClientEvent = u8;
        type ServerEvent = u16;
    }

    #[derive(Clone, Copy, Default)]
    struct TestProvider;

    impl Provider for TestProvider {
        const NAME: &'static str = "test";
    }

    #[derive(Clone, Copy, Default)]
    struct TestWire;

    impl WireOperation for TestWire {
        const NAME: &'static str = "test.wire";
    }

    #[derive(Clone, Copy, Default)]
    struct TestTransformation;

    impl OperationTransformation<Duplex, TestWire> for TestTransformation {
        const FIDELITY: Fidelity = Fidelity::Exact;
    }

    impl TransformationKind<Duplex, TestWire> for TestTransformation {
        const NAME: &'static str = "duplex.to_test";
    }

    impl TransformationForDelivery<Duplex, TestWire, Complete> for TestTransformation {}
    impl TransformationForDelivery<Duplex, TestWire, Session> for TestTransformation {}

    impl SupportsWire<TestWire, Complete> for TestProvider {}
    impl SupportsWire<TestWire, Session> for TestProvider {}

    fn accepts_delivery<O: Operation, D: DeliveryFor<O>>(_: D) {}

    #[test]
    fn session_delivery_is_available_only_for_session_operations() {
        accepts_delivery::<CompleteOnly, Complete>(Complete);
        accepts_delivery::<Duplex, Complete>(Complete);
        accepts_delivery::<Duplex, Session>(Session);

        let complete = OperationPlan::<Duplex, _, _, Complete, _>::new(
            TestProvider,
            TestWire,
            TestTransformation,
        );
        let session = OperationPlan::<Duplex, _, _, Session, _>::new(
            TestProvider,
            TestWire,
            TestTransformation,
        );

        assert_eq!(complete.delivery(), Delivery::Complete);
        assert_eq!(session.delivery(), Delivery::Session);
        assert_eq!(Session::DELIVERY, Delivery::Session);
    }
}
