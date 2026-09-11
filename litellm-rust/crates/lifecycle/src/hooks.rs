use std::future::{Future, Ready, ready};

use crate::Context;

pub trait Hooks<InitialRequest, ProviderRequest, Response, E, M>: Send + Sync {
    type PrepareFuture<'a>: Future<Output = Result<InitialRequest, E>> + Send + 'a
    where
        Self: 'a,
        InitialRequest: 'a,
        E: 'a,
        M: 'a;

    type FinalizeRequestFuture<'a>: Future<Output = Result<ProviderRequest, E>> + Send + 'a
    where
        Self: 'a,
        InitialRequest: 'a,
        ProviderRequest: 'a,
        E: 'a,
        M: 'a;

    type SuccessFuture<'a>: Future<Output = ()> + Send + 'a
    where
        Self: 'a,
        Response: 'a,
        M: 'a;

    type FailureFuture<'a>: Future<Output = ()> + Send + 'a
    where
        Self: 'a,
        E: 'a,
        M: 'a;

    fn prepare<'a>(
        &'a self,
        context: &'a Context<M>,
        request: InitialRequest,
    ) -> Self::PrepareFuture<'a>;

    fn finalize_request<'a>(
        &'a self,
        context: &'a Context<M>,
        request: InitialRequest,
    ) -> Self::FinalizeRequestFuture<'a>;

    fn on_success<'a>(
        &'a self,
        context: &'a Context<M>,
        response: &'a Response,
    ) -> Self::SuccessFuture<'a>;

    fn on_failure<'a>(&'a self, context: &'a Context<M>, error: &'a E) -> Self::FailureFuture<'a>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NoopHooks;

impl<Request, Response, E, M> Hooks<Request, Request, Response, E, M> for NoopHooks
where
    Request: Send + 'static,
    E: Send + 'static,
{
    type PrepareFuture<'a>
        = Ready<Result<Request, E>>
    where
        Self: 'a,
        Request: 'a,
        E: 'a,
        M: 'a;

    type FinalizeRequestFuture<'a>
        = Ready<Result<Request, E>>
    where
        Self: 'a,
        Request: 'a,
        E: 'a,
        M: 'a;

    type SuccessFuture<'a>
        = Ready<()>
    where
        Self: 'a,
        Response: 'a,
        M: 'a;

    type FailureFuture<'a>
        = Ready<()>
    where
        Self: 'a,
        E: 'a,
        M: 'a;

    fn prepare<'a>(
        &'a self,
        _context: &'a Context<M>,
        request: Request,
    ) -> Self::PrepareFuture<'a> {
        ready(Ok(request))
    }

    fn finalize_request<'a>(
        &'a self,
        _context: &'a Context<M>,
        request: Request,
    ) -> Self::FinalizeRequestFuture<'a> {
        ready(Ok(request))
    }

    fn on_success<'a>(
        &'a self,
        _context: &'a Context<M>,
        _response: &'a Response,
    ) -> Self::SuccessFuture<'a> {
        ready(())
    }

    fn on_failure<'a>(
        &'a self,
        _context: &'a Context<M>,
        _error: &'a E,
    ) -> Self::FailureFuture<'a> {
        ready(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Eq, PartialEq)]
    struct InitialRequest {
        model: String,
    }

    #[derive(Debug, Eq, PartialEq)]
    struct ProviderRequest {
        endpoint: String,
    }

    struct Response;

    #[derive(Debug, Eq, PartialEq)]
    enum OperationError {
        MissingModel,
    }

    struct TransformingHooks;

    impl Hooks<InitialRequest, ProviderRequest, Response, OperationError, ()> for TransformingHooks {
        type PrepareFuture<'a> = Ready<Result<InitialRequest, OperationError>>;
        type FinalizeRequestFuture<'a> = Ready<Result<ProviderRequest, OperationError>>;
        type SuccessFuture<'a> = Ready<()>;
        type FailureFuture<'a> = Ready<()>;

        fn prepare<'a>(
            &'a self,
            _context: &'a Context<()>,
            request: InitialRequest,
        ) -> Self::PrepareFuture<'a> {
            ready(if request.model.is_empty() {
                Err(OperationError::MissingModel)
            } else {
                Ok(request)
            })
        }

        fn finalize_request<'a>(
            &'a self,
            _context: &'a Context<()>,
            request: InitialRequest,
        ) -> Self::FinalizeRequestFuture<'a> {
            ready(Ok(ProviderRequest {
                endpoint: format!("https://provider.example/models/{}", request.model),
            }))
        }

        fn on_success<'a>(
            &'a self,
            _context: &'a Context<()>,
            _response: &'a Response,
        ) -> Self::SuccessFuture<'a> {
            ready(())
        }

        fn on_failure<'a>(
            &'a self,
            _context: &'a Context<()>,
            _error: &'a OperationError,
        ) -> Self::FailureFuture<'a> {
            ready(())
        }
    }

    #[tokio::test]
    async fn hooks_finalize_a_distinct_request_and_preserve_the_error_type() {
        let hooks = TransformingHooks;
        let context = Context::new("call-1", ());
        let prepared = hooks
            .prepare(
                &context,
                InitialRequest {
                    model: "model-1".into(),
                },
            )
            .await
            .unwrap();
        let provider_request = hooks.finalize_request(&context, prepared).await.unwrap();
        let error = hooks
            .prepare(
                &context,
                InitialRequest {
                    model: String::new(),
                },
            )
            .await
            .unwrap_err();

        assert_eq!(
            provider_request,
            ProviderRequest {
                endpoint: "https://provider.example/models/model-1".into()
            }
        );
        assert_eq!(error, OperationError::MissingModel);
    }
}
