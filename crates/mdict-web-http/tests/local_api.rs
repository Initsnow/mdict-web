use std::fs;
use std::path::{Path, PathBuf};

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use mdict_web_http::{FrontendAssets, HttpState, router};
use mdict_web_service::ReloadableDictionaryService;
use tower::ServiceExt;

#[tokio::test]
async fn local_fixture_http_smoke_test() {
    let Some((mdx_path, mdd_path)) = local_fixture_paths() else {
        return;
    };

    let fixture_dir = reusable_fixture_dir();
    fs::create_dir_all(&fixture_dir).expect("fixture dir should be created");
    let config_path = write_config(&fixture_dir, &mdx_path, &mdd_path);
    let frontend_dist = write_frontend_dist(&fixture_dir);
    let service = ReloadableDictionaryService::load_from_path(&config_path)
        .await
        .expect("service should load");
    let snapshot = service.snapshot().await;
    let body_limit = snapshot.config().server.request_body_limit_bytes;
    let metrics_path = snapshot.config().observability.metrics_path.clone();
    drop(snapshot);

    let app = router(
        HttpState::new(service, None, FrontendAssets::new(frontend_dist)),
        body_limit,
        &metrics_path,
    );

    let list = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/dictionaries")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("list request should succeed");
    assert_eq!(list.status(), StatusCode::OK);

    let frontend_index = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("frontend index request should succeed");
    assert_eq!(frontend_index.status(), StatusCode::OK);
    let frontend_index_body = to_bytes(frontend_index.into_body(), usize::MAX)
        .await
        .expect("frontend index body should decode");
    let frontend_index_text =
        String::from_utf8(frontend_index_body.to_vec()).expect("frontend index is utf-8");
    assert!(
        frontend_index_text.contains("mdict-web frontend"),
        "{frontend_index_text}"
    );

    let frontend_route = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/client/route")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("frontend route request should succeed");
    assert_eq!(frontend_route.status(), StatusCode::OK);

    let frontend_asset = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/favicon.svg")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("frontend asset request should succeed");
    assert_eq!(frontend_asset.status(), StatusCode::OK);

    let unknown_api = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/unknown")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("unknown api request should succeed");
    assert_eq!(unknown_api.status(), StatusCode::NOT_FOUND);

    let suggest = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/dictionaries/ldoce5pp/suggest?q=app&limit=5")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("suggest request should succeed");
    assert_eq!(suggest.status(), StatusCode::OK);
    let suggest_body = to_bytes(suggest.into_body(), usize::MAX)
        .await
        .expect("suggest body should decode");
    let suggest_text = String::from_utf8(suggest_body.to_vec()).expect("suggest body is utf-8");
    assert!(
        suggest_text.contains("\"match_type\":\"prefix\""),
        "{suggest_text}"
    );
    assert!(suggest_text.contains("\"app\""), "{suggest_text}");

    let search_suggest = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/search/suggest?q=app&limit=6")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("global suggest request should succeed");
    assert_eq!(search_suggest.status(), StatusCode::OK);
    let search_suggest_body = to_bytes(search_suggest.into_body(), usize::MAX)
        .await
        .expect("global suggest body should decode");
    let search_suggest_text =
        String::from_utf8(search_suggest_body.to_vec()).expect("global suggest body is utf-8");
    assert!(
        search_suggest_text.contains("\"dictionary_id\":\"ldoce5pp\""),
        "{search_suggest_text}"
    );
    assert!(
        search_suggest_text.contains("\"dictionary_id\":\"ldoce5pp_alt\""),
        "{search_suggest_text}"
    );

    let lookup = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/dictionaries/ldoce5pp/entries/lookup?key=Apple")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("lookup request should succeed");
    assert_eq!(lookup.status(), StatusCode::OK);
    let lookup_body = to_bytes(lookup.into_body(), usize::MAX)
        .await
        .expect("lookup body should decode");
    let lookup_text = String::from_utf8(lookup_body.to_vec()).expect("lookup body is utf-8");
    assert!(
        lookup_text.contains("\"resolved_key\":\"apple\""),
        "{lookup_text}"
    );

    let redirect_lookup = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/dictionaries/ldoce5pp/entries/lookup?key=build-up")
                .body(Body::empty())
                .expect("redirect lookup request should build"),
        )
        .await
        .expect("redirect lookup request should succeed");
    assert_eq!(redirect_lookup.status(), StatusCode::OK);
    let redirect_lookup_body = to_bytes(redirect_lookup.into_body(), usize::MAX)
        .await
        .expect("redirect lookup body should decode");
    let redirect_lookup_text =
        String::from_utf8(redirect_lookup_body.to_vec()).expect("redirect lookup body is utf-8");
    assert!(
        redirect_lookup_text.contains("\"resolved_key\":\"build up\""),
        "{redirect_lookup_text}"
    );
    assert!(
        redirect_lookup_text.contains("\"redirected_from\":\"build-up\""),
        "{redirect_lookup_text}"
    );

    let search_lookup = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/search/lookup?key=Apple")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("global lookup request should succeed");
    assert_eq!(search_lookup.status(), StatusCode::OK);
    let search_lookup_body = to_bytes(search_lookup.into_body(), usize::MAX)
        .await
        .expect("global lookup body should decode");
    let search_lookup_text =
        String::from_utf8(search_lookup_body.to_vec()).expect("global lookup body is utf-8");
    assert!(
        search_lookup_text.contains("\"dictionary_id\":\"ldoce5pp\""),
        "{search_lookup_text}"
    );
    assert!(
        search_lookup_text.contains("\"dictionary_id\":\"ldoce5pp_alt\""),
        "{search_lookup_text}"
    );

    let content = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/dictionaries/ldoce5pp/entries/content?key=Apple")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("content request should succeed");
    assert_eq!(content.status(), StatusCode::OK);
    assert_eq!(
        content
            .headers()
            .get("content-security-policy")
            .and_then(|value| value.to_str().ok()),
        Some(
            "default-src 'none'; script-src 'unsafe-inline'; img-src 'self' data: blob:; media-src 'self' data:; style-src 'self' 'unsafe-inline'; font-src 'self' data:; frame-ancestors 'self'; base-uri 'none'; form-action 'none'; connect-src 'none'"
        )
    );
    let content_etag = content
        .headers()
        .get("etag")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
        .expect("content response should include etag");
    let content_body = to_bytes(content.into_body(), usize::MAX)
        .await
        .expect("content body should decode");
    let content_text = String::from_utf8(content_body.to_vec()).expect("content body is utf-8");
    assert!(
        content_text.contains("/api/v1/dictionaries/ldoce5pp/resources/content?key="),
        "{content_text}"
    );
    assert!(content_text.contains("data-audio-href="), "{content_text}");
    assert!(content_text.contains("<script>"), "{content_text}");
    assert!(
        content_text.contains("class=\"speaker brefile fa fa-volume-up\""),
        "{content_text}"
    );
    assert!(
        content_text.contains(
            "data-audio-href=\"/api/v1/dictionaries/ldoce5pp/resources/content?key=sound%3A"
        ),
        "{content_text}"
    );
    assert!(
        content_text.contains("const audio = new Audio()"),
        "{content_text}"
    );

    let redirect_content = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/dictionaries/ldoce5pp/entries/content?key=build-up")
                .body(Body::empty())
                .expect("redirect content request should build"),
        )
        .await
        .expect("redirect content request should succeed");
    assert_eq!(redirect_content.status(), StatusCode::OK);
    let redirect_content_body = to_bytes(redirect_content.into_body(), usize::MAX)
        .await
        .expect("redirect content body should decode");
    let redirect_content_text =
        String::from_utf8(redirect_content_body.to_vec()).expect("redirect content body is utf-8");
    assert!(
        !redirect_content_text.contains("@@@LINK="),
        "{redirect_content_text}"
    );
    assert!(
        redirect_content_text
            .contains("/api/v1/dictionaries/ldoce5pp/resources/content?key=LM5style%2Ecss"),
        "{redirect_content_text}"
    );

    let content_not_modified = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/dictionaries/ldoce5pp/entries/content?key=Apple")
                .header("if-none-match", &content_etag)
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("conditional content request should succeed");
    assert_eq!(content_not_modified.status(), StatusCode::NOT_MODIFIED);
    assert_eq!(
        content_not_modified
            .headers()
            .get("etag")
            .and_then(|value| value.to_str().ok()),
        Some(content_etag.as_str())
    );
    assert_eq!(
        content_not_modified
            .headers()
            .get("content-security-policy")
            .and_then(|value| value.to_str().ok()),
        Some(
            "default-src 'none'; script-src 'unsafe-inline'; img-src 'self' data: blob:; media-src 'self' data:; style-src 'self' 'unsafe-inline'; font-src 'self' data:; frame-ancestors 'self'; base-uri 'none'; form-action 'none'; connect-src 'none'"
        )
    );

    let resource = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/dictionaries/ldoce5pp/resources/content?key=%5CLM5style.css")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("resource request should succeed");
    assert_eq!(resource.status(), StatusCode::OK);
    let resource_body = to_bytes(resource.into_body(), usize::MAX)
        .await
        .expect("resource body should decode");
    let resource_text = String::from_utf8(resource_body.to_vec()).expect("resource is utf-8");
    assert!(resource_text.contains("url("), "{resource_text}");

    let audio_resource = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(
                    "/api/v1/dictionaries/ldoce5pp/resources/content?key=sound%3A%2F%2Fmedia%2Fenglish%2FameProns%2Flaadbuild-up.mp3",
                )
                .body(Body::empty())
                .expect("audio resource request should build"),
        )
        .await
        .expect("audio resource request should succeed");
    assert_eq!(audio_resource.status(), StatusCode::OK);
    assert!(
        audio_resource
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("audio/")),
    );
    let audio_resource_body = to_bytes(audio_resource.into_body(), usize::MAX)
        .await
        .expect("audio resource body should decode");
    assert!(!audio_resource_body.is_empty());

    let reload_unauthorized = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/admin/reload")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("unauthorized reload request should succeed");
    assert_eq!(reload_unauthorized.status(), StatusCode::UNAUTHORIZED);

    let reload_ok = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/admin/reload")
                .header("authorization", "Bearer integration-test-token")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("authorized reload request should succeed");
    assert_eq!(reload_ok.status(), StatusCode::OK);
}

