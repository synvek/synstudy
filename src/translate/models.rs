//! On-demand model management: catalog, local layout, download, SHA-256
//! verification, and per-file status.
//!
//! Models are **never** bundled with the app. They are fetched at first use
//! into `platform::models_dir()` (`<data>/models/ocr/*` and `<data>/models/mt/*`)
//! with progress callbacks, a size/hash check, and mirror fallback. All network
//! work here is *blocking* and must be driven from a background thread — never
//! from the UI thread (see `translate::mod`).
//!
//! NOTE ON URLS / HASHES: each file currently has a single ModelScope source
//! (`synvek/*` repos, uploaded by us — the authoritative provenance for these
//! exact artifacts; for MT_MODEL the file is our own mainline-llama.cpp Q4_K_M
//! quant of the official tencent/HY-MT1.5-1.8B weights, and the HY Community
//! License plus runtime consent gate apply — see license/HY-MT-NOTICE.txt). Every
//! `sha256` is the digest computed from a verified download, so each fetch is
//! integrity-checked against it (see [`download_one`]). Additional mirrors are
//! intentionally not wired in yet; they will be added once trusted alternatives
//! exist. Requests must carry a non-empty `User-Agent` or ModelScope's LFS CDN
//! returns `403` (set in [`download_group`]).

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// One downloadable artifact.
#[derive(Debug, Clone, Copy)]
pub struct ModelFile {
    /// Local file name (also the last URL path segment).
    pub name: &'static str,
    /// Sub-directory under `models_dir()`: `"ocr"` or `"mt"`.
    pub subdir: &'static str,
    /// Human-readable label shown in the UI.
    pub label: &'static str,
    /// Download mirrors, tried in order until one succeeds.
    pub urls: &'static [&'static str],
    /// Approximate on-disk size in bytes (0 == unknown; used for the progress
    /// bar total and as a sanity floor).
    pub size_hint: u64,
    /// Expected SHA-256 hex digest, or `None` to skip the hash check.
    pub sha256: Option<&'static str>,
}

/// OCR detection model (PP-OCRv5 mobile, MNN). Shared by all languages.
pub const OCR_DET: ModelFile = ModelFile {
    name: "PP-OCRv5_mobile_det.mnn",
    subdir: "ocr",
    label: "OCR 文本检测模型",
    urls: &[
        "https://www.modelscope.cn/models/synvek/PP-OCR/resolve/master/PP-OCRv5_mobile_det.mnn",
    ],
    size_hint: 4_760_244,
    sha256: Some("326f846bb5c903282e116ea089e8796b67921586726cca9457730436a79684c3"),
};

/// OCR recognition model (PP-OCRv5 mobile multi-language, MNN).
pub const OCR_REC: ModelFile = ModelFile {
    name: "PP-OCRv5_mobile_rec.mnn",
    subdir: "ocr",
    label: "OCR 文本识别模型",
    urls: &[
        "https://www.modelscope.cn/models/synvek/PP-OCR/resolve/master/PP-OCRv5_mobile_rec.mnn",
    ],
    size_hint: 16_531_596,
    sha256: Some("c809800b09263a8d18c678c211e470ffc464cbb33db2e6bde0244766f3feb0db"),
};

/// Character-set dictionary for the recognition model.
pub const OCR_KEYS: ModelFile = ModelFile {
    name: "ppocr_keys_v5.txt",
    subdir: "ocr",
    label: "OCR 字符表",
    urls: &[
        "https://www.modelscope.cn/models/synvek/PP-OCR/resolve/master/ppocr_keys_v5.txt",
    ],
    size_hint: 74_008,
    sha256: Some("f2ed6bb20a850ce4767fa9b4622d9b282985ab7f0ea8f8c11abd790ca6d2ff94"),
};

/// Translation model: HY-MT1.5-1.8B, mainline-llama.cpp compatible Q4_K_M GGUF.
/// ~1.08 GB (1,133,080,224 bytes). Served from our `synvek` ModelScope repo.
/// Provenance: we converted this quant from the official tencent weights — the
/// official AngelSlim GGUFs need unmerged llama.cpp kernels. Download is
/// gated behind explicit acceptance of the Tencent HY Community License (see
/// `pages/translate.rs`; full terms in `license/HY-MT-LICENSE.txt`).
pub const MT_MODEL: ModelFile = ModelFile {
    name: "hy-mt1.5-1.8b-q4_k_m.gguf",
    subdir: "mt",
    label: "翻译模型 HY-MT1.5-1.8B (Q4_K_M)",
    urls: &[
        "https://www.modelscope.cn/models/synvek/HY-MT1.5-1.8B-GGUF/resolve/master/hy-mt1.5-1.8b-q4_k_m.gguf",
    ],
    size_hint: 1_133_080_224,
    sha256: Some("7b9474cac7e0c640eb1949b9c6007550d11cfca504bc6d5393300134811e9628"),
};

