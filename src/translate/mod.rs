//! Screenshot/photo translation orchestration.
//!
//! All heavy work (model download, OCR, LLM inference) runs on a single
//! background worker thread owned by a process-wide singleton [`engine`]. The
//! UI never blocks and never touches a `Signal` from that thread: the worker
//! pushes [`Msg`]s into a shared inbox that the page drains on a timer and
//! applies to its own reactive state. This keeps the Dioxus "Signals are only
//! written from the reactive scope" rule intact.
//!
//! Flow per request: image → OCR lines → HY-MT translation (streamed as tokens).

pub mod models;
pub mod mt;
pub mod ocr;

use std::collections::VecDeque;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};

use image::DynamicImage;

use models::DownloadEvent;
use mt::Translator;
use ocr::Ocr;

/// (value, label) language options shared by the translate page and the
/// settings screen. HY-MT supports many more languages; this covers the common
/// Phase 1 cases. Values match the language names HY-MT expects in its prompt.
pub const LANGS: [(&str, &str); 8] = [
    ("Chinese", "中文"),
    ("English", "英语"),
    ("Japanese", "日语"),
    ("Korean", "韩语"),
    ("French", "法语"),
    ("German", "德语"),
    ("Spanish", "西班牙语"),
    ("Russian", "俄语"),
];

/// A translation job plus the runtime configuration captured when it was queued.
pub struct TranslateRequest {
    pub image: DynamicImage,
    pub target: String,
    pub src: String,
    /// Offload layers to GPU (Metal/Vulkan/CUDA) when loading the MT model.
    pub gpu: bool,
    /// Context window for the MT model (desktop 2048 / mobile 1024).
    pub n_ctx: u32,
    /// Keep the MT model loaded after this job (off frees memory when idle).
    pub resident: bool,
}

/// Commands the UI can send to the worker thread.
pub enum Command {
    Translate(TranslateRequest),
    /// Translate already-recognized (and possibly user-corrected) text without
    /// running OCR again. Lets the user fix recognition errors and re-translate.
    TranslateText(TextRequest),
    DownloadOcr,
    DownloadMt,
    /// Drop any resident models to reclaim memory.
    Unload,
    /// Release all resident models and stop the worker. Sent on app shutdown so
    /// the llama/Metal resources are freed *before* the process exits; otherwise
    /// ggml's Metal device destructor asserts (`Abort trap: 6`) at teardown.
    Shutdown,
}

/// A text-only translation job: the source text is supplied directly, so no OCR
/// runs. Shares the runtime configuration fields with [`TranslateRequest`].
pub struct TextRequest {
    pub text: String,
    pub target: String,
    pub src: String,
    /// Offload layers to GPU (Metal/Vulkan/CUDA) when loading the MT model.
    pub gpu: bool,
    /// Context window for the MT model (desktop 2048 / mobile 1024).
    pub n_ctx: u32,
    /// Keep the MT model loaded after this job (off frees memory when idle).
    pub resident: bool,
}

/// Messages the worker thread reports back to the UI.
#[derive(Debug, Clone)]
pub enum Msg {
    Log(String),
    DownloadProgress {
        file: String,
        received: u64,
        total: u64,
    },
    DownloadDone {
        file: String,
    },
    DownloadError {
        file: String,
        message: String,
    },
    /// OCR finished; recognized lines in reading order.
    OcrText(Vec<String>),
    /// One streamed translation piece.
    Token(String),
    /// Translation finished with the full text.
    Done(String),
    Error(String),
}

/// Thread-safe sink the worker uses to publish [`Msg`]s.
#[derive(Clone)]
struct Emitter(Arc<Mutex<VecDeque<Msg>>>);

impl Emitter {
    fn send(&self, m: Msg) {
        if let Ok(mut q) = self.0.lock() {
            q.push_back(m);
        }
    }

    fn forward_download(&self, ev: DownloadEvent) {
        match ev {
            DownloadEvent::Start { name, total } => {
                let size = if total > 0 {
                    format!("（约 {} MB）", total / 1024 / 1024)
                } else {
                    String::new()
                };
                self.send(Msg::Log(format!("开始下载：{name}{size}")));
            }
            DownloadEvent::Progress {
                name,
                received,
                total,
            } => self.send(Msg::DownloadProgress {
                file: name,
                received,
                total,
            }),
            DownloadEvent::Done { name } => self.send(Msg::DownloadDone { file: name }),
            DownloadEvent::Error { name, message } => self.send(Msg::DownloadError {
                file: name,
                message,
            }),
        }
    }
}

