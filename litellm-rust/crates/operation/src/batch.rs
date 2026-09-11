use std::marker::PhantomData;

use crate::Operation;

pub struct Batch<O: Operation, A: BatchAction<O> = Submit>(PhantomData<fn() -> (O, A)>);

pub trait BatchAction<O: Operation>: Send + Sync + 'static {
    type Request<'a>
    where
        O: 'a;
    type Response;

    const NAME: &'static str;
}

pub struct Submit;
pub struct Retrieve;
pub struct Cancel;
pub struct Results;

impl<O, A> Operation for Batch<O, A>
where
    O: Operation,
    A: BatchAction<O>,
{
    type Request<'a>
        = A::Request<'a>
    where
        Self: 'a;
    type Response = A::Response;

    const NAME: &'static str = A::NAME;
}

impl<O: Operation> BatchAction<O> for Submit {
    type Request<'a>
        = BatchSubmitRequest<O::Request<'a>>
    where
        O: 'a;
    type Response = BatchJob;

    const NAME: &'static str = "batch.submit";
}

impl<O: Operation> BatchAction<O> for Retrieve {
    type Request<'a>
        = BatchRetrieveRequest
    where
        O: 'a;
    type Response = BatchJob;

    const NAME: &'static str = "batch.retrieve";
}

impl<O: Operation> BatchAction<O> for Cancel {
    type Request<'a>
        = BatchCancelRequest
    where
        O: 'a;
    type Response = BatchJob;

    const NAME: &'static str = "batch.cancel";
}

impl<O: Operation> BatchAction<O> for Results {
    type Request<'a>
        = BatchResultsRequest
    where
        O: 'a;
    type Response = BatchResultsResponse<O::Response>;

    const NAME: &'static str = "batch.results";
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchSubmitRequest<I> {
    pub items: Vec<BatchItem<I>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchItem<I> {
    pub custom_id: String,
    pub input: I,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchRetrieveRequest {
    pub batch_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchCancelRequest {
    pub batch_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchResultsRequest {
    pub batch_id: String,
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BatchStatus {
    Queued,
    Running,
    Finalizing,
    Completed,
    Failed,
    Cancelling,
    Cancelled,
    Expired,
    Unknown(String),
}

impl BatchStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Cancelled | Self::Expired
        )
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BatchRequestCounts {
    pub total: u64,
    pub completed: u64,
    pub failed: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchJob {
    pub id: String,
    pub status: BatchStatus,
    pub request_counts: BatchRequestCounts,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchResultsResponse<R> {
    pub items: Vec<BatchItemResult<R>>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchItemResult<R> {
    pub custom_id: String,
    pub outcome: BatchItemOutcome<R>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BatchItemOutcome<R> {
    Succeeded { response: R },
    Failed { error: BatchItemError },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchItemError {
    pub code: Option<String>,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Example;

    impl Operation for Example {
        type Request<'a> = &'a str;
        type Response = String;

        const NAME: &'static str = "example";
    }

    #[test]
    fn batch_actions_preserve_the_semantic_contract() {
        fn submit_name(_: Batch<Example>) -> &'static str {
            Batch::<Example>::NAME
        }

        assert_eq!(submit_name(Batch(PhantomData)), "batch.submit");
        assert_eq!(Batch::<Example, Results>::NAME, "batch.results");
    }

    #[test]
    fn only_finished_statuses_are_terminal() {
        assert!(!BatchStatus::Running.is_terminal());
        assert!(BatchStatus::Completed.is_terminal());
        assert!(BatchStatus::Failed.is_terminal());
    }
}
