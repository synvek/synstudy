//! Screenshot/photo translation page.
//!
//! Compiled in two shapes:
//! - **feature off** (default): a short placeholder, so the app still builds and
//!   the nav entry never panics.
//! - **feature on**: capture → ROI box-select → OCR → streamed translation, with
//!   copy / wordbook / TTS actions and a model download/status panel. All heavy
//!   work is delegated to `crate::translate::engine()`; this file only drives UI
//!   state from the engine's message inbox.

#[cfg(not(feature = "translate"))]
use dioxus::prelude::*;

/// Placeholder rendered when the `translate` feature is not compiled in.
#[cfg(not(feature = "translate"))]
#[component]
pub fn Translate() -> Element {
    rsx! {
        div { class: "page empty-pick",
            h1 { "翻译" }
            p { class: "muted",
                "当前构建未包含翻译功能。请使用 `cargo run --features translate`（或对应平台的打包配置）重新编译后启用截图 / 拍照翻译。"
            }
        }
    }
}

#[cfg(feature = "translate")]
mod imp {
    use std::time::Duration;

    #[cfg(any(target_os = "ios", target_os = "android"))]
    use base64::Engine as _;
    use dioxus::prelude::*;
    use image::DynamicImage;

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    use crate::capture;
    use crate::db::{dict_repo::DictRepo, progress_repo::ProgressRepo};
    use crate::state::{now_ts, Settings, SETTINGS_KEY};
    use crate::translate::models::{self, ModelStatus, MT_MODEL, OCR_DET};
    use crate::translate::{engine, Command, Msg, TextRequest, TranslateRequest, LANGS};

    /// Context window for the MT model: smaller on mobile to save RAM.
    fn default_n_ctx() -> u32 {
        if cfg!(any(target_os = "ios", target_os = "android")) {
            1024
        } else {
            2048
        }
    }

    #[derive(Clone, Default)]
    struct Ui {
        status: String,
        ocr: String,
        translation: String,
        busy: bool,
        /// (received, total) while downloading, else None.
        download: Option<(u64, u64)>,
        /// Blocking notice rendered as a centered modal; `None` when nothing is
        /// pending. See [`Alert`].
        alert: Option<Alert>,
    }

    /// A notice the user must not miss. The inline status line sits at the top of
    /// the page, which on mobile is already off-screen once the user scrolled
    /// down to the text box or the model panel — so those messages are also
    /// raised as a modal.
    ///
    /// `scroll_to` names the DOM id to bring into view after the user confirms,
    /// but only on mobile: on desktop the model panel needs no hunting.
    #[derive(Clone)]
    struct Alert {
        message: String,
        scroll_to: Option<&'static str>,
    }

    impl Ui {
        /// Report `msg` inline (status line) *and* as a modal.
        fn raise(&mut self, msg: &str, scroll_to: Option<&'static str>) {
            self.status = msg.to_string();
            self.alert = Some(Alert {
                message: msg.to_string(),
                scroll_to,
            });
        }
    }

    /// DOM ids of the two model download rows, used as mobile scroll targets
    /// after an alert is confirmed. Keep in sync with the rsx below.
    const DL_ROW_OCR: &str = "dl-row-ocr";
    const DL_ROW_MT: &str = "dl-row-mt";

    /// Outcome of a background capture / file-dialog / region-grab thread, pushed
    /// onto a shared queue and drained by the UI poll loop (signals can only be
    /// written from the reactive context, never from the worker thread itself).
    enum CaptureOutcome {
        /// A fresh frame is ready: the image plus a status note for the UI.
        Ready(DynamicImage, String),
        /// The operation failed: an error message to surface in the status line.
        Failed(String),
    }

