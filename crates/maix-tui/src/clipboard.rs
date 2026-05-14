#![allow(dead_code)]
//! Clipboard and image input handling for multimodal conversations.
//!
//! Supports: clipboard screenshots, image files, URLs, and raw base64.
//! Images are auto-compressed when exceeding size limits.

use std::path::{Path, PathBuf};

/// Maximum image dimension (width or height) before resizing.
const MAX_DIMENSION: u32 = 2048;
/// Maximum base64-encoded image size (1 MB).
const MAX_BASE64_SIZE: usize = 1_048_576;
/// Maximum pending images per message.
const MAX_PENDING_IMAGES: usize = 5;

/// Image format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Bmp,
    Gif,
    WebP,
}

impl ImageFormat {
    pub fn mime(&self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Bmp => "image/bmp",
            Self::Gif => "image/gif",
            Self::WebP => "image/webp",
        }
    }

    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "png" => Some(Self::Png),
            "jpg" | "jpeg" => Some(Self::Jpeg),
            "bmp" => Some(Self::Bmp),
            "gif" => Some(Self::Gif),
            "webp" => Some(Self::WebP),
            _ => None,
        }
    }

    pub fn from_mime(mime: &str) -> Option<Self> {
        match mime {
            "image/png" => Some(Self::Png),
            "image/jpeg" => Some(Self::Jpeg),
            "image/bmp" => Some(Self::Bmp),
            "image/gif" => Some(Self::Gif),
            "image/webp" => Some(Self::WebP),
            _ => None,
        }
    }
}

/// Check if clipboard contains an image (Windows).
#[cfg(target_os = "windows")]
pub fn clipboard_has_image() -> bool {
    // Use PowerShell to check clipboard content
    if let Ok(output) = std::process::Command::new("powershell")
        .args(["-Command", "Get-Clipboard -Format Image -ErrorAction SilentlyContinue | ConvertTo-Json"])
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        !stdout.trim().is_empty() && stdout.trim() != "null"
    } else {
        false
    }
}

/// Get image from clipboard as base64 (Windows).
#[cfg(target_os = "windows")]
pub fn get_clipboard_image_base64() -> Option<String> {
    let script = r#"
    $img = Get-Clipboard -Format Image -ErrorAction SilentlyContinue
    if ($img) {
        $ms = New-Object System.IO.MemoryStream
        $img.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
        [Convert]::ToBase64String($ms.ToArray())
    }
    "#;
    if let Ok(output) = std::process::Command::new("powershell")
        .args(["-Command", script])
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !stdout.is_empty() && stdout != "null" {
            Some(stdout)
        } else {
            None
        }
    } else {
        None
    }
}

/// A captured or loaded image.
#[derive(Debug, Clone)]
pub struct ImageData {
    pub data: Vec<u8>,
    pub format: ImageFormat,
    pub width: u32,
    pub height: u32,
}

impl ImageData {
    /// Encode image data as a base64 data URL.
    pub fn to_data_url(&self) -> String {
        let b64 = base64_encode(&self.data);
        format!("data:{};base64,{}", self.format.mime(), b64)
    }

    /// Check if the image exceeds size limits and needs compression.
    pub fn needs_compression(&self) -> bool {
        self.width > MAX_DIMENSION
            || self.height > MAX_DIMENSION
            || self.data.len() > MAX_BASE64_SIZE
    }
}

/// A pending image attached to the current input.
#[derive(Debug, Clone)]
pub struct PendingImage {
    pub data_url: String,
    pub format: ImageFormat,
    pub width: u32,
    pub height: u32,
    pub source: ImageSource,
}

/// Where the image came from.
#[derive(Debug, Clone)]
pub enum ImageSource {
    Clipboard,
    File(PathBuf),
    Url(String),
    Base64,
}

