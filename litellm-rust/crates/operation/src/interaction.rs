use std::{convert::Infallible, marker::PhantomData};

use crate::{Error, EventStream};

pub type Never = Infallible;

pub trait InteractionOperation: Send + Sync + 'static {
    type Config: Send + Sync + 'static;
    type Input: Send + 'static;
    type Event: Send + 'static;
}

pub trait InteractionWireProtocol: Send + Sync + 'static {
    type Request: Send + 'static;
    type ClientMessage: Send + 'static;
    type ServerOutput: Delivery;
}

pub struct Single<T>(PhantomData<fn() -> T>);

pub struct Streaming<T>(PhantomData<fn() -> T>);

mod sealed {
    pub trait Sealed {}

    impl<T> Sealed for super::Single<T> {}
    impl<T> Sealed for super::Streaming<T> {}
}

pub trait Delivery: sealed::Sealed {
    type Message: Send + 'static;
    type Value: Send + 'static;
}

impl<T: Send + 'static> Delivery for Single<T> {
    type Message = T;
    type Value = T;
}

impl<T: Send + 'static> Delivery for Streaming<T> {
    type Message = T;
    type Value = EventStream<T, Error>;
}

pub type ServerMessage<P> = <<P as InteractionWireProtocol>::ServerOutput as Delivery>::Message;
pub type ServerValue<P> = <<P as InteractionWireProtocol>::ServerOutput as Delivery>::Value;
pub type SignalOf<A> = Signal<
    <<A as StateMachineAdapter>::Caller as InteractionOperation>::Input,
    ServerMessage<<A as StateMachineAdapter>::Wire>,
>;
pub type ActionsOf<A> = Vec<
    Action<
        <<A as StateMachineAdapter>::Wire as InteractionWireProtocol>::ClientMessage,
        <<A as StateMachineAdapter>::Caller as InteractionOperation>::Event,
    >,
>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloseReason {
    Eof,
    Cancelled,
    Transport,
}

pub enum Signal<I, W> {
    Input(I),
    InputClosed,
    Native(W),
    NativeClosed(CloseReason),
}

pub enum Action<W, E> {
    Send(W),
    FinishInput,
    Emit(E),
    Finish,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Context;

pub trait StateMachineAdapter: Sized + Send + Sync + 'static {
    type Caller: InteractionOperation;
    type Wire: InteractionWireProtocol;
    type State: Send + 'static;

    fn prepare(
        &self,
        config: &<<Self as StateMachineAdapter>::Caller as InteractionOperation>::Config,
        cx: Context,
    ) -> Result<Prepared<Self>, Error>;

    fn step(
        state: &mut Self::State,
        signal: SignalOf<Self>,
        cx: &mut Context,
    ) -> Result<ActionsOf<Self>, Error>;
}

pub struct Prepared<A: StateMachineAdapter> {
    pub open: <A::Wire as InteractionWireProtocol>::Request,
    pub state: A::State,
    pub cx: Context,
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Unary;

    impl InteractionOperation for Unary {
        type Config = String;
        type Input = Never;
        type Event = usize;
    }

    struct UnaryWire;

    impl InteractionWireProtocol for UnaryWire {
        type Request = String;
        type ClientMessage = Never;
        type ServerOutput = Single<String>;
    }

    struct UnaryAdapter;

    impl StateMachineAdapter for UnaryAdapter {
        type Caller = Unary;
        type Wire = UnaryWire;
        type State = ();

        fn prepare(&self, config: &String, cx: Context) -> Result<Prepared<Self>, Error> {
            Ok(Prepared {
                open: config.clone(),
                state: (),
                cx,
            })
        }

        fn step(
            _: &mut Self::State,
            signal: SignalOf<Self>,
            _: &mut Context,
        ) -> Result<ActionsOf<Self>, Error> {
            match signal {
                Signal::Input(never) => match never {},
                Signal::InputClosed => Ok(Vec::new()),
                Signal::Native(value) => Ok(vec![Action::Emit(value.len()), Action::Finish]),
                Signal::NativeClosed(_) => Err(Error::UnexpectedEof),
            }
        }
    }

    struct StreamWire;

    impl InteractionWireProtocol for StreamWire {
        type Request = ();
        type ClientMessage = Never;
        type ServerOutput = Streaming<u8>;
    }

    fn accepts_single(_: ServerValue<UnaryWire>) {}
    fn accepts_stream(_: ServerValue<StreamWire>) {}

    #[test]
    fn delivery_selects_the_native_runtime_shape() {
        accepts_single(String::new());
        let _ = accepts_stream;
    }

    #[test]
    fn adapter_step_only_accepts_its_declared_wire_message() {
        let mut state = ();
        let mut cx = Context::default();
        let actions = UnaryAdapter::step(&mut state, Signal::Native("done".into()), &mut cx)
            .expect("valid native message");
        assert!(matches!(
            actions.as_slice(),
            [Action::Emit(4), Action::Finish]
        ));
    }
}
