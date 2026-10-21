use super::*;

pub const MAX_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone)]
pub struct Image {
    pub(super) bytes: Arc<[u8]>,
    filename: String,
    mime: &'static str,
    width: u32,
    height: u32,
}
impl std::fmt::Debug for Image {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AvatarImage")
            .field("mime", &self.mime)
            .field("width", &self.width)
            .field("height", &self.height)
            .field("length", &self.bytes.len())
            .finish_non_exhaustive()
    }
}
impl Image {
    /// CPU decoding belongs on a host blocking worker, never a render callback.
    /// Keep the original file bytes/MIME/name for the actual multipart upload.
    pub fn parse(filename: String, mime: &str, bytes: Vec<u8>) -> Result<Self, Error> {
        let mime = match mime {
            "image/png" => "image/png",
            "image/jpeg" => "image/jpeg",
            "image/gif" => "image/gif",
            _ => return Err(Error::Api("invalid_type".into())),
        };
        if bytes.len() > MAX_BYTES {
            return Err(Error::Api("max_size".into()));
        }
        // A browser File name is not a path. Refuse framing controls rather than
        // silently replacing original names or introducing multipart headers.
        if filename.is_empty()
            || filename.chars().any(char::is_control)
            || filename.contains(['/', '\\'])
        {
            return Err(Error::Api("avatar_invalid_filename".into()));
        }
        if crate::detect_image_mime(&bytes) != Some(mime) {
            return Err(Error::Api("invalid_type".into()));
        }
        let reader = || {
            ::image::ImageReader::new(std::io::Cursor::new(&bytes))
                .with_guessed_format()
                .map_err(|_| Error::Api("invalid_type".into()))
        };
        let (width, height) = reader()?
            .into_dimensions()
            .map_err(|_| Error::Api("invalid_type".into()))?;
        if width > 2000 || height > 2000 {
            return Err(Error::Api("too_large".into()));
        }
        if width < 128 || height < 128 {
            return Err(Error::Api("too_small".into()));
        }
        let mut decoder = reader()?;
        let mut limits = ::image::Limits::default();
        limits.max_image_width = Some(2000);
        limits.max_image_height = Some(2000);
        limits.max_alloc = Some(64 * 1024 * 1024);
        decoder.limits(limits);
        // Actual decode detects corrupt/truncated image data. GIF first frame is
        // sufficient for dimensions; every original frame remains in upload bytes.
        decoder
            .decode()
            .map_err(|_| Error::Api("avatar_decode_failed".into()))?;
        Ok(Self {
            bytes: bytes.into(),
            filename,
            mime,
            width,
            height,
        })
    }
    pub fn filename(&self) -> &str {
        &self.filename
    }
    pub fn mime(&self) -> &'static str {
        self.mime
    }
    pub fn bytes(&self) -> Arc<[u8]> {
        self.bytes.clone()
    }
    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
}
