use crate::models::{DeviceStreamStatus, StreamPhase};
use bytes::Bytes;
use chrono::Utc;
use dashmap::DashMap;
use image::codecs::jpeg::JpegEncoder;
use image::{ColorType, Rgb, RgbImage};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{broadcast, mpsc};

struct RawFrameJob {
    dev_id: String,
    data: Vec<u8>,
    width: u32,
    height: u32,
    bytes_per_line: u32,
    pix_format: i32,
}

pub struct ScreenMonitorService {
    latest_frames: Arc<DashMap<String, Bytes>>,
    statuses: Arc<DashMap<String, DeviceStreamStatus>>,
    last_enqueue_ts: DashMap<String, Instant>,
    broadcasters: Arc<DashMap<String, broadcast::Sender<Bytes>>>,
    frame_tx: mpsc::Sender<RawFrameJob>,
}

impl ScreenMonitorService {
    pub fn new() -> Self {
        let latest_frames: Arc<DashMap<String, Bytes>> = Arc::new(DashMap::new());
        let statuses: Arc<DashMap<String, DeviceStreamStatus>> = Arc::new(DashMap::new());
        let broadcasters: Arc<DashMap<String, broadcast::Sender<Bytes>>> = Arc::new(DashMap::new());

        // 有界队列解耦 C++ SDK 回调线程与 JPEG 编码线程，避免阻塞原生回调
        let (frame_tx, mut frame_rx) = mpsc::channel::<RawFrameJob>(8);

        let lf = latest_frames.clone();
        let st = statuses.clone();
        let bc = broadcasters.clone();

        std::thread::Builder::new()
            .name("pico-screen-encoder".into())
            .spawn(move || {
                while let Some(job) = frame_rx.blocking_recv() {
                    let jpeg_opt = encode_frame_to_jpeg(&job);

                    if let Some(jpeg) = jpeg_opt {
                        let bytes = Bytes::from(jpeg);
                        lf.insert(job.dev_id.clone(), bytes.clone());

                        let now = Utc::now();
                        st.entry(job.dev_id.clone())
                            .and_modify(|s| {
                                if s.first_frame_time.is_none() {
                                    s.first_frame_time = Some(now);
                                }
                                if let Some(prev) = s.last_frame_time {
                                    let dt = (now - prev).num_milliseconds().max(1) as f64 / 1000.0;
                                    s.fps = (1.0 / dt).clamp(0.1, 60.0);
                                }
                                s.last_frame_time = Some(now);
                                s.total_frames += 1;
                                s.last_width = job.width;
                                s.last_height = job.height;
                                s.pix_format = job.pix_format;
                                s.phase = StreamPhase::Streaming;
                                s.diagnostic_message =
                                    format!("实时画面推流中 ({}x{})", job.width, job.height);
                            })
                            .or_insert_with(|| {
                                let mut s = DeviceStreamStatus::new(job.dev_id.clone());
                                s.first_frame_time = Some(now);
                                s.last_frame_time = Some(now);
                                s.total_frames = 1;
                                s.last_width = job.width;
                                s.last_height = job.height;
                                s.pix_format = job.pix_format;
                                s.phase = StreamPhase::Streaming;
                                s.diagnostic_message =
                                    format!("实时画面推流中 ({}x{})", job.width, job.height);
                                s
                            });

                        if let Some(sender) = bc.get(&job.dev_id) {
                            let _ = sender.send(bytes);
                        }
                    }
                }
            })
            .expect("failed to spawn screen encoder thread");

        Self {
            latest_frames,
            statuses,
            last_enqueue_ts: DashMap::new(),
            broadcasters,
            frame_tx,
        }
    }

    pub fn get_stream_status(&self, sn: &str) -> DeviceStreamStatus {
        self.statuses
            .entry(sn.to_string())
            .or_insert_with(|| DeviceStreamStatus::new(sn.to_string()))
            .clone()
    }

