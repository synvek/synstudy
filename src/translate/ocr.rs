//! Thin wrapper over `rust-paddle-ocr` (PaddleOCR models on the MNN engine).
//!
//! Loads the PP-OCRv5 mobile detection + recognition models from
//! `platform::models_dir()/ocr/` and turns an image into a list of recognized
//! text lines. Loading is relatively cheap but not free, so the engine keeps one
//! [`Ocr`] instance resident across translations (see `translate::mod`).

use image::DynamicImage;
use rust_paddle_ocr::{Det, Rec};

use super::models::{self, ModelStatus, OCR_DET, OCR_KEYS, OCR_REC};

/// A loaded OCR pipeline (detection + recognition).
pub struct Ocr {
    det: Det,
    rec: Rec,
}

impl Ocr {
    /// Load the OCR models from disk. Fails with a human-readable message when
    /// the files are missing (the caller offers a download first) or when MNN
    /// rejects them.
    pub fn load() -> Result<Self, String> {
        for f in [&OCR_DET, &OCR_REC, &OCR_KEYS] {
            if models::status(f) != ModelStatus::Ready {
                return Err(format!("OCR 模型缺失：{}（请先下载）", f.label));
            }
        }
        let det_path = models::local_path(&OCR_DET);
        let rec_path = models::local_path(&OCR_REC);
        let keys_path = models::local_path(&OCR_KEYS);

        // `from_file` accepts a path-like argument; pass `&str` so it is
        // compatible whether the signature is `&str` or `impl AsRef<Path>`.
        let det = Det::from_file(det_path.to_string_lossy().as_ref())
            .map_err(|e| format!("加载检测模型失败: {e:?}"))?
            // PP-OCRv5 recommended detection tuning.
            .with_rect_border_size(12)
            .with_merge_boxes(false)
            .with_merge_threshold(1);

        let rec = Rec::from_file(
            rec_path.to_string_lossy().as_ref(),
            keys_path.to_string_lossy().as_ref(),
        )
        .map_err(|e| format!("加载识别模型失败: {e:?}"))?
        // A lower floor than the crate default (0.6) keeps thin Latin glyphs that
        // PP-OCR scores modestly; screenshot text is clean enough that the extra
        // noise is negligible while the missing-character rate drops sharply.
        .with_min_score(0.35)
        .with_punct_min_score(0.1);

        Ok(Self { det, rec })
    }

    /// Run detection + recognition over a whole image, returning the recognized
    /// lines in reading order with blanks dropped.
    pub fn recognize(&mut self, img: &DynamicImage) -> Result<Vec<String>, String> {
        // PP-OCR's recognizer downscales each crop to 48px tall but never upscales
        // it, so short text lines (very common in screenshots) reach the model
        // below its design height and lose characters — thin Latin glyphs first.
        // Measure the detected line height once and, when the text is small,
        // upscale the whole image toward the recognizer's sweet spot before the
        // real detection+recognition pass.
        let img = self.maybe_upscale(img);
        self.recognize_once(&img)
    }

    /// Detection + recognition over an image already at a good working size.
    fn recognize_once(&mut self, img: &DynamicImage) -> Result<Vec<String>, String> {
        let text_images = self
            .det
            .find_text_img(img)
            .map_err(|e| format!("文本检测失败: {e:?}"))?;
        let mut lines = Vec::with_capacity(text_images.len());
        for region in &text_images {
            // A single unrecognizable crop should not abort the whole pass.
            if let Ok(text) = self.rec.predict_str(region) {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    lines.push(trimmed.to_string());
                }
            }
        }
        Ok(lines)
    }

    /// Upscale `img` when its detected text boxes are shorter than the height the
    /// recognizer works best at. Returns the original image untouched when the
    /// text is already large enough (or detection found nothing to measure).
    fn maybe_upscale(&mut self, img: &DynamicImage) -> DynamicImage {
        // Detected boxes already carry the recognizer's border padding, so aim a
        // little above the model's fixed 48px input height for a safety margin.
        const TARGET_BOX_H: f64 = 60.0;
        const MAX_SCALE: f64 = 3.0;
        const MIN_USEFUL_SCALE: f64 = 1.2;

        // A measuring detection pass; if it fails, let the real pass report it.
        let Ok(rects) = self.det.find_text_rect(img) else {
            return img.clone();
        };
        if rects.is_empty() {
            return img.clone();
        }
        let mut heights: Vec<f64> = rects.iter().map(|r| r.height() as f64).collect();
        heights.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let median = heights[heights.len() / 2];
        if median <= 0.0 {
            return img.clone();
        }
        let scale = (TARGET_BOX_H / median).min(MAX_SCALE);
        if scale < MIN_USEFUL_SCALE {
            return img.clone();
        }
        let nw = ((img.width() as f64) * scale).round().max(1.0) as u32;
        let nh = ((img.height() as f64) * scale).round().max(1.0) as u32;
        // CatmullRom matches the recognizer's own resampling filter and is fast
        // enough for the enlarged canvases this produces.
        img.resize_exact(nw, nh, image::imageops::FilterType::CatmullRom)
    }
}
