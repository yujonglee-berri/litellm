use crate::{AttemptFailure, AttemptOutcome, Deployment, Error, RoutingStateStore, TokenUsage};

#[derive(Debug)]
pub struct Reserved<L> {
    lease: L,
}

#[derive(Debug)]
pub struct InFlight<L> {
    lease: L,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finished {
    outcome: AttemptOutcome,
}

#[must_use = "attempt lifecycle results should not be discarded"]
#[derive(Debug)]
pub struct Attempt<'a, S, P>
where
    S: RoutingStateStore,
{
    deployment: &'a Deployment,
    state_store: &'a S,
    phase: P,
}

impl<'a, S> Attempt<'a, S, Reserved<S::Lease>>
where
    S: RoutingStateStore,
{
    pub(crate) fn new(deployment: &'a Deployment, state_store: &'a S, lease: S::Lease) -> Self {
        Self {
            deployment,
            state_store,
            phase: Reserved { lease },
        }
    }

    pub fn dispatch(self) -> Attempt<'a, S, InFlight<S::Lease>> {
        Attempt {
            deployment: self.deployment,
            state_store: self.state_store,
            phase: InFlight {
                lease: self.phase.lease,
            },
        }
    }

    pub async fn cancel(self) -> Result<Attempt<'a, S, Finished>, Error> {
        let outcome = AttemptOutcome::CancelledBeforeDispatch;
        self.state_store
            .finish(self.phase.lease, outcome.clone())
            .await?;
        Ok(Attempt {
            deployment: self.deployment,
            state_store: self.state_store,
            phase: Finished { outcome },
        })
    }
}

impl<'a, S> Attempt<'a, S, InFlight<S::Lease>>
where
    S: RoutingStateStore,
{
    pub async fn succeed(self, usage: TokenUsage) -> Result<Attempt<'a, S, Finished>, Error> {
        self.finish(AttemptOutcome::Success(usage)).await
    }

    pub async fn fail(self, failure: AttemptFailure) -> Result<Attempt<'a, S, Finished>, Error> {
        self.finish(AttemptOutcome::Failure(failure)).await
    }

    async fn finish(self, outcome: AttemptOutcome) -> Result<Attempt<'a, S, Finished>, Error> {
        self.state_store
            .finish(self.phase.lease, outcome.clone())
            .await?;
        Ok(Attempt {
            deployment: self.deployment,
            state_store: self.state_store,
            phase: Finished { outcome },
        })
    }
}

impl<S, P> Attempt<'_, S, P>
where
    S: RoutingStateStore,
{
    pub fn deployment(&self) -> &Deployment {
        self.deployment
    }
}

impl<S> Attempt<'_, S, Finished>
where
    S: RoutingStateStore,
{
    pub fn outcome(&self) -> &AttemptOutcome {
        &self.phase.outcome
    }
}