/// The three OCR artifacts, in dependency order.
pub const OCR_FILES: [&ModelFile; 3] = [&OCR_DET, &OCR_REC, &OCR_KEYS];

/// Local path of a model file (parent dirs are created on demand during download).
pub fn local_path(f: &ModelFile) -> PathBuf {
    crate::platform::models_dir().join(f.subdir).join(f.name)
}

/// Lifecycle state of a single model file on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelStatus {
    /// Absent, or a leftover partial download.
    Missing,
    /// Present and passes the size (and, when known, hash) check.
    Ready,
    /// Present but failed verification — must be re-downloaded.
    Invalid,
}

/// Cheap on-disk status check: existence + a minimum size floor. The full
/// SHA-256 recomputation only happens right after a download ([`verify`]), not
/// on every UI poll, to keep status queries fast.
pub fn status(f: &ModelFile) -> ModelStatus {
    let path = local_path(f);
    let meta = match fs::metadata(&path) {
        Ok(m) => m,
        Err(_) => return ModelStatus::Missing,
    };
    if !meta.is_file() {
        return ModelStatus::Missing;
    }
    // A partial download leaves a `.part` sibling, so a full-size file here is
    // trusted; allow a small tolerance below the hint.
    if f.size_hint > 0 && meta.len() + 4096 < f.size_hint {
        return ModelStatus::Invalid;
    }
    ModelStatus::Ready
}

/// True when every file in `files` is [`ModelStatus::Ready`].
pub fn group_ready(files: &[&ModelFile]) -> bool {
    files.iter().all(|f| status(f) == ModelStatus::Ready)
}

/// Streaming progress / completion events emitted by [`download_group`].
#[derive(Debug, Clone)]
pub enum DownloadEvent {
    /// A file started; `total` is 0 when the server omitted Content-Length.
    Start { name: String, total: u64 },
    /// Bytes received so far for the current file.
    Progress { name: String, received: u64, total: u64 },
    /// A file finished and verified.
    Done { name: String },
    /// A file failed on all mirrors.
    Error { name: String, message: String },
}

/// Delete a model file (used by the settings "清理模型" action).
#[allow(dead_code)] // wired into the settings UI in a follow-up
pub fn remove(f: &ModelFile) {
    let _ = fs::remove_file(local_path(f));
    let _ = fs::remove_file(part_path(f));
}

fn part_path(f: &ModelFile) -> PathBuf {
    let mut p = local_path(f).into_os_string();
    p.push(".part");
    PathBuf::from(p)
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

/// Recompute and check a downloaded file. Returns `Ok(())` when the size floor
/// and (if recorded) the SHA-256 digest both match.
#[allow(dead_code)] // on-demand re-verify; reserved for the settings "校验" action
pub fn verify(f: &ModelFile, path: &Path) -> Result<(), String> {
    let len = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    if f.size_hint > 0 && len + 4096 < f.size_hint {
        return Err(format!(
            "文件过小 ({} 字节，预期约 {} 字节)，下载可能不完整",
            len, f.size_hint
        ));
    }
    if let Some(expected) = f.sha256 {
        let mut file = fs::File::open(path).map_err(|e| format!("无法打开文件校验: {e}"))?;
        let mut hasher = Sha256::new();
        let mut buf = [0u8; 64 * 1024];
        loop {
            let n = std::io::Read::read(&mut file, &mut buf)
                .map_err(|e| format!("校验读取失败: {e}"))?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        let got = hex(&hasher.finalize());
        if !got.eq_ignore_ascii_case(expected) {
            return Err(format!("SHA-256 校验失败 (期望 {expected}，实际 {got})"));
        }
    }
    Ok(())
}

/// Ensure a single file is present and valid, downloading it (with mirror
/// fallback) when necessary. Emits [`DownloadEvent`]s to `on_event`.
fn ensure_file(
    client: &reqwest::blocking::Client,
    f: &ModelFile,
    on_event: &mut dyn FnMut(DownloadEvent),
) -> Result<(), String> {
    if status(f) == ModelStatus::Ready {
        on_event(DownloadEvent::Done {
            name: f.name.to_string(),
        });
        return Ok(());
    }
    let dest = local_path(f);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("无法创建目录: {e}"))?;
    }
    let mut last_err = String::from("无可用下载源");
    for url in f.urls {
        match download_one(client, url, f, &dest, on_event) {
            Ok(()) => return Ok(()),
            Err(e) => last_err = e,
        }
    }
    let _ = fs::remove_file(part_path(f));
    Err(last_err)
}

