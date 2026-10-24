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
    #[cfg(any(target_os = "ios", target_os = "android"))]
    use crate::translate::TranslatePagesRequest;
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
    /// `license_ack` marks the consent prompt shown before the HY-MT model
    /// download: confirming it persists the acceptance and closes (the user
    /// then triggers the download themselves).
    #[derive(Clone)]
    struct Alert {
        message: String,
        scroll_to: Option<&'static str>,
        license_ack: bool,
    }

    impl Ui {
        /// Report `msg` inline (status line) *and* as a modal.
        fn raise(
            &mut self,
            msg: &str,
            scroll_to: Option<&'static str>,
            license_ack: bool,
        ) {
            self.status = msg.to_string();
            self.alert = Some(Alert {
                message: msg.to_string(),
                scroll_to,
                license_ack,
            });
        }
    }

    /// DOM ids of the two model download rows, used as mobile scroll targets
    /// after an alert is confirmed. Keep in sync with the rsx below.
    const DL_ROW_OCR: &str = "dl-row-ocr";
    const DL_ROW_MT: &str = "dl-row-mt";

    /// DOM id of the OCR/译文 result block — the mobile auto-scroll target
    /// when a translation finishes. Keep in sync with the rsx below.
    const RESULT_BLOCK: &str = "translate-result";

    /// Progress-repo KV key recording that the user accepted the Tencent HY
    /// Community License before the first HY-MT model download (persisted via
    /// `ProgressRepo`, the project convention for simple scalar UI state).
    const MT_LICENSE_KEY: &str = "mt_license_accepted";

    /// HY-MT 社区许可协议全文，以设置页同款手风琴在「模型」区可离线展开阅读
    /// （文本编译期嵌入；条款同时随包归档于 license/HY-MT-LICENSE.txt）。
    const MT_LICENSE_DOCS: &[crate::pages::settings::LicenseDoc] = &[crate::pages::settings::LicenseDoc {
        name: "腾讯 HY 社区许可协议（HY-MT1.5-1.8B）",
        usage: "下载即须同意；点击接受即生效",
        lic: "Tencent HY Community License",
        url: "https://github.com/Tencent-Hunyuan/Hy-MT/blob/main/License.txt",
        text: include_str!("../../license/HY-MT-LICENSE.txt"),
    }];

    /// Outcome of a background capture / file-dialog / region-grab thread, pushed
    /// onto a shared queue and drained by the UI poll loop (signals can only be
    /// written from the reactive context, never from the worker thread itself).
    enum CaptureOutcome {
        /// A fresh frame is ready: the image plus a status note for the UI.
        /// Desktop-only: mobile captures always arrive as `AppendPages` batches.
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        Ready(DynamicImage, String),
        /// A batch of scanned pages (mobile camera shots / multi-select album):
        /// appended to the page collection, with the last one shown.
        #[cfg(any(target_os = "ios", target_os = "android"))]
        AppendPages(Vec<DynamicImage>),
        /// A touch box-select reported by the mobile JS pointer bridge, in
        /// image pixels (x, y, w, h).
        #[cfg(any(target_os = "ios", target_os = "android"))]
        Roi((f64, f64, f64, f64)),
        /// The operation failed: an error message to surface in the status line.
        Failed(String),
    }

    /// Inbox shared with the background capture threads.
    type CaptureQueue =
        std::sync::Arc<std::sync::Mutex<std::collections::VecDeque<CaptureOutcome>>>;

    /// Translate page state that has to outlive the component.
    ///
    /// The router unmounts this page on every tab switch, and a `use_signal`
    /// handle dies with its scope — without hoisting, peeking at another tab and
    /// coming back wiped the screenshot the user just took along with the OCR
    /// text and the translation. These signals are created once in the app root
    /// (see `App`'s providers), so navigating away only *hides* the page.
    ///
    /// `target` / `src` are deliberately not here: they are re-seeded from the
    /// persisted `Settings` on every mount, which yields the same value.
    /// Contexts always outlive the page, so creating the extra signals here is
    /// safe even when this component unmounts and remounts.
    #[derive(Clone, Copy)]
    pub struct PageStore {
        ui: Signal<Ui>,
        preview: Signal<Option<String>>,
        frame: Signal<Option<DynamicImage>>,
        roi: Signal<Option<(f64, f64, f64, f64)>>,
        drag: Signal<Option<(f64, f64)>>,
        capture_out: Signal<CaptureQueue>,
        /// Scanned pages (mobile 拍照/相册批量导入); the frame/preview signals
        /// always mirror the selected entry. A desktop single capture is stored
        /// as a one-page collection, so 「全部页」 stays correct everywhere.
        pages: Signal<Vec<DynamicImage>>,
        /// Index of the page currently shown in the preview.
        sel: Signal<usize>,
        /// Encode slots for the strip thumbnails: one entry per page, each
        /// `None` until the poll loop has JPEG-encoded that page (one per
        /// tick), `Some(Some(uri))` when done, `Some(None)` on encode failure.
        /// Encoding full-size camera photos on every poll tick would be far too
        /// slow, hence the lazy per-slot cache.
        thumbs: Signal<Vec<Option<Option<String>>>>,
    }

    /// Copy handle for page-collection operations inside per-thumbnail click
    /// closures (a `FnMut` closure capturing Signal is not `Copy`, so the
    /// project convention wraps them in a small `Copy` struct).
    /// Signal's mutators need a `mut` binding, so every body starts by
    /// rebinding the Copy handle (same idiom as `apply_frame` below).
    /// Mobile-only: only the scan strip (and `PageOps`) drive the collection.
    #[cfg(any(target_os = "ios", target_os = "android"))]
    #[derive(Clone, Copy)]
    struct PageSel {
        pages: Signal<Vec<DynamicImage>>,
        sel: Signal<usize>,
    }

    #[cfg(any(target_os = "ios", target_os = "android"))]
    impl PageSel {
        /// Make page `i` the current frame: refresh the preview, reset the ROI
        /// and clear the previous results (they belong to the old page).
        fn select(
            self,
            i: usize,
            mut preview: Signal<Option<String>>,
            mut frame: Signal<Option<DynamicImage>>,
            mut roi: Signal<Option<(f64, f64, f64, f64)>>,
            mut ui: Signal<Ui>,
        ) {
            let Some(img) = self.pages.read().get(i).cloned() else { return };
            let mut sel = self.sel;
            sel.set(i);
            preview.set(to_data_uri(&img));
            frame.set(Some(img));
            roi.set(None);
            ui.with_mut(|u| {
                u.ocr.clear();
                u.translation.clear();
            });
        }

        /// Drop one page, clamping the selection; when the last page goes the
        /// preview returns to the empty-state hint.
        fn remove(
            self,
            i: usize,
            mut preview: Signal<Option<String>>,
            mut frame: Signal<Option<DynamicImage>>,
            mut roi: Signal<Option<(f64, f64, f64, f64)>>,
            ui: Signal<Ui>,
        ) {
            if i >= self.pages.read().len() {
                return;
            }
            let mut pages = self.pages;
            pages.with_mut(|p| p.remove(i));
            let len = self.pages.read().len();
            if len == 0 {
                let mut sel = self.sel;
                preview.set(None);
                frame.set(None);
                roi.set(None);
                sel.set(0);
            } else {
                let keep = self.sel.read().min(len - 1);
                self.select(keep, preview, frame, roi, ui);
            }
        }
    }

    /// Copy handle for the per-thumbnail click closures of the scan strip:
    /// built on mobile only, where the strip itself is rendered.
    #[cfg(any(target_os = "ios", target_os = "android"))]
    #[derive(Clone, Copy)]
    struct PageOps {
        inner: PageSel,
        preview: Signal<Option<String>>,
        frame: Signal<Option<DynamicImage>>,
        roi: Signal<Option<(f64, f64, f64, f64)>>,
        ui: Signal<Ui>,
        thumbs: Signal<Vec<Option<Option<String>>>>,
    }

    #[cfg(any(target_os = "ios", target_os = "android"))]
    impl PageOps {
        /// Show page `i` as the current frame (resets its ROI and results).
        fn select(self, i: usize) {
            self.inner.select(i, self.preview, self.frame, self.roi, self.ui);
        }

        /// Drop page `i`; the selection clamp inside `PageSel::remove` re-shows
        /// a surviving page, so the preview never goes blank while pages stay.
        /// The removed tile's thumbnail slot is retired with the page; every
        /// later slot now points at the wrong image, so from `i` on they are
        /// cleared to `None` and the poll loop re-encodes one per tick.
        fn remove(self, i: usize) {
            self.inner.remove(i, self.preview, self.frame, self.roi, self.ui);
            let mut thumbs = self.thumbs;
            thumbs.with_mut(|t| {
                if i < t.len() {
                    t[i..].fill(None);
                }
            });
        }
    }

    impl PageStore {
        /// Build the persistent signals. **Must** be called from the app root
        /// scope, where contexts live for the whole session — signals created in
        /// a page scope are dropped when the router unmounts that page, which is
        /// exactly the bug this struct exists to avoid.
        pub fn provide() -> Self {
            Self {
                ui: Signal::new(Ui::default()),
                preview: Signal::new(None),
                frame: Signal::new(None),
                roi: Signal::new(None),
                drag: Signal::new(None),
                capture_out: Signal::new(std::sync::Arc::new(std::sync::Mutex::new(
                    std::collections::VecDeque::new(),
                ))),
                pages: Signal::new(Vec::new()),
                sel: Signal::new(0),
                thumbs: Signal::new(Vec::new()),
            }
        }
    }

    /// Target width (CSS px) of a scan-strip thumbnail before JPEG re-encode.
    const THUMB_W: u32 = 144;

    /// Small JPEG data-URI for the scan strip; returns `None` on any encode
    /// failure so the thumbnail simply shows as a blank tile.
    fn to_thumb_uri(img: &DynamicImage) -> Option<String> {
        if img.width() == 0 || img.height() == 0 {
            return None;
        }
        let th = img.resize(
            THUMB_W,
            (THUMB_W as f64 * img.height() as f64 / img.width() as f64).max(1.0) as u32,
            image::imageops::FilterType::Triangle,
        );
        let mut buf: Vec<u8> = Vec::new();
        th.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Jpeg)
            .ok()?;
        Some(format!("data:image/jpeg;base64,{}", b64(&buf)))
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

    /// Rotate/flip a picker image per the `UIImageOrientation` value reported
    /// by `UIImagePickerController` (see platform/ios.rs — the JPEG pixels are
    /// sensor-raw and the `image` crate ignores EXIF orientation, so portrait
    /// shots land sideways unless we apply the transform ourselves).
    /// UIImageOrientation maps onto EXIF orientations 1/3/8/6/2/4/5/7; the
    /// arms below are the standard EXIF display transforms.
    #[cfg(target_os = "ios")]
    fn apply_ui_orientation(img: DynamicImage, orientation: u8) -> DynamicImage {
        match orientation {
            1 => img.rotate180(), // down (EXIF 3)
            2 => img.rotate270(), // left (EXIF 8)
            3 => img.rotate90(),  // right (EXIF 6) — the common portrait shot
            4 => img.fliph(),     // upMirrored (EXIF 2)
            5 => img.flipv(),     // downMirrored (EXIF 4)
            6 => img.rotate90().fliph(),  // leftMirrored (EXIF 5, transpose)
            7 => img.rotate270().fliph(), // rightMirrored (EXIF 7, transverse)
            _ => img,             // up (0) or unknown: already upright
        }
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

        // Anything the user could lose by tapping another tab lives in
        // `PageStore` (app-root signals), not in this scope.
        let store: PageStore = use_context();
        let mut ui = store.ui;
        // HY-MT 许可同意标记：首次渲染从进度库 KV 读一次，之后只在确认弹窗里写。
        let mut mt_ok = use_signal(|| progress.get_setting(MT_LICENSE_KEY).as_deref() == Some("1"));
        // 「查看许可全文」手风琴的展开态（仅本页面局部，无需持久化）。
        let mt_license_open = use_signal(|| Option::<&'static str>::None);
        let mut target = use_signal(|| settings.read().translate_target_lang.clone());
        let mut src = use_signal(|| settings.read().translate_src_lang.clone());
        let preview = store.preview;
        // The captured frame lives in a plain signal (never rendered directly);
        // only its PNG data-URI preview goes into `preview`.
        let frame = store.frame;
        let mut drag = store.drag;
        let mut roi = store.roi;

        // Shared inbox for background capture threads. It lives in the store too,
        // so a screenshot captured just before a tab switch is still here to be
        // drained when the page comes back.
        let capture_out = store.capture_out;

        // Fold a freshly obtained frame into the preview (reset ROI + results).
        // A single capture replaces the whole page collection: 截图/打开图片 is
        // a fresh start, not an addition to a previous scan.
        // Only ever called from the reactive poll loop, never from a worker. The
        // captured signals are Copy handles, so we shadow them as `mut` locals to
        // keep this an `Fn` closure (`.set` needs `&mut`, but not of the capture).
        // Desktop-only: mobile frames go through `apply_pages` (scan collection).
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        let apply_frame = move |img: DynamicImage, note: String| {
            let mut preview = preview;
            let mut frame = frame;
            let mut roi = roi;
            let mut ui = ui;
            let mut pages = store.pages;
            let mut sel = store.sel;
            let mut thumbs = store.thumbs;
            roi.set(None);
            preview.set(to_data_uri(&img));
            // A single capture is a one-page collection, so 「翻译全部页」 and the
            // strip logic stay uniform across platforms (strip hides at <2).
            // Clone before the move into `frame` (DynamicImage is not Copy).
            pages.set(vec![img.clone()]);
            sel.set(0);
            thumbs.set(vec![None]);
            frame.set(Some(img));
            ui.with_mut(|u| {
                u.status = note;
                u.ocr.clear();
                u.translation.clear();
            });
        };

        // Append a batch of scanned pages (mobile camera shot / album
        // multi-select) and show the newest one. Same reactive-only contract as
        // `apply_frame`.
        #[cfg(any(target_os = "ios", target_os = "android"))]
        let apply_pages = move |imgs: Vec<DynamicImage>, note: String| {
            let mut pages = store.pages;
            let mut thumbs = store.thumbs;
            let mut ui = ui;
            if imgs.is_empty() {
                return;
            }
            pages.with_mut(|p| p.extend(imgs));
            // Placeholder slots: the poll loop encodes one thumbnail per tick.
            thumbs.with_mut(|t| t.resize(store.pages.read().len(), None));
            PageSel {
                pages: store.pages,
                sel: store.sel,
            }
            .select(
                store.pages.read().len() - 1,
                preview,
                frame,
                roi,
                ui,
            );
            ui.with_mut(|u| u.status = note);
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
                        Msg::Done(full) => {
                            ui.with_mut(|u| {
                                u.busy = false;
                                u.translation = full;
                                u.status = "翻译完成".to_string();
                            });
                            // Mobile: pull the result block into view the moment a
                            // translation finishes — the 识别翻译 button sits above a
                            // tall preview, so the answer would otherwise render off-
                            // screen below the fold. Fired here in the poll loop, NOT
                            // in a `use_effect`: Dioxus 0.8 wraps effect closures in
                            // `use_callback`, which *replaces* the stored closure on
                            // every re-render — any dedupe guard captured inside it is
                            // re-seeded from the just-incremented state before the
                            // effect reruns, so it never sees a change (this killed
                            // two earlier attempts on device). `Done` is a discrete
                            // event, so no guard is needed at all.
                            //
                            // Also avoid plain `scrollIntoView`: the app shell is
                            // `html,body{overflow:hidden}` + `.content{overflow-y:
                            // auto}`, so the real scroller is `.content`. Scroll it
                            // explicitly by rect math, and repeat once after 300ms in
                            // case the layout is still settling (the final translation
                            // text is committed to the DOM right after this eval).
                            #[cfg(any(target_os = "ios", target_os = "android"))]
                            document::eval(&format!(
                                "function __synstudyScrollResult() {{\
                                     var el = document.getElementById('{RESULT_BLOCK}');\
                                     var sc = document.querySelector('.content');\
                                     if (!el) return;\
                                     if (sc) {{\
                                         var d = el.getBoundingClientRect().top - sc.getBoundingClientRect().top;\
                                         sc.scrollTo({{ top: sc.scrollTop + d - 12, behavior: 'smooth' }});\
                                     }} else {{\
                                         el.scrollIntoView({{ behavior: 'smooth', block: 'start' }});\
                                     }}\
                                 }}\
                                 __synstudyScrollResult();\
                                 setTimeout(__synstudyScrollResult, 300);"
                            ));
                        }
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
                        #[cfg(not(any(target_os = "ios", target_os = "android")))]
                        CaptureOutcome::Ready(img, note) => apply_frame(img, note),
                        #[cfg(any(target_os = "ios", target_os = "android"))]
                        CaptureOutcome::AppendPages(imgs) => apply_pages(
                            imgs,
                            "已加入扫描页：可在图上滑动框选当前页，或点「翻译全部页」一次识别整叠照片。"
                                .to_string(),
                        ),
                        #[cfg(any(target_os = "ios", target_os = "android"))]
                        CaptureOutcome::Roi(rect) => {
                            roi.set(Some(rect));
                        }
                        CaptureOutcome::Failed(msg) => ui.with_mut(|u| u.status = msg),
                    }
                }
                // Encode at most one pending strip thumbnail per tick so a batch
                // of fresh camera photos never blocks the UI in one long frame.
                // (Position is computed in its own statement: the read guard on
                // `thumbs` must be dropped before the write below.)
                let pending = store
                    .thumbs
                    .read()
                    .iter()
                    .position(|slot| slot.is_none());
                if let Some(i) = pending {
                    let uri = store.pages.read().get(i).and_then(to_thumb_uri);
                    let mut thumbs = store.thumbs;
                    thumbs.with_mut(|t| {
                        if let Some(slot) = t.get_mut(i) {
                            *slot = Some(uri);
                        }
                    });
                }
            }
        });

        // --- mobile photo input & touch ROI (iOS / Android) ---------------------
        // On WKWebView (iOS) and Android WebView, `<input type="file">` opens the
        // native system picker; with `capture` it goes straight to the camera.
        // Dioxus's native file bridge doesn't carry file bytes across IPC on
        // those platforms, so we inject persistent JS listeners via
        // `document::eval` that read each selected file with
        // `FileReader.readAsDataURL` and send a JSON envelope back through the
        // `dioxus.send()` channel:
        //   {"kind":"file","uris":[dataUrl,...]}  one or many picked photos
        //   {"kind":"roi","x":..,"y":..,"w":..,"h":..}
        //       a pointer (touch) box-select on the preview image, in image
        //       pixels. Mouse drag stays on the Rust handlers (desktop); the JS
        //       listener filters `pointerType == 'mouse'` out so the two paths
        //       never double-report on hybrids.
        #[cfg(any(target_os = "ios", target_os = "android"))]
        {
            let q_capture = capture_out;
            #[derive(serde::Deserialize)]
            struct Bridge {
                #[serde(default)]
                kind: String,
                #[serde(default)]
                uris: Vec<String>,
                #[serde(default)]
                x: f64,
                #[serde(default)]
                y: f64,
                #[serde(default)]
                w: f64,
                #[serde(default)]
                h: f64,
            }
            use_future(move || async move {
                let mut eval = document::eval(
                    r#"
                    const FILES = (input) => {
                        const fs = Array.from(input.files || []);
                        input.value = '';
                        if (!fs.length) return;
                        let pending = fs.length;
                        const uris = new Array(fs.length);
                        fs.forEach((file, i) => {
                            const reader = new FileReader();
                            reader.onload = () => {
                                uris[i] = reader.result;
                                if (--pending === 0) {
                                    dioxus.send(JSON.stringify({ kind: 'file', uris }));
                                }
                            };
                            reader.onerror = () => {
                                if (--pending === 0) {
                                    dioxus.send(JSON.stringify({ kind: 'file', uris: [] }));
                                }
                            };
                            reader.readAsDataURL(file);
                        });
                    };
                    // File picks: a *delegated* change listener on document,
                    // not `input.onchange = …` at eval time — this eval can
                    // race the page's first DOM flush (both travel over the
                    // same async IPC), and a missed binding would leave the
                    // picker silently dead. It also *replaces* the previous
                    // mount's handler: each eval owns a fresh Rust channel, and
                    // a listener closing over the old, dropped one would lose
                    // every later `dioxus.send`.
                    const pick = (e) => {
                        const t = e.target;
                        if (t && t.tagName === 'INPUT'
                            && (t.id === 'mobile-translate-camera-input'
                                || t.id === 'mobile-translate-album-input')) {
                            FILES(t);
                        }
                    };
                    if (window.__synstudyFilePick) {
                        document.removeEventListener('change', window.__synstudyFilePick);
                    }
                    window.__synstudyFilePick = pick;
                    document.addEventListener('change', pick);
                    // Touch box-select over the preview image. The img renders
                    // at its natural pixel size, but page-level pinch zoom (the
                    // one thing users do to aim a ROI on a phone) scales it, so
                    // divide through the rendered/natural ratio to get pixels.
                    // Same replace-on-eval pattern as the file listener above:
                    // a bind-once guard would keep the pointer handlers closed
                    // over a stale, dropped channel after a page remount.
                    if (window.__synstudyRoiHandlers) {
                        const old = window.__synstudyRoiHandlers;
                        document.removeEventListener('pointerdown', old.down);
                        document.removeEventListener('pointermove', old.move);
                        document.removeEventListener('pointerup', old.up);
                        document.removeEventListener('pointercancel', old.cancel);
                    }
                    let anchor = null;
                    const pt = (e) => {
                        const img = e.target.closest && e.target.closest('.shot-img');
                        if (!img) return null;
                        const r = img.getBoundingClientRect();
                        if (!r.width || !img.naturalWidth) return null;
                        const s = img.naturalWidth / r.width;
                        return { x: (e.clientX - r.left) * s, y: (e.clientY - r.top) * s };
                    };
                    const sendBox = (a, b) => dioxus.send(JSON.stringify({
                        kind: 'roi',
                        x: Math.min(a.x, b.x), y: Math.min(a.y, b.y),
                        w: Math.abs(b.x - a.x), h: Math.abs(b.y - a.y),
                    }));
                    const down = (e) => {
                        if (e.pointerType === 'mouse') return;
                        anchor = pt(e);
                        if (anchor) e.preventDefault();
                    };
                    const move = (e) => {
                        if (!anchor || e.pointerType === 'mouse') return;
                        const p = pt(e);
                        if (p) { e.preventDefault(); sendBox(anchor, p); }
                    };
                    const up = (e) => {
                        if (!anchor || e.pointerType === 'mouse') return;
                        const p = pt(e);
                        if (p) sendBox(anchor, p);
                        anchor = null;
                    };
                    const cancel = () => { anchor = null; };
                    document.addEventListener('pointerdown', down, { passive: false });
                    document.addEventListener('pointermove', move, { passive: false });
                    document.addEventListener('pointerup', up);
                    document.addEventListener('pointercancel', cancel);
                    window.__synstudyRoiHandlers = { down, move, up, cancel };
                    "#,
                );
                loop {
                    match eval.recv::<String>().await {
                        Ok(json) => {
                            let b: Bridge = match serde_json::from_str(&json) {
                                Ok(b) => b,
                                Err(_) => continue,
                            };
                            let out = match b.kind.as_str() {
                                "roi" => Some(CaptureOutcome::Roi((b.x, b.y, b.w, b.h))),
                                _ => 'decode: {
                                    let mut imgs = Vec::new();
                                    for uri in &b.uris {
                                        match decode_data_url(uri) {
                                            Ok(img) => imgs.push(img),
                                            Err(e) => break 'decode Some(CaptureOutcome::Failed(e)),
                                        }
                                    }
                                    if imgs.is_empty() {
                                        if b.uris.is_empty() {
                                            None // picker cancelled: stay silent
                                        } else {
                                            Some(CaptureOutcome::Failed(
                                                "图片解码失败".to_string(),
                                            ))
                                        }
                                    } else {
                                        Some(CaptureOutcome::AppendPages(imgs))
                                    }
                                }
                            };
                            if let Some(out) = out {
                                if let Ok(mut q) = q_capture.read().lock() {
                                    q.push_back(out);
                                }
                            }
                        }
                        Err(_) => break, // eval channel closed (page unmount)
                    }
                }
            });
        }

        // ROI overlay position as a percentage of the *natural* frame size.
        // Mobile fits the preview to the pane width (main.css), so px offsets
        // would misalign at the reduced scale; percentages track the rendered
        // box at any scale. Desktop renders 1:1, where % and px coincide.
        let roi_style = {
            let nat = frame
                .read()
                .as_ref()
                .map(|img| (img.width() as f64, img.height() as f64));
            match (roi(), nat) {
                (Some((x, y, w, h)), Some((nw, nh))) if nw > 0.0 && nh > 0.0 => format!(
                    "left:{:.4}%;top:{:.4}%;width:{:.4}%;height:{:.4}%",
                    x / nw * 100.0,
                    y / nh * 100.0,
                    w / nw * 100.0,
                    h / nh * 100.0
                ),
                _ => String::new(),
            }
        };

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
        let (capture_group, batch_group) = {
            // Photo-scan translation: OCR every page in order, then translate
            // the combined text as one document (engine-side
            // `Command::TranslatePages`). One model load serves the whole batch.
            let do_translate_all = move |_| {
                if !ocr_ready {
                    ui.with_mut(|u| {
                        u.raise(
                            "还未下载 OCR 模型（约 21 MB），无法识别图片文字。请在下方「模型」区下载后重试。",
                            Some(DL_ROW_OCR),
                            false,
                        )
                    });
                    return;
                }
                if !mt_ready {
                    ui.with_mut(|u| {
                        u.raise(
                            "还未下载翻译模型 HY-MT1.5-1.8B（约 1.1 GB），无法翻译。请在下方「模型」区下载后重试。",
                            Some(DL_ROW_MT),
                            false,
                        )
                    });
                    return;
                }
                let imgs: Vec<DynamicImage> = store.pages.read().clone();
                if imgs.is_empty() {
                    ui.with_mut(|u| u.status = "请先拍照或从相册选择图片".to_string());
                    return;
                }
                let s = settings.read().clone();
                engine().send(Command::TranslatePages(TranslatePagesRequest {
                    images: imgs,
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
                    u.status = "扫描翻译中…".to_string();
                });
            };
            // Drop the whole scan collection and return to the empty state.
            let do_clear_pages = move |_| {
                let (mut pages, mut thumbs, mut sel) = (store.pages, store.thumbs, store.sel);
                let (mut preview, mut frame, mut roi) = (preview, frame, roi);
                pages.set(Vec::new());
                thumbs.set(Vec::new());
                sel.set(0);
                preview.set(None);
                frame.set(None);
                roi.set(None);
                ui.with_mut(|u| {
                    u.ocr.clear();
                    u.translation.clear();
                    u.status = "已清空扫描页".to_string();
                });
            };
            // 拍照/相册 entry points, split by platform:
            //
            // iOS — native `UIImagePickerController` (see platform/ios.rs).
            // The HTML `<input type="file">` route is dead there: WKWebView
            // silently refuses the file panel for the custom `dioxus://`
            // scheme origin, no matter how the input is activated (all three
            // gesture-preserving variants were verified dead on device). The
            // picked image arrives as JPEG bytes over an mpsc channel; a
            // worker thread decodes it into the same `capture_out` queue the
            // rest of the capture flows use.
            //
            // Android — the HTML inputs stay: wry implements
            // `onShowFileChooser` (ACTION_IMAGE_CAPTURE + FileProvider for the
            // camera, ACTION_GET_CONTENT for the album), so the WebView file
            // panel genuinely works there. `.file-btn` (main.css) renders the
            // input itself as the button; results arrive through the
            // delegated change listener of the JS bridge above (same ids).
            #[cfg(target_os = "ios")]
            let pick_group = {
                let q_pick = capture_out;
                let desktop = dioxus::desktop::use_window();
                let do_pick = move |camera: bool| {
                    let (tx, rx) = std::sync::mpsc::channel::<(Vec<u8>, u8)>();
                    let vc = desktop.ui_view_controller();
                    match crate::platform::pick_image(camera, &vc, tx) {
                        Ok(()) => {
                            let q = q_pick.read().clone();
                            std::thread::spawn(move || {
                                while let Ok((bytes, orientation)) = rx.recv() {
                                    let out = match image::load_from_memory(&bytes) {
                                        Ok(img) => CaptureOutcome::AppendPages(vec![
                                            apply_ui_orientation(img, orientation),
                                        ]),
                                        Err(e) => {
                                            CaptureOutcome::Failed(format!("图片解码失败：{e}"))
                                        }
                                    };
                                    if let Ok(mut q) = q.lock() {
                                        q.push_back(out);
                                    }
                                }
                            });
                        }
                        Err(e) => {
                            if let Ok(mut q) = q_pick.read().lock() {
                                q.push_back(CaptureOutcome::Failed(e));
                            }
                        }
                    }
                };
                let pick_cam = do_pick.clone();
                let pick_album = do_pick;
                rsx! {
                    button { class: "btn", onclick: move |_| pick_cam(true), "拍照" }
                    button { class: "btn", onclick: move |_| pick_album(false), "相册" }
                }
            };
            #[cfg(target_os = "android")]
            let pick_group = rsx! {
                input {
                    class: "btn file-btn",
                    id: "mobile-translate-camera-input",
                    r#type: "file",
                    accept: "image/*",
                    capture: "environment",
                    "data-label": "拍照",
                }
                input {
                    class: "btn file-btn",
                    id: "mobile-translate-album-input",
                    r#type: "file",
                    accept: "image/*",
                    multiple: "true",
                    "data-label": "相册",
                }
            };
            let n_pages = store.pages.read().len();
            // Two separate fragments: 拍照/相册 always sit on the first row,
            // the batch actions join the second row alongside the language
            // selects (see .capture-row in main.css).
            (
                rsx! { {pick_group} },
                rsx! {
                    if n_pages > 1 {
                        button {
                            class: "btn",
                            disabled: cur.busy,
                            onclick: do_translate_all,
                            "翻译全部 {n_pages} 页"
                        }
                        button { class: "btn ghost", onclick: do_clear_pages, "清空" }
                    }
                },
            )
        };
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        let batch_group = rsx! {};

        let do_translate = move |_| {
            // 模型缺失在点击时就地预检：引擎报的「…（请先下载）」是异步回到顶部
            // 状态行的，移动端用户滚不到那儿。弹窗 + 确认后滚到对应的下载行。
            if !ocr_ready {
                ui.with_mut(|u| {
                    u.raise(
                        "还未下载 OCR 模型（约 21 MB），无法识别图片文字。请在下方「模型」区下载后重试。",
                        Some(DL_ROW_OCR),
                        false,
                    )
                });
                return;
            }
            if !mt_ready {
                ui.with_mut(|u| {
                    u.raise(
                        "还未下载翻译模型 HY-MT1.5-1.8B（约 1.1 GB），无法翻译。请在下方「模型」区下载后重试。",
                        Some(DL_ROW_MT),
                        false,
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
                        false,
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
                        false,
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
        // HY-MT 权重适用《腾讯 HY 社区许可协议》（点击接受即生效），首次下载前
        // 必须先展示条款要点并取得明确同意；同意状态持久化，之后不再重复询问。
        // 弹窗内逐字列出协议 §3(d) 要求随分发附带的法定 NOTICE，并按其要求披露
        // 实际提供方为 Synstudy（与 license/HY-MT-NOTICE.txt 保持一致）。
        let do_download_mt = move |_| {
            if !*mt_ok.peek() {
                ui.with_mut(|u| {
                    u.raise(
                        "\u{300c}翻译模型 HY-MT1.5-1.8B\u{300d}适用《腾讯 HY 社区许可协议》（Tencent HY Community License Agreement），并非 MIT / Apache 等标准开源许可。\n\
                         完整条款可在下方「模型」区的「查看许可全文」展开阅读，或访问官方仓库查看。\n\
                         要点：本应用面向授权地域（不含欧盟 / 英国 / 韩国）内的简体中文用户；月活跃用户超 1 亿须另行向腾讯申请授权；不得将本应用置于腾讯名义或造成官方关联印象；下载源为 Synstudy 托管的 synvek/HY-MT1.5-1.8B-GGUF 量化镜像（非腾讯官方原始文件）。\n\
                         协议 §3(d) 法定声明：Tencent HY is licensed under the Tencent HY Community License Agreement, Copyright © 2025 Tencent. All Rights Reserved. The trademark rights of “Tencent HY” are owned by Tencent or its affiliate. 本功能与模型镜像的实际提供方为 Synstudy，与腾讯无隶属、关联、赞助或背书关系。\n\
                         点击「同意并继续」即表示你已阅读并同意该协议。",
                        None,
                        true,
                    )
                });
                return;
            }
            engine().send(Command::DownloadMt);
        };
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

        // Fullscreen viewer state. Tapping the (mobile-fitted, small) preview
        // opens a full-viewport overlay where the same image is shown as large
        // as the screen allows and box-select keeps working: the desktop mouse
        // handlers bind to `.fs-frame` (image stays 1:1 there, viewer scrolls),
        // and the mobile JS touch bridge matches `.shot-img`, which the
        // fullscreen image also carries (ratio conversion included).
        let mut fullscreen = use_signal(|| false);
        // Manual 90° rotation of the current page (fullscreen viewer buttons):
        // fixes photos whose embedded orientation was missing or wrong, and
        // keeps the stored page, preview, frame and thumbnail in sync. The old
        // ROI is dropped since the axes swap.
        let do_rotate = move |clockwise: bool| {
            let idx = *store.sel.read();
            let (mut pages, mut thumbs) = (store.pages, store.thumbs);
            let (mut preview, mut frame, mut roi) = (preview, frame, roi);
            let Some(img) = pages.read().get(idx).cloned() else {
                return;
            };
            let rotated = if clockwise {
                img.rotate90()
            } else {
                img.rotate270()
            };
            pages.with_mut(|p| {
                if let Some(slot) = p.get_mut(idx) {
                    *slot = rotated.clone();
                }
            });
            thumbs.with_mut(|t| {
                if let Some(slot) = t.get_mut(idx) {
                    *slot = None; // poll loop re-encodes one thumb per tick
                }
            });
            preview.set(to_data_uri(&rotated));
            frame.set(Some(rotated));
            roi.set(None);
        };
        let rot_ccw = do_rotate.clone();
        let rot_cw = do_rotate;
        // Mobile-only trigger: on desktop the image click is owned by ROI
        // dragging, and a small stray drag would otherwise pop the viewer.
        #[cfg(any(target_os = "ios", target_os = "android"))]
        let open_fs = move |_: Event<MouseData>| fullscreen.set(true);
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        let open_fs = move |_: Event<MouseData>| {};

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
            "点「拍照」直接取景拍摄（每拍一张加入扫描集），或「相册」一次多选多张；也可直接在右下「识别文本」框输入文字后点「翻译此文本」。首次使用需在下方下载模型。"
        } else {
            "点击「截取屏幕」「框选区域」或「打开图片」开始，或在下方下载模型后使用。"
        };

        // Scan strip (≥2 pages): tap a thumbnail to switch the current frame,
        // × removes one page. Built as an `Element` before the page rsx because
        // rsx! children cannot hold `let` bindings (same pattern as the alert
        // modal below). Mobile-gated: on desktop the capture pipeline only ever
        // holds a one-page collection, so nothing is constructed there.
        #[cfg(any(target_os = "ios", target_os = "android"))]
        let scan_strip = {
            let page_ops = PageOps {
                inner: PageSel {
                    pages: store.pages,
                    sel: store.sel,
                },
                preview,
                frame,
                roi,
                ui,
                thumbs: store.thumbs,
            };
            // (is_current, 1-based page number, thumbnail URI once encoded).
            let strip_items: Vec<(bool, usize, Option<String>)> = {
                let sel = *store.sel.read();
                let thumbs = store.thumbs.read();
                (0..store.pages.read().len())
                    .map(|i| {
                        (
                            i == sel,
                            i + 1,
                            thumbs.get(i).and_then(|s| s.clone()).flatten(),
                        )
                    })
                    .collect()
            };
            if strip_items.len() > 1 {
                let count = strip_items.len();
                rsx! {
                    div { class: "scan-strip",
                        span { class: "scan-label", "扫描 {count} 页" }
                        for (i, (active, page_no, uri)) in strip_items.iter().enumerate() {
                            div { key: "{page_no}", class: "scan-tile",
                                button {
                                    class: if *active { "scan-thumb active" } else { "scan-thumb" },
                                    disabled: cur.busy,
                                    onclick: move |_| page_ops.select(i),
                                    if let Some(u) = uri.as_ref() {
                                        img { class: "scan-thumb-img", src: "{u}", alt: "第 {page_no} 页" }
                                    }
                                    span { class: "scan-thumb-no", "{page_no}" }
                                }
                                button {
                                    class: "scan-x",
                                    title: "移除此页",
                                    disabled: cur.busy,
                                    onclick: move |_| page_ops.remove(i),
                                    "×"
                                }
                            }
                        }
                    }
                }
            } else {
                rsx! {}
            }
        };
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        let scan_strip = rsx! {};

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
                let license_ack = alert.license_ack;
                // Tapping the dimmed backdrop just dismisses — no navigation.
                let close = move |_| ui.with_mut(|u| u.alert = None);
                // On the consent alert, confirming *is* the acceptance: persist
                // first, then close; the user then taps 「下载」 themselves (the
                // gate now passes). Ordinary alerts also close + scroll.
                let confirm = move |_| {
                    if license_ack {
                        progress.set_setting(MT_LICENSE_KEY, "1");
                        mt_ok.set(true);
                    }
                    ui.with_mut(|u| u.alert = None);
                    if cfg!(any(target_os = "ios", target_os = "android")) {
                        if let Some(id) = scroll_target {
                            document::eval(&format!(
                                "var el = document.getElementById('{id}'); if (el) el.scrollIntoView({{ behavior: 'smooth', block: 'center' }});"
                            ));
                        }
                    }
                };
                // rsx! children cannot hold `let` bindings, so the button choice
                // is computed outside the tree.
                let alert_btn = if license_ack {
                    rsx! { button { class: "btn primary", onclick: confirm, "同意并继续" } }
                } else {
                    rsx! {
                        button { class: "btn primary", onclick: confirm,
                            if cfg!(any(target_os = "ios", target_os = "android")) { "去下载" } else { "知道了" }
                        }
                    }
                };
                rsx! {
                    div { class: "alert-mask", onclick: close,
                        div { class: "alert-card",
                            p { class: "alert-msg", "{message}" }
                            div { class: "alert-actions",
                                {alert_btn}
                            }
                        }
                    }
                }
            }
            None => rsx! {},
        };

        rsx! {
            div { class: "page translate",
                div { class: "page-head",
                    h1 { "翻译" }
                    div { class: "translate-actions",
                        // Mobile wraps deliberately onto two rows: this first
                        // row holds only 拍照/相册; the second row carries the
                        // batch actions, language selects and translate
                        // buttons. Desktop keeps one inline row (display:
                        // contents on .capture-row).
                        div { class: "capture-row", {capture_group} }
                        {batch_group}
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
                                    img {
                                        class: "shot-img",
                                        src: "{uri}",
                                        alt: "截图预览",
                                        draggable: "false",
                                        onclick: open_fs,
                                    }
                                    if !roi_style.is_empty() {
                                        div { class: "roi", style: "{roi_style}" }
                                    }
                                }
                            }
                            // Page strip of a multi-shot scan (mobile only);
                            // sits between the image and the ROI editors so the
                            // selection always refers to the visible frame.
                            {scan_strip}
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
                    div { class: "translate-result", id: "{RESULT_BLOCK}",
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
                        label { "翻译模型 HY-MT1.5-1.8B（约 1.1 GB，Synstudy 托管的量化镜像）" }
                        if mt_ready {
                            span { class: "ok", "已就绪" }
                        } else {
                            button { class: "btn small", onclick: do_download_mt, "下载" }
                        }
                    }
                    // 下载前必读的社区许可协议全文：就地可展开，与关于页同款组件。
                    div { class: "muted hint", "下载前请阅读并同意适用许可（腾讯 HY 社区许可协议，非标准开源许可）。" }
                    {crate::pages::settings::license_docs_view(MT_LICENSE_DOCS, mt_license_open)}
                    div { class: "setting-row",
                        label { "内存" }
                        button { class: "btn small ghost", onclick: do_unload, "卸载模型" }
                    }
                    if !ocr_det_ready {
                        p { class: "muted hint", "首次使用需下载模型。" }
                    }
                }

                // Fullscreen preview + box-select overlay (tap the preview
                // image on mobile to open; tap the dimmed surround or 「关闭」
                // to leave). ROI edits made here flow through the same signal,
                // so the selection is still there after closing.
                if fullscreen() {
                    if let Some(uri) = preview() {
                        div {
                            class: "fs-viewer",
                            onclick: move |_| fullscreen.set(false),
                            div { class: "fs-bar",
                                span { class: "fs-hint", "在图上拖动即可框选区域，点空白处关闭" }
                                div { class: "fs-btns",
                                    button { class: "btn small", onclick: move |_| rot_ccw(false), "⟲ 左转" }
                                    button { class: "btn small", onclick: move |_| rot_cw(true), "⟳ 右转" }
                                    button {
                                        class: "btn small",
                                        onclick: move |_| fullscreen.set(false),
                                        "关闭"
                                    }
                                }
                            }
                            div {
                                class: "fs-frame",
                                onclick: |e| e.stop_propagation(),
                                onmousedown: on_down,
                                onmousemove: on_move,
                                onmouseup: on_up,
                                onmouseleave: on_up,
                                img {
                                    class: "shot-img fs-img",
                                    src: "{uri}",
                                    alt: "全屏预览",
                                    draggable: "false",
                                }
                                if !roi_style.is_empty() {
                                    div { class: "roi", style: "{roi_style}" }
                                }
                            }
                        }
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
pub use imp::{PageStore, TranslateInner as Translate};
