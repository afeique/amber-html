//! UniFFI binding generator (Plans.md 6.1). Generates foreign-language bindings
//! from the built `amber_core` cdylib's embedded metadata, e.g.:
//!
//! ```sh
//! cargo build -p amber-core
//! cargo run -p uniffi-bindgen -- generate \
//!     --library target/debug/libamber_core.dylib \
//!     --language python --out-dir bindings/python
//! ```
fn main() {
    uniffi::uniffi_bindgen_main()
}
