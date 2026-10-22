//! iOS backend.
//!
//! Uses `objc2` + `objc2-foundation` to reach `NSHomeDirectory()` for the
//! sandbox Documents folder and `Bundle.main.resourceURL` for read-only
//! resources shipped inside the `.app`/`.ipa`.
//!
//! On non-iOS targets this module is inert; the parent `#[cfg]` in `mod.rs`
//! strips all calls to it.

// The stub `not(target_os = "ios")` variants below exist purely so this file
// still typechecks when the crate is compiled for desktop or Android.
#![allow(dead_code)]

use std::path::PathBuf;

/// Absolute path of `NSHomeDirectory()/Documents` — the writable sandbox area
/// that survives app upgrades and is exposed in the Files app.
#[cfg(target_os = "ios")]
pub fn documents_dir() -> PathBuf {
    use objc2_foundation::NSHomeDirectory;
    let home = NSHomeDirectory();
    let home_str: String = home.to_string();
    PathBuf::from(home_str).join("Documents")
}

#[cfg(not(target_os = "ios"))]
pub fn documents_dir() -> PathBuf {
    PathBuf::from(".")
}

/// `Bundle.main.resourceURL.path()` — where the packer drops `synstudy.db`
/// alongside the executable inside the app bundle.
#[cfg(target_os = "ios")]
pub fn bundle_resource_dir() -> Option<PathBuf> {
    use objc2_foundation::NSBundle;
    // `mainBundle()` returns a non-null `Retained<NSBundle>`; `resourceURL()`
    // and `path()` are the only fallible steps.
    let bundle = NSBundle::mainBundle();
    let url = bundle.resourceURL()?;
    let path = url.path()?;
    Some(PathBuf::from(path.to_string()))
}

#[cfg(not(target_os = "ios"))]
pub fn bundle_resource_dir() -> Option<PathBuf> {
    None
}

/// Bundled read-only dictionary database inside the `.ipa`.
#[cfg(target_os = "ios")]
pub fn bundle_dict_db() -> Option<PathBuf> {
    let dir = bundle_resource_dir()?;
    let cand = dir.join("synstudy.db");
    if cand.exists() {
        Some(cand)
    } else {
        None
    }
}

#[cfg(not(target_os = "ios"))]
pub fn bundle_dict_db() -> Option<PathBuf> {
    None
}

// ---------------------------------------------------------------------------
// Native image picker (camera / photo library)
//
// `<input type="file">` is dead on iOS in this app: the page is served from
// the custom `dioxus://` URL scheme, and iOS WebKit silently refuses to open
// the file panel for such an origin — verified on device for every activation
// path (programmatic click via eval, label[for] forwarding, and tapping the
// input itself). Unlike Android, iOS WKWebView also exposes no delegate hook
// to intercept the chooser. So the translate UI calls into here instead and
// we present `UIImagePickerController` from Rust. The picked image crosses
// back as JPEG bytes on `tx`; the caller decodes them like any other capture.
// ---------------------------------------------------------------------------

/// Present the system image picker (`camera` = rear camera, otherwise the
/// photo library) from `presenter` and send the picked image back as
/// `(JPEG bytes, UIImageOrientation)` on `tx`. The JPEG pixels are sensor-raw
/// (orientation is metadata only, which the `image` crate ignores), so the
/// caller must rotate per the orientation value — see `apply_ui_orientation`
/// in the translate page. Must be called on the main thread (Dioxus event
/// handlers are). Errors are returned synchronously; a cancelled pick simply
/// sends nothing.
#[cfg(target_os = "ios")]
pub fn pick_image(
    camera: bool,
    presenter: &objc2_ui_kit::UIViewController,
    tx: std::sync::mpsc::Sender<(Vec<u8>, u8)>,
) -> Result<(), String> {
    image_picker::pick(camera, presenter, tx)
}

#[cfg(target_os = "ios")]
mod image_picker {
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::{define_class, msg_send, MainThreadMarker, MainThreadOnly};
    use objc2_foundation::{NSDictionary, NSObject, NSObjectProtocol};
    use objc2_ui_kit::{
        UIImagePickerController, UIImagePickerControllerDelegate, UIImagePickerControllerInfoKey,
        UIImagePickerControllerOriginalImage, UIImagePickerControllerSourceType,
        UINavigationControllerDelegate, UIImage, UIViewController,
    };
    use std::sync::mpsc::Sender;
    use std::sync::{Mutex, OnceLock};

    /// Channel of the in-flight pick: `(JPEG bytes, UIImageOrientation)`.
    /// The delegate is a process-wide singleton
    /// (`UIImagePickerController.delegate` is a *weak* property, so the object
    /// must outlive every presentation), hence the per-call sender is swapped
    /// through this slot instead of an ivar.
    static TX: Mutex<Option<Sender<(Vec<u8>, u8)>>> = Mutex::new(None);

