use std::future::Future;

use crate::Auth;

pub struct ResolvedAuth<A, C> {
    pub authenticator: A,
    pub headers: Vec<(String, String)>,
    pub context: C,
}

pub trait ResolveAuth<Input: ?Sized, Services: ?Sized>: Send + Sync {
    type Authenticator: Auth;
    type Context: Send + Sync;
    type Error;

    fn resolve(
        &self,
        input: &Input,
        services: &Services,
    ) -> impl Future<Output = Result<ResolvedAuth<Self::Authenticator, Self::Context>, Self::Error>> + Send;
}
