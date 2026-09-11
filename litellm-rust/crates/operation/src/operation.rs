pub trait Operation: Send + Sync + 'static {
    type Request<'a>
    where
        Self: 'a;
    type Response;

    const NAME: &'static str;
}

pub trait StreamingOperation: Operation {
    type StreamEvent;
}

pub trait SessionOperation: Operation {
    type ClientEvent;
    type ServerEvent;
}
