use std::path::{Component, Path as StdPath, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::header::{
    AUTHORIZATION, CACHE_CONTROL, CONTENT_LENGTH, CONTENT_SECURITY_POLICY, CONTENT_TYPE, ETAG,
    IF_NONE_MATCH, X_CONTENT_TYPE_OPTIONS,
};
use axum::http::{HeaderMap, HeaderValue, Request, Response, StatusCode};
use axum::middleware::{self, Next};
use axum::response::IntoResponse;
use axum::{Json, Router, routing::get, routing::post};
use mdict_web_domain::{HealthResponse, ReadinessResponse, ResourceBody, ServiceError};
use mdict_web_service::{ReloadableDictionaryService, ServiceBuildError};
use metrics_exporter_prometheus::PrometheusHandle;
use serde::Deserialize;
use tokio::sync::Mutex;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::trace::TraceLayer;
use uuid::Uuid;

const ENTRY_CSP: &str = "default-src 'none'; script-src 'unsafe-inline'; img-src 'self' data: blob:; media-src 'self' data:; style-src 'self' 'unsafe-inline'; font-src 'self' data:; frame-ancestors 'self'; base-uri 'none'; form-action 'none'; connect-src 'none'";

#[derive(Clone)]
pub struct HttpState {
    pub service: ReloadableDictionaryService,
    pub metrics: Option<PrometheusHandle>,
    frontend: Option<FrontendAssets>,
    rate_limiter: BasicRateLimiter,
}

pub fn router(state: HttpState, request_body_limit_bytes: usize, metrics_path: &str) -> Router {
    let metrics_path = metrics_path.trim().to_owned();

    let mut router = Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/api/v1/dictionaries", get(list_dictionaries))
        .route("/api/v1/search/suggest", get(suggest_search))
        .route("/api/v1/search/lookup", get(lookup_search))
        .route(
            "/api/v1/dictionaries/{dictionary_id}",
            get(dictionary_detail),
        )
        .route(
            "/api/v1/dictionaries/{dictionary_id}/suggest",
            get(suggest_dictionary),
        )
        .route(
            "/api/v1/dictionaries/{dictionary_id}/entries/lookup",
            get(lookup_entry),
        )
        .route(
            "/api/v1/dictionaries/{dictionary_id}/entries/content",
            get(entry_content),
        )
        .route(
            "/api/v1/dictionaries/{dictionary_id}/resources/content",
            get(resource_content),
        )
        .route("/api/v1/admin/reload", post(reload_admin))
        .layer(axum::extract::DefaultBodyLimit::max(
            request_body_limit_bytes,
        ))
        .layer(TraceLayer::new_for_http())
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            rate_limit_middleware,
        ));

    if metrics_path == "/metrics" {
        router = router.route("/metrics", get(metrics_endpoint));
    } else {
        router = router.route(&metrics_path, get(metrics_endpoint));
    }

    if state.frontend.is_some() {
        router = router
            .route("/", get(frontend_index))
            .route("/{*path}", get(frontend_path));
    }

    router.with_state(state)
}

impl HttpState {
    pub fn new(
        service: ReloadableDictionaryService,
        metrics: Option<PrometheusHandle>,
        frontend: Option<FrontendAssets>,
    ) -> Self {
        Self {
            service,
            metrics,
            frontend,
            rate_limiter: BasicRateLimiter::default(),
        }
    }
}

#[derive(Clone)]
pub struct FrontendAssets {
    dist_dir: Arc<PathBuf>,
}

impl FrontendAssets {
    pub fn new(dist_dir: PathBuf) -> Option<Self> {
        dist_dir.join("index.html").exists().then_some(Self {
            dist_dir: Arc::new(dist_dir),
        })
    }

    fn index_path(&self) -> PathBuf {
        self.dist_dir.join("index.html")
    }
}

#[derive(Clone, Default)]
struct BasicRateLimiter {
    inner: Arc<Mutex<RateLimitBucket>>,
}

#[derive(Debug)]
struct RateLimitBucket {
    window_started_at: Instant,
    count: u64,
}

impl Default for RateLimitBucket {
    fn default() -> Self {
        Self {
            window_started_at: Instant::now(),
            count: 0,
        }
    }
}

impl BasicRateLimiter {
    async fn allow(&self, limit: u64) -> bool {
        if limit == 0 {
            return true;
        }

        let mut bucket = self.inner.lock().await;
        if bucket.window_started_at.elapsed() >= Duration::from_secs(1) {
            bucket.window_started_at = Instant::now();
            bucket.count = 0;
        }
        if bucket.count >= limit {
            return false;
        }
        bucket.count += 1;
        true
    }
}

