use litellm_auth::AuthResolver;
use litellm_operation::{CompleteHooks, FailureHook, NoHooks, OperationCodec};

use crate::{Error, ExecuteOperation, ResolveEndpoint};

pub struct Pipeline<A, E, C, X, H = NoHooks> {
    auth: A,
    endpoint: E,
    codec: C,
    execution: X,
    hooks: H,
}

impl<A, E, C, X, H> Pipeline<A, E, C, X, H> {
    pub fn new(auth: A, endpoint: E, codec: C, execution: X, hooks: H) -> Self {
        Self {
            auth,
            endpoint,
            codec,
            execution,
            hooks,
        }
    }
}

impl<A, E, C, X, H> Pipeline<A, E, C, X, H>
where
    C: OperationCodec,
    H: CompleteHooks<C::Call, C::Response>,
{
    pub async fn handle<Services>(
        &self,
        call: C::Call,
        services: &Services,
    ) -> Result<C::Response, Error>
    where
        Services: Sync,
        A: AuthResolver<C::Call, Services>,
        Error: From<A::Error>,
        E: ResolveEndpoint<A::AuthContext, C::Params, C::Call, Services>,
        X: ExecuteOperation<C, A::Authenticator, A::AuthContext>,
    {
        let call = report_failure(
            &self.hooks,
            self.hooks.pre_call(call).map_err(Error::Adapter),
        )?;
        let params = report_failure(
            &self.hooks,
            self.codec.params(&call).map_err(Error::Adapter),
        )?;
        let authentication = report_failure(
            &self.hooks,
            self.auth
                .resolve(&call, services)
                .await
                .map_err(Error::from),
        )?;
        let endpoint = report_failure(
            &self.hooks,
            self.endpoint
                .resolve(&call, services, &authentication.context, &params),
        )?;
        let call = report_failure(
            &self.hooks,
            self.hooks
                .during_call(call, &endpoint)
                .map_err(Error::Adapter),
        )?;
        let response = report_failure(
            &self.hooks,
            self.execution
                .execute(&self.codec, &call, &params, &endpoint, &authentication)
                .await,
        )?;
        let response = report_failure(
            &self.hooks,
            self.hooks
                .post_call(&call, response)
                .map_err(Error::Adapter),
        )?;
        self.hooks.on_success(&call, &response);
        Ok(response)
    }
}

fn report_failure<T, H>(hooks: &H, result: Result<T, Error>) -> Result<T, Error>
where
    H: FailureHook,
{
    match result {
        Ok(value) => Ok(value),
        Err(error) => {
            let operation_error = litellm_operation::Error::InvalidRequest(error.to_string());
            hooks.on_failure(&operation_error);
            Err(error)
        }
    }
}
