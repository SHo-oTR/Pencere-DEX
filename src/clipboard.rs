use std::ffi::c_void;
use windows_sys::Win32::System::DataExchange::*;
use windows_sys::Win32::System::Memory::*;
use windows_sys::Win32::System::Ole::{CF_HDROP, CF_UNICODETEXT};
use windows_sys::Win32::UI::Shell::*;

/// Sets UTF-16 text to Windows clipboard
pub fn set_clipboard_text(text: &str) {
    let utf16: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes_len = utf16.len() * std::mem::size_of::<u16>();

    unsafe {
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return;
        }

        EmptyClipboard();

        let h_mem = GlobalAlloc(GMEM_MOVEABLE, bytes_len);
        if !h_mem.is_null() {
            let ptr = GlobalLock(h_mem);
            if !ptr.is_null() {
                std::ptr::copy_nonoverlapping(utf16.as_ptr() as *const c_void, ptr, bytes_len);
                GlobalUnlock(h_mem);
                SetClipboardData(CF_UNICODETEXT as u32, h_mem);
            }
        }

        CloseClipboard();
    }
}

/// Reads UTF-16 text from Windows clipboard
pub fn get_clipboard_text() -> Option<String> {
    unsafe {
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return None;
        }

        let h_data = GetClipboardData(CF_UNICODETEXT as u32);
        let result = if !h_data.is_null() {
            let ptr = GlobalLock(h_data) as *const u16;
            if !ptr.is_null() {
                let mut len = 0;
                while *ptr.add(len) != 0 {
                    len += 1;
                }
                let slice = std::slice::from_raw_parts(ptr, len);
                let text = String::from_utf16_lossy(slice);
                GlobalUnlock(h_data);
                Some(text)
            } else {
                None
            }
        } else {
            None
        };

        CloseClipboard();
        result
    }
}

/// If clipboard contains files (CF_HDROP), returns the path of the first file
pub fn get_clipboard_file_path() -> Option<String> {
    unsafe {
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return None;
        }

        let h_data = GetClipboardData(CF_HDROP as u32);
        let result = if !h_data.is_null() {
            let hdrop = h_data as HDROP;
            let file_count = DragQueryFileW(hdrop, 0xFFFFFFFF, std::ptr::null_mut(), 0);
            if file_count > 0 {
                let mut buf = [0u16; 1024];
                let len = DragQueryFileW(hdrop, 0, buf.as_mut_ptr(), buf.len() as u32);
                if len > 0 {
                    Some(String::from_utf16_lossy(&buf[..len as usize]))
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        CloseClipboard();
        result
    }
}
