//! Seeing the screen: capture, shrink to what a model can use, encode as PNG.
//!
//! Capture is platform code (`capture`); the rest is plain and tested everywhere. A screenshot is
//! shrunk before it is sent: a 4K screen at full size costs many times the tokens of a 1568-pixel
//! one, with no gain in what the model can read.

/// A captured picture: `width * height` pixels, 4 bytes each in B, G, R, A order (Windows' order).
pub struct Frame {
    pub width: usize,
    pub height: usize,
    pub bgra: Vec<u8>,
}

/// What part of the screen to take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// The main display.
    Primary,
    /// All displays together, as one picture.
    All,
}

impl Scope {
    pub fn parse(value: Option<&str>) -> Result<Self, String> {
        match value.map(str::trim) {
            None | Some("") | Some("primary") => Ok(Scope::Primary),
            Some("all") => Ok(Scope::All),
            Some(other) => Err(format!(
                "unknown screen {other:?}; use \"primary\" or \"all\""
            )),
        }
    }
}

/// Longest side, in pixels, of a picture sent to the model.
pub const DEFAULT_MAX_EDGE: usize = 1568;
const MIN_MAX_EDGE: usize = 512;
const MAX_MAX_EDGE: usize = 2048;

pub fn clamp_edge(requested: Option<i64>) -> usize {
    match requested {
        Some(n) if n > 0 => (n as usize).clamp(MIN_MAX_EDGE, MAX_MAX_EDGE),
        _ => DEFAULT_MAX_EDGE,
    }
}

/// The picture as it will be sent: RGB bytes and its size.
pub struct Picture {
    pub width: usize,
    pub height: usize,
    pub rgb: Vec<u8>,
    /// Size of the screen it was taken from, before shrinking.
    pub source_width: usize,
    pub source_height: usize,
}

/// Shrink to at most `max_edge` on the longest side by averaging the pixels each new pixel covers
/// (so thin text stays legible), and drop the alpha channel.
pub fn shrink(frame: &Frame, max_edge: usize) -> Picture {
    let (w, h) = (frame.width.max(1), frame.height.max(1));
    let scale = (max_edge as f64 / w.max(h) as f64).min(1.0);
    let (nw, nh) = (
        ((w as f64 * scale).round() as usize).max(1),
        ((h as f64 * scale).round() as usize).max(1),
    );
    let mut rgb = Vec::with_capacity(nw * nh * 3);
    for y in 0..nh {
        let (y0, y1) = (y * h / nh, ((y + 1) * h / nh).max(y * h / nh + 1).min(h));
        for x in 0..nw {
            let (x0, x1) = (x * w / nw, ((x + 1) * w / nw).max(x * w / nw + 1).min(w));
            let (mut b, mut g, mut r, mut n) = (0u32, 0u32, 0u32, 0u32);
            for yy in y0..y1 {
                let row = yy * w * 4;
                for xx in x0..x1 {
                    let at = row + xx * 4;
                    if let Some(px) = frame.bgra.get(at..at + 3) {
                        b += px[0] as u32;
                        g += px[1] as u32;
                        r += px[2] as u32;
                        n += 1;
                    }
                }
            }
            let n = n.max(1);
            rgb.extend_from_slice(&[(r / n) as u8, (g / n) as u8, (b / n) as u8]);
        }
    }
    Picture {
        width: nw,
        height: nh,
        rgb,
        source_width: w,
        source_height: h,
    }
}

pub fn encode_png(picture: &Picture) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, picture.width as u32, picture.height as u32);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|e| format!("could not encode the screenshot: {e}"))?;
        writer
            .write_image_data(&picture.rgb)
            .map_err(|e| format!("could not encode the screenshot: {e}"))?;
    }
    Ok(out)
}

