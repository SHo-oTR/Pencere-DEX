use crate::capture::capture_frame;
use crate::clipboard::{get_clipboard_file_path, get_clipboard_text, set_clipboard_text};
use crate::diff::compute_diff;
use crate::encoder::{encode_image, ImageFormat, QUALITY_PROFILES};
use crate::input::*;
use crate::win32::*;
use axum::{
    body::Body,
    extract::{
        multipart::Multipart,
        ws::{Message, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::Notify;
use windows_sys::Win32::Foundation::HWND;

const INDEX_HTML: &str = include_str!("../static/index.html");

#[derive(Clone)]
pub struct AppState {
    pub key: String,
    pub cur_hwnd: Arc<AtomicIsize>,
    pub pw_mode: Arc<AtomicBool>,
    pub base_windows: Arc<Mutex<Option<HashSet<isize>>>>,
    pub base_fg: Arc<AtomicIsize>,
}

#[derive(Deserialize)]
pub struct AuthQuery {
    pub k: Option<String>,
}

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(index_handler))
        .route("/ws", get(ws_handler))
        .route("/upload", post(upload_handler))
        .route("/dl", get(download_handler))
        .with_state(state)
}

async fn index_handler() -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "text/html; charset=utf-8".parse().unwrap());
    headers.insert(header::CACHE_CONTROL, "no-store, no-cache, must-revalidate".parse().unwrap());
    (headers, INDEX_HTML)
}

async fn upload_handler(
    State(state): State<AppState>,
    Query(query): Query<AuthQuery>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    if query.k.as_deref() != Some(&state.key) {
        return (StatusCode::FORBIDDEN, "Forbidden").into_response();
    }

    let download_dir = dirs_downloads().unwrap_or_else(|| PathBuf::from("."));
    let mut saved_name = String::new();
    let mut total_size: usize = 0;

    while let Ok(Some(mut field)) = multipart.next_field().await {
        let file_name = field
            .file_name()
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("upload_{}.bin", Instant::now().elapsed().as_secs()));

        let clean_name = Path::new(&file_name)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("upload.bin");

        let mut dest_path = download_dir.join(clean_name);
        let mut idx = 1;
        let stem = Path::new(clean_name).file_stem().and_then(|s| s.to_str()).unwrap_or("file");
        let ext = Path::new(clean_name).extension().and_then(|s| s.to_str()).unwrap_or("");

        while dest_path.exists() {
            let new_name = if ext.is_empty() {
                format!("{} ({})", stem, idx)
            } else {
                format!("{} ({}).{}", stem, idx, ext)
            };
            dest_path = download_dir.join(new_name);
            idx += 1;
        }

        if let Ok(mut file) = File::create(&dest_path) {
            while let Ok(Some(chunk)) = field.chunk().await {
                let _ = file.write_all(&chunk);
                total_size += chunk.len();
            }
            let path_str = dest_path.to_string_lossy().to_string();
            set_clipboard_text(&path_str);
            saved_name = dest_path.file_name().unwrap().to_string_lossy().to_string();
        }
    }

    let res = serde_json::json!({
        "ok": true,
        "name": saved_name,
        "size": total_size
    });
    (StatusCode::OK, [(header::CONTENT_TYPE, "application/json")], res.to_string()).into_response()
}

fn dirs_downloads() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(|p| PathBuf::from(p).join("Downloads"))
}

async fn download_handler(
    State(state): State<AppState>,
    Query(query): Query<AuthQuery>,
) -> impl IntoResponse {
    if query.k.as_deref() != Some(&state.key) {
        return (StatusCode::FORBIDDEN, "Forbidden").into_response();
    }

    if let Some(file_path) = get_clipboard_file_path() {
        let path = Path::new(&file_path);
        if path.exists() && path.is_file() {
            if let Ok(contents) = std::fs::read(path) {
                let file_name = path.file_name().unwrap().to_string_lossy();
                let cd = format!("attachment; filename=\"{}\"", file_name);
                return (
                    StatusCode::OK,
                    [
                        (header::CONTENT_TYPE, "application/octet-stream"),
                        (header::CONTENT_DISPOSITION, &cd),
                    ],
                    Body::from(contents),
                )
                    .into_response();
            }
        }
    }

    (StatusCode::NOT_FOUND, "Once PC'de bir dosya kopyalayin (Ctrl+C).").into_response()
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Query(query): Query<AuthQuery>,
) -> Response {
    if query.k.as_deref() != Some(&state.key) {
        return (StatusCode::FORBIDDEN, "Forbidden").into_response();
    }
    ws.on_upgrade(move |socket| handle_ws(socket, state))
}