#[derive(Debug, Deserialize)]
struct SuggestQuery {
    q: String,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct SearchSuggestQuery {
    q: String,
    limit: Option<usize>,
    #[serde(default)]
    dictionary_id: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct KeyQuery {
    key: String,
}

#[derive(Debug, Deserialize)]
struct SearchLookupQuery {
    key: String,
    #[serde(default)]
    dictionary_id: Vec<String>,
}

async fn healthz() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_owned(),
    })
}

async fn readyz(State(state): State<HttpState>) -> Json<ReadinessResponse> {
    let snapshot = state.service.snapshot().await;
    Json(snapshot.readiness())
}

async fn list_dictionaries(
    State(state): State<HttpState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let request_id = request_id(&headers);
    let snapshot = state.service.snapshot().await;
    JsonResponse::json_or_error(Ok(snapshot.list_dictionaries()), &request_id)
}

async fn dictionary_detail(
    Path(dictionary_id): Path<String>,
    State(state): State<HttpState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let request_id = request_id(&headers);
    let snapshot = state.service.snapshot().await;
    JsonResponse::json_or_error(snapshot.dictionary_detail(&dictionary_id), &request_id)
}

async fn suggest_search(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Query(query): Query<SearchSuggestQuery>,
) -> impl IntoResponse {
    let request_id = request_id(&headers);
    let snapshot = state.service.snapshot().await;
    JsonResponse::json_or_error(
        snapshot
            .search_suggest(query.q, query.limit, query.dictionary_id)
            .await,
        &request_id,
    )
}

async fn suggest_dictionary(
    Path(dictionary_id): Path<String>,
    State(state): State<HttpState>,
    headers: HeaderMap,
    Query(query): Query<SuggestQuery>,
) -> impl IntoResponse {
    let request_id = request_id(&headers);
    let snapshot = state.service.snapshot().await;
    JsonResponse::json_or_error(
        snapshot.suggest(&dictionary_id, query.q, query.limit).await,
        &request_id,
    )
}

async fn lookup_search(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Query(query): Query<SearchLookupQuery>,
) -> impl IntoResponse {
    let request_id = request_id(&headers);
    let snapshot = state.service.snapshot().await;
    JsonResponse::json_or_error(
        snapshot.search_lookup(query.key, query.dictionary_id).await,
        &request_id,
    )
}

async fn lookup_entry(
    Path(dictionary_id): Path<String>,
    State(state): State<HttpState>,
    headers: HeaderMap,
    Query(query): Query<KeyQuery>,
) -> impl IntoResponse {
    let request_id = request_id(&headers);
    let snapshot = state.service.snapshot().await;
    JsonResponse::json_or_error(
        snapshot.lookup(&dictionary_id, query.key).await,
        &request_id,
    )
}

async fn entry_content(
    Path(dictionary_id): Path<String>,
    State(state): State<HttpState>,
    headers: HeaderMap,
    Query(query): Query<KeyQuery>,
) -> Response<Body> {
    let request_id = request_id(&headers);
    let snapshot = state.service.snapshot().await;
    match snapshot.entry_content(&dictionary_id, query.key).await {
        Ok(content) => {
            if let Some(mut response) = maybe_not_modified(&headers, &content.etag) {
                set_header(
                    response.headers_mut(),
                    CACHE_CONTROL,
                    &content.cache_control,
                );
                set_entry_security_headers(response.headers_mut());
                set_header(response.headers_mut(), "x-request-id", &request_id);
                response
            } else {
                let mut response = Response::new(Body::from(content.html));
                *response.status_mut() = StatusCode::OK;
                set_header(
                    response.headers_mut(),
                    CONTENT_TYPE,
                    "text/html; charset=utf-8",
                );
                set_header(response.headers_mut(), ETAG, &content.etag);
                set_header(
                    response.headers_mut(),
                    CACHE_CONTROL,
                    &content.cache_control,
                );
                set_entry_security_headers(response.headers_mut());
                set_header(response.headers_mut(), "x-request-id", &request_id);
                response
            }
        }
        Err(error) => error_response(error, &request_id),
    }
}