fn download_one(
    client: &reqwest::blocking::Client,
    url: &str,
    f: &ModelFile,
    dest: &Path,
    on_event: &mut dyn FnMut(DownloadEvent),
) -> Result<(), String> {
    let mut resp = client
        .get(url)
        .send()
        .map_err(|e| format!("请求失败: {e}"))?
        .error_for_status()
        .map_err(|e| format!("HTTP 错误: {e}"))?;

    let total = resp.content_length().unwrap_or(f.size_hint);
    on_event(DownloadEvent::Start {
        name: f.name.to_string(),
        total,
    });

    let part = part_path(f);
    let mut out = fs::File::create(&part).map_err(|e| format!("无法写入临时文件: {e}"))?;
    let mut hasher = Sha256::new();
    let mut received: u64 = 0;
    let mut last_report: u64 = 0;
    // The blocking `Response` implements `std::io::Read`; stream through it in
    // fixed buffers (the async `bytes_stream`/`chunk` APIs are not available here).
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = resp.read(&mut buf).map_err(|e| format!("下载中断: {e}"))?;
        if n == 0 {
            break;
        }
        let chunk = &buf[..n];
        out.write_all(chunk).map_err(|e| format!("写入失败: {e}"))?;
        hasher.update(chunk);
        received += n as u64;
        // Throttle progress callbacks to roughly every 256 KiB.
        if received - last_report >= 256 * 1024 {
            last_report = received;
            on_event(DownloadEvent::Progress {
                name: f.name.to_string(),
                received,
                total,
            });
        }
    }
    on_event(DownloadEvent::Progress {
        name: f.name.to_string(),
        received,
        total,
    });
    out.flush().map_err(|e| format!("写入失败: {e}"))?;
    drop(out);

    // Verify before promoting `.part` -> final name.
    if let Some(expected) = f.sha256 {
        let got = hex(&hasher.finalize());
        if !got.eq_ignore_ascii_case(expected) {
            let _ = fs::remove_file(&part);
            return Err(format!("SHA-256 校验失败 (期望 {expected}，实际 {got})"));
        }
    } else if f.size_hint > 0 && received + 4096 < f.size_hint {
        let _ = fs::remove_file(&part);
        return Err(format!(
            "文件过小 ({received} 字节，预期约 {} 字节)",
            f.size_hint
        ));
    }

    fs::rename(&part, dest).map_err(|e| format!("无法落地文件: {e}"))?;
    on_event(DownloadEvent::Done {
        name: f.name.to_string(),
    });
    Ok(())
}

/// Download every file of a group, skipping those already [`ModelStatus::Ready`].
/// Blocking; run on a worker thread. Reports per-file progress via `on_event`.
pub fn download_group(
    files: &[&ModelFile],
    mut on_event: impl FnMut(DownloadEvent),
) -> Result<(), String> {
    let client = reqwest::blocking::Client::builder()
        // ModelScope's LFS CDN (and some HF mirrors) reject requests that carry
        // no User-Agent with `403 Forbidden` after the `resolve` 302 redirect.
        // reqwest sends no UA by default, so we must set a non-empty one here.
        .user_agent(concat!(
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) ",
            "AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36 synstudy"
        ))
        .timeout(std::time::Duration::from_secs(3600))
        .build()
        .map_err(|e| format!("无法创建下载客户端: {e}"))?;
    for f in files {
        if let Err(e) = ensure_file(&client, f, &mut on_event) {
            on_event(DownloadEvent::Error {
                name: f.name.to_string(),
                message: e.clone(),
            });
            return Err(e);
        }
    }
    Ok(())
}