#[derive(Serialize)]
struct WindowItem {
    k: String,
    n: String,
    w: Vec<(isize, String)>,
}

#[derive(Serialize)]
struct ListMsg {
    cur: isize,
    g: Vec<WindowItem>,
    mons: usize,
    mon: usize,
    aok: bool,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    ic: HashMap<String, String>,
}

async fn handle_ws(socket: WebSocket, state: AppState) {
    let (mut ws_sender, mut ws_receiver) = socket.split();

    // Send sample format tests
    let test_json = serde_json::json!({
        "test": {
            "jpeg": "data:image/jpeg;base64,/9j/4AAQSkZJRg==",
            "png": "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
            "webp": "data:image/webp;base64,UklGRh4AAABXRUJQVlA4TBEAAAAvAAAAAAfQ//73v/+BiOh/AAA="
        }
    });
    let _ = ws_sender.send(Message::Text(test_json.to_string())).await;

    let ws_sender = Arc::new(tokio::sync::Mutex::new(ws_sender));
    let notify_kick = Arc::new(Notify::new());

    // Receiver task
    let state_rec = state.clone();
    let notify_rec = notify_kick.clone();
    let ws_sender_rec = ws_sender.clone();

    let mut rec_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_receiver.next().await {
            match msg {
                Message::Binary(bin) => {
                    handle_binary_input(&bin, &state_rec);
                    notify_rec.notify_one();
                }
                Message::Text(text) => {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) {
                        handle_text_msg(val, &state_rec, &ws_sender_rec).await;
                        notify_rec.notify_one();
                    }
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
    });

    // Sender / Screen Pusher task
    let state_send = state.clone();
    let ws_sender_send = ws_sender.clone();
    let notify_send = notify_kick.clone();

    let mut send_task = tokio::spawn(async move {
        let mut prev_pixels: Vec<u32> = Vec::new();
        let mut prev_width = 0usize;
        let mut prev_height = 0usize;
        let mut last_hwnd: isize = -999;
        let mut last_list_time = Instant::now() - Duration::from_secs(10);
        let mut sent_icons: HashSet<String> = HashSet::new();

        let quality_idx = 1usize; // Default: Medium (720p)
        let zoom = 1.0f32;

        loop {
            let hwnd_isize = state_send.cur_hwnd.load(Ordering::Relaxed);

            // Check if minimized: if window became iconic, fall back to desktop
            if hwnd_isize != 0 && is_window_iconic(hwnd_isize as HWND) {
                state_send.cur_hwnd.store(0, Ordering::Relaxed);
            }

            // Periodically send window list
            if last_list_time.elapsed() > Duration::from_millis(700) {
                last_list_time = Instant::now();
                let windows = enumerate_windows();
                let mut groups: HashMap<String, Vec<(isize, String)>> = HashMap::new();
                let mut new_icons: HashMap<String, String> = HashMap::new();

                for w in &windows {
                    groups.entry(w.exe_path.clone()).or_default().push((w.hwnd, w.title.clone()));
                    if !w.exe_path.is_empty() && !sent_icons.contains(&w.exe_path) {
                        let icon_data = extract_icon_data_url(&w.exe_path);
                        if !icon_data.is_empty() {
                            new_icons.insert(w.exe_path.clone(), icon_data);
                            sent_icons.insert(w.exe_path.clone());
                        }
                    }
                }

                let mut g_list: Vec<WindowItem> = groups
                    .into_iter()
                    .map(|(k, mut v)| {
                        v.sort_by_key(|item| item.0);
                        WindowItem {
                            n: get_app_name(&k),
                            k,
                            w: v,
                        }
                    })
                    .collect();
                g_list.sort_by(|a, b| a.n.cmp(&b.n));

                let list_msg = ListMsg {
                    cur: hwnd_isize,
                    g: g_list,
                    mons: 1,
                    mon: 1,
                    aok: false,
                    ic: new_icons,
                };

                if let Ok(json_str) = serde_json::to_string(&list_msg) {
                    let mut lock = ws_sender_send.lock().await;
                    let _ = lock.send(Message::Text(json_str)).await;
                }
            }

            // Capture frame
            let pw_mode = state_send.pw_mode.load(Ordering::Relaxed);
            if let Some(frame) = capture_frame(hwnd, pw_mode) {
                let w = frame.width as usize;
                let h = frame.height as usize;

                // Cast BGRA bytes to u32 pixels
                let cur_pixels: &[u32] = unsafe {
                    std::slice::from_raw_parts(frame.pixels_bgra.as_ptr() as *const u32, w * h)
                };

                let window_switched = hwnd_isize != last_hwnd || w != prev_width || h != prev_height;
                let (rects, _diff_frac) = if window_switched {
                    (vec![crate::diff::Rect { x0: 0, y0: 0, x1: w, y1: h }], 1.0)
                } else {
                    compute_diff(&prev_pixels, cur_pixels, w, h)
                };

                if !rects.is_empty() {
                    let q_prof = &QUALITY_PROFILES[quality_idx.min(QUALITY_PROFILES.len() - 1)];
                    let target_w = (w as f32 * zoom).min(q_prof.max_width as f32) as u32;
                    let target_h = (h as f32 * target_w as f32 / w as f32).max(1.0) as u32;

                    // Convert BGRA to RGB for image encoding
                    let mut rgb_full = vec![0u8; w * h * 3];
                    for i in 0..(w * h) {
                        rgb_full[i * 3 + 0] = frame.pixels_bgra[i * 4 + 2]; // R
                        rgb_full[i * 3 + 1] = frame.pixels_bgra[i * 4 + 1]; // G
                        rgb_full[i * 3 + 2] = frame.pixels_bgra[i * 4 + 0]; // B
                    }

                    let img_buffer = image::RgbImage::from_raw(w as u32, h as u32, rgb_full).unwrap();
                    let scaled_img = if target_w != w as u32 {
                        image::imageops::resize(&img_buffer, target_w, target_h, image::imageops::FilterType::Triangle)
                    } else {
                        img_buffer
                    };

                    let mut parts: Vec<(u16, u16, u16, u16, u16, u16, u16, Vec<u8>)> = Vec::new();
                    let ow = target_w as u16;
                    let oh = target_h as u16;

                    for r in rects {
                        let a0 = ((r.x0 as f32 * target_w as f32) / w as f32) as u32;
                        let b0 = ((r.y0 as f32 * target_h as f32) / h as f32) as u32;
                        let a1 = (((r.x1 as f32 * target_w as f32) / w as f32).ceil() as u32).min(target_w);
                        let b1 = (((r.y1 as f32 * target_h as f32) / h as f32).ceil() as u32).min(target_h);

                        if a1 <= a0 || b1 <= b0 {
                            continue;
                        }

                        let crop_w = a1 - a0;
                        let crop_h = b1 - b0;
                        let cropped = image::imageops::crop_imm(&scaled_img, a0, b0, crop_w, crop_h).to_image();
                        let is_full = (a0 == 0 && b0 == 0 && a1 == target_w && b1 == target_h) as u16;

                        let fmt = ImageFormat::Jpeg; // High speed, widely supported
                        if let Ok(data) = encode_image(cropped.as_raw(), crop_w, crop_h, fmt, quality_idx) {
                            let flags = is_full | (fmt.code() << 1);
                            parts.push((ow, oh, a0 as u16, b0 as u16, crop_w as u16, crop_h as u16, flags, data));
                        }
                    }

                    if !parts.is_empty() {
                        let packed = pack_parts(hwnd_isize as i32, &parts);
                        let mut lock = ws_sender_send.lock().await;
                        for packet in packed {
                            let _ = lock.send(Message::Binary(packet)).await;
                        }
                    }

                    prev_pixels = cur_pixels.to_vec();
                    prev_width = w;
                    prev_height = h;
                    last_hwnd = hwnd_isize;
                }
            }

            // Sleep with reactive wakeup
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(40)) => {},
                _ = notify_send.notified() => {},
            }
        }
    });

    tokio::select! {
        _ = (&mut rec_task) => send_task.abort(),
        _ = (&mut send_task) => rec_task.abort(),
    }
}

