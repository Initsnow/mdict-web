use std::fs;
use std::path::{Path, PathBuf};

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use mdict_web_http::{HttpState, router};
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
    let service = ReloadableDictionaryService::load_from_path(&config_path)
        .await
        .expect("service should load");
    let snapshot = service.snapshot().await;
    let body_limit = snapshot.config().server.request_body_limit_bytes;
    let metrics_path = snapshot.config().observability.metrics_path.clone();
    drop(snapshot);

    let app = router(HttpState::new(service, None), body_limit, &metrics_path);

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
    assert!(!content_text.to_ascii_lowercase().contains("<script"));

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
    let mdx =
        PathBuf::from("/home/initsnow/projects/mdict-rs/tmp-dict/LDOCE5++/LDOCE5++ V 2-15.mdx");
    let mdd =
        PathBuf::from("/home/initsnow/projects/mdict-rs/tmp-dict/LDOCE5++/LDOCE5++ V 2-15.mdd");
    if mdx.exists() && mdd.exists() {
        Some((mdx, mdd))
    } else {
        None
    }
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
"#,
        dir.join("index").display(),
        mdx_path.display(),
        mdd_path.display()
    );
    let path = dir.join("mdict-web.toml");
    fs::write(&path, config).expect("config file should be written");
    path
}
