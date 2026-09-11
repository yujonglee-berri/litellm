use litellm_auth::{Auth, ResolvedAuth};
use litellm_operation::{Error, ExecuteOperation, OperationCodec};
use litellm_transport::Transport;
use serde::Serialize;
use serde::de::DeserializeOwned;

#[derive(Debug)]
pub struct InlineJsonExecution<T> {
    #[allow(dead_code, reason = "read once the TODO migration lands")]
    transport: T,
}

impl<T> InlineJsonExecution<T> {
    pub fn new(transport: T) -> Self {
        Self { transport }
    }
}

impl<C, A, AuthContext, T> ExecuteOperation<C, A, AuthContext> for InlineJsonExecution<T>
where
    C: OperationCodec,
    C::WireRequest: Serialize,
    C::WireResponse: DeserializeOwned,
    A: Auth,
    AuthContext: Send + Sync,
    T: Transport + Send + Sync,
{
    async fn execute(
        &self,
        _codec: &C,
        _call: &C::Call,
        _params: &C::Params,
        _endpoint: &str,
        _auth: &ResolvedAuth<A, AuthContext>,
    ) -> Result<C::Response, Error> {
        todo!("TODO(ocr-migration): fetch or inline the document before the JSON send")
    }
}

#[derive(Debug)]
pub struct DocumentIntelligenceExecution<T> {
    #[allow(dead_code, reason = "read once the TODO migration lands")]
    transport: T,
}

impl<T> DocumentIntelligenceExecution<T> {
    pub fn new(transport: T) -> Self {
        Self { transport }
    }
}

impl<C, A, AuthContext, T> ExecuteOperation<C, A, AuthContext> for DocumentIntelligenceExecution<T>
where
    C: OperationCodec,
    C::WireRequest: Serialize,
    C::WireResponse: DeserializeOwned,
    A: Auth,
    AuthContext: Send + Sync,
    T: Transport + Send + Sync,
{
    async fn execute(
        &self,
        _codec: &C,
        _call: &C::Call,
        _params: &C::Params,
        _endpoint: &str,
        _auth: &ResolvedAuth<A, AuthContext>,
    ) -> Result<C::Response, Error> {
        todo!("TODO(ocr-migration): submit, poll the operation-location, then decode")
    }
}

#[derive(Debug)]
pub struct ReductoExecution<T> {
    #[allow(dead_code, reason = "read once the TODO migration lands")]
    transport: T,
}

impl<T> ReductoExecution<T> {
    pub fn new(transport: T) -> Self {
        Self { transport }
    }
}

impl<C, A, AuthContext, T> ExecuteOperation<C, A, AuthContext> for ReductoExecution<T>
where
    C: OperationCodec,
    C::WireRequest: Serialize,
    C::WireResponse: DeserializeOwned,
    A: Auth,
    AuthContext: Send + Sync,
    T: Transport + Send + Sync,
{
    async fn execute(
        &self,
        _codec: &C,
        _call: &C::Call,
        _params: &C::Params,
        _endpoint: &str,
        _auth: &ResolvedAuth<A, AuthContext>,
    ) -> Result<C::Response, Error> {
        todo!("TODO(ocr-migration): upload the document, then parse and decode")
    }
}