    pub fn get_all_stream_statuses(&self) -> HashMap<String, DeviceStreamStatus> {
        self.statuses
            .iter()
            .map(|e| (e.key().clone(), e.value().clone()))
            .collect()
    }

    pub fn record_start_requested(&self, sn: &str, ret: i32, msg: &str, phase: StreamPhase) {
        let now = Utc::now();
        self.statuses
            .entry(sn.to_string())
            .and_modify(|s| {
                s.is_requested = true;
                s.last_start_result = ret;
                s.diagnostic_message = msg.to_string();
                s.phase = phase;
                s.requested_time = Some(now);
            })
            .or_insert_with(|| {
                let mut s = DeviceStreamStatus::new(sn.to_string());
                s.is_requested = true;
                s.last_start_result = ret;
                s.diagnostic_message = msg.to_string();
                s.phase = phase;
                s.requested_time = Some(now);
                s
            });
    }

    pub fn record_stop_requested(&self, sn: &str) {
        self.statuses
            .entry(sn.to_string())
            .and_modify(|s| {
                s.is_requested = false;
                s.phase = StreamPhase::Standby;
                s.diagnostic_message = "省流待命中 (已手动停止拉流)".to_string();
            })
            .or_insert_with(|| {
                let mut s = DeviceStreamStatus::new(sn.to_string());
                s.diagnostic_message = "省流待命中 (已手动停止拉流)".to_string();
                s
            });
    }

    /// 在 SDK C 回调中安全快速拷贝原始帧，限频 15fps 后投递编码线程
    pub unsafe fn enqueue_raw_frame(
        &self,
        dev_id: String,
        addr: *const u8,
        data_size: u32,
        width: u32,
        height: u32,
        bytes_per_line: u32,
        pix_format: i32,
    ) {
        if dev_id.is_empty() || addr.is_null() || width == 0 || height == 0 || width > 4096 || height > 4096 {
            return;
        }

        // 前置限频 (~15fps = 66ms)，保护 CPU 与内存带宽
        let now = Instant::now();
        let is_first = !self.last_enqueue_ts.contains_key(&dev_id);
        if let Some(mut last) = self.last_enqueue_ts.get_mut(&dev_id) {
            if now.duration_since(*last).as_millis() < 66 {
                return;
            }
            *last = now;
        } else {
            self.last_enqueue_ts.insert(dev_id.clone(), now);
        }

        let copy_len = if data_size > 0 {
            data_size as usize
        } else if matches!(pix_format, 4 | 5 | 16 | 17) {
            let stride = if bytes_per_line >= width * 4 {
                bytes_per_line
            } else {
                width * 4
            };
            (stride as usize).saturating_mul(height as usize)
        } else if pix_format == 13 {
            let stride = if bytes_per_line >= width * 3 {
                bytes_per_line
            } else {
                width * 3
            };
            (stride as usize).saturating_mul(height as usize)
        } else {
            let stride = bytes_per_line.max(width) as usize;
            stride.saturating_mul(height as usize).saturating_mul(3) / 2
        };

        if copy_len == 0 || copy_len > 32 * 1024 * 1024 {
            return;
        }

        if is_first {
            tracing::info!(
                "[ScreenMonitor] 收到首帧回调 SN={} {}x{} stride={} size={} copy_len={} pixFormat={}",
                dev_id,
                width,
                height,
                bytes_per_line,
                data_size,
                copy_len,
                pix_format
            );
        }

        let slice = std::slice::from_raw_parts(addr, copy_len);
        let data = slice.to_vec();

        let _ = self.frame_tx.try_send(RawFrameJob {
            dev_id,
            data,
            width,
            height,
            bytes_per_line,
            pix_format,
        });
    }

    pub fn get_latest_frame(&self, sn: &str) -> Option<Bytes> {
        self.latest_frames.get(sn).map(|v| v.clone())
    }

    pub fn subscribe(&self, sn: &str) -> broadcast::Receiver<Bytes> {
        let sender = self
            .broadcasters
            .entry(sn.to_string())
            .or_insert_with(|| broadcast::channel(4).0);
        sender.subscribe()
    }

