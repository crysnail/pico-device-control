pub mod routes;
pub mod ws;

use crate::state::AppState;
use axum::http::{header, HeaderValue, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use rust_embed::Embed;
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};

#[derive(Embed)]
#[folder = "ui/"]
struct EmbeddedUi;

pub fn build_router(state: Arc<AppState>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/ws/pico", get(ws::ws_handler))
        .merge(routes::devices::router())
        .merge(routes::batch::router())
        .merge(routes::config::router())
        .merge(routes::stream::router())
        .merge(routes::adb::router())
        .merge(routes::ota::router())
        .merge(routes::system::router())
        .fallback(static_handler)
        .layer(cors)
        .with_state(state)
}

fn percent_decode_utf8(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (
                (bytes[i + 1] as char).to_digit(16),
                (bytes[i + 2] as char).to_digit(16),
            ) {
                out.push(((h << 4) | l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| input.to_string())
}

async fn static_handler(uri: Uri) -> Response {
    let decoded = percent_decode_utf8(uri.path());
    let raw_path = decoded.trim_start_matches('/');
    let path = if raw_path.is_empty() {
        "index.html"
    } else {
        raw_path
    };

    if let Some(content) = EmbeddedUi::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        return (
            StatusCode::OK,
            [
                (
                    header::CONTENT_TYPE,
                    HeaderValue::from_str(mime.as_ref())
                        .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
                ),
                (
                    header::CACHE_CONTROL,
                    HeaderValue::from_static("no-store, must-revalidate"),
                ),
            ],
            content.data.into_owned(),
        )
            .into_response();
    }

    // SPA 回退到 index.html
    if let Some(index) = EmbeddedUi::get("index.html") {
        return (
            StatusCode::OK,
            [
                (
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("text/html; charset=utf-8"),
                ),
                (
                    header::CACHE_CONTROL,
                    HeaderValue::from_static("no-store, must-revalidate"),
                ),
            ],
            index.data.into_owned(),
        )
            .into_response();
    }

    (StatusCode::NOT_FOUND, "UI Asset Not Found").into_response()
}

pub fn spawn_embedded_server(state: Arc<AppState>) {
    let port = state.web_port;
    let app = build_router(state);

    tokio::spawn(async move {
        let addr = SocketAddr::from(([0, 0, 0, 0], port));
        match tokio::net::TcpListener::bind(addr).await {
            Ok(listener) => {
                tracing::info!("★ 内嵌 Rust HTTP/WebSocket 后台服务已启动: http://0.0.0.0:{}", port);
                if let Err(err) = axum::serve(listener, app).await {
                    tracing::error!("内嵌 HTTP 服务异常退出: {}", err);
                }
            }
            Err(err) => {
                tracing::error!("绑定端口 {} 失败 (可能已被其他实例占用): {}", port, err);
            }
        }
    });
}
