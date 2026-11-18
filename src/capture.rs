use crate::win32::get_window_rect;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;

// Declare PrintWindow directly to keep this binding independent of generated module features.
#[link(name = "user32")]
extern "system" {
    #[link_name = "PrintWindow"]
    fn print_window(hwnd: HWND, hdc_blt: HDC, flags: u32) -> BOOL;
}

pub struct RawFrame {
    pub width: u32,
    pub height: u32,
    pub pixels_bgra: Vec<u8>,
    pub rect: (i32, i32, i32, i32),
}

/// Captures either full screen or a specific window
pub fn capture_frame(hwnd: HWND, pw_mode: bool) -> Option<RawFrame> {
    let (l, t, r, b) = get_window_rect(hwnd);
    let width = (r - l).max(0) as u32;
    let height = (b - t).max(0) as u32;

    if width < 2 || height < 2 {
        return None;
    }

    unsafe {
        let screen_dc = GetDC(std::ptr::null_mut());
        if screen_dc.is_null() {
            return None;
        }

        let mem_dc = CreateCompatibleDC(screen_dc);
        if mem_dc.is_null() {
            ReleaseDC(std::ptr::null_mut(), screen_dc);
            return None;
        }

        let bmp = CreateCompatibleBitmap(screen_dc, width as i32, height as i32);
        if bmp.is_null() {
            DeleteDC(mem_dc);
            ReleaseDC(std::ptr::null_mut(), screen_dc);
            return None;
        }

        let old_bmp = SelectObject(mem_dc, bmp);

        let mut captured = false;
        if !hwnd.is_null() && pw_mode {
            // PW_RENDERFULLCONTENT = 2
            let ok = PrintWindow(hwnd, mem_dc, 2);
            captured = ok != 0;
        }

        if !captured {
            let raster_op = SRCCOPY | CAPTUREBLT;
            BitBlt(
                mem_dc,
                0,
                0,
                width as i32,
                height as i32,
                screen_dc,
                l,
                t,
                raster_op,
            );
        }

        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32), // Negative height = top-down DIB
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

        let byte_len = (width * height * 4) as usize;
        let mut pixels = vec![0u8; byte_len];

        GetDIBits(
            mem_dc,
            bmp,
            0,
            height,
            pixels.as_mut_ptr() as *mut _,
            &mut bmi,
            DIB_RGB_COLORS,
        );

        SelectObject(mem_dc, old_bmp);
        DeleteObject(bmp);
        DeleteDC(mem_dc);
        ReleaseDC(std::ptr::null_mut(), screen_dc);

        Some(RawFrame {
            width,
            height,
            pixels_bgra: pixels,
            rect: (l, t, r, b),
        })
    }
}