/// Take the screen. Windows: the screen as the desktop shows it (GDI), per the user's display
/// scaling. Other systems are not supported yet.
#[cfg(windows)]
pub fn capture(scope: Scope) -> Result<Frame, String> {
    use std::ffi::c_void;
    use std::mem::{size_of, zeroed};
    use std::ptr::null_mut;
    use windows_sys::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
        GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CAPTUREBLT,
        DIB_RGB_COLORS, SRCCOPY,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SetProcessDPIAware, SM_CXSCREEN, SM_CXVIRTUALSCREEN, SM_CYSCREEN,
        SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
    };

    // Without this a scaled display reports its scaled size and the picture is cut off. It fails
    // harmlessly when the process already declared its awareness (the desktop app does).
    unsafe { SetProcessDPIAware() };
    let (x, y, width, height) = unsafe {
        match scope {
            Scope::Primary => (
                0,
                0,
                GetSystemMetrics(SM_CXSCREEN),
                GetSystemMetrics(SM_CYSCREEN),
            ),
            Scope::All => (
                GetSystemMetrics(SM_XVIRTUALSCREEN),
                GetSystemMetrics(SM_YVIRTUALSCREEN),
                GetSystemMetrics(SM_CXVIRTUALSCREEN),
                GetSystemMetrics(SM_CYVIRTUALSCREEN),
            ),
        }
    };
    if width <= 0 || height <= 0 {
        return Err("the screen has no size; is a display connected and unlocked?".to_owned());
    }
    let mut bgra = vec![0u8; width as usize * height as usize * 4];
    let copied = unsafe {
        let screen = GetDC(null_mut());
        if screen.is_null() {
            return Err("could not reach the screen".to_owned());
        }
        let memory = CreateCompatibleDC(screen);
        let bitmap = CreateCompatibleBitmap(screen, width, height);
        let previous = SelectObject(memory, bitmap);
        // CAPTUREBLT includes layered (transparent) windows, which many modern apps are.
        let blitted = BitBlt(
            memory,
            0,
            0,
            width,
            height,
            screen,
            x,
            y,
            SRCCOPY | CAPTUREBLT,
        );
        let mut info: BITMAPINFO = zeroed();
        info.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = width;
        info.bmiHeader.biHeight = -height; // negative: rows from the top
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = BI_RGB;
        let rows = GetDIBits(
            memory,
            bitmap,
            0,
            height as u32,
            bgra.as_mut_ptr() as *mut c_void,
            &mut info,
            DIB_RGB_COLORS,
        );
        SelectObject(memory, previous);
        DeleteObject(bitmap);
        DeleteDC(memory);
        ReleaseDC(null_mut(), screen);
        blitted != 0 && rows != 0
    };
    if !copied {
        return Err("Windows would not hand over the screen (a secure desktop such as the lock screen or a UAC prompt cannot be captured)".to_owned());
    }
    Ok(Frame {
        width: width as usize,
        height: height as usize,
        bgra,
    })
}

#[cfg(not(windows))]
pub fn capture(_scope: Scope) -> Result<Frame, String> {
    Err("seeing the screen is only supported on Windows so far".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(width: usize, height: usize, bgra: [u8; 4]) -> Frame {
        Frame {
            width,
            height,
            bgra: bgra.repeat(width * height),
        }
    }

    #[test]
    fn a_big_screen_is_shrunk_to_the_longest_side_keeping_proportions() {
        let picture = shrink(&solid(3840, 2160, [10, 20, 30, 255]), 1568);
        assert_eq!((picture.width, picture.height), (1568, 882));
        assert_eq!((picture.source_width, picture.source_height), (3840, 2160));
        assert_eq!(picture.rgb.len(), 1568 * 882 * 3);
        // B,G,R in becomes R,G,B out.
        assert_eq!(&picture.rgb[..3], &[30, 20, 10]);
    }

    #[test]
    fn a_small_screen_is_never_enlarged() {
        let picture = shrink(&solid(800, 600, [0, 0, 0, 255]), 1568);
        assert_eq!((picture.width, picture.height), (800, 600));
        let tiny = shrink(&solid(1, 1, [1, 2, 3, 255]), 1568);
        assert_eq!((tiny.width, tiny.height), (1, 1));
    }

    #[test]
    fn shrinking_averages_so_a_thin_line_does_not_vanish() {
        // 4 pixels wide, black with one white column: halving averages pairs.
        let mut frame = solid(4, 2, [0, 0, 0, 255]);
        for y in 0..2 {
            let at = (y * 4 + 1) * 4;
            frame.bgra[at..at + 3].copy_from_slice(&[255, 255, 255]);
        }
        let picture = shrink(&frame, 2);
        assert_eq!((picture.width, picture.height), (2, 1));
        assert_eq!(picture.rgb[0], 127); // (0 + 255) / 2
        assert_eq!(picture.rgb[3], 0);
    }

    #[test]
    fn png_output_is_a_valid_image_of_the_right_size() {
        let picture = shrink(&solid(64, 32, [200, 100, 50, 255]), 1568);
        let bytes = encode_png(&picture).unwrap();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        let decoder = png::Decoder::new(std::io::Cursor::new(&bytes));
        let mut reader = decoder.read_info().unwrap();
        let mut buffer = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut buffer).unwrap();
        assert_eq!((info.width, info.height), (64, 32));
        assert_eq!(&buffer[..3], &[50, 100, 200]);
    }

    #[test]
    fn scope_and_edge_arguments_are_checked() {
        assert_eq!(Scope::parse(None).unwrap(), Scope::Primary);
        assert_eq!(Scope::parse(Some("all")).unwrap(), Scope::All);
        assert!(Scope::parse(Some("2")).is_err());
        assert_eq!(clamp_edge(None), 1568);
        assert_eq!(clamp_edge(Some(100)), 512);
        assert_eq!(clamp_edge(Some(99_999)), 2048);
    }
}
