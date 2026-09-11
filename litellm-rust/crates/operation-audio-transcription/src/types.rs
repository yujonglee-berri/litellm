use litellm_operation::{Operation, SessionOperation};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AudioTranscription;

impl Operation for AudioTranscription {
    type Request<'a> = AudioTranscriptionRequest<'a>;
    type Response = AudioTranscriptionResponse;

    const NAME: &'static str = "audio_transcription";
}

impl SessionOperation for AudioTranscription {
    type ClientEvent = AudioTranscriptionClientEvent;
    type ServerEvent = AudioTranscriptionServerEvent;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AudioTranscriptionRequest<'a> {
    pub model: &'a str,
    pub language: Option<&'a str>,
    pub source: TranscriptionSource<'a>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TranscriptionSource<'a> {
    File(AudioInput<'a>),
    Live,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioInput<'a> {
    Bytes {
        data: &'a [u8],
        filename: Option<&'a str>,
        media_type: Option<&'a str>,
    },
    Uri(&'a str),
}

impl AudioInput<'_> {
    pub fn uri(&self) -> Option<&str> {
        match self {
            Self::Uri(uri) => Some(uri),
            Self::Bytes { .. } => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioTranscriptionResponse {
    pub text: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AudioTranscriptionClientEvent {
    Audio(Vec<u8>),
    Finalize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AudioTranscriptionServerEvent {
    TranscriptDelta(String),
    Completed(AudioTranscriptionResponse),
}

#[cfg(test)]
mod tests {
    use super::*;
    use litellm_operation::{
        Complete, Delivery, DeliveryFor, Fidelity, OperationPlan, OperationTransformation,
        Provider, Session, SupportsWire, TransformationForDelivery, TransformationKind,
        WireOperation,
    };

    #[test]
    fn file_source_keeps_bytes_and_uri_distinct() {
        let bytes = AudioInput::Bytes {
            data: &[1, 2, 3],
            filename: Some("clip.wav"),
            media_type: Some("audio/wav"),
        };
        let uri = AudioInput::Uri("https://example.com/clip.wav");

        assert_eq!(bytes.uri(), None);
        assert_eq!(uri.uri(), Some("https://example.com/clip.wav"));
        assert_eq!(
            audio_len(AudioTranscriptionRequest {
                model: "whisper-1",
                language: Some("en"),
                source: TranscriptionSource::File(bytes),
            }),
            3
        );
    }

    fn audio_len(request: <AudioTranscription as Operation>::Request<'_>) -> usize {
        match request.source {
            TranscriptionSource::File(AudioInput::Bytes { data, .. }) => data.len(),
            TranscriptionSource::File(AudioInput::Uri(_)) | TranscriptionSource::Live => 0,
        }
    }

    #[derive(Clone, Copy, Default)]
    struct TestProvider;

    impl Provider for TestProvider {
        const NAME: &'static str = "test";
    }

    #[derive(Clone, Copy, Default)]
    struct FileWire;

    impl WireOperation for FileWire {
        const NAME: &'static str = "test.transcribe";
    }

    #[derive(Clone, Copy, Default)]
    struct LiveWire;

    impl WireOperation for LiveWire {
        const NAME: &'static str = "test.listen";
    }

    #[derive(Clone, Copy, Default)]
    struct FileTransformation;

    impl OperationTransformation<AudioTranscription, FileWire> for FileTransformation {
        const FIDELITY: Fidelity = Fidelity::Exact;
    }

    impl TransformationKind<AudioTranscription, FileWire> for FileTransformation {
        const NAME: &'static str = "audio_transcription.to_test_file";
    }

    impl TransformationForDelivery<AudioTranscription, FileWire, Complete> for FileTransformation {}

    #[derive(Clone, Copy, Default)]
    struct LiveTransformation;

    impl OperationTransformation<AudioTranscription, LiveWire> for LiveTransformation {
        const FIDELITY: Fidelity = Fidelity::Exact;
    }

    impl TransformationKind<AudioTranscription, LiveWire> for LiveTransformation {
        const NAME: &'static str = "audio_transcription.to_test_live";
    }

    impl TransformationForDelivery<AudioTranscription, LiveWire, Session> for LiveTransformation {}

    impl SupportsWire<FileWire, Complete> for TestProvider {}
    impl SupportsWire<LiveWire, Session> for TestProvider {}

    fn accepts_delivery<O: Operation, D: DeliveryFor<O>>(_: D) {}

    #[test]
    fn file_and_live_are_distinct_typed_deliveries() {
        accepts_delivery::<AudioTranscription, Complete>(Complete);
        accepts_delivery::<AudioTranscription, Session>(Session);

        let file = OperationPlan::<AudioTranscription, _, _, Complete, _>::new(
            TestProvider,
            FileWire,
            FileTransformation,
        );
        let live = OperationPlan::<AudioTranscription, _, _, Session, _>::new(
            TestProvider,
            LiveWire,
            LiveTransformation,
        );

        let live_request = AudioTranscriptionRequest {
            model: "nova-3",
            language: Some("en"),
            source: TranscriptionSource::Live,
        };
        let event: <AudioTranscription as SessionOperation>::ClientEvent =
            AudioTranscriptionClientEvent::Audio(b"pcm".to_vec());

        assert_eq!(file.delivery(), Delivery::Complete);
        assert_eq!(live.delivery(), Delivery::Session);
        assert_eq!(live_request.model, "nova-3");
        assert!(matches!(event, AudioTranscriptionClientEvent::Audio(data) if data == b"pcm"));
    }
}
