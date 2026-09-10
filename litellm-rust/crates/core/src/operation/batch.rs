use std::marker::PhantomData;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::{Operation, OperationKind};

pub struct Batch<O: Operation, Action: BatchAction<O> = Submit>(PhantomData<fn() -> (O, Action)>);

pub trait BatchAction<O: Operation>: Send + Sync + 'static {
    type Request<'a>;
    type Response;

    const KIND: OperationKind;
}

pub struct Submit;
pub struct Retrieve;
pub struct Cancel;
pub struct Results;

impl<O, Action> Operation for Batch<O, Action>
where
    O: Operation,
    Action: BatchAction<O>,
{
    type Request<'a> = Action::Request<'a>;
    type Response = Action::Response;

    const KIND: OperationKind = Action::KIND;
}

impl<O: Operation> BatchAction<O> for Submit {
    type Request<'a> = BatchSubmitRequest<O::Request<'a>>;
    type Response = BatchJob;

    const KIND: OperationKind = OperationKind::BatchSubmit;
}

impl<O: Operation> BatchAction<O> for Retrieve {
    type Request<'a> = BatchRetrieveRequest;
    type Response = BatchJob;

    const KIND: OperationKind = OperationKind::BatchRetrieve;
}

impl<O: Operation> BatchAction<O> for Cancel {
    type Request<'a> = BatchCancelRequest;
    type Response = BatchJob;

    const KIND: OperationKind = OperationKind::BatchCancel;
}

impl<O: Operation> BatchAction<O> for Results {
    type Request<'a> = BatchResultsRequest;
    type Response = BatchResultsResponse<O::Response>;

    const KIND: OperationKind = OperationKind::BatchResults;
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BatchSubmitRequest<I> {
    pub items: Vec<BatchItem<I>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BatchItem<I> {
    pub custom_id: String,
    pub input: I,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchRetrieveRequest {
    pub batch_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchCancelRequest {
    pub batch_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchResultsRequest {
    pub batch_id: String,
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
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
    pub fn as_str(&self) -> &str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Finalizing => "finalizing",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelling => "cancelling",
            Self::Cancelled => "cancelled",
            Self::Expired => "expired",
            Self::Unknown(value) => value,
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Cancelled | Self::Expired
        )
    }
}

impl From<String> for BatchStatus {
    fn from(value: String) -> Self {
        match value.as_str() {
            "queued" => Self::Queued,
            "running" => Self::Running,
            "finalizing" => Self::Finalizing,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            "cancelling" => Self::Cancelling,
            "cancelled" => Self::Cancelled,
            "expired" => Self::Expired,
            _ => Self::Unknown(value),
        }
    }
}

impl Serialize for BatchStatus {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for BatchStatus {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer).map(Self::from)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchRequestCounts {
    pub total: u64,
    pub completed: u64,
    pub failed: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchJob {
    pub id: String,
    pub status: BatchStatus,
    pub request_counts: BatchRequestCounts,
    pub created_at: Option<u64>,
    pub completed_at: Option<u64>,
    pub expires_at: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BatchResultsResponse<R> {
    pub items: Vec<BatchItemResult<R>>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BatchItemResult<R> {
    pub custom_id: String,
    pub outcome: BatchItemOutcome<R>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BatchItemOutcome<R> {
    Succeeded { response: R },
    Failed { error: BatchItemError },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchItemError {
    pub code: Option<String>,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_status_excludes_work_that_can_still_change() {
        for status in [
            BatchStatus::Queued,
            BatchStatus::Running,
            BatchStatus::Finalizing,
            BatchStatus::Cancelling,
            BatchStatus::Unknown("provider_state".into()),
        ] {
            assert!(!status.is_terminal());
        }
        for status in [
            BatchStatus::Completed,
            BatchStatus::Failed,
            BatchStatus::Cancelled,
            BatchStatus::Expired,
        ] {
            assert!(status.is_terminal());
        }
    }

    #[test]
    fn unknown_status_round_trips_as_the_provider_string() {
        let status: BatchStatus = serde_json::from_str("\"scheduled\"").unwrap();
        assert_eq!(status, BatchStatus::Unknown("scheduled".into()));
        assert_eq!(serde_json::to_string(&status).unwrap(), "\"scheduled\"");
    }
}