async fn resource_content(
    Path(dictionary_id): Path<String>,
    State(state): State<HttpState>,
    headers: HeaderMap,
    Query(query): Query<KeyQuery>,
) -> Response<Body> {
    let request_id = request_id(&headers);
    let snapshot = state.service.snapshot().await;
    match snapshot.resource_content(&dictionary_id, query.key).await {
        Ok(content) => {
            if let Some(mut response) = maybe_not_modified(&headers, &content.etag) {
                set_header(
                    response.headers_mut(),
                    CACHE_CONTROL,
                    &content.cache_control,
                );
                set_header(response.headers_mut(), "x-request-id", &request_id);
                response
            } else {
                let body = match content.body {
                    ResourceBody::Bytes(bytes) => Body::from(bytes),
                    ResourceBody::Stream(stream) => Body::from_stream(stream),
                };
                let mut response = Response::new(body);
                *response.status_mut() = StatusCode::OK;
                set_header(response.headers_mut(), CONTENT_TYPE, &content.content_type);
                set_header(response.headers_mut(), ETAG, &content.etag);
                set_header(
                    response.headers_mut(),
                    CACHE_CONTROL,
                    &content.cache_control,
                );
                if let Some(content_length) = content.content_length {
                    set_header(
                        response.headers_mut(),
                        CONTENT_LENGTH,
                        &content_length.to_string(),
                    );
                }
                set_header(response.headers_mut(), X_CONTENT_TYPE_OPTIONS, "nosniff");
                set_header(response.headers_mut(), "x-request-id", &request_id);
                response
            }
        }
        Err(error) => error_response(error, &request_id),
    }
}

async fn reload_admin(State(state): State<HttpState>, headers: HeaderMap) -> Response<Body> {
    let request_id = request_id(&headers);
    let snapshot = state.service.snapshot().await;
    let Some(expected_token) = snapshot.config().admin.reload_token.clone() else {
        return error_response(
            ServiceError::unauthorized("admin reload is not configured"),
            &request_id,
        );
    };
    drop(snapshot);

    match bearer_token(&headers) {
        Some(token) if token == expected_token => match state.service.reload().await {
            Ok(response) => json_ok(response, &request_id),
            Err(error) => error_response(map_build_error(error), &request_id),
        },
        _ => error_response(
            ServiceError::unauthorized("invalid admin token"),
            &request_id,
        ),
    }
}

async fn metrics_endpoint(State(state): State<HttpState>, headers: HeaderMap) -> Response<Body> {
    let request_id = request_id(&headers);
    match &state.metrics {
        Some(handle) => {
            let mut response = Response::new(Body::from(handle.render()));
            *response.status_mut() = StatusCode::OK;
            set_header(
                response.headers_mut(),
                CONTENT_TYPE,
                "text/plain; version=0.0.4",
            );
            set_header(response.headers_mut(), "x-request-id", &request_id);
            response
        }
        None => error_response(
            ServiceError::dictionary_unavailable("metrics", "metrics endpoint is disabled"),
            &request_id,
        ),
    }
}

async fn frontend_index(State(state): State<HttpState>, headers: HeaderMap) -> Response<Body> {
    let request_id = request_id(&headers);
    serve_frontend(&state, None, &request_id).await
}

async fn frontend_path(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Path(path): Path<String>,
) -> Response<Body> {
    let request_id = request_id(&headers);
    serve_frontend(&state, Some(path), &request_id).await
}

async fn rate_limit_middleware(
    State(state): State<HttpState>,
    request: Request<Body>,
    next: Next,
) -> Response<Body> {
    let request_id = request_id(request.headers());
    let limit = state
        .service
        .snapshot()
        .await
        .config()
        .server
        .rate_limit_per_second;

    if state.rate_limiter.allow(limit).await {
        next.run(request).await
    } else {
        error_response(
            ServiceError::rate_limited("rate limit exceeded for the current 1-second window"),
            &request_id,
        )
    }
}

async fn serve_frontend(
    state: &HttpState,
    path: Option<String>,
    request_id: &str,
) -> Response<Body> {
    let Some(frontend) = &state.frontend else {
        return not_found_response(request_id);
    };

    if let Some(path) = &path {
        if reserved_route_path(path) {
            return not_found_response(request_id);
        }
    }

    let Some(asset_path) = resolve_frontend_asset(frontend, path.as_deref()) else {
        return not_found_response(request_id);
    };

    let file_path = asset_path.path;
    match tokio::fs::read(&file_path).await {
        Ok(bytes) => {
            let mut response = Response::new(Body::from(bytes));
            *response.status_mut() = StatusCode::OK;
            set_header(
                response.headers_mut(),
                CONTENT_TYPE,
                &asset_content_type(&file_path, asset_path.is_index),
            );
            set_header(
                response.headers_mut(),
                CACHE_CONTROL,
                asset_cache_control(path.as_deref(), asset_path.is_index),
            );
            set_header(response.headers_mut(), X_CONTENT_TYPE_OPTIONS, "nosniff");
            set_header(response.headers_mut(), "x-request-id", request_id);
            response
        }
        Err(_) => not_found_response(request_id),
    }
}

