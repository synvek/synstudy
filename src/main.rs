//! Thin desktop binary shim. All logic lives in the `synstudy` library crate
//! so the same entry can be reused by the iOS staticlib and the Android cdylib
//! (see `src/lib.rs`).

fn main() {
    synstudy::run();
}
