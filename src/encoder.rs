use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::codecs::webp::WebPEncoder;
use image::ImageEncoder;
use image::ColorType;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    WebP = 0,
    Avif = 1,
    Jpeg = 2,
    Png = 3,
}

impl ImageFormat {
    pub fn code(&self) -> u16 {
        *self as u16
    }
}

pub struct QualityProfile {
    pub max_width: u32,
    pub jpeg_quality: u8,
    pub webp_quality: u8,
}

pub const QUALITY_PROFILES: [QualityProfile; 4] = [
    QualityProfile { max_width: 1000, jpeg_quality: 70, webp_quality: 65 },
    QualityProfile { max_width: 720,  jpeg_quality: 60, webp_quality: 55 },
    QualityProfile { max_width: 540,  jpeg_quality: 50, webp_quality: 45 },
    QualityProfile { max_width: 400,  jpeg_quality: 42, webp_quality: 35 },
];

/// Encodes an RGB image into the target format
pub fn encode_image(
    rgb_data: &[u8],
    width: u32,
    height: u32,
    fmt: ImageFormat,
    quality_idx: usize,
) -> Result<Vec<u8>, image::ImageError> {
    let q_profile = &QUALITY_PROFILES[quality_idx.min(QUALITY_PROFILES.len() - 1)];
    let mut out = Vec::new();

    match fmt {
        ImageFormat::Jpeg => {
            let mut enc = JpegEncoder::new_with_quality(&mut out, q_profile.jpeg_quality);
            enc.encode(rgb_data, width, height, ColorType::Rgb8.into())?;
        }
        ImageFormat::Png => {
            let enc = PngEncoder::new(&mut out);
            enc.write_image(rgb_data, width, height, image::ExtendedColorType::Rgb8)?;
        }
        ImageFormat::WebP => {
            let enc = WebPEncoder::new_lossless(&mut out);
            enc.encode(rgb_data, width, height, ColorType::Rgb8.into())?;
        }
        ImageFormat::Avif => {
            // Fallback to high-speed JPEG if AVIF encoder is not compiled in
            let mut enc = JpegEncoder::new_with_quality(&mut out, q_profile.jpeg_quality);
            enc.encode(rgb_data, width, height, ColorType::Rgb8.into())?;
        }
    }

    Ok(out)
}

/// Checks if an image is mostly flat interface colors (suitable for PNG)
pub fn is_flat_ui(rgb: &[u8], width: usize, height: usize) -> bool {
    let mut colors = std::collections::HashSet::new();
    let step = 3;
    let mut y = 0;
    while y < height {
        let mut x = 0;
        while x < width {
            let idx = (y * width + x) * 3;
            if idx + 2 < rgb.len() {
                let c = ((rgb[idx] as u32) << 16) | ((rgb[idx + 1] as u32) << 8) | (rgb[idx + 2] as u32);
                colors.insert(c);
                if colors.len() > 64 {
                    return false;
                }
            }
            x += step;
        }
        y += step;
    }
    true
}
