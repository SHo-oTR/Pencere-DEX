use crate::win32::get_window_rect;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

pub fn move_cursor(hwnd: HWND, nx: f32, ny: f32) {
    let (l, t, r, b) = get_window_rect(hwnd);
    let width = (r - l) as f32;
    let height = (b - t) as f32;

    let target_x = (l as f32 + nx * width) as i32;
    let target_y = (t as f32 + ny * height) as i32;

    unsafe {
        SetCursorPos(target_x, target_y);
    }
}

pub fn mouse_click(btn: &str, count: usize) {
    let (down, up) = match btn {
        "l" => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
        "r" => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
        "m" => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP),
        _ => return,
    };

    unsafe {
        for _ in 0..count {
            mouse_event(down, 0, 0, 0, 0);
            mouse_event(up, 0, 0, 0, 0);
        }
    }
}

pub fn mouse_down() {
    unsafe {
        mouse_event(MOUSEEVENTF_LEFTDOWN, 0, 0, 0, 0);
    }
}

pub fn mouse_up() {
    unsafe {
        mouse_event(MOUSEEVENTF_LEFTUP, 0, 0, 0, 0);
    }
}

pub fn mouse_scroll(dy: i32, dx: i32) {
    unsafe {
        if dy != 0 {
            mouse_event(MOUSEEVENTF_WHEEL, 0, 0, dy, 0);
        }
        if dx != 0 {
            mouse_event(MOUSEEVENTF_HWHEEL, 0, 0, dx, 0);
        }
    }
}

pub fn send_key(vk: u8, ctrl: bool, alt: bool, shift: bool, win: bool) {
    unsafe {
        let is_extended = (0x21..=0x2E).contains(&vk);
        let ext_flag = if is_extended { 1 } else { 0 };

        if ctrl { keybd_event(0x11, 0, 0, 0); }
        if alt { keybd_event(0x12, 0, 0, 0); }
        if shift { keybd_event(0x10, 0, 0, 0); }
        if win { keybd_event(0x5B, 0, 0, 0); }

        keybd_event(vk, 0, ext_flag, 0);
        keybd_event(vk, 0, ext_flag | KEYEVENTF_KEYUP, 0);

        if win { keybd_event(0x5B, 0, KEYEVENTF_KEYUP, 0); }
        if shift { keybd_event(0x10, 0, KEYEVENTF_KEYUP, 0); }
        if alt { keybd_event(0x12, 0, KEYEVENTF_KEYUP, 0); }
        if ctrl { keybd_event(0x11, 0, KEYEVENTF_KEYUP, 0); }
    }
}

pub fn send_text(text: &str) {
    let utf16_units: Vec<u16> = text.encode_utf16().collect();
    if utf16_units.is_empty() {
        return;
    }

    let mut inputs = Vec::with_capacity(utf16_units.len() * 2);

    for &code in &utf16_units {
        // Key down
        inputs.push(INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: 0,
                    wScan: code,
                    dwFlags: KEYEVENTF_UNICODE,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        });

        // Key up
        inputs.push(INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: 0,
                    wScan: code,
                    dwFlags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        });
    }

    unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        );
    }
}

pub fn toggle_start_menu() {
    unsafe {
        keybd_event(0x5B, 0, 0, 0);
        keybd_event(0x5B, 0, KEYEVENTF_KEYUP, 0);
    }
}

pub fn show_desktop() {
    unsafe {
        keybd_event(0x5B, 0, 0, 0);
        keybd_event(0x44, 0, 0, 0);
        keybd_event(0x44, 0, KEYEVENTF_KEYUP, 0);
        keybd_event(0x5B, 0, KEYEVENTF_KEYUP, 0);
    }
}
