use std::time::Duration;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Phase {
    Prepare,
    Transform,
    FinalizeRequest,
    Authenticate,
    Send,
    Decode,
    Success,
    Failure,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Context<M = ()> {
    pub call_id: String,
    pub metadata: M,
}

impl<M> Context<M> {
    pub fn new(call_id: impl Into<String>, metadata: M) -> Self {
        Self {
            call_id: call_id.into(),
            metadata,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhaseTiming {
    pub phase: Phase,
    pub duration: Duration,
}

pub trait Observer<M>: Send + Sync {
    fn on_phase_start(&self, _context: &Context<M>, _phase: Phase) {}
    fn on_phase_end(&self, _context: &Context<M>, _timing: PhaseTiming) {}
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NoopObserver;

impl<M> Observer<M> for NoopObserver {}
