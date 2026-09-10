use std::future::{Future, Ready, ready};
use std::time::Duration;

use crate::{Deployment, Error};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CapacityDemand {
    pub estimated_input_tokens: Option<u64>,
    pub max_output_tokens: Option<u64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdmissionRejectionKind {
    RateLimit,
    ConcurrencyLimit,
    BudgetLimit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdmissionRejection {
    pub kind: AdmissionRejectionKind,
    pub retry_after: Option<Duration>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Admission<L> {
    Admitted(L),
    Rejected(AdmissionRejection),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttemptFailureKind {
    Authentication,
    InvalidRequest,
    RateLimit,
    Timeout,
    Connection,
    Upstream,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttemptFailure {
    pub kind: AttemptFailureKind,
    pub status: Option<u16>,
    pub retry_after: Option<Duration>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AttemptOutcome {
    Success(TokenUsage),
    Failure(AttemptFailure),
    CancelledBeforeDispatch,
}

pub trait RoutingStateStore: Send + Sync {
    type Lease: Send + 'static;
    type ReserveFuture<'a>: Future<Output = Result<Admission<Self::Lease>, Error>> + Send + 'a
    where
        Self: 'a;
    type FinishFuture<'a>: Future<Output = Result<(), Error>> + Send + 'a
    where
        Self: 'a;

    fn try_reserve<'a>(
        &'a self,
        deployment: &'a Deployment,
        demand: CapacityDemand,
    ) -> Self::ReserveFuture<'a>;

    fn finish<'a>(&'a self, lease: Self::Lease, outcome: AttemptOutcome) -> Self::FinishFuture<'a>;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UnconstrainedStateStore;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnconstrainedLease;

impl RoutingStateStore for UnconstrainedStateStore {
    type Lease = UnconstrainedLease;
    type ReserveFuture<'a> = Ready<Result<Admission<Self::Lease>, Error>>;
    type FinishFuture<'a> = Ready<Result<(), Error>>;

    fn try_reserve<'a>(
        &'a self,
        _deployment: &'a Deployment,
        _demand: CapacityDemand,
    ) -> Self::ReserveFuture<'a> {
        ready(Ok(Admission::Admitted(UnconstrainedLease)))
    }

    fn finish<'a>(
        &'a self,
        _lease: Self::Lease,
        _outcome: AttemptOutcome,
    ) -> Self::FinishFuture<'a> {
        ready(Ok(()))
    }
}
