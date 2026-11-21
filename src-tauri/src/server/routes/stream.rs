use crate::services::screen_monitor::ScreenMonitorService;
use crate::state::AppState;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use bytes::Bytes;
use futures_util::stream;
use serde_json::json;
use std::sync::Arc;
use tokio::time::{timeout, Duration};

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/stream/suite-status", get(get_suite_status))
        .route("/api/stream/reconnect", post(reconnect_suite))
        .route("/api/stream/{sn}/start", post(start_stream))
        .route("/api/stream/{sn}/stop", post(stop_stream))
        .route("/api/stream/{sn}/frame", get(get_frame))
        .route("/api/stream/{sn}/status", get(get_stream_status))
        .route("/api/stream/{sn}/mjpeg", get(get_mjpeg_stream))
}

async fn get_suite_status(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(state.sdk_service.check_suite_health().await)
}

async fn reconnect_suite(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let ok = state.sdk_service.reconnect_sdk().await;
    let health = state.sdk_service.check_suite_health().await;
    Json(json!({ "success": ok, "health": health }))
}

async fn start_stream(
    State(state): State<Arc<AppState>>,
    Path(sn): Path<String>,
) -> impl IntoResponse {
    let ret = state.device_manager.start_monitor(&sn);
    let status = state.screen_monitor.get_stream_status(&sn);
    Json(json!({ "success": ret == 0, "ret": ret, "status": status }))
}

async fn stop_stream(
    State(state): State<Arc<AppState>>,
    Path(sn): Path<String>,
) -> impl IntoResponse {
    let ret = state.device_manager.stop_monitor(&sn);
    let status = state.screen_monitor.get_stream_status(&sn);
    Json(json!({ "success": true, "ret": ret, "status": status }))
}

async fn get_frame(
    State(state): State<Arc<AppState>>,
    Path(sn): Path<String>,
) -> impl IntoResponse {
    let frame = match state.screen_monitor.get_latest_frame(&sn) {
        Some(f) if !f.is_empty() => f,
        _ => {
            let health = state.sdk_service.check_suite_health().await;
            ScreenMonitorService::generate_placeholder_jpeg(health.is_port_open && health.is_sdk_active)
        }
    };

    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/jpeg"));
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-cache, no-store, must-revalidate"),
    );
    (StatusCode::OK, headers, frame)
}

async fn get_stream_status(
    State(state): State<Arc<AppState>>,
    Path(sn): Path<String>,
) -> impl IntoResponse {
    let frame = state.screen_monitor.get_latest_frame(&sn);
    let has_real = frame.as_ref().map(|f| !f.is_empty()).unwrap_or(false);
    let length = frame.as_ref().map(|f| f.len()).unwrap_or(0);
    let stream_status = state.screen_monitor.get_stream_status(&sn);
    let suite_health = state.sdk_service.check_suite_health().await;

    Json(json!({
        "hasRealFrame": has_real,
        "length": length,
        "streamStatus": stream_status,
        "suiteHealth": suite_health
    }))
}

struct MjpegGuard {
    _sn: String,
}

fn format_mjpeg_part(jpeg: &Bytes) -> Bytes {
    let header = format!(
        "--frame\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\n\r\n",
        jpeg.len()
    );
    let mut buf = Vec::with_capacity(header.len() + jpeg.len() + 2);
    buf.extend_from_slice(header.as_bytes());
    buf.extend_from_slice(jpeg);
    buf.extend_from_slice(b"\r\n");
    Bytes::from(buf)
}

async fn get_mjpeg_stream(
    State(state): State<Arc<AppState>>,
    Path(sn): Path<String>,
) -> impl IntoResponse {
    if !state.screen_monitor.get_stream_status(&sn).is_requested {
        state.device_manager.start_monitor(&sn);
    }
    let rx = state.screen_monitor.subscribe(&sn);
    let health = state.sdk_service.check_suite_health().await;
    let is_ready = health.is_port_open && health.is_sdk_active;

    let initial_jpeg = state
        .screen_monitor
        .get_latest_frame(&sn)
        .unwrap_or_else(|| ScreenMonitorService::generate_placeholder_jpeg(is_ready));

    let guard = MjpegGuard {
        _sn: sn.clone(),
    };

    let body_stream = stream::unfold(
        (Some(initial_jpeg), rx, guard, state, sn, is_ready),
        |(mut init, mut rx, guard, state, sn, ready)| async move {
            if let Some(first) = init.take() {
                return Some((Ok::<Bytes, std::io::Error>(format_mjpeg_part(&first)), (None, rx, guard, state, sn, ready)));
            }

            let next_frame = match timeout(Duration::from_millis(1200), rx.recv()).await {
                Ok(Ok(frame)) => frame,
                Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => state
                    .screen_monitor
                    .get_latest_frame(&sn)
                    .unwrap_or_else(|| ScreenMonitorService::generate_placeholder_jpeg(ready)),
                _ => state
                    .screen_monitor
                    .get_latest_frame(&sn)
                    .unwrap_or_else(|| ScreenMonitorService::generate_placeholder_jpeg(ready)),
            };

            Some((
                Ok::<Bytes, std::io::Error>(format_mjpeg_part(&next_frame)),
                (None, rx, guard, state, sn, ready),
            ))
        },
    );

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("multipart/x-mixed-replace; boundary=frame"),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-cache, private, no-store, must-revalidate"),
    );
    (StatusCode::OK, headers, Body::from_stream(body_stream))
}
