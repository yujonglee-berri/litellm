use std::marker::PhantomData;
use std::time::Instant;

use crate::{Context, Observer, Phase, PhaseTiming};

pub struct Ready;

pub struct Active {
    phase: Phase,
    started_at: Instant,
}

pub struct Succeeded;

pub struct Failed<E> {
    error: E,
}

pub struct Lifecycle<'a, M, State> {
    context: Context<M>,
    observer: &'a dyn Observer<M>,
    state: State,
    marker: PhantomData<&'a ()>,
}

impl<'a, M> Lifecycle<'a, M, Ready> {
    pub fn new(context: Context<M>, observer: &'a dyn Observer<M>) -> Self {
        Self {
            context,
            observer,
            state: Ready,
            marker: PhantomData,
        }
    }

    pub fn start(self, phase: Phase) -> Lifecycle<'a, M, Active> {
        self.observer.on_phase_start(&self.context, phase);
        Lifecycle {
            context: self.context,
            observer: self.observer,
            state: Active {
                phase,
                started_at: Instant::now(),
            },
            marker: PhantomData,
        }
    }

    pub fn succeed(self) -> Lifecycle<'a, M, Succeeded> {
        Lifecycle {
            context: self.context,
            observer: self.observer,
            state: Succeeded,
            marker: PhantomData,
        }
    }

    pub fn fail<E>(self, error: E) -> Lifecycle<'a, M, Failed<E>> {
        Lifecycle {
            context: self.context,
            observer: self.observer,
            state: Failed { error },
            marker: PhantomData,
        }
    }
}

impl<'a, M> Lifecycle<'a, M, Active> {
    pub fn finish(self) -> Lifecycle<'a, M, Ready> {
        self.observer.on_phase_end(
            &self.context,
            PhaseTiming {
                phase: self.state.phase,
                duration: self.state.started_at.elapsed(),
            },
        );
        Lifecycle {
            context: self.context,
            observer: self.observer,
            state: Ready,
            marker: PhantomData,
        }
    }
}

impl<'a, M, State> Lifecycle<'a, M, State> {
    pub fn context(&self) -> &Context<M> {
        &self.context
    }
}

impl<'a, M, E> Lifecycle<'a, M, Failed<E>> {
    pub fn error(&self) -> &E {
        &self.state.error
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct RecordingObserver(Mutex<Vec<(Phase, bool)>>);

    impl Observer<()> for RecordingObserver {
        fn on_phase_start(&self, _context: &Context<()>, phase: Phase) {
            self.0.lock().unwrap().push((phase, true));
        }

        fn on_phase_end(&self, _context: &Context<()>, timing: PhaseTiming) {
            self.0.lock().unwrap().push((timing.phase, false));
        }
    }

    #[test]
    fn phases_must_finish_before_terminal_transition() {
        let observer = RecordingObserver::default();
        let lifecycle = Lifecycle::new(Context::new("call-1", ()), &observer)
            .start(Phase::Transform)
            .finish()
            .succeed();

        assert_eq!(lifecycle.context().call_id, "call-1");
        assert_eq!(
            *observer.0.lock().unwrap(),
            vec![(Phase::Transform, true), (Phase::Transform, false)]
        );
    }

    #[test]
    fn failure_retains_the_operation_specific_error() {
        let lifecycle = Lifecycle::new(Context::new("call-2", ()), &crate::NoopObserver)
            .fail("provider unavailable");

        assert_eq!(lifecycle.error(), &"provider unavailable");
    }
}
