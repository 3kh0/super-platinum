use super::*;
/// Resource ceiling, not a Slack eligibility claim. Captured dimensions allow
/// 4000x4000; RGBA16 needs128,000,000 raster bytes.256MiB accommodates more than
/// twice that worst-case raster, encoding overhead and metadata. Reads stream
/// into this bound; the multipart sender does not duplicate the full image.
pub const MAX_BYTES: usize = 256 * 1024 * 1024;
#[derive(Clone)]
pub struct Image {
    bytes: Arc<[u8]>,
    preview: Arc<[u8]>,
    mime: &'static str,
    width: u32,
    height: u32,
}
impl std::fmt::Debug for Image {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PhotoImage")
            .field("mime", &self.mime)
            .field("width", &self.width)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}
impl Image {
    pub fn parse(bytes: Vec<u8>) -> Result<Self, Error> {
        if bytes.len() > MAX_BYTES {
            return Err(Error::Api("photo_file_too_large".into()));
        }
        let mime = crate::detect_image_mime(&bytes)
            .filter(|m| matches!(*m, "image/png" | "image/jpeg" | "image/gif"))
            .ok_or_else(|| Error::Api("photo_invalid_type".into()))?;
        let (width, height) = ::image::ImageReader::new(std::io::Cursor::new(&bytes))
            .with_guessed_format()
            .map_err(|_| Error::Api("photo_invalid_type".into()))?
            .into_dimensions()
            .map_err(|_| Error::Api("photo_invalid_type".into()))?;
        if !(36..=4000).contains(&width) || !(36..=4000).contains(&height) {
            return Err(Error::Api("photo_invalid_dimensions".into()));
        }
        // Called from desktop spawn_blocking. Decode only a GIF's first frame;
        // original upload bytes and server crop coordinates remain untouched.
        let mut reader = ::image::ImageReader::new(std::io::Cursor::new(&bytes))
            .with_guessed_format()
            .map_err(|_| Error::Api("photo_invalid_type".into()))?;
        let mut limits = ::image::Limits::default();
        limits.max_image_width = Some(4000);
        limits.max_image_height = Some(4000);
        // Strict dimensional limit; decoder allocation limit is best effort as
        // documented by image::Limits, not a universal process-memory bound.
        limits.max_alloc = Some(MAX_BYTES as u64);
        reader.limits(limits);
        let decoded = reader
            .decode()
            .map_err(|_| Error::Api("photo_decode_failed".into()))?;
        let thumb = decoded
            .thumbnail(width.min(480), height.min(480))
            .to_rgba8();
        drop(decoded);
        let mut preview = Vec::new();
        use ::image::ImageEncoder;
        ::image::codecs::png::PngEncoder::new(&mut preview)
            .write_image(
                thumb.as_raw(),
                thumb.width(),
                thumb.height(),
                ::image::ExtendedColorType::Rgba8,
            )
            .map_err(|_| Error::Api("photo_preview_failed".into()))?;
        Ok(Self {
            bytes: bytes.into(),
            preview: preview.into(),
            mime,
            width,
            height,
        })
    }
    pub fn bytes(&self) -> Arc<[u8]> {
        self.bytes.clone()
    }
    pub fn preview_bytes(&self) -> Arc<[u8]> {
        self.preview.clone()
    }
    pub fn mime(&self) -> &'static str {
        self.mime
    }
    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SquareCrop {
    x: u32,
    y: u32,
    size: u32,
}
impl SquareCrop {
    /// E5ZS normalizeSquareCrop, restricted to finite actual image geometry.
    pub fn normalize(x: f64, y: f64, size: f64, width: u32, height: u32) -> Option<Self> {
        if ![x, y, size].into_iter().all(f64::is_finite) || width == 0 || height == 0 {
            return None;
        }
        let size = size.round().max(0.0).min(width.min(height) as f64) as u32;
        if size == 0 {
            return None;
        }
        Some(Self {
            x: x.round().max(0.0).min((width - size) as f64) as u32,
            y: y.round().max(0.0).min((height - size) as f64) as u32,
            size,
        })
    }
    pub fn x(self) -> u32 {
        self.x
    }
    pub fn y(self) -> u32 {
        self.y
    }
    pub fn size(self) -> u32 {
        self.size
    }
}
