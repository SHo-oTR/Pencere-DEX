use std::collections::HashMap;
use std::ffi::c_void;
use std::path::Path;
use std::sync::Mutex;
use image::ImageEncoder;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Dwm::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::Threading::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

// Explicit links avoid relying on generated windows-sys re-exports.
const KEYEVENTF_KEYUP: u32 = 0x0002;

#[link(name = "user32")]
extern "system" {
    #[link_name = "keybd_event"]
    fn keybd_event_ffi(b_vk: u8, b_scan: u8, dw_flags: u32, dw_extra_info: usize);
}

#[link(name = "shell32")]
extern "system" {
    #[link_name = "ExtractIconExW"]
    fn extract_icon_ex_w(
        file_name: *const u16,
        icon_index: i32,
        large_icon: *mut HICON,
        small_icon: *mut HICON,
        icon_count: u32,
    ) -> u32;
}

static ICON_CACHE: std::sync::OnceLock<Mutex<HashMap<String, String>>> =
    std::sync::OnceLock::new();

fn get_icon_cache() -> &'static Mutex<HashMap<String, String>> {
    ICON_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Sets process as DPI aware so coordinate calculations match screen pixels 1:1
pub fn set_dpi_aware() {
    unsafe {
        SetProcessDPIAware();
    }
}

/// Returns (left, top, right, bottom) of window or full screen
pub fn get_window_rect(hwnd: HWND) -> (i32, i32, i32, i32) {
    if hwnd.is_null() {
        unsafe {
            let w = GetSystemMetrics(SM_CXSCREEN);
            let h = GetSystemMetrics(SM_CYSCREEN);
            return (0, 0, w, h);
        }
    }

    unsafe {
        let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        let hr = DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS as u32,
            &mut r as *mut _ as *mut c_void,
            std::mem::size_of::<RECT>() as u32,
        );
        if hr == 0 {
            return (r.left, r.top, r.right, r.bottom);
        }

        let mut gr = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        GetWindowRect(hwnd, &mut gr);
        (gr.left, gr.top, gr.right, gr.bottom)
    }
}

/// Queries full executable path of the process owning the window
pub fn get_exe_of_window(hwnd: HWND) -> String {
    unsafe {
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == 0 {
            return String::new();
        }

        let ph = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if ph.is_null() {
            return String::new();
        }

        let mut buf = [0u16; 520];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(ph, 0, buf.as_mut_ptr(), &mut len);
        CloseHandle(ph);

        if ok != 0 && len > 0 {
            String::from_utf16_lossy(&buf[..len as usize])
        } else {
            String::new()
        }
    }
}

pub struct WindowInfo {
    pub hwnd: isize,
    pub title: String,
    pub exe_path: String,
}

struct EnumState {
    windows: Vec<WindowInfo>,
}

unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let state = &mut *(lparam as *mut EnumState);

    if IsWindowVisible(hwnd) == 0 {
        return 1;
    }

    let mut buf = [0u16; 512];
    let len = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
    if len <= 0 {
        return 1;
    }
    let title = String::from_utf16_lossy(&buf[..len as usize]);
    if title == "Program Manager" {
        return 1;
    }

    let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
    if (ex_style as u32 & WS_EX_TOOLWINDOW) != 0 {
        return 1;
    }

    let mut cloaked: i32 = 0;
    DwmGetWindowAttribute(
        hwnd,
        DWMWA_CLOAKED as u32,
        &mut cloaked as *mut _ as *mut c_void,
        std::mem::size_of::<i32>() as u32,
    );
    if cloaked != 0 {
        return 1;
    }

    let exe = get_exe_of_window(hwnd);
    state.windows.push(WindowInfo {
        hwnd: hwnd as isize,
        title: title.chars().take(60).collect(),
        exe_path: exe,
    });

    1
}

/// Enumerates visible desktop windows
pub fn enumerate_windows() -> Vec<WindowInfo> {
    let mut state = EnumState { windows: Vec::new() };
    unsafe {
        EnumWindows(Some(enum_windows_proc), &mut state as *mut _ as LPARAM);
    }
    state.windows
}