    /// Minimal standard-base64 encoder (no extra dependency) for the PNG preview.
    fn b64(data: &[u8]) -> String {
        const TBL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
        for chunk in data.chunks(3) {
            let b0 = chunk[0] as u32;
            let b1 = *chunk.get(1).unwrap_or(&0) as u32;
            let b2 = *chunk.get(2).unwrap_or(&0) as u32;
            let n = (b0 << 16) | (b1 << 8) | b2;
            out.push(TBL[((n >> 18) & 0x3f) as usize] as char);
            out.push(TBL[((n >> 12) & 0x3f) as usize] as char);
            if chunk.len() > 1 {
                out.push(TBL[((n >> 6) & 0x3f) as usize] as char);
            } else {
                out.push('=');
            }
            if chunk.len() > 2 {
                out.push(TBL[(n & 0x3f) as usize] as char);
            } else {
                out.push('=');
            }
        }
        out
    }

    /// Decode a data-URL (`data:image/jpeg;base64,...`) into an image.
    /// Strips the MIME header, base64-decodes, then feeds `image::load_from_memory`.
    #[cfg(any(target_os = "ios", target_os = "android"))]
    fn decode_data_url(data_url: &str) -> Result<DynamicImage, String> {
        // Format: "data:<mime>;base64,<payload>"
        let b64_part = data_url
            .find(";base64,")
            .map(|i| &data_url[i + 8..])
            .unwrap_or(data_url); // tolerate a raw base64 string
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64_part)
            .map_err(|e| format!("base64 解码: {e}"))?;
        image::load_from_memory(&bytes).map_err(|e| format!("图片解码: {e}"))
    }

    fn to_data_uri(img: &DynamicImage) -> Option<String> {
        let mut buf: Vec<u8> = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
            .ok()?;
        Some(format!("data:image/png;base64,{}", b64(&buf)))
    }

    /// Crop `img` to the ROI (image-pixel rect), clamped to bounds. Returns the
    /// original image when there is no meaningful selection.
    fn apply_roi(img: DynamicImage, roi: Option<(f64, f64, f64, f64)>) -> DynamicImage {
        let (w, h) = (img.width(), img.height());
        let Some((x, y, rw, rh)) = roi else {
            return img;
        };
        let x = x.max(0.0).min(w as f64) as u32;
        let y = y.max(0.0).min(h as f64) as u32;
        let rw = rw.max(0.0).min((w - x) as f64) as u32;
        let rh = rh.max(0.0).min((h - y) as f64) as u32;
        // Ignore tiny/accidental drags.
        if rw < 8 || rh < 8 {
            return img;
        }
        img.crop_imm(x, y, rw, rh)
    }

    #[component]
    pub fn TranslateInner() -> Element {
        let mut settings: Signal<Settings> = use_context();
        let progress: ProgressRepo = use_context();
        let dict: DictRepo = use_context();

        let mut ui = use_signal(Ui::default);
        let mut target = use_signal(|| settings.read().translate_target_lang.clone());
        let mut src = use_signal(|| settings.read().translate_src_lang.clone());
        let preview = use_signal(|| Option::<String>::None);
        // The captured frame lives in a plain signal (never rendered directly);
        // only its PNG data-URI preview goes into `preview`.
        let frame = use_signal(|| Option::<DynamicImage>::None);
        let mut drag = use_signal(|| Option::<(f64, f64)>::None);
        let mut roi = use_signal(|| Option::<(f64, f64, f64, f64)>::None);

        // Shared inbox for background capture threads. The Arc is created once
        // (`use_signal` persists it across renders) and cloned into each worker,
        // so every thread and the poll loop below all see the same queue.
        let capture_out = use_signal(|| {
            std::sync::Arc::new(std::sync::Mutex::new(
                std::collections::VecDeque::<CaptureOutcome>::new(),
            ))
        });

        // Fold a freshly obtained frame into the preview (reset ROI + results).
        // Only ever called from the reactive poll loop, never from a worker. The
        // captured signals are Copy handles, so we shadow them as `mut` locals to
        // keep this an `Fn` closure (`.set` needs `&mut`, but not of the capture).
        let apply_frame = move |img: DynamicImage, note: String| {
            let mut preview = preview;
            let mut frame = frame;
            let mut roi = roi;
            let mut ui = ui;
            preview.set(to_data_uri(&img));
            frame.set(Some(img));
            roi.set(None);
            ui.with_mut(|u| {
                u.status = note;
                u.ocr.clear();
                u.translation.clear();
            });
        };

        // Poll the engine inbox and fold messages into UI state. Runs for the
        // lifetime of the page; draining is cheap when idle. `futures_timer::Delay`
        // is used for the tick because Dioxus 0.8's desktop prelude ships no
        // timer future (its `TimeoutFuture` is gloo/wasm-only).
        use_future(move || async move {
            loop {
                futures_timer::Delay::new(Duration::from_millis(80)).await;
                for m in engine().drain() {
                    match m {
                        Msg::Log(s) => ui.with_mut(|u| u.status = s),
                        Msg::DownloadProgress { file, received, total } => {
                            ui.with_mut(|u| {
                                u.download = Some((received, total));
                                u.status = format!("下载中：{file}");
                            })
                        }
                        Msg::DownloadDone { file } => {
                            ui.with_mut(|u| {
                                u.download = None;
                                u.status = format!("已就绪：{file}");
                            });
                        }
                        Msg::DownloadError { file, message } => ui.with_mut(|u| {
                            u.download = None;
                            u.status = format!("下载失败（{file}）：{message}");
                        }),
                        Msg::OcrText(lines) => ui.with_mut(|u| {
                            u.ocr = lines.join("\n");
                            u.status = "识别完成，正在翻译…".to_string();
                        }),
                        Msg::Token(piece) => {
                            ui.with_mut(|u| u.translation.push_str(&piece));
                        }
                        Msg::Done(full) => ui.with_mut(|u| {
                            u.busy = false;
                            u.translation = full;
                            u.status = "翻译完成".to_string();
                        }),
                        Msg::Error(e) => ui.with_mut(|u| {
                            u.busy = false;
                            u.status = format!("出错：{e}");
                        }),
                    }
                }
                // Drain background capture / file-open results.
                let done: Vec<CaptureOutcome> = capture_out
                    .read()
                    .clone()
                    .lock()
                    .map(|mut q| q.drain(..).collect())
                    .unwrap_or_default();
                for out in done {
                    match out {
                        CaptureOutcome::Ready(img, note) => apply_frame(img, note),
                        CaptureOutcome::Failed(msg) => ui.with_mut(|u| u.status = msg),
                    }
                }
            }
        });

        // --- mobile file picker (iOS / Android) --------------------------------
        // On WKWebView (iOS) and Android WebView, `<input type="file">` opens the
        // native system picker (Camera / Photo Library / Files). Dioxus's native
        // file bridge doesn't carry file bytes across IPC on those platforms, so
        // we inject a persistent JS listener via `document::eval` that reads each
        // selected file with `FileReader.readAsDataURL` and sends the base64
        // result back through the `dioxus.send()` channel.
        #[cfg(any(target_os = "ios", target_os = "android"))]
        {
            let q_capture = capture_out;
            use_future(move || async move {
                // The JS sets up an onchange handler on the hidden file input.
                // Each file pick produces one `dioxus.send(dataUrl)` message.
                let mut eval = document::eval(
                    r#"
                    const input = document.getElementById('mobile-translate-file-input');
                    if (input) {
                        input.onchange = (e) => {
                            const file = e.target.files && e.target.files[0];
                            if (file) {
                                const reader = new FileReader();
                                reader.onload = () => {
                                    dioxus.send(reader.result);
                                    reader.onload = null;
                                };
                                reader.onerror = () => { dioxus.send(''); };
                                reader.readAsDataURL(file);
                            }
                            input.value = '';
                        };
                    }
                    "#,
                );
                loop {
                    match eval.recv::<String>().await {
                        Ok(data_url) if !data_url.is_empty() => {
                            match decode_data_url(&data_url) {
                                Ok(img) => {
                                    let note = "已选择图片：可拖拽框选区域或用数值选区，或直接点「识别并翻译」处理整图。".to_string();
                                    if let Ok(mut q) = q_capture.read().lock() {
                                        q.push_back(CaptureOutcome::Ready(img, note));
                                    }
                                }
                                Err(e) => {
                                    if let Ok(mut q) = q_capture.read().lock() {
                                        q.push_back(CaptureOutcome::Failed(format!("{}", e)));
                                    }
                                }
                            }
                        }
                        Ok(_) => continue, // empty send = reader error
                        Err(_) => break,   // eval channel closed (page unmount)
                    }
                }
            });
        }

        let cur = ui();
        let target_val = target();
        let src_val = src();
        let ocr_ready = models::group_ready(&models::OCR_FILES);
        let mt_ready = models::status(&MT_MODEL) == ModelStatus::Ready;
        let ocr_det_ready = models::status(&OCR_DET) == ModelStatus::Ready;

        // --- actions -------------------------------------------------------
        // Screen capture / region grab / file-open are desktop-only; on mobile
        // the frame arrives via the file/camera picker (Phase 3). The button
        // group is built in a cfg-gated binding and spliced in as an expression
        // child, because this Dioxus version's rsx! rejects `#[cfg]` on children.
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        let capture_group = {
            // Each entry point runs its blocking work (native dialog / system
            // grab / image decode) on a short-lived thread and pushes the result
            // to `capture_out`, so the webview UI never freezes mid-operation.
            // rfd in particular must NOT be called on the main thread on macOS.
            let do_capture = move |_| {
                let q = capture_out.read().clone();
                std::thread::spawn(move || {
                    let out = match capture::capture_primary() {
                        Ok(img) => CaptureOutcome::Ready(
                            img,
                            "已截屏：拖拽框选或用下方数值选择区域，或直接点「翻译」处理整屏。"
                                .to_string(),
                        ),
                        Err(e) => CaptureOutcome::Failed(format!("截屏失败：{e}")),
                    };
                    if let Ok(mut q) = q.lock() {
                        q.push_back(out);
                    }
                });
            };

            // macOS: system interactive rectangle grab (pixel-exact, any size).
            #[cfg(target_os = "macos")]
            let region_btn = {
                let do_region = move |_| {
                    let q = capture_out.read().clone();
                    std::thread::spawn(move || {
                        let out = match capture::capture_region() {
                            Ok(img) => CaptureOutcome::Ready(
                                img,
                                "已框选区域：点「翻译」处理所选区域。".to_string(),
                            ),
                            // Deliberate cancel: do nothing but report it.
                            Err(capture::RegionError::Cancelled) =>
                                CaptureOutcome::Failed("已取消区域截取".to_string()),
                            // Permission / empty-rect failure: fall back to a
                            // full-screen grab so in-app drag / numeric ROI still
                            // lets the user pick a region.
                            Err(capture::RegionError::Failed(reason)) => {
                                match capture::capture_primary() {
                                    Ok(img) => CaptureOutcome::Ready(
                                        img,
                                        format!(
                                            "无法系统框选（{reason}），已改抓整屏：请拖拽或用 X/Y/宽/高 选区后再点「翻译」。"
                                        ),
                                    ),
                                    Err(e) => CaptureOutcome::Failed(format!(
                                        "区域截取失败：{reason}；全屏抓取也失败：{e}"
                                    )),
                                }
                            }
                        };
                        if let Ok(mut q) = q.lock() {
                            q.push_back(out);
                        }
                    });
                };
                rsx! { button { class: "btn", onclick: do_region, "框选区域" } }
            };
            #[cfg(not(target_os = "macos"))]
            let region_btn = rsx! {};

            // Open a local image via the native file dialog.
            let do_open = move |_| {
                let q = capture_out.read().clone();
                std::thread::spawn(move || {
                    let Some(path) = rfd::FileDialog::new()
                        .set_title("选择要翻译的图片")
                        .add_filter(
                            "图片",
                            &["png", "jpg", "jpeg", "bmp", "webp", "gif", "tif", "tiff"],
                        )
                        .pick_file()
                    else {
                        return; // cancelled
                    };
                    let out = match image::open(&path) {
                        Ok(img) => {
                            let name = path
                                .file_name()
                                .map(|s| s.to_string_lossy().to_string())
                                .unwrap_or_default();
                            CaptureOutcome::Ready(
                                img,
                                format!("已打开图片：{name}。可框选区域后点「翻译」。"),
                            )
                        }
                        Err(e) => CaptureOutcome::Failed(format!("打开图片失败：{e}")),
                    };
                    if let Ok(mut q) = q.lock() {
                        q.push_back(out);
                    }
                });
            };

            rsx! {
                button { class: "btn", onclick: do_capture, "截取屏幕" }
                {region_btn}
                button { class: "btn", onclick: do_open, "打开图片" }
            }
        };
        #[cfg(any(target_os = "ios", target_os = "android"))]
        let capture_group = {
            // Mobile: trigger the hidden file input via JS .click() (WKWebView
            // shows the native Camera / Photo Library / Files action sheet).
            let do_pick = move |_| {
                document::eval(
                    "document.getElementById('mobile-translate-file-input')?.click();",
                );
            };
            rsx! {
                button { class: "btn", onclick: do_pick, "选择图片" }
            }
        };

        let do_translate = move |_| {
            // 模型缺失在点击时就地预检：引擎报的「…（请先下载）」是异步回到顶部
            // 状态行的，移动端用户滚不到那儿。弹窗 + 确认后滚到对应的下载行。
            if !ocr_ready {
                ui.with_mut(|u| {
                    u.raise(
                        "还未下载 OCR 模型（约 21 MB），无法识别图片文字。请在下方「模型」区下载后重试。",
                        Some(DL_ROW_OCR),
                    )
                });
                return;
            }
            if !mt_ready {
                ui.with_mut(|u| {
                    u.raise(
                        "还未下载翻译模型 HY-MT1.5-1.8B（约 1.1 GB），无法翻译。请在下方「模型」区下载后重试。",
                        Some(DL_ROW_MT),
                    )
                });
                return;
            }
            let Some(img) = frame.read().clone() else {
                ui.with_mut(|u| {
                    u.status = if cfg!(any(target_os = "ios", target_os = "android")) {
                        "请先选择图片，或在下方「识别文本」框输入文本后点「翻译此文本」。".to_string()
                    } else {
                        "请先截取屏幕".to_string()
                    }
                });
                return;
            };
            let img = apply_roi(img, roi());
            let s = settings.read().clone();
            engine().send(Command::Translate(TranslateRequest {
                image: img,
                target: s.translate_target_lang.clone(),
                src: s.translate_src_lang.clone(),
                gpu: s.translate_gpu,
                n_ctx: default_n_ctx(),
                resident: s.translate_keep_model_resident,
            }));
            ui.with_mut(|u| {
                u.busy = true;
                u.translation.clear();
                u.ocr.clear();
                u.status = "处理中…".to_string();
            });
        };

        // Translate the (possibly hand-corrected) recognition text directly,
        // without re-running OCR over the screenshot region. This is the fix-up
        // path: edit the textarea, then re-translate exactly what it says.
        let do_translate_text = move |_| {
            // 只需 MT（走的是文本，不跑 OCR），缺失时同样弹窗并指向下载行。
            if !mt_ready {
                ui.with_mut(|u| {
                    u.raise(
                        "还未下载翻译模型 HY-MT1.5-1.8B（约 1.1 GB），无法翻译。请在下方「模型」区下载后重试。",
                        Some(DL_ROW_MT),
                    )
                });
                return;
            }
            let text = ui.read().ocr.clone();
            if text.trim().is_empty() {
                // 这个按钮在页面底部，提示必须弹窗才看得见。
                ui.with_mut(|u| {
                    u.raise(
                        "识别文本为空：请先截屏识别，或在上方文本框直接输入文字。",
                        None,
                    )
                });
                return;
            }
            let s = settings.read().clone();
            engine().send(Command::TranslateText(TextRequest {
                text,
                target: s.translate_target_lang.clone(),
                src: s.translate_src_lang.clone(),
                gpu: s.translate_gpu,
                n_ctx: default_n_ctx(),
                resident: s.translate_keep_model_resident,
            }));
            ui.with_mut(|u| {
                u.busy = true;
                u.translation.clear();
                u.status = "正在翻译当前文本…".to_string();
            });
        };

        let do_download_ocr = move |_| engine().send(Command::DownloadOcr);
        let do_download_mt = move |_| engine().send(Command::DownloadMt);
        let do_unload = move |_| engine().send(Command::Unload);

        let do_copy = move |_| {
            let text = ui.read().translation.clone();
            document::eval(&format!(
                "navigator.clipboard && navigator.clipboard.writeText({});",
                js_string(&text)
            ));
        };

        let do_wordbook = {
            let progress = progress.clone();
            let dict = dict.clone();
            move |_| {
                let source = ui.read().ocr.clone();
                let now = now_ts();
                let mut added = 0usize;
                for token in source.split(|c: char| !c.is_alphabetic()) {
                    let w = token.trim();
                    if w.len() < 2 || !w.chars().all(|c| c.is_ascii_alphabetic()) {
                        continue;
                    }
                    if let Some(word) = dict.get_by_word(w) {
                        progress.add_star(word.id, now);
                        added += 1;
                    }
                }
                ui.with_mut(|u| {
                    u.status = if added > 0 {
                        format!("已将 {added} 个识别到的单词加入生词本")
                    } else {
                        "未在词典中找到可加入的单词".to_string()
                    }
                });
            }
        };

        let persist_lang = {
            let progress = progress.clone();
            move |is_target: bool, v: String| {
                let mut s = settings.read().clone();
                if is_target {
                    s.translate_target_lang = v.clone();
                    target.set(v);
                } else {
                    s.translate_src_lang = v.clone();
                    src.set(v);
                }
                settings.set(s.clone());
                if let Ok(json) = serde_json::to_string(&s) {
                    progress.set_setting(SETTINGS_KEY, &json);
                }
            }
        };

        // ROI drag handlers (image is shown at natural size, so element coords
        // map 1:1 to image pixels). The <img> is made non-draggable so the
        // browser doesn't hijack mousedown+move as a native image drag.
        let on_down = move |e: Event<MouseData>| {
            let c = e.element_coordinates();
            drag.set(Some((c.x, c.y)));
            roi.set(None);
        };
        let on_move = move |e: Event<MouseData>| {
            if let Some((x0, y0)) = drag() {
                let c = e.element_coordinates();
                let x1 = c.x;
                let y1 = c.y;
                roi.set(Some((
                    x0.min(x1),
                    y0.min(y1),
                    (x1 - x0).abs(),
                    (y1 - y0).abs(),
                )));
            }
        };
        let on_up = move |_| drag.set(None);

        // Numeric fine-tuning of the selection (image pixels): guarantees
        // "select position and size" even where drag coordinates are unreliable.
        let set_roi_axis = move |axis: usize, v: f64| {
            let mut r = roi().unwrap_or((0.0, 0.0, 0.0, 0.0));
            match axis {
                0 => r.0 = v,
                1 => r.1 = v,
                2 => r.2 = v,
                _ => r.3 = v,
            }
            roi.set(Some(r));
        };
        let (rx, ry, rw, rh) = roi().unwrap_or((0.0, 0.0, 0.0, 0.0));

        let pct = cur.download.map(|(r, t)| {
            if t > 0 {
                ((r as f64 / t as f64) * 100.0).clamp(0.0, 100.0)
            } else {
                0.0
            }
        });

        let empty_hint = if cfg!(any(target_os = "ios", target_os = "android")) {
            "点击「选择图片」从相册/相机取图开始；也可直接在右下「识别文本」框输入文字后点「翻译此文本」。首次使用需在下方下载模型。"
        } else {
            "点击「截取屏幕」「框选区域」或「打开图片」开始，或在下方下载模型后使用。"
        };

        // Blocking notice: the status line sits above the fold and is invisible to
        // a mobile user who tapped a button at the bottom of the page, so the same
        // text is raised here as a modal. Both platforms get the dialog (one UX);
        // only mobile scrolls to the download row on confirm. Built as an `Element`
        // before the page rsx because `rsx!` children cannot hold `let` bindings
        // (same reason `capture_group` above is built outside).
        let alert_modal = match cur.alert.clone() {
            Some(alert) => {
                let message = alert.message;
                let scroll_target = alert.scroll_to;
                // Tapping the dimmed backdrop just dismisses — no navigation.
                let close = move |_| ui.with_mut(|u| u.alert = None);
                let confirm = move |_| {
                    ui.with_mut(|u| u.alert = None);
                    if cfg!(any(target_os = "ios", target_os = "android")) {
                        if let Some(id) = scroll_target {
                            document::eval(&format!(
                                "var el = document.getElementById('{id}'); if (el) el.scrollIntoView({{ behavior: 'smooth', block: 'center' }});"
                            ));
                        }
                    }
                };
                rsx! {
                    div { class: "alert-mask", onclick: close,
                        div { class: "alert-card",
                            p { class: "alert-msg", "{message}" }
                            div { class: "alert-actions",
                                button { class: "btn primary", onclick: confirm,
                                    if cfg!(any(target_os = "ios", target_os = "android")) { "去下载" } else { "知道了" }
                                }
                            }
                        }
                    }
                }
            }
            None => rsx! {},
        };

        rsx! {
            div { class: "page translate",
                // Hidden file input for the mobile picker (iOS/Android); harmless
                // on desktop where the eval listener is never started.
                input {
                    id: "mobile-translate-file-input",
                    r#type: "file",
                    accept: "image/*",
                    style: "display:none",
                }
                div { class: "page-head",
                    h1 { "翻译" }
                    div { class: "translate-actions",
                        {capture_group}
                        select {
                            class: "select",
                            title: "源语言（可自动识别）",
                            value: "{src_val}",
                            onchange: {
                                let mut persist_lang = persist_lang.clone();
                                move |e| persist_lang(false, e.data().value())
                            },
                            option { key: "auto", value: "auto", "自动识别" }
                            for (val, label) in LANGS {
                                option { key: "{val}", value: "{val}", "{label}" }
                            }
                        }
                        span { class: "muted lang-arrow", "→" }
                        select {
                            class: "select",
                            title: "目标语言",
                            value: "{target_val}",
                            onchange: {
                                let mut persist_lang = persist_lang.clone();
                                move |e| persist_lang(true, e.data().value())
                            },
                            for (val, label) in LANGS {
                                option { key: "{val}", value: "{val}", "{label}" }
                            }
                        }
                        button {
                            class: "btn primary",
                            disabled: cur.busy,
                            onclick: do_translate,
                            if cur.busy { "处理中…" } else { "识别并翻译" }
                        }
                    }
                }

                p { class: "muted status", "{cur.status}" }

                div { class: "translate-grid",
                    // Left/top: screenshot preview + ROI overlay. On desktop the
                    // pane is a flex column: .shot-scroll takes the remaining
                    // height and scrolls the natural-size capture internally,
                    // while .roi-editor sits below it as an always-visible
                    // footer (never overlapping the image). On mobile the pane
                    // itself scrolls and the editor stays in flow under the img.
                    div { class: "translate-preview scroll",
                        if let Some(uri) = preview() {
                            div { class: "shot-scroll",
                                div {
                                    class: "shot",
                                    onmousedown: on_down,
                                    onmousemove: on_move,
                                    onmouseup: on_up,
                                    onmouseleave: on_up,
                                    img { class: "shot-img", src: "{uri}", alt: "截图预览", draggable: "false" }
                                    if let Some((x, y, w, h)) = roi() {
                                        div {
                                            class: "roi",
                                            style: "left:{x}px;top:{y}px;width:{w}px;height:{h}px"
                                        }
                                    }
                                }
                            }
                            // Precise position/size editors (image pixels), the
                            // reliable fallback wherever drag is unavailable.
                            div { class: "roi-editor",
                                span { class: "roi-title", "选区（像素）" }
                                label { class: "roi-field", "X"
                                    input { r#type: "number", min: "0", step: "1", value: "{rx:.0}",
                                        oninput: { let mut f = set_roi_axis; move |e| f(0, e.data().value().parse::<f64>().unwrap_or(0.0)) } }
                                }
                                label { class: "roi-field", "Y"
                                    input { r#type: "number", min: "0", step: "1", value: "{ry:.0}",
                                        oninput: { let mut f = set_roi_axis; move |e| f(1, e.data().value().parse::<f64>().unwrap_or(0.0)) } }
                                }
                                label { class: "roi-field", "宽"
                                    input { r#type: "number", min: "0", step: "1", value: "{rw:.0}",
                                        oninput: { let mut f = set_roi_axis; move |e| f(2, e.data().value().parse::<f64>().unwrap_or(0.0)) } }
                                }
                                label { class: "roi-field", "高"
                                    input { r#type: "number", min: "0", step: "1", value: "{rh:.0}",
                                        oninput: { let mut f = set_roi_axis; move |e| f(3, e.data().value().parse::<f64>().unwrap_or(0.0)) } }
                                }
                                button { class: "btn small ghost", onclick: move |_| roi.set(None), "清除选区" }
                            }
                        } else {
                            p { class: "muted pad", "{empty_hint}" }
                        }
                    }

                    // Right/bottom: OCR source (editable) + streamed translation.
                    div { class: "translate-result",
                        label { class: "field-label", "识别文本（可编辑纠正）" }
                        textarea {
                            class: "input",
                            rows: "4",
                            value: "{cur.ocr}",
                            oninput: move |e| ui.with_mut(|u| u.ocr = e.data().value()),
                        }
                        p { class: "muted hint", "识别有误？直接在上方修改，再点「翻译此文本」即可重新翻译，不会重新截屏识别。" }
                        label { class: "field-label", "译文" }
                        div { class: "output", "{cur.translation}" }
                        div { class: "translate-actions",
                            button {
                                class: "btn small primary",
                                disabled: cur.busy,
                                onclick: do_translate_text,
                                if cur.busy { "翻译中…" } else { "翻译此文本" }
                            }
                            button { class: "btn small", onclick: do_copy, "复制" }
                            button { class: "btn small", onclick: do_wordbook, "加入生词本" }
                        }
                    }
                }

                // Model download / status panel.
                section { class: "setting-group",
                    h3 { "模型" }
                    if let Some(p) = pct {
                        div { class: "progress",
                            div { class: "progress-fill", style: "width:{p:.0}%" }
                        }
                        p { class: "muted hint", "下载中… {p:.0}%" }
                    }
                    div { class: "setting-row", id: "{DL_ROW_OCR}",
                        label { "OCR 模型（约 21 MB）" }
                        if ocr_ready {
                            span { class: "ok", "已就绪" }
                        } else {
                            button { class: "btn small", onclick: do_download_ocr, "下载" }
                        }
                    }
                    div { class: "setting-row", id: "{DL_ROW_MT}",
                        label { "翻译模型 HY-MT1.5-1.8B（约 1.1 GB）" }
                        if mt_ready {
                            span { class: "ok", "已就绪" }
                        } else {
                            button { class: "btn small", onclick: do_download_mt, "下载" }
                        }
                    }
                    div { class: "setting-row",
                        label { "内存" }
                        button { class: "btn small ghost", onclick: do_unload, "卸载模型" }
                    }
                    if !ocr_det_ready {
                        p { class: "muted hint", "首次使用需下载模型；下载后会做完整性校验，仅从官方源获取，不随安装包分发。" }
                    }
                }

                {alert_modal}
            }
        }
    }

    /// Escape + quote a string as a JS string literal.
    fn js_string(s: &str) -> String {
        let mut out = String::with_capacity(s.len() + 2);
        out.push('`');
        for c in s.chars() {
            match c {
                '`' => out.push_str("\\`"),
                '\\' => out.push_str("\\\\"),
                '$' => out.push_str("\\$"),
                c => out.push(c),
            }
        }
        out.push('`');
        out
    }
}

#[cfg(feature = "translate")]
pub use imp::TranslateInner as Translate;
