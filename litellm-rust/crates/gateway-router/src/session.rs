use std::time::Duration;

use crate::{Admission, Attempt, CapacityDemand, Error, Reserved, Router, RoutingStateStore};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ready;

#[must_use = "a route session must select and reserve an attempt"]
#[derive(Debug)]
pub struct RouteSession<'a, S, P>
where
    S: RoutingStateStore,
{
    router: &'a Router<S>,
    model: String,
    _phase: P,
}

impl<'a, S> RouteSession<'a, S, Ready>
where
    S: RoutingStateStore,
{
    pub(crate) fn new(router: &'a Router<S>, model: String) -> Self {
        Self {
            router,
            model,
            _phase: Ready,
        }
    }

    pub async fn select_and_reserve(
        self,
        demand: CapacityDemand,
    ) -> Result<Attempt<'a, S, Reserved<S::Lease>>, Error> {
        let mut candidates = self.router.candidates(&self.model);
        if candidates.is_empty() {
            return Err(Error::NoDeployment { model: self.model });
        }

        let mut retry_after = None;
        while let Some(deployment) = self.router.routing_strategy().select(&candidates) {
            match self
                .router
                .state_store()
                .try_reserve(deployment, demand)
                .await?
            {
                Admission::Admitted(lease) => {
                    return Ok(Attempt::new(deployment, self.router.state_store(), lease));
                }
                Admission::Rejected(rejection) => {
                    retry_after = minimum_duration(retry_after, rejection.retry_after);
                    candidates.retain(|candidate| !std::ptr::eq(*candidate, deployment));
                }
            }
        }

        Err(Error::AdmissionRejected {
            model: self.model,
            retry_after,
        })
    }
}