/// Packs image parts into binary frames compatible with client JS
fn pack_parts(hwnd: i32, parts: &[(u16, u16, u16, u16, u16, u16, u16, Vec<u8>)]) -> Vec<Vec<u8>> {
    if parts.len() > 1 {
        let count = parts.len() as u16;
        let mut batch = Vec::new();
        batch.extend_from_slice(&(-1i32).to_le_bytes());
        batch.extend_from_slice(&count.to_le_bytes());

        for (ow, oh, x, y, w, h, flags, data) in parts {
            batch.extend_from_slice(&hwnd.to_le_bytes());
            batch.extend_from_slice(&ow.to_le_bytes());
            batch.extend_from_slice(&oh.to_le_bytes());
            batch.extend_from_slice(&x.to_le_bytes());
            batch.extend_from_slice(&y.to_le_bytes());
            batch.extend_from_slice(&w.to_le_bytes());
            batch.extend_from_slice(&h.to_le_bytes());
            batch.extend_from_slice(&flags.to_le_bytes());
            batch.extend_from_slice(&(data.len() as u32).to_le_bytes());
            batch.extend_from_slice(data);
        }
        vec![batch]
    } else {
        parts
            .iter()
            .map(|(ow, oh, x, y, w, h, flags, data)| {
                let mut single = Vec::with_capacity(18 + data.len());
                single.extend_from_slice(&hwnd.to_le_bytes());
                single.extend_from_slice(&ow.to_le_bytes());
                single.extend_from_slice(&oh.to_le_bytes());
                single.extend_from_slice(&x.to_le_bytes());
                single.extend_from_slice(&y.to_le_bytes());
                single.extend_from_slice(&w.to_le_bytes());
                single.extend_from_slice(&h.to_le_bytes());
                single.extend_from_slice(&flags.to_le_bytes());
                single.extend_from_slice(data);
                single
            })
            .collect()
    }
}

