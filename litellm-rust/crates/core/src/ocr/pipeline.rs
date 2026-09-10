use std::future::Future;

use crate::Error;
use crate::call_lifecycle::{CallLifecycle, CallLifecycleContext};

use super::OcrClient;
use super::auth::ResolveOcrAuth;
use super::codecs::{DecodeOcrResponse, EncodeOcrRequest};
use super::endpoints::ResolveOcrEndpoint;
use super::execution::ExecuteOcr;
use super::hooks::OcrLifecycleHooks;
use super::registry::OcrPipelineKind;
use super::types::{LiteLLMOcrRequest, LiteLLMOcrResponse};

pub(crate) trait Ocr: Send + Sync {
    fn handle(
        &self,
        request: LiteLLMOcrRequest,
    ) -> impl Future<Output = Result<LiteLLMOcrResponse, Error>> + Send;
}

#[derive(Clone)]
pub(crate) struct OcrPipeline<A, E, C, X> {
    client: OcrClient,
    auth: A,
    endpoint: E,
    codec: C,
    execution: X,
}

impl<A, E, C, X> OcrPipeline<A, E, C, X> {
    pub(crate) fn new(client: OcrClient, auth: A, endpoint: E, codec: C, execution: X) -> Self {
        Self {
            client,
            auth,
            endpoint,
            codec,
            execution,
        }
    }
}

impl<A, E, C, X> Ocr for OcrPipeline<A, E, C, X>
where
    A: ResolveOcrAuth,
    E: ResolveOcrEndpoint<A::Context, C::Params>,
    C: EncodeOcrRequest + DecodeOcrResponse,
    X: ExecuteOcr<C, A::Authenticator, A::Context>,
{
    async fn handle(&self, request: LiteLLMOcrRequest) -> Result<LiteLLMOcrResponse, Error> {
        let prepare = tracing::trace_span!(
            target: "litellm::function_trace",
            "prepare_ocr_call"
        );
        self.handle_with_prepare(request, prepare).await
    }
}

impl<A, E, C, X> OcrPipeline<A, E, C, X>
where
    A: ResolveOcrAuth,
    E: ResolveOcrEndpoint<A::Context, C::Params>,
    C: EncodeOcrRequest + DecodeOcrResponse,
    X: ExecuteOcr<C, A::Authenticator, A::Context>,
{
    async fn handle_with_prepare(
        &self,
        request: LiteLLMOcrRequest,
        prepare: tracing::Span,
    ) -> Result<LiteLLMOcrResponse, Error> {
        let context = CallLifecycleContext::new(
            "ocr",
            request.model.clone(),
            request.pipeline.provider().as_str(),
            request
                .litellm_call_id
                .clone()
                .unwrap_or_else(|| format!("ocr-{:032x}", rand::random::<u128>())),
        );
        let hooks = OcrLifecycleHooks {
            hooks: request.hooks.clone(),
            provider_name: context.custom_llm_provider.clone(),
        };
        CallLifecycle::default()
            .run(context, request, &hooks, |request| async move {
                self.execute(&request, &prepare).await
            })
            .await
    }
}

impl<A, E, C, X> OcrPipeline<A, E, C, X>
where
    A: ResolveOcrAuth,
    E: ResolveOcrEndpoint<A::Context, C::Params>,
    C: EncodeOcrRequest + DecodeOcrResponse,
    X: ExecuteOcr<C, A::Authenticator, A::Context>,
{
    #[tracing::instrument(
        name = "execute_ocr_provider_call",
        target = "litellm::function_trace",
        level = "trace",
        skip_all
    )]
    async fn execute(
        &self,
        request: &LiteLLMOcrRequest,
        prepare: &tracing::Span,
    ) -> Result<LiteLLMOcrResponse, Error> {
        let params = prepare.in_scope(|| self.codec.params(request))?;
        let authentication = self.auth.resolve(request, &self.client).await?;
        let endpoint = self
            .endpoint
            .resolve(request, &authentication.context, &params)?;
        self.execution
            .execute(
                &self.client,
                &self.codec,
                request,
                &params,
                &endpoint,
                &authentication,
            )
            .await
            .map_err(Error::from)
    }
}

pub(crate) async fn perform_ocr_request(
    client: &OcrClient,
    request: LiteLLMOcrRequest,
) -> Result<LiteLLMOcrResponse, Error> {
    dispatch_ocr_request(client, request, None).await
}

pub(crate) async fn perform_prepared_ocr_request(
    client: &OcrClient,
    request: LiteLLMOcrRequest,
    prepare: tracing::Span,
) -> Result<LiteLLMOcrResponse, Error> {
    dispatch_ocr_request(client, request, Some(prepare)).await
}

async fn dispatch_ocr_request(
    client: &OcrClient,
    request: LiteLLMOcrRequest,
    prepare: Option<tracing::Span>,
) -> Result<LiteLLMOcrResponse, Error> {
    macro_rules! execute_selected_pipeline {
        ($( $variant:ident, $auth:expr, $endpoint:expr, $codec:expr, $execution:expr, $provider:ident; )+) => {
            match request.pipeline {
                $( OcrPipelineKind::$variant => {
                    let pipeline = OcrPipeline::new(
                        client.clone(),
                        $auth,
                        $endpoint,
                        $codec,
                        $execution,
                    );
                    match prepare {
                        Some(prepare) => pipeline.handle_with_prepare(request, prepare).await,
                        None => pipeline.handle(request).await,
                    }
                }, )+
            }
        };
    }
    super::registry::for_each_ocr_pipeline!(execute_selected_pipeline)
}
