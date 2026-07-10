//! Shell icon extraction: one code path for files, folders, executables, and Store apps.
//!
//! `IShellItemImageFactory::GetImage` resolves the icon for any shell parsing name —
//! absolute paths and `shell:AppsFolder\<AUMID>` alike. Icons are handed to the UI as
//! data URLs so the webview needs no filesystem access. COM is initialized lazily once
//! per calling thread.

use base64::Engine;
use windows::core::PCWSTR;
use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::{
    DeleteObject, GetDC, GetDIBits, ReleaseDC, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP,
};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Shell::{IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_ICONONLY};

/// Rendered at 30 CSS px; 64 physical keeps rows crisp on hi-DPI displays.
const ICON_SIZE: i32 = 64;

thread_local! {
    static COM_INIT: () = {
        let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    };
}

/// Icon of a shell item as a `data:image/png;base64,…` URL, or `None` if the shell
/// can't produce one (the UI falls back to a monogram).
pub fn icon_data_url(parsing_name: &str) -> Option<String> {
    COM_INIT.with(|()| ());
    let rgba = shell_icon_rgba(parsing_name)?;
    let png = encode_png(&rgba)?;
    Some(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png)
    ))
}

fn shell_icon_rgba(parsing_name: &str) -> Option<Vec<u8>> {
    let wide: Vec<u16> = parsing_name.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let factory: IShellItemImageFactory = SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None).ok()?;
        let bitmap = factory
            .GetImage(
                SIZE {
                    cx: ICON_SIZE,
                    cy: ICON_SIZE,
                },
                SIIGBF_ICONONLY,
            )
            .ok()?;
        let pixels = read_bgra(bitmap);
        let _ = DeleteObject(bitmap.into());
        pixels
    }
}

unsafe fn read_bgra(bitmap: HBITMAP) -> Option<Vec<u8>> {
    let hdc = GetDC(None);
    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: ICON_SIZE,
            biHeight: -ICON_SIZE, // negative = top-down rows
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut pixels = vec![0u8; (ICON_SIZE * ICON_SIZE * 4) as usize];
    let copied = GetDIBits(
        hdc,
        bitmap,
        0,
        ICON_SIZE as u32,
        Some(pixels.as_mut_ptr().cast()),
        &mut info,
        DIB_RGB_COLORS,
    );
    let _ = ReleaseDC(None, hdc);
    if copied == 0 {
        return None;
    }
    // BGRA -> RGBA; icons drawn without an alpha channel come back fully transparent.
    let opaque = pixels.chunks_exact(4).all(|px| px[3] == 0);
    for px in pixels.chunks_exact_mut(4) {
        px.swap(0, 2);
        if opaque {
            px[3] = 255;
        }
    }
    Some(pixels)
}

fn encode_png(rgba: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, ICON_SIZE as u32, ICON_SIZE as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().ok()?;
    writer.write_image_data(rgba).ok()?;
    writer.finish().ok()?;
    Some(out)
}