fn handle_binary_input(bin: &[u8], state: &AppState) {
    if bin.is_empty() {
        return;
    }
    let op = bin[0];
    if op == 6 {
        // Ack
        return;
    }
    if bin.len() < 9 {
        return;
    }

    let x = f32::from_le_bytes([bin[1], bin[2], bin[3], bin[4]]);
    let y = f32::from_le_bytes([bin[5], bin[6], bin[7], bin[8]]);
    let hwnd = state.cur_hwnd.load(Ordering::Relaxed) as HWND;

    match op {
        1 => {
            // Scroll
            if bin.len() >= 11 {
                let d = i16::from_le_bytes([bin[9], bin[10]]) as i32;
                move_cursor(hwnd, x, y);
                mouse_scroll(d, 0);
            }
        }
        7 => {
            // 2-way scroll
            if bin.len() >= 13 {
                let dy = i16::from_le_bytes([bin[9], bin[10]]) as i32;
                let dx = i16::from_le_bytes([bin[11], bin[12]]) as i32;
                move_cursor(hwnd, x, y);
                mouse_scroll(dy, dx);
            }
        }
        2 => {
            // Drag
            move_cursor(hwnd, x, y);
        }
        3 => {
            // Move
            move_cursor(hwnd, x, y);
        }
        4 => {
            // Down
            move_cursor(hwnd, x, y);
            mouse_down();
        }
        5 => {
            // Up
            move_cursor(hwnd, x, y);
            mouse_up();
        }
        _ => {}
    }
}

