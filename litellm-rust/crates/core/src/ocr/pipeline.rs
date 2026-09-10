use std::future::Future;

use crate::Error;
use crate::call_lifecycle::{CallLifecycle, CallLifecycleContext};
use crate::operation::{CapabilitySupport, DeliveryMode, OperationPlan, Provider};

use super::OcrClient;
use super::endpoints::ResolveOcrEndpoint;
use super::execution::ExecuteOcr;
use super::hooks::OcrLifecycleHooks;
use super::registry::OcrPlan;
use super::transformations::{OcrParameterInput, OcrTransformation};
use super::types::{LiteLLMOcrRequest, LiteLLMOcrResponse};
use crate::auth::ResolveAuth;

pub(crate) trait Ocr: Send + Sync {
    fn handle(
        &self,
        request: LiteLLMOcrRequest,
    ) -> impl Future<Output = Result<LiteLLMOcrResponse, Error>> + Send;
}

#[derive(Clone)]
pub(crate) struct OcrPipeline<A, E, D, X> {
    client: OcrClient,
    auth: A,
    endpoint: E,
    transformation: D,
    execution: X,
}

impl<A, E, D, X> OcrPipeline<A, E, D, X> {
    pub(crate) fn new(
        client: OcrClient,
        auth: A,
        endpoint: E,
        transformation: D,
        execution: X,
    ) -> Self {
        Self {
            client,
            auth,
            endpoint,
            transformation,
            execution,
        }
    }
}

impl<A, E, D, X> Ocr for OcrPipeline<A, E, D, X>
where
    A: ResolveAuth<LiteLLMOcrRequest, OcrClient, Error = super::error::OcrError>,
    E: ResolveOcrEndpoint<A::Context, D::Params>,
    D: OcrTransformation,
    X: ExecuteOcr<D, A::Authenticator, A::Context>,
{
    async fn handle(&self, request: LiteLLMOcrRequest) -> Result<LiteLLMOcrResponse, Error> {
        let prepare = tracing::trace_span!(
            target: "litellm::function_trace",
            "prepare_ocr_call"
        );
        self.handle_with_prepare(request, prepare).await
    }
}

impl<A, E, D, X> OcrPipeline<A, E, D, X>
where
    A: ResolveAuth<LiteLLMOcrRequest, OcrClient, Error = super::error::OcrError>,
    E: ResolveOcrEndpoint<A::Context, D::Params>,
    D: OcrTransformation,
    X: ExecuteOcr<D, A::Authenticator, A::Context>,
{
    async fn handle_with_prepare(
        &self,
        request: LiteLLMOcrRequest,
        prepare: tracing::Span,
    ) -> Result<LiteLLMOcrResponse, Error> {
        let context = CallLifecycleContext::new(
            "ocr",
            request.model.clone(),
            request.plan.provider().name(),
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

impl<A, E, D, X> OcrPipeline<A, E, D, X>
where
    A: ResolveAuth<LiteLLMOcrRequest, OcrClient, Error = super::error::OcrError>,
    E: ResolveOcrEndpoint<A::Context, D::Params>,
    D: OcrTransformation,
    X: ExecuteOcr<D, A::Authenticator, A::Context>,
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
        if self.transformation.wire_operation() != request.plan.wire_operation() {
            return Err(Error::InvalidProvider(
                "OCR transformation does not match the resolved wire operation".into(),
            ));
        }
        if request.plan.delivery_support(DeliveryMode::Complete) != CapabilitySupport::Supported {
            return Err(Error::Unsupported("OCR delivery mode"));
        }
        let params = prepare.in_scope(|| {
            self.transformation.transform_parameters(OcrParameterInput {
                optional_params: request.optional_params.clone(),
            })
        })?;
        let authentication = self.auth.resolve(request, &self.client).await?;
        let endpoint = self
            .endpoint
            .resolve(request, &authentication.context, &params)?;
        self.execution
            .execute(
                &self.client,
                &self.transformation,
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
        ($( $variant:ident, $auth:expr, $endpoint:expr, $transformation:expr, $execution:expr, $provider:ident; )+) => {
            match request.plan {
                $( OcrPlan::$variant => {
                    let pipeline = OcrPipeline::new(
                        client.clone(),
                        $auth,
                        $endpoint,
                        $transformation,
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
