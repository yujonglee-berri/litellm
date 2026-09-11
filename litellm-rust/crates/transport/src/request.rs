use std::future::Future;
use std::marker::PhantomData;

use litellm_auth::Auth;
use reqwest::{Client, Request, Response};

use crate::Error;

#[derive(Debug)]
pub struct Unauthenticated;

#[derive(Debug)]
pub struct Authenticated;

#[derive(Debug)]
pub struct FinalRequest<State> {
    request: Request,
    state: PhantomData<State>,
}

impl FinalRequest<Unauthenticated> {
    pub fn new(request: Request) -> Self {
        Self {
            request,
            state: PhantomData,
        }
    }

    pub async fn authenticate(
        self,
        auth: &impl Auth,
    ) -> Result<FinalRequest<Authenticated>, Error> {
        let request = auth.authenticate(self.request).await?;
        Ok(FinalRequest {
            request,
            state: PhantomData,
        })
    }
}

impl<State> FinalRequest<State> {
    pub fn request(&self) -> &Request {
        &self.request
    }
}

impl FinalRequest<Authenticated> {
    pub fn into_request(self) -> Request {
        self.request
    }
}

pub trait Transport {
    fn send(
        &self,
        request: FinalRequest<Authenticated>,
    ) -> impl Future<Output = Result<Response, Error>> + Send;
}

#[derive(Clone, Debug)]
pub struct ReqwestTransport {
    client: Client,
}

impl ReqwestTransport {
    pub fn new(client: Client) -> Self {
        Self { client }
    }
}

impl Transport for ReqwestTransport {
    async fn send(&self, request: FinalRequest<Authenticated>) -> Result<Response, Error> {
        Ok(self.client.execute(request.into_request()).await?)
    }
}

#[cfg(test)]
mod tests {
    use litellm_auth::{AuthScheme, NoAuth};
    use reqwest::Method;

    use super::*;

    #[tokio::test]
    async fn authentication_preserves_the_final_request() {
        let request = Request::new(
            Method::POST,
            "https://example.com/final?version=1".parse().unwrap(),
        );
        let authenticated = FinalRequest::new(request)
            .authenticate(&NoAuth)
            .await
            .unwrap();

        assert_eq!(NoAuth.scheme(), AuthScheme::None);
        assert_eq!(authenticated.request().method(), Method::POST);
        assert_eq!(authenticated.request().url().path(), "/final");
    }
}