fn minimum_duration(left: Option<Duration>, right: Option<Duration>) -> Option<Duration> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (left, right) => left.or(right),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::future::{Ready as ReadyFuture, ready};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::{
        AdmissionRejection, AdmissionRejectionKind, AttemptFailure, AttemptFailureKind,
        AttemptOutcome, Deployment, LiteLLMParams, TokenUsage,
    };

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct Lease(String);

    #[derive(Debug, Default)]
    struct RecordingStore {
        admissions: HashMap<String, Admission<Lease>>,
        outcomes: Mutex<Vec<(Lease, AttemptOutcome)>>,
    }

    impl RoutingStateStore for RecordingStore {
        type Lease = Lease;
        type ReserveFuture<'a> = ReadyFuture<Result<Admission<Self::Lease>, Error>>;
        type FinishFuture<'a> = ReadyFuture<Result<(), Error>>;

        fn try_reserve<'a>(
            &'a self,
            deployment: &'a Deployment,
            _demand: CapacityDemand,
        ) -> Self::ReserveFuture<'a> {
            ready(Ok(self
                .admissions
                .get(&deployment.model_name)
                .cloned()
                .unwrap_or_else(|| {
                    Admission::Admitted(Lease(deployment.model_name.clone()))
                })))
        }

        fn finish<'a>(
            &'a self,
            lease: Self::Lease,
            outcome: AttemptOutcome,
        ) -> Self::FinishFuture<'a> {
            self.outcomes
                .lock()
                .expect("outcomes lock should be available")
                .push((lease, outcome));
            ready(Ok(()))
        }
    }

    #[derive(Debug, Default)]
    struct RejectOnceStore {
        reservation_attempts: AtomicUsize,
    }

    impl RoutingStateStore for RejectOnceStore {
        type Lease = ();
        type ReserveFuture<'a> = ReadyFuture<Result<Admission<Self::Lease>, Error>>;
        type FinishFuture<'a> = ReadyFuture<Result<(), Error>>;

        fn try_reserve<'a>(
            &'a self,
            _deployment: &'a Deployment,
            _demand: CapacityDemand,
        ) -> Self::ReserveFuture<'a> {
            let attempt = self.reservation_attempts.fetch_add(1, Ordering::Relaxed);
            if attempt == 0 {
                return ready(Ok(Admission::Rejected(AdmissionRejection {
                    kind: AdmissionRejectionKind::RateLimit,
                    retry_after: None,
                })));
            }
            ready(Ok(Admission::Admitted(())))
        }

        fn finish<'a>(
            &'a self,
            _lease: Self::Lease,
            _outcome: AttemptOutcome,
        ) -> Self::FinishFuture<'a> {
            ready(Ok(()))
        }
    }

    fn deployment(name: &str) -> Deployment {
        Deployment {
            model_name: name.to_string(),
            litellm_params: LiteLLMParams {
                model: name.to_string(),
                api_key: None,
                api_base: None,
            },
        }
    }

    #[tokio::test]
    async fn records_success_only_after_dispatch() {
        let router = Router::with_state(vec![deployment("model")], RecordingStore::default());
        let finished = router
            .begin("model")
            .select_and_reserve(CapacityDemand::default())
            .await
            .expect("attempt should be admitted")
            .dispatch()
            .succeed(TokenUsage {
                input_tokens: 20,
                output_tokens: 10,
            })
            .await
            .expect("attempt should finish");

        assert_eq!(
            finished.outcome(),
            &AttemptOutcome::Success(TokenUsage {
                input_tokens: 20,
                output_tokens: 10,
            })
        );
        assert_eq!(router.state_store().outcomes.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn records_a_typed_provider_failure() {
        let router = Router::with_state(vec![deployment("model")], RecordingStore::default());
        let failure = AttemptFailure {
            kind: AttemptFailureKind::RateLimit,
            status: Some(429),
            retry_after: Some(Duration::from_secs(2)),
        };
        let finished = router
            .begin("model")
            .select_and_reserve(CapacityDemand::default())
            .await
            .expect("attempt should be admitted")
            .dispatch()
            .fail(failure.clone())
            .await
            .expect("attempt should finish");

        assert_eq!(finished.outcome(), &AttemptOutcome::Failure(failure));
    }

    #[tokio::test]
    async fn rejects_when_no_deployment_has_capacity() {
        let rejection = AdmissionRejection {
            kind: AdmissionRejectionKind::RateLimit,
            retry_after: Some(Duration::from_secs(3)),
        };
        let store = RecordingStore {
            admissions: HashMap::from([("model".to_string(), Admission::Rejected(rejection))]),
            outcomes: Mutex::default(),
        };
        let router = Router::with_state(vec![deployment("model")], store);

        let result = router
            .begin("model")
            .select_and_reserve(CapacityDemand::default())
            .await;
        assert!(matches!(
            result,
            Err(Error::AdmissionRejected {
                model,
                retry_after,
            }) if model == "model" && retry_after == Some(Duration::from_secs(3))
        ));
        assert!(router.state_store().outcomes.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn retries_selection_after_local_admission_rejection() {
        let router = Router::with_state(
            vec![deployment("model"), deployment("model")],
            RejectOnceStore::default(),
        );

        let finished = router
            .begin("model")
            .select_and_reserve(CapacityDemand::default())
            .await
            .expect("second deployment should be admitted")
            .cancel()
            .await
            .expect("reservation should be released");

        assert_eq!(finished.outcome(), &AttemptOutcome::CancelledBeforeDispatch);
        assert_eq!(
            router
                .state_store()
                .reservation_attempts
                .load(Ordering::Relaxed),
            2
        );
    }

    #[tokio::test]
    async fn cancellation_releases_a_reservation_before_dispatch() {
        let router = Router::with_state(vec![deployment("model")], RecordingStore::default());
        let finished = router
            .begin("model")
            .select_and_reserve(CapacityDemand::default())
            .await
            .expect("attempt should be admitted")
            .cancel()
            .await
            .expect("reservation should be released");

        assert_eq!(finished.outcome(), &AttemptOutcome::CancelledBeforeDispatch);
    }
}