async fn handle_text_msg(
    m: serde_json::Value,
    state: &AppState,
    ws_sender: &Arc<tokio::sync::Mutex<futures_util::stream::SplitSink<WebSocket, Message>>>,
) {
    let t = match m.get("t").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return,
    };

    let hwnd = state.cur_hwnd.load(Ordering::Relaxed) as HWND;

    match t {
        "pong" | "ack" | "hello" | "pause" => {}
        "sel" => {
            if let Some(h) = m.get("h").and_then(|v| v.as_i64()) {
                let target = h as isize;
                state.cur_hwnd.store(target, Ordering::Relaxed);
                focus_window(target as HWND);
            }
        }
        "start" => {
            let wins: HashSet<isize> = enumerate_windows().into_iter().map(|w| w.hwnd).collect();
            *state.base_windows.lock().unwrap() = Some(wins);
            state.base_fg.store(get_foreground_window(), Ordering::Relaxed);
            state.cur_hwnd.store(0, Ordering::Relaxed);
            toggle_start_menu();
        }
        "desk" => {
            state.cur_hwnd.store(0, Ordering::Relaxed);
            show_desktop();
        }
        "close" => {
            if let Some(h) = m.get("h").and_then(|v| v.as_i64()) {
                close_window(h as HWND);
            }
        }
        "view" => {
            if let Some(pw) = m.get("pw").and_then(|v| v.as_bool()) {
                state.pw_mode.store(pw, Ordering::Relaxed);
            }
        }
        "click" => {
            if let (Some(x), Some(y)) = (m.get("x").and_then(|v| v.as_f64()), m.get("y").and_then(|v| v.as_f64())) {
                move_cursor(hwnd, x as f32, y as f32);
            }
            mouse_click("l", 1);
        }
        "rclick" => {
            if let (Some(x), Some(y)) = (m.get("x").and_then(|v| v.as_f64()), m.get("y").and_then(|v| v.as_f64())) {
                move_cursor(hwnd, x as f32, y as f32);
            }
            mouse_click("r", 1);
        }
        "mclick" => {
            if let (Some(x), Some(y)) = (m.get("x").and_then(|v| v.as_f64()), m.get("y").and_then(|v| v.as_f64())) {
                move_cursor(hwnd, x as f32, y as f32);
            }
            mouse_click("m", 1);
        }
        "scroll" => {
            let dy = m.get("d").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let dx = m.get("dx").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            if let (Some(x), Some(y)) = (m.get("x").and_then(|v| v.as_f64()), m.get("y").and_then(|v| v.as_f64())) {
                move_cursor(hwnd, x as f32, y as f32);
            }
            mouse_scroll(dy, dx);
        }
        "down" => {
            if let (Some(x), Some(y)) = (m.get("x").and_then(|v| v.as_f64()), m.get("y").and_then(|v| v.as_f64())) {
                move_cursor(hwnd, x as f32, y as f32);
            }
            mouse_down();
        }
        "drag" => {
            if let (Some(x), Some(y)) = (m.get("x").and_then(|v| v.as_f64()), m.get("y").and_then(|v| v.as_f64())) {
                move_cursor(hwnd, x as f32, y as f32);
            }
        }
        "up" => {
            mouse_up();
        }
        "text" => {
            if let Some(s) = m.get("s").and_then(|v| v.as_str()) {
                send_text(s);
            }
        }
        "bs" => {
            let count = m.get("n").and_then(|v| v.as_u64()).unwrap_or(1).min(50);
            for _ in 0..count {
                send_key(0x08, false, false, false, false);
            }
        }
        "key" => {
            if let Some(vk) = m.get("vk").and_then(|v| v.as_u64()) {
                let ctrl = m.get("ctrl").and_then(|v| v.as_bool()).unwrap_or(false);
                let alt = m.get("alt").and_then(|v| v.as_bool()).unwrap_or(false);
                let shift = m.get("shift").and_then(|v| v.as_bool()).unwrap_or(false);
                let win = m.get("win").and_then(|v| v.as_bool()).unwrap_or(false);
                send_key(vk as u8, ctrl, alt, shift, win);
            }
        }
        "clip" => {
            if let Some(s) = m.get("s").and_then(|v| v.as_str()) {
                set_clipboard_text(s);
            }
            if m.get("paste").and_then(|v| v.as_bool()).unwrap_or(false) {
                send_key(0x56, true, false, false, false);
            }
        }
        "copy" => {
            send_key(0x43, true, false, false, false);
            if let Some(text) = get_clipboard_text() {
                let clip_json = serde_json::json!({
                    "t": "clip",
                    "s": text
                });
                let mut lock = ws_sender.lock().await;
                let _ = lock.send(Message::Text(clip_json.to_string())).await;
            }
        }
        _ => {}
    }
}