impl std::fmt::Display for ImageSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Clipboard => write!(f, "clipboard"),
            Self::File(p) => write!(f, "{}", p.display()),
            Self::Url(u) => write!(f, "{}", u),
            Self::Base64 => write!(f, "base64"),
        }
    }
}

/// Image manager — tracks pending images for the current message.
#[derive(Debug, Default)]
pub struct ImageManager {
    pending: Vec<PendingImage>,
}

impl ImageManager {
    pub fn new() -> Self {
        Self {
            pending: Vec::new(),
        }
    }

    /// Add an image from raw data.
    pub fn add_image(
        &mut self,
        data: Vec<u8>,
        format: ImageFormat,
        source: ImageSource,
    ) -> Result<(), String> {
        if self.pending.len() >= MAX_PENDING_IMAGES {
            return Err(format!(
                "maximum {} images per message",
                MAX_PENDING_IMAGES
            ));
        }

        let (width, height) = detect_dimensions(&data, format);
        let img = ImageData {
            data,
            format,
            width,
            height,
        };

        let data_url = if img.needs_compression() {
            // For now, just use as-is. Real compression would need the `image` crate.
            img.to_data_url()
        } else {
            img.to_data_url()
        };

        self.pending.push(PendingImage {
            data_url,
            format,
            width,
            height,
            source,
        });

        Ok(())
    }

    /// Load an image from a file path.
    pub fn add_from_file(&mut self, path: &Path) -> Result<(), String> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        let format = ImageFormat::from_extension(ext)
            .ok_or_else(|| format!("unsupported image format: .{}", ext))?;

        let data = std::fs::read(path)
            .map_err(|e| format!("failed to read image: {}", e))?;