    pub fn subscriber_count(&self, sn: &str) -> usize {
        self.broadcasters
            .get(sn)
            .map(|s| s.receiver_count())
            .unwrap_or(0)
    }

    /// 生成轻量级占位 JPEG 帧（无真机推流时保活使用）
    pub fn generate_placeholder_jpeg(is_suite_ready: bool) -> Bytes {
        let width = 640u32;
        let height = 360u32;
        let mut img = RgbImage::new(width, height);

        let (r0, g0, b0) = if is_suite_ready {
            (15u8, 23u8, 42u8)
        } else {
            (30u8, 20u8, 28u8)
        };

        for (x, y, pixel) in img.enumerate_pixels_mut() {
            // 简洁深色科技网格底纹
            let grid = (x % 40 == 0 || y % 40 == 0) as u8 * 10;
            let accent = if y < 6 {
                if is_suite_ready {
                    (16, 185, 129)
                } else {
                    (244, 63, 94)
                }
            } else {
                (
                    r0.saturating_add(grid),
                    g0.saturating_add(grid),
                    b0.saturating_add(grid),
                )
            };
            *pixel = Rgb([accent.0, accent.1, accent.2]);
        }

        let mut buf = Vec::with_capacity(16 * 1024);
        let mut enc = JpegEncoder::new_with_quality(&mut buf, 75);
        let _ = enc.encode(img.as_raw(), width, height, ColorType::Rgb8.into());
        Bytes::from(buf)
    }
}

fn encode_frame_to_jpeg(job: &RawFrameJob) -> Option<Vec<u8>> {
    let w = job.width as usize;
    let h = job.height as usize;
    if w == 0 || h == 0 {
        return None;
    }
    match job.pix_format {
        4 | 5 => bgra_to_jpeg(&job.data, job.width, job.height, job.bytes_per_line, 75),
        16 | 17 => rgba_to_jpeg(&job.data, job.width, job.height, job.bytes_per_line, 75),
        13 => rgb24_to_jpeg(&job.data, job.width, job.height, job.bytes_per_line, 75),
        18 => yuv420p_to_jpeg(&job.data, job.width, job.height, job.bytes_per_line, 75),
        _ => {
            if job.data.len() >= w * h * 4 {
                bgra_to_jpeg(&job.data, job.width, job.height, job.bytes_per_line, 75)
            } else if job.data.len() >= w * h * 3 {
                rgb24_to_jpeg(&job.data, job.width, job.height, job.bytes_per_line, 75)
            } else {
                yuv420p_to_jpeg(&job.data, job.width, job.height, job.bytes_per_line, 75)
            }
        }
    }
}

fn rgba_to_jpeg(data: &[u8], width: u32, height: u32, bytes_per_line: u32, quality: u8) -> Option<Vec<u8>> {
    let w = width as usize;
    let h = height as usize;
    let stride = if (bytes_per_line as usize) >= w * 4 {
        bytes_per_line as usize
    } else {
        w * 4
    };
    if data.len() < stride * h {
        return None;
    }
    let mut rgb = vec![0u8; w * h * 3];
    for y in 0..h {
        let src_row = &data[y * stride..y * stride + w * 4];
        let dst_row = &mut rgb[y * w * 3..(y + 1) * w * 3];
        for x in 0..w {
            let si = x * 4;
            let di = x * 3;
            dst_row[di] = src_row[si];
            dst_row[di + 1] = src_row[si + 1];
            dst_row[di + 2] = src_row[si + 2];
        }
    }
    let mut out = Vec::with_capacity(w * h / 4);
    let mut enc = JpegEncoder::new_with_quality(&mut out, quality);
    enc.encode(&rgb, width, height, ColorType::Rgb8.into()).ok()?;
    Some(out)
}

