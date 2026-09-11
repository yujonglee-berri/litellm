use std::future::Future;

use crate::Auth;

pub struct ResolvedAuth<A, C> {
    pub authenticator: A,
    pub headers: Vec<(String, String)>,
    pub context: C,
}

pub trait AuthResolver<Input: ?Sized, Services: ?Sized>: Send + Sync {
    type Authenticator: Auth;
    type AuthContext: Send + Sync;
    type Error;

    fn resolve(
        &self,
        input: &Input,
        services: &Services,
    ) -> impl Future<
        Output = Result<ResolvedAuth<Self::Authenticator, Self::AuthContext>, Self::Error>,
    > + Send;
}