/// Handle to the singleton worker: a command sender plus the shared inbox.
pub struct Engine {
    cmd_tx: Sender<Command>,
    inbox: Arc<Mutex<VecDeque<Msg>>>,
    /// Worker thread handle, kept so [`shutdown`] can join it after the models
    /// are released. Taken (set to `None`) on the first shutdown call.
    worker: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl Engine {
    /// Queue a command for the worker thread.
    pub fn send(&self, cmd: Command) {
        let _ = self.cmd_tx.send(cmd);
    }

    /// Drain and return all pending messages since the last call.
    pub fn drain(&self) -> Vec<Msg> {
        match self.inbox.lock() {
            Ok(mut q) => q.drain(..).collect(),
            Err(_) => Vec::new(),
        }
    }
}

static ENGINE: OnceLock<Engine> = OnceLock::new();

/// The process-wide translation engine, started lazily on first access.
pub fn engine() -> &'static Engine {
    ENGINE.get_or_init(|| {
        let (cmd_tx, cmd_rx): (Sender<Command>, Receiver<Command>) = channel();
        let inbox: Arc<Mutex<VecDeque<Msg>>> = Arc::new(Mutex::new(VecDeque::new()));
        let emit = Emitter(Arc::clone(&inbox));
        let worker = std::thread::Builder::new()
            .name("synstudy-translate".to_string())
            .spawn(move || worker(cmd_rx, emit))
            .expect("failed to spawn the translation worker thread");
        Engine {
            cmd_tx,
            inbox,
            worker: Mutex::new(Some(worker)),
        }
    })
}

/// Release the resident models and stop the worker thread, blocking until the
/// GPU/native resources are actually freed. Called from the desktop shutdown
/// hook so ggml's Metal device sees no live resources when the process exits
/// (otherwise it asserts and the app aborts on close). Idempotent and a no-op if
/// the engine was never started.
pub fn shutdown() {
    let Some(engine) = ENGINE.get() else { return };
    engine.send(Command::Shutdown);
    if let Ok(mut handle) = engine.worker.lock() {
        if let Some(handle) = handle.take() {
            let _ = handle.join();
        }
    }
}

/// Arm a `libc::atexit` hook that releases the resident models during process
/// teardown. This is the reliable path on macOS: tao calls `exit()` from inside
/// its event loop, and the Dioxus window-event hook is not guaranteed to run
/// (and finish joining the worker) before `exit()` reaches ggml's static
/// destructors.
///
/// Ordering is the whole point: ggml's Metal device lives in a function-local
/// `static std::vector` whose destructor is registered with `__cxa_atexit` the
/// first time a model touches the GPU. `__cxa_finalize` runs handlers in reverse
/// registration order, so we MUST register *after* that (i.e. after the first
/// successful load) for our hook to run *before* ggml's device destructor and
/// free the Metal buffers while the process is still healthy.
#[cfg(target_os = "macos")]
fn arm_exit_hook() {
    use std::sync::Once;
    static ARMED: Once = Once::new();
    ARMED.call_once(|| unsafe {
        libc::atexit(exit_hook);
    });
}

#[cfg(target_os = "macos")]
extern "C" fn exit_hook() {
    shutdown();
}

#[cfg(not(target_os = "macos"))]
fn arm_exit_hook() {}