fn local_fixture_paths() -> Option<(PathBuf, PathBuf)> {
    let candidates = [
        (
            PathBuf::from(
                "/home/initsnow/Documents/Dictionaries/英汉/LDOCE5++/LDOCE5++ V 2-15.mdx",
            ),
            PathBuf::from(
                "/home/initsnow/Documents/Dictionaries/英汉/LDOCE5++/LDOCE5++ V 2-15.mdd",
            ),
        ),
        (
            PathBuf::from("/home/initsnow/projects/mdict-rs/tmp-dict/LDOCE5++/LDOCE5++ V 2-15.mdx"),
            PathBuf::from("/home/initsnow/projects/mdict-rs/tmp-dict/LDOCE5++/LDOCE5++ V 2-15.mdd"),
        ),
    ];

    for (mdx, mdd) in candidates {
        if mdx.exists() && mdd.exists() {
            return Some((mdx, mdd));
        }
    }

    None
}

fn reusable_fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/local-fixtures/http-smoke")
}

fn write_config(dir: &Path, mdx_path: &Path, mdd_path: &Path) -> PathBuf {
    let config = format!(
        r#"
[server]
bind = "127.0.0.1:18080"

[index]
dir = "{}"

[observability]
metrics_enabled = false
metrics_path = "/metrics"

[admin]
reload_token = "integration-test-token"

[[catalog.bundles]]
dictionary_id = "ldoce5pp"
display_name = "LDOCE5++"
mdx_path = "{}"
mdd_path = "{}"

[[catalog.bundles]]
dictionary_id = "ldoce5pp_alt"
display_name = "LDOCE5++ Alt"
mdx_path = "{}"
mdd_path = "{}"
"#,
        dir.join("index").display(),
        mdx_path.display(),
        mdd_path.display(),
        mdx_path.display(),
        mdd_path.display()
    );
    let path = dir.join("mdict-web.toml");
    fs::write(&path, config).expect("config file should be written");
    path
}

fn write_frontend_dist(dir: &Path) -> PathBuf {
    let dist = dir.join("frontend-dist");
    fs::create_dir_all(&dist).expect("frontend dist dir should exist");
    fs::create_dir_all(dist.join("assets")).expect("frontend assets dir should exist");
    fs::write(
        dist.join("index.html"),
        "<!doctype html><html><body>mdict-web frontend</body></html>",
    )
    .expect("frontend index should be written");
    fs::write(dist.join("favicon.svg"), "<svg></svg>").expect("favicon should be written");
    fs::write(dist.join("assets/app.js"), "console.log('mdict-web');")
        .expect("frontend asset should be written");
    dist
}