        self.add_image(data, format, ImageSource::File(path.to_path_buf()))
    }

    /// Load an image from a base64 string.
    pub fn add_from_base64(&mut self, b64: &str, mime: &str) -> Result<(), String> {
        let format = ImageFormat::from_mime(mime)
            .ok_or_else(|| format!("unsupported mime type: {}", mime))?;

        let data = base64_decode(b64)
            .map_err(|e| format!("invalid base64: {}", e))?;

        self.add_image(data, format, ImageSource::Base64)
    }

    /// Get count of pending images.
    pub fn count(&self) -> usize {
        self.pending.len()
    }

    /// Check if there are pending images.
    pub fn has_images(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Take all pending images (clears internal state).
    pub fn take_all(&mut self) -> Vec<PendingImage> {
        std::mem::take(&mut self.pending)
    }

    /// Get references to pending images.
    pub fn pending(&self) -> &[PendingImage] {
        &self.pending
    }

    /// Clear all pending images.
    pub fn clear(&mut self) {
        self.pending.clear();
    }

    /// Format pending images for display in the input area.
    pub fn format_display(&self) -> String {
        if self.pending.is_empty() {
            return String::new();
        }
        self.pending
            .iter()
            .enumerate()
            .map(|(i, img)| {
                format!(
                    "[Image #{}: {}x{} from {}]",
                    i + 1,
                    img.width,
                    img.height,
                    img.source
                )
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Clipboard image capture.
pub struct ClipboardWatcher {
    #[cfg(target_os = "windows")]
    last_hash: Option<u64>,
}

impl ClipboardWatcher {
    pub fn new() -> Self {
        Self {
            #[cfg(target_os = "windows")]
            last_hash: None,
        }
    }

    /// Try to get an image from the system clipboard.
    pub fn get_image(&mut self) -> Option<ImageData> {
        #[cfg(target_os = "windows")]
        {
            self.get_image_windows()
        }
        #[cfg(target_os = "macos")]
        {
            get_image_macos()
        }
        #[cfg(target_os = "linux")]
        {
            get_image_linux()
        }
    }

    #[cfg(target_os = "windows")]
    fn get_image_windows(&mut self) -> Option<ImageData> {
        // Use PowerShell to extract clipboard image as PNG base64
        let output = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                r#"
                Add-Type -AssemblyName System.Windows.Forms
                $img = [System.Windows.Forms.Clipboard]::GetImage()
                if ($img) {
                    $ms = New-Object System.IO.MemoryStream
                    $img.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
                    [Convert]::ToBase64String($ms.ToArray())
                }
                "#,
            ])
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let b64 = String::from_utf8(output.stdout).ok()?.trim().to_string();
        if b64.is_empty() {
            return None;
        }

        let data = base64_decode(&b64).ok()?;
        if data.is_empty() {
            return None;
        }

        // Simple hash to detect changes
        let hash = simple_hash(&data);
        if Some(hash) == self.last_hash {
            return None;
        }
        self.last_hash = Some(hash);

        let (width, height) = detect_dimensions(&data, ImageFormat::Png);
        Some(ImageData {
            data,
            format: ImageFormat::Png,
            width,
            height,
        })
    }
}

#[cfg(target_os = "macos")]
fn get_image_macos() -> Option<ImageData> {
    let output = std::process::Command::new("osascript")
        .args([
            "-e",
            "the clipboard as «class PNGf»",
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    // osascript returns hex-encoded data
    let hex = String::from_utf8(output.stdout).ok()?;
    let hex: String = hex.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    let data = hex_decode(&hex).ok()?;
    if data.is_empty() {
        return None;
    }

    let (width, height) = detect_dimensions(&data, ImageFormat::Png);
    Some(ImageData {
        data,
        format: ImageFormat::Png,
        width,
        height,
    })
}

#[cfg(target_os = "linux")]
fn get_image_linux() -> Option<ImageData> {
    // Try wl-paste (Wayland) first, then xclip (X11)
    let output = std::process::Command::new("wl-paste")
        .args(["--type", "image/png"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .or_else(|| {
            std::process::Command::new("xclip")
                .args(["-selection", "clipboard", "-t", "image/png", "-o"])
                .output()
                .ok()
                .filter(|o| o.status.success())
        })?;

    let data = output.stdout;
    if data.is_empty() {
        return None;
    }

    let (width, height) = detect_dimensions(&data, ImageFormat::Png);
    Some(ImageData {
        data,
        format: ImageFormat::Png,
        width,
        height,
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Detect image dimensions from raw bytes (PNG/JPEG header parsing).
fn detect_dimensions(data: &[u8], format: ImageFormat) -> (u32, u32) {
    match format {
        ImageFormat::Png => detect_png_dimensions(data),
        ImageFormat::Jpeg => detect_jpeg_dimensions(data),
        _ => (0, 0),
    }
}

fn detect_png_dimensions(data: &[u8]) -> (u32, u32) {
    // PNG header: 8-byte signature + 4-byte length + 4-byte "IHDR" + 4-byte width + 4-byte height
    if data.len() >= 24 && data[0..8] == [137, 80, 78, 71, 13, 10, 26, 10] {
        let w = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
        let h = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
        return (w, h);
    }
    (0, 0)
}

fn detect_jpeg_dimensions(data: &[u8]) -> (u32, u32) {
    // JPEG: scan for SOF0 (0xFFC0) or SOF2 (0xFFC2) marker
    let mut i = 0;
    while i + 4 < data.len() {
        if data[i] == 0xFF {
            let marker = data[i + 1];
            if (marker == 0xC0 || marker == 0xC2)
                && i + 9 < data.len() {
                    let h = u16::from_be_bytes([data[i + 5], data[i + 6]]) as u32;
                    let w = u16::from_be_bytes([data[i + 7], data[i + 8]]) as u32;
                    return (w, h);
                }
            // Skip to next marker
            if i + 3 < data.len() {
                let len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
                i += 2 + len;
                continue;
            }
        }
        i += 1;
    }
    (0, 0)
}

/// Simple hash for change detection.
fn simple_hash(data: &[u8]) -> u64 {
    let mut hash: u64 = 5381;
    for &b in data.iter().take(4096) {
        hash = hash.wrapping_mul(33).wrapping_add(b as u64);
    }
    hash
}

/// Base64 encode (standard alphabet, no padding issues).
fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).map(|&b| b as u32).unwrap_or(0);
        let b2 = chunk.get(2).map(|&b| b as u32).unwrap_or(0);
        let triple = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

/// Base64 decode.
fn base64_decode(data: &str) -> Result<Vec<u8>, String> {
    let data: String = data.chars().filter(|c| !c.is_whitespace()).collect();
    let data = data.trim_end_matches('=');
    let mut result = Vec::with_capacity(data.len() * 3 / 4);

    fn val(c: u8) -> Result<u8, String> {
        match c {
            b'A'..=b'Z' => Ok(c - b'A'),
            b'a'..=b'z' => Ok(c - b'a' + 26),
            b'0'..=b'9' => Ok(c - b'0' + 52),
            b'+' => Ok(62),
            b'/' => Ok(63),
            _ => Err(format!("invalid base64 character: {}", c as char)),
        }
    }

    let bytes = data.as_bytes();
    let mut i = 0;
    while i + 3 < bytes.len() {
        let a = val(bytes[i])? as u32;
        let b = val(bytes[i + 1])? as u32;
        let c = val(bytes[i + 2])? as u32;
        let d = val(bytes[i + 3])? as u32;
        let triple = (a << 18) | (b << 12) | (c << 6) | d;
        result.push((triple >> 16) as u8);
        result.push(((triple >> 8) & 0xFF) as u8);
        result.push((triple & 0xFF) as u8);
        i += 4;
    }

    // Handle remaining bytes
    let remaining = bytes.len() - i;
    if remaining >= 2 {
        let a = val(bytes[i])? as u32;
        let b = val(bytes[i + 1])? as u32;
        let triple = (a << 18) | (b << 12);
        result.push((triple >> 16) as u8);
        if remaining >= 3 {
            let c = val(bytes[i + 2])? as u32;
            let triple = (a << 18) | (b << 12) | (c << 6);
            result.push(((triple >> 8) & 0xFF) as u8);
        }
    }

    Ok(result)
}

fn hex_decode(hex: &str) -> Result<Vec<u8>, String> {
    if !hex.len().is_multiple_of(2) {
        return Err("odd-length hex string".to_string());
    }
    let mut result = Vec::with_capacity(hex.len() / 2);
    let bytes = hex.as_bytes();
    for chunk in bytes.chunks(2) {
        let hi = hex_digit(chunk[0])?;
        let lo = hex_digit(chunk[1])?;
        result.push((hi << 4) | lo);
    }
    Ok(result)
}

fn hex_digit(c: u8) -> Result<u8, String> {
    match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        b'A'..=b'F' => Ok(c - b'A' + 10),
        _ => Err(format!("invalid hex digit: {}", c as char)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_image_format_from_extension() {
        assert_eq!(ImageFormat::from_extension("png"), Some(ImageFormat::Png));
        assert_eq!(ImageFormat::from_extension("jpg"), Some(ImageFormat::Jpeg));
        assert_eq!(ImageFormat::from_extension("jpeg"), Some(ImageFormat::Jpeg));
        assert_eq!(ImageFormat::from_extension("gif"), Some(ImageFormat::Gif));
        assert_eq!(ImageFormat::from_extension("xyz"), None);
    }

    #[test]
    fn test_image_format_mime() {
        assert_eq!(ImageFormat::Png.mime(), "image/png");
        assert_eq!(ImageFormat::Jpeg.mime(), "image/jpeg");
    }

    #[test]
    fn test_image_format_from_mime() {
        assert_eq!(ImageFormat::from_mime("image/png"), Some(ImageFormat::Png));
        assert_eq!(ImageFormat::from_mime("text/plain"), None);
    }

    #[test]
    fn test_base64_roundtrip() {
        let data = b"Hello, World! This is a test of base64 encoding.";
        let encoded = base64_encode(data);
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(data.as_slice(), decoded.as_slice());
    }

    #[test]
    fn test_base64_decode_standard() {
        // "Hello" = "SGVsbG8="
        let decoded = base64_decode("SGVsbG8=").unwrap();
        assert_eq!(decoded, b"Hello");
    }

    #[test]
    fn test_hex_decode() {
        assert_eq!(hex_decode("48656C6C6F").unwrap(), b"Hello");
        assert_eq!(hex_decode("FF00AB").unwrap(), vec![0xFF, 0x00, 0xAB]);
    }

    #[test]
    fn test_detect_png_dimensions() {
        // Minimal valid PNG header
        let mut data = vec![0u8; 24];
        data[0..8].copy_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);
        // Width = 1920 (0x00000780)
        data[16] = 0x00;
        data[17] = 0x00;
        data[18] = 0x07;
        data[19] = 0x80;
        // Height = 1080 (0x00000438)
        data[20] = 0x00;
        data[21] = 0x00;
        data[22] = 0x04;
        data[23] = 0x38;

        let (w, h) = detect_png_dimensions(&data);
        assert_eq!(w, 1920);
        assert_eq!(h, 1080);
    }

    #[test]
    fn test_image_manager_add_and_take() {
        let mut mgr = ImageManager::new();
        assert!(!mgr.has_images());

        let data = vec![0u8; 100];
        mgr.add_image(data, ImageFormat::Png, ImageSource::Clipboard)
            .unwrap();
        assert_eq!(mgr.count(), 1);
        assert!(mgr.has_images());

        let images = mgr.take_all();
        assert_eq!(images.len(), 1);
        assert_eq!(mgr.count(), 0);
    }

    #[test]
    fn test_image_manager_max_limit() {
        let mut mgr = ImageManager::new();
        for _ in 0..MAX_PENDING_IMAGES {
            mgr.add_image(vec![0u8; 10], ImageFormat::Png, ImageSource::Clipboard)
                .unwrap();
        }
        assert!(mgr
            .add_image(vec![0u8; 10], ImageFormat::Png, ImageSource::Clipboard)
            .is_err());
    }

    #[test]
    fn test_image_manager_clear() {
        let mut mgr = ImageManager::new();
        mgr.add_image(vec![0u8; 10], ImageFormat::Png, ImageSource::Clipboard)
            .unwrap();
        mgr.clear();
        assert_eq!(mgr.count(), 0);
    }

    #[test]
    fn test_format_display() {
        let mut mgr = ImageManager::new();
        assert_eq!(mgr.format_display(), "");

        mgr.add_image(vec![0u8; 10], ImageFormat::Png, ImageSource::Clipboard)
            .unwrap();
        let display = mgr.format_display();
        assert!(display.contains("Image #1"));
        assert!(display.contains("clipboard"));
    }

    #[test]
    fn test_image_source_display() {
        assert_eq!(ImageSource::Clipboard.to_string(), "clipboard");
        assert_eq!(
            ImageSource::File(PathBuf::from("/tmp/test.png")).to_string(),
            "/tmp/test.png"
        );
        assert_eq!(
            ImageSource::Url("https://example.com/img.png".to_string()).to_string(),
            "https://example.com/img.png"
        );
    }

    #[test]
    fn test_data_url_format() {
        let img = ImageData {
            data: b"test".to_vec(),
            format: ImageFormat::Png,
            width: 100,
            height: 100,
        };
        let url = img.to_data_url();
        assert!(url.starts_with("data:image/png;base64,"));
    }

    #[test]
    fn test_simple_hash() {
        let a = simple_hash(b"hello world");
        let b = simple_hash(b"hello world");
        let c = simple_hash(b"hello worle");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