fn maybe_not_modified(headers: &HeaderMap, etag: &str) -> Option<Response<Body>> {
    let if_none_match = headers.get(IF_NONE_MATCH)?.to_str().ok()?;
    let matched = if_none_match
        .split(',')
        .map(str::trim)
        .any(|candidate| candidate == etag || candidate == "*");
    if !matched {
        return None;
    }

    let mut response = Response::new(Body::empty());
    *response.status_mut() = StatusCode::NOT_MODIFIED;
    set_header(response.headers_mut(), ETAG, etag);
    Some(response)
}

fn set_entry_security_headers(headers: &mut HeaderMap) {
    set_header(headers, CONTENT_SECURITY_POLICY, ENTRY_CSP);
    set_header(headers, X_CONTENT_TYPE_OPTIONS, "nosniff");
}

fn request_id(headers: &HeaderMap) -> String {
    headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
        .unwrap_or_else(|| Uuid::now_v7().to_string())
}

fn bearer_token(headers: &HeaderMap) -> Option<String> {
    let header = headers.get(AUTHORIZATION)?.to_str().ok()?;
    header
        .strip_prefix("Bearer ")
        .map(str::trim)
        .map(str::to_owned)
}

fn map_build_error(error: ServiceBuildError) -> ServiceError {
    ServiceError::internal(error.to_string())
}

fn json_ok<T: serde::Serialize>(value: T, request_id: &str) -> Response<Body> {
    let mut response = Json(value).into_response();
    set_header(response.headers_mut(), "x-request-id", request_id);
    response
}

fn error_response(error: ServiceError, request_id: &str) -> Response<Body> {
    let status = error.status;
    let mut response = Json(error.into_envelope(request_id.to_owned())).into_response();
    *response.status_mut() = status;
    set_header(response.headers_mut(), "x-request-id", request_id);
    response
}

fn not_found_response(request_id: &str) -> Response<Body> {
    let mut response = Response::new(Body::empty());
    *response.status_mut() = StatusCode::NOT_FOUND;
    set_header(response.headers_mut(), "x-request-id", request_id);
    response
}

fn set_header(headers: &mut HeaderMap, name: impl axum::http::header::IntoHeaderName, value: &str) {
    if let Ok(value) = HeaderValue::from_str(value) {
        headers.insert(name, value);
    }
}

struct FrontendAssetPath {
    path: PathBuf,
    is_index: bool,
}

fn reserved_route_path(path: &str) -> bool {
    path == "api"
        || path.starts_with("api/")
        || path == "healthz"
        || path == "readyz"
        || path == "metrics"
        || path.starts_with("metrics/")
}

fn resolve_frontend_asset(
    frontend: &FrontendAssets,
    path: Option<&str>,
) -> Option<FrontendAssetPath> {
    let Some(path) = path.filter(|value| !value.is_empty()) else {
        return Some(FrontendAssetPath {
            path: frontend.index_path(),
            is_index: true,
        });
    };

    let relative = sanitize_relative_path(path)?;
    let candidate = frontend.dist_dir.join(&relative);
    if candidate.is_file() {
        return Some(FrontendAssetPath {
            path: candidate,
            is_index: false,
        });
    }

    (StdPath::new(path).extension().is_none()).then(|| FrontendAssetPath {
        path: frontend.index_path(),
        is_index: true,
    })
}

fn sanitize_relative_path(path: &str) -> Option<PathBuf> {
    let relative = StdPath::new(path);
    let mut sanitized = PathBuf::new();

    for component in relative.components() {
        match component {
            Component::Normal(part) => sanitized.push(part),
            Component::CurDir => {}
            _ => return None,
        }
    }

    (!sanitized.as_os_str().is_empty()).then_some(sanitized)
}

fn asset_content_type(path: &StdPath, is_index: bool) -> String {
    if is_index {
        "text/html; charset=utf-8".to_owned()
    } else {
        mime_guess::from_path(path)
            .first_or_octet_stream()
            .essence_str()
            .to_owned()
    }
}

fn asset_cache_control(path: Option<&str>, is_index: bool) -> &'static str {
    if is_index {
        "no-cache"
    } else if matches!(path, Some(value) if value.starts_with("assets/")) {
        "public, max-age=31536000, immutable"
    } else {
        "public, max-age=3600"
    }
}

struct JsonResponse;

impl JsonResponse {
    fn json_or_error<T: serde::Serialize>(
        result: Result<T, ServiceError>,
        request_id: &str,
    ) -> Response<Body> {
        match result {
            Ok(value) => json_ok(value, request_id),
            Err(error) => error_response(error, request_id),
        }
    }
}
