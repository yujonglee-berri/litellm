#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("agent gateway route is not implemented")]
    RouteNotImplemented,
}
