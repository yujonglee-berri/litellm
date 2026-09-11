use crate::Error;

pub trait ResolveEndpoint<AuthContext, Params, Call, Context>: Send + Sync {
    fn resolve(
        &self,
        call: &Call,
        context: &Context,
        auth: &AuthContext,
        params: &Params,
    ) -> Result<String, Error>;
}
