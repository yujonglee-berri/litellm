use std::time::Duration;

use axum::Router;
use axum::extract::{MatchedPath, OriginalUri};
use axum::http::{Request, Response};
use opentelemetry::global;
use opentelemetry_http::HeaderExtractor;
use tower_http::classify::ServerErrorsFailureClass;
use tower_http::trace::TraceLayer;
use tracing::{Span, info_span};
use tracing_opentelemetry::OpenTelemetrySpanExt;

use crate::{Route, RouteContext};

pub fn instrument(router: Router) -> Router {
    router.layer(
        TraceLayer::new_for_http()
            .make_span_with(|request: &Request<_>| {
                let path = request
                    .extensions()
                    .get::<OriginalUri>()
                    .map_or_else(|| request.uri().path(), |uri| uri.path());
                let template = request
                    .extensions()
                    .get::<MatchedPath>()
                    .map(|matched| matched.as_str().to_string());
                let context =
                    RouteContext::new(request.method().clone(), Route::new(path, template));
                let span = info_span!(
                    "http.request",
                    otel.name = %context.span_name(),
                    otel.kind = "server",
                    http.request.method = %context.method(),
                    http.route = %context.route().telemetry_path(),
                    http.response.status_code = tracing::field::Empty,
                    error.type = tracing::field::Empty,
                );
                let parent = global::get_text_map_propagator(|propagator| {
                    propagator.extract(&HeaderExtractor(request.headers()))
                });
                drop(span.set_parent(parent));
                span
            })
            .on_response(|response: &Response<_>, _latency: Duration, span: &Span| {
                span.record("http.response.status_code", response.status().as_u16());
            })
            .on_failure(
                |failure: ServerErrorsFailureClass, _latency: Duration, span: &Span| {
                    span.record("error.type", failure.to_string());
                },
            ),
    )
}

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::get;
    use tower::ServiceExt;

    use super::instrument;

    #[tokio::test]
    async fn layer_preserves_router_behavior() {
        let response = instrument(
            Router::new().route("/v1/items/:id", get(|| async { StatusCode::NO_CONTENT })),
        )
        .oneshot(
            Request::builder()
                .uri("/v1/items/item-1")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }
}