/// Worker loop: owns the (lazily loaded) OCR + MT handles, processes commands
/// sequentially, and reports progress through `emit`.
fn worker(rx: Receiver<Command>, emit: Emitter) {
    let mut ocr: Option<Ocr> = None;
    let mut mt: Option<Translator> = None;

    while let Ok(cmd) = rx.recv() {
        match cmd {
            Command::DownloadOcr => {
                emit.send(Msg::Log("开始下载 OCR 模型…".to_string()));
                let e = emit.clone();
                let res = models::download_group(&models::OCR_FILES, |ev| e.forward_download(ev));
                if let Err(err) = res {
                    emit.send(Msg::Error(err));
                }
            }
            Command::DownloadMt => {
                emit.send(Msg::Log("开始下载翻译模型（约 1 GB）…".to_string()));
                let e = emit.clone();
                let res = models::download_group(&[&models::MT_MODEL], |ev| e.forward_download(ev));
                if let Err(err) = res {
                    emit.send(Msg::Error(err));
                }
            }
            Command::Unload => {
                mt = None;
                ocr = None;
                emit.send(Msg::Log("已卸载模型，释放内存。".to_string()));
            }
            Command::Shutdown => {
                // Leave the loop; `mt`/`ocr` are dropped as `worker` returns,
                // freeing the Metal/llama resources. The joining `shutdown()`
                // caller knows the GPU is clear once join returns.
                break;
            }
            Command::Translate(req) => {
                // 1) OCR — load the pipeline on first use.
                if ocr.is_none() {
                    match Ocr::load() {
                        Ok(o) => ocr = Some(o),
                        Err(e) => {
                            emit.send(Msg::Error(e));
                            continue;
                        }
                    }
                }
                let lines = match ocr.as_mut().unwrap().recognize(&req.image) {
                    Ok(l) => l,
                    Err(e) => {
                        emit.send(Msg::Error(e));
                        continue;
                    }
                };
                emit.send(Msg::OcrText(lines.clone()));

                // 2) MT over the recognized text (streamed back as tokens).
                run_mt(
                    &mut mt,
                    &lines.join("\n"),
                    &req.target,
                    &req.src,
                    req.gpu,
                    req.n_ctx,
                    req.resident,
                    &emit,
                );
            }
            Command::TranslateText(req) => {
                // No OCR: translate the user-supplied (possibly corrected) text.
                run_mt(
                    &mut mt,
                    &req.text,
                    &req.target,
                    &req.src,
                    req.gpu,
                    req.n_ctx,
                    req.resident,
                    &emit,
                );
            }
        }
    }
}

/// Ensure the MT model is loaded, then translate `source_text`, streaming each
/// decoded piece back through `emit`. Shared by the image and text-only paths.
/// When `resident` is false the model is dropped afterwards to reclaim ~1 GB.
#[allow(clippy::too_many_arguments)]
fn run_mt(
    mt: &mut Option<Translator>,
    source_text: &str,
    target: &str,
    src: &str,
    gpu: bool,
    n_ctx: u32,
    resident: bool,
    emit: &Emitter,
) {
    if source_text.trim().is_empty() {
        emit.send(Msg::Done(String::new()));
        return;
    }

    // Load weights on first use (or after an unload).
    if mt.is_none() {
        emit.send(Msg::Log("正在加载翻译模型…".to_string()));
        match Translator::load(gpu, n_ctx) {
            Ok(t) => {
                *mt = Some(t);
                // The Metal device now exists, so ggml's exit destructor is
                // registered; arm ours afterwards so it runs first (see above).
                arm_exit_hook();
            }
            Err(e) => {
                emit.send(Msg::Error(e));
                return;
            }
        }
    }

    // Stream tokens back as they decode.
    let e = emit.clone();
    let mut on_token = move |piece: &str| e.send(Msg::Token(piece.to_string()));
    match mt
        .as_ref()
        .unwrap()
        .translate(source_text, target, src, &mut on_token)
    {
        Ok(full) => emit.send(Msg::Done(full)),
        Err(err) => emit.send(Msg::Error(err)),
    }

    if !resident {
        *mt = None; // free ~1 GB when the user opted out of residency
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// Reproduces the app lifecycle that used to abort on close: run a resident
    /// translation and return *without* an explicit shutdown, exactly like the
    /// app (which relies on the `atexit` hook armed during model load). If the
    /// hook releases the Metal resources during teardown the binary exits 0;
    /// otherwise ggml's Metal device destructor aborts (`Abort trap: 6`).
    #[test]
    #[ignore = "needs the HY-MT model; verifies the atexit hook frees Metal before teardown"]
    fn resident_model_released_at_exit() {
        let eng = engine();
        eng.send(Command::TranslateText(TextRequest {
            text: "Hello".to_string(),
            target: "Chinese".to_string(),
            src: "auto".to_string(),
            gpu: true,
            n_ctx: 2048,
            resident: true, // keep the model loaded, exactly like the app default
        }));

        let start = Instant::now();
        loop {
            if eng
                .drain()
                .iter()
                .any(|m| matches!(m, Msg::Done(_) | Msg::Error(_)))
            {
                break;
            }
            assert!(
                start.elapsed() < Duration::from_secs(180),
                "timed out waiting for the translation"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
        // Deliberately no shutdown() here: the model is still resident on the
        // worker thread when this test returns, so only the `atexit` hook can
        // release it before the process (and ggml's device destructor) runs.
    }
}
