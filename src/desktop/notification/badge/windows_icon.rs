//! Owned GDI resources; numeric glyphs are text, the unread symbol is Material.
use super::super::raster;
use windows::Win32::{
    Foundation::{COLORREF, RECT},
    Graphics::Gdi::*,
    UI::WindowsAndMessaging::*,
};
use windows::core::w;

pub(super) struct Icon(pub HICON);
impl Drop for Icon {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyIcon(self.0);
        }
    }
}
struct Bitmap(HBITMAP);
impl Drop for Bitmap {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(HGDIOBJ(self.0.0));
        }
    }
}
struct Dc(HDC);
impl Drop for Dc {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteDC(self.0);
        }
    }
}
struct Font(HFONT);
impl Drop for Font {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(HGDIOBJ(self.0.0));
        }
    }
}
struct Selection(HDC, HGDIOBJ);
impl Selection {
    fn new(dc: HDC, object: HGDIOBJ) -> Self {
        Self(dc, unsafe { SelectObject(dc, object) })
    }
}
impl Drop for Selection {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.0, self.1);
        }
    }
}

fn dib(size: usize) -> Result<(Bitmap, *mut u8), String> {
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: size as i32,
            biHeight: -(size as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits = std::ptr::null_mut();
    let bitmap = unsafe { CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0) }
        .map_err(|error| error.to_string())?;
    Ok((Bitmap(bitmap), bits.cast()))
}

fn numeric(label: &str, size: usize) -> Result<Vec<u8>, String> {
    let label = raster::display_count(label);
    let (bitmap, bits) = dib(size)?;
    let dc = Dc(unsafe { CreateCompatibleDC(None) });
    if dc.0.0.is_null() {
        return Err("CreateCompatibleDC failed".into());
    }
    let font = Font(unsafe {
        CreateFontW(
            -((size * if label.len() > 2 { 56 } else { 76 } / 100) as i32),
            0,
            0,
            0,
            FW_BOLD.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            ANTIALIASED_QUALITY,
            0,
            w!("Segoe UI"),
        )
    });
    if font.0.0.is_null() {
        return Err("CreateFontW failed".into());
    }
    let _bitmap_selection = Selection::new(dc.0, HGDIOBJ(bitmap.0.0));
    let _font_selection = Selection::new(dc.0, HGDIOBJ(font.0.0));
    let bytes = unsafe { std::slice::from_raw_parts_mut(bits, size * size * 4) };
    bytes.fill(0);
    let mut text: Vec<u16> = label.encode_utf16().collect();
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: size as i32,
        bottom: size as i32,
    };
    unsafe {
        SetBkMode(dc.0, TRANSPARENT);
        SetTextColor(dc.0, COLORREF(0x00ff_ffff));
        if DrawTextW(
            dc.0,
            &mut text,
            &mut rect,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        ) == 0
        {
            return Err("DrawTextW failed".into());
        }
        GdiFlush().ok().map_err(|error| error.to_string())?;
    }
    let mut pixels = vec![0_u8; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            // Circular background belongs to the numeric label, not a symbol.
            let mut covered = 0_u16;
            for sy in 0..4 {
                for sx in 0..4 {
                    let dx = x as f64 + (f64::from(sx) + 0.5) / 4.0 - size as f64 / 2.0;
                    let dy = y as f64 + (f64::from(sy) + 0.5) / 4.0 - size as f64 / 2.0;
                    covered += u16::from(dx * dx + dy * dy < (size as f64 / 2.0).powi(2));
                }
            }
            let offset = (y * size + x) * 4;
            let alpha = covered * 255 / 16;
            let ink = u16::from(bytes[offset]); // grayscale, no ClearType fringes
            let pale = ((48 + (207 * ink / 255)) * alpha / 255) as u8;
            pixels[offset..offset + 4].copy_from_slice(&[pale, pale, alpha as u8, alpha as u8]);
        }
    }
    Ok(pixels)
}

pub(super) fn create(label: &str, size: usize) -> Result<Icon, String> {
    let pixels = if label == "•" {
        raster::symbol(size)?
    } else {
        numeric(label, size)?
    };
    let (color, bits) = dib(size)?;
    unsafe {
        std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits, pixels.len());
    }
    let mask_bytes = vec![0_u8; size.div_ceil(16) * 2 * size];
    let mask = Bitmap(unsafe {
        CreateBitmap(
            size as i32,
            size as i32,
            1,
            1,
            Some(mask_bytes.as_ptr().cast()),
        )
    });
    if mask.0.0.is_null() {
        return Err("CreateBitmap mask failed".into());
    }
    let info = ICONINFO {
        fIcon: true.into(),
        hbmMask: mask.0,
        hbmColor: color.0,
        ..Default::default()
    };
    // CreateIconIndirect copies both bitmaps; guards release them on every path.
    unsafe { CreateIconIndirect(&info) }
        .map(Icon)
        .map_err(|error| error.to_string())
}