/// Extracts application icon and encodes it as a base64 Data URL (data:image/png;base64,...)
pub fn extract_icon_data_url(exe_path: &str) -> String {
    if exe_path.is_empty() {
        return String::new();
    }

    {
        let cache = get_icon_cache().lock().unwrap();
        if let Some(cached) = cache.get(exe_path) {
            return cached.clone();
        }
    }

    let result = unsafe {
        let mut path_utf16: Vec<u16> = exe_path.encode_utf16().chain(std::iter::once(0)).collect();
        let mut hicon_large: HICON = std::ptr::null_mut();
        let mut hicon_small: HICON = std::ptr::null_mut();

        let count = extract_icon_ex_w(
            path_utf16.as_ptr(),
            0,
            &mut hicon_large,
            &mut hicon_small,
            1,
        );
        let hicon = if !hicon_large.is_null() {
            hicon_large
        } else if !hicon_small.is_null() {
            hicon_small
        } else {
            std::ptr::null_mut()
        };

        if count > 0 && !hicon.is_null() {
            let size = 32i32;
            let dc0 = GetDC(std::ptr::null_mut());
            let mdc = CreateCompatibleDC(dc0);
            let bmp = CreateCompatibleBitmap(dc0, size, size);
            let old_bmp = SelectObject(mdc, bmp);

            DrawIconEx(mdc, 0, 0, hicon, size, size, 0, std::ptr::null_mut(), 3);

            let mut bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: size,
                    biHeight: -size, // top-down
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB,
                    biSizeImage: 0,
                    biXPelsPerMeter: 0,
                    biYPelsPerMeter: 0,
                    biClrUsed: 0,
                    biClrImportant: 0,
                },
                bmiColors: [RGBQUAD { rgbBlue: 0, rgbGreen: 0, rgbRed: 0, rgbReserved: 0 }],
            };

            let mut raw_pixels = vec![0u8; (size * size * 4) as usize];
            GetDIBits(mdc, bmp, 0, size as u32, raw_pixels.as_mut_ptr() as *mut _, &mut bmi, DIB_RGB_COLORS);

            SelectObject(mdc, old_bmp);
            DeleteObject(bmp);
            DeleteDC(mdc);
            ReleaseDC(std::ptr::null_mut(), dc0);

            if !hicon_large.is_null() {
                DestroyIcon(hicon_large);
            }
            if !hicon_small.is_null() {
                DestroyIcon(hicon_small);
            }

            // Convert BGRA to RGBA for PNG encoding
            let mut rgba = vec![0u8; (size * size * 4) as usize];
            for i in 0..(size * size) as usize {
                rgba[i * 4 + 0] = raw_pixels[i * 4 + 2]; // R
                rgba[i * 4 + 1] = raw_pixels[i * 4 + 1]; // G
                rgba[i * 4 + 2] = raw_pixels[i * 4 + 0]; // B
                rgba[i * 4 + 3] = raw_pixels[i * 4 + 3]; // A
            }

            let mut png_bytes = Vec::new();
            let png_encoder = image::codecs::png::PngEncoder::new(&mut png_bytes);
            if png_encoder.write_image(
                &rgba,
                size as u32,
                size as u32,
                image::ExtendedColorType::Rgba8,
            ).is_ok() {
                format!("data:image/png;base64,{}", base64::Engine::encode(&base64::engine::general_purpose::STANDARD, png_bytes))
            } else {
                String::new()
            }
        } else {
            String::new()
        }
    };

    let mut cache = get_icon_cache().lock().unwrap();
    cache.insert(exe_path.to_string(), result.clone());
    result
}

/// Brings window to front and restores/maximizes
pub fn focus_window(hwnd: HWND) {
    if hwnd.is_null() {
        return;
    }
    unsafe {
        if IsIconic(hwnd) != 0 {
            ShowWindow(hwnd, SW_RESTORE);
        }
        ShowWindow(hwnd, SW_SHOWMAXIMIZED);

        // Alt key trick to bypass Windows SetForegroundWindow restrictions
        keybd_event_ffi(0x12, 0, 0, 0);
        keybd_event_ffi(0x12, 0, KEYEVENTF_KEYUP, 0);

        SetForegroundWindow(hwnd);
    }
}

pub fn is_window_valid(hwnd: HWND) -> bool {
    if hwnd.is_null() {
        true
    } else {
        unsafe { IsWindow(hwnd) != 0 }
    }
}

pub fn is_window_iconic(hwnd: HWND) -> bool {
    if hwnd.is_null() {
        false
    } else {
        unsafe { IsIconic(hwnd) != 0 }
    }
}

pub fn close_window(hwnd: HWND) {
    if !hwnd.is_null() {
        unsafe {
            PostMessageW(hwnd, WM_CLOSE, 0, 0);
        }
    }
}

pub fn get_foreground_window() -> isize {
    unsafe { GetForegroundWindow() as isize }
}

pub fn get_app_name(exe_path: &str) -> String {
    Path::new(exe_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("?")
        .to_string()
}
