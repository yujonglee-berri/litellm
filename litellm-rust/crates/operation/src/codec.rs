use crate::{Delivery, Error};

pub trait OperationCodec: Send + Sync {
    type Call: Send + Sync;
    type Context: Send + Sync;
    type Params: Send + Sync;
    type WireRequest: Send;
    type WireResponse: Send;
    type Response: Send;

    fn params(&self, call: &Self::Call) -> Result<Self::Params, Error>;

    fn encode(
        &self,
        call: &Self::Call,
        params: &Self::Params,
        delivery: Delivery,
    ) -> Result<Self::WireRequest, Error>;

    fn decode(
        &self,
        call: &Self::Call,
        response: Self::WireResponse,
    ) -> Result<Self::Response, Error>;

    fn protocol_headers(&self) -> Vec<(String, String)> {
        Vec::new()
    }
}
