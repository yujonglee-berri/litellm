use litellm_operation::{Delivery, Error, OperationCodec};
use litellm_operation_ocr::{OcrCall, OcrResponse, OcrResponseFormat};
use litellm_protocol::mistral::ocr::{MistralOcrParams, MistralOcrRequest, MistralOcrResponse};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MistralOcrAdapter;

impl OperationCodec for MistralOcrAdapter {
    type Call = OcrCall;
    type Context = ();
    type Params = MistralOcrParams;
    type WireRequest = MistralOcrRequest<litellm_operation_ocr::OcrDocument>;
    type WireResponse = MistralOcrResponse;
    type Response = OcrResponse;

    fn params(&self, call: &OcrCall) -> Result<MistralOcrParams, Error> {
        serde_json::from_value(call.parameters.clone().into()).map_err(|error| {
            Error::InvalidRequest(format!("invalid Mistral OCR parameters: {error}"))
        })
    }

    fn encode(
        &self,
        call: &OcrCall,
        params: &MistralOcrParams,
        _delivery: Delivery,
    ) -> Result<Self::WireRequest, Error> {
        Ok(MistralOcrRequest {
            model: call.model.clone(),
            document: call.document.clone(),
            params: params.clone(),
        })
    }

    fn decode(&self, call: &OcrCall, response: MistralOcrResponse) -> Result<OcrResponse, Error> {
        let native = (call.response_format == OcrResponseFormat::Native)
            .then(|| serde_json::to_value(&response))
            .transpose()
            .map_err(|error| Error::InvalidResponse(format!("native response failed: {error}")))?;
        let mut extra_fields = response.extra_fields;
        extra_fields.remove("object");
        extra_fields.remove("provider_native_response");
        Ok(OcrResponse {
            pages: response.pages,
            model: response.model.unwrap_or_else(|| call.model.clone()),
            document_annotation: response.document_annotation,
            usage_info: response.usage_info,
            object: "ocr".into(),
            extra_fields,
            provider_native_response: native,
        })
    }
}