    fn send_bytes(bytes: Vec<u8>, orientation: u8) {
        if let Ok(mut slot) = TX.lock() {
            if let Some(tx) = slot.take() {
                let _ = tx.send((bytes, orientation));
            }
        }
    }

    /// `Retained<PickerDelegate>` is `!Send + !Sync` (MainThreadOnly); the
    /// singleton is only ever touched from `pick` and the delegate callbacks,
    /// both of which run on the main thread, so the `Sync` needed for `static`
    /// is sound here.
    struct DelegateHolder(Retained<PickerDelegate>);
    // SAFETY: see above — main-thread-only access.
    unsafe impl Send for DelegateHolder {}
    unsafe impl Sync for DelegateHolder {}

    define_class!(
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "SynstudyImagePickerDelegate"]
        struct PickerDelegate;

        unsafe impl NSObjectProtocol for PickerDelegate {}

        unsafe impl UIImagePickerControllerDelegate for PickerDelegate {
            #[unsafe(method(imagePickerController:didFinishPickingMediaWithInfo:))]
            unsafe fn did_finish(
                &self,
                picker: &UIImagePickerController,
                info: &NSDictionary<UIImagePickerControllerInfoKey, AnyObject>,
            ) {
                picker.dismissViewControllerAnimated_completion(true, None);
                // `OriginalImage` holds the picked UIImage. Send the JPEG's
                // sensor-raw pixels *plus* the UIImageOrientation value: no
                // encoder-side baking (png_representation left portrait shots
                // sideways on device), the Rust side rotates deterministically
                // instead. JPEG also keeps a 12 MP shot at a few MB — PNG
                // would be tens.
                let key: &'static UIImagePickerControllerInfoKey =
                    unsafe { UIImagePickerControllerOriginalImage };
                let img = info
                    .objectForKey(key)
                    .and_then(|obj| obj.downcast::<UIImage>().ok());
                if let Some(img) = img {
                    let orientation = img.imageOrientation().0 as u8;
                    if let Some(data) = img.jpeg_representation(0.9) {
                        send_bytes(data.to_vec(), orientation);
                    }
                }
            }

            #[unsafe(method(imagePickerControllerDidCancel:))]
            fn did_cancel(&self, picker: &UIImagePickerController) {
                picker.dismissViewControllerAnimated_completion(true, None);
            }
        }

        unsafe impl UINavigationControllerDelegate for PickerDelegate {}
    );

    impl PickerDelegate {
        /// `define_class!` generates no constructor; this is the plain
        /// `alloc`/`init` pair (no ivars — the channel lives in `TX`). Plain
        /// `msg_send![this, init]` (not `super`): the `super` form wants a
        /// `PartialInit` receiver, which only `set_ivars` produces.
        fn new(mtm: MainThreadMarker) -> Retained<Self> {
            let this = Self::alloc(mtm);
            unsafe { msg_send![this, init] }
        }
    }

    #[allow(deprecated)] // PhotoLibrary: PHPicker needs objc2-photos-ui (unavailable offline); the deprecated source still works and requires no permission.
    pub fn pick(
        camera: bool,
        presenter: &UIViewController,
        tx: Sender<(Vec<u8>, u8)>,
    ) -> Result<(), String> {
        let mtm = MainThreadMarker::new().ok_or("必须在主线程调用系统选图器")?;
        let source = if camera {
            UIImagePickerControllerSourceType::Camera
        } else {
            UIImagePickerControllerSourceType::PhotoLibrary
        };
        if !UIImagePickerController::isSourceTypeAvailable(source, mtm) {
            return Err(if camera {
                "此设备没有可用相机".to_string()
            } else {
                "此设备没有可用相册".to_string()
            });
        }
        // Create the delegate once and leak it into a static: the picker's
        // `delegate` property is weak, so dropping our strong reference would
        // leave it dangling mid-presentation.
        static DELEGATE: OnceLock<DelegateHolder> = OnceLock::new();
        let delegate = DELEGATE.get_or_init(|| DelegateHolder(PickerDelegate::new(mtm)));
        // Register the channel before presenting; an abandoned pick's sender
        // is simply replaced.
        match TX.lock() {
            Ok(mut slot) => *slot = Some(tx),
            Err(_) => return Err("选图器状态异常".to_string()),
        }
        let picker = UIImagePickerController::new(mtm);
        picker.setSourceType(source);
        let any: &AnyObject = unsafe { &*(Retained::as_ptr(&delegate.0) as *const AnyObject) };
        // SAFETY: `any` is our PickerDelegate, which conforms to both
        // UIImagePickerControllerDelegate and UINavigationControllerDelegate.
        unsafe { picker.setDelegate(Some(any)) };
        presenter.presentViewController_animated_completion(&picker, true, None);
        Ok(())
    }
}