fn rgb24_to_jpeg(data: &[u8], width: u32, height: u32, bytes_per_line: u32, quality: u8) -> Option<Vec<u8>> {
    let w = width as usize;
    let h = height as usize;
    let stride = if (bytes_per_line as usize) >= w * 3 {
        bytes_per_line as usize
    } else {
        w * 3
    };
    if data.len() < stride * h {
        return None;
    }
    let mut rgb = vec![0u8; w * h * 3];
    for y in 0..h {
        let src_row = &data[y * stride..y * stride + w * 3];
        let dst_row = &mut rgb[y * w * 3..(y + 1) * w * 3];
        dst_row.copy_from_slice(src_row);
    }
    let mut out = Vec::with_capacity(w * h / 4);
    let mut enc = JpegEncoder::new_with_quality(&mut out, quality);
    enc.encode(&rgb, width, height, ColorType::Rgb8.into()).ok()?;
    Some(out)
}

fn bgra_to_jpeg(data: &[u8], width: u32, height: u32, bytes_per_line: u32, quality: u8) -> Option<Vec<u8>> {
    let w = width as usize;
    let h = height as usize;
    let stride = if (bytes_per_line as usize) >= w * 4 {
        bytes_per_line as usize
    } else {
        w * 4
    };
    if data.len() < stride * h {
        return None;
    }

    let mut rgb = vec![0u8; w * h * 3];
    for y in 0..h {
        let src_row = &data[y * stride..y * stride + w * 4];
        let dst_row = &mut rgb[y * w * 3..(y + 1) * w * 3];
        for x in 0..w {
            let si = x * 4;
            let di = x * 3;
            dst_row[di] = src_row[si + 2];     // R
            dst_row[di + 1] = src_row[si + 1]; // G
            dst_row[di + 2] = src_row[si];     // B
        }
    }

    let mut out = Vec::with_capacity(w * h / 4);
    let mut enc = JpegEncoder::new_with_quality(&mut out, quality);
    enc.encode(&rgb, width, height, ColorType::Rgb8.into()).ok()?;
    Some(out)
}

fn yuv420p_to_jpeg(data: &[u8], width: u32, height: u32, bytes_per_line: u32, quality: u8) -> Option<Vec<u8>> {
    let w = width as usize;
    let h = height as usize;
    let y_stride = if (bytes_per_line as usize) >= w && data.len() >= (bytes_per_line as usize) * h * 3 / 2 {
        bytes_per_line as usize
    } else {
        w
    };
    let uv_stride = y_stride / 2;
    let y_plane_size = y_stride * h;
    let uv_plane_size = uv_stride * (h / 2);
    if data.len() < y_plane_size + uv_plane_size * 2 {
        return None;
    }

    let y_plane = &data[0..y_plane_size];
    let u_plane = &data[y_plane_size..y_plane_size + uv_plane_size];
    let v_plane = &data[y_plane_size + uv_plane_size..y_plane_size + uv_plane_size * 2];

    let mut rgb = vec![0u8; w * h * 3];

    for j in 0..h {
        let y_row = j * y_stride;
        let uv_row = (j / 2) * uv_stride;
        for i in 0..w {
            let y_val = y_plane[y_row + i] as i32;
            let uv_idx = uv_row + (i / 2);
            let u_val = u_plane[uv_idx] as i32 - 128;
            let v_val = v_plane[uv_idx] as i32 - 128;

            let c = (y_val - 16).max(0);
            let r = (298 * c + 409 * v_val + 128) >> 8;
            let g = (298 * c - 100 * u_val - 208 * v_val + 128) >> 8;
            let b = (298 * c + 516 * u_val + 128) >> 8;

            let di = (j * w + i) * 3;
            rgb[di] = r.clamp(0, 255) as u8;
            rgb[di + 1] = g.clamp(0, 255) as u8;
            rgb[di + 2] = b.clamp(0, 255) as u8;
        }
    }

    let mut out = Vec::with_capacity((w * h) / 4);
    let mut enc = JpegEncoder::new_with_quality(&mut out, quality);
    enc.encode(&rgb, width, height, ColorType::Rgb8.into()).ok()?;
    Some(out)
}
