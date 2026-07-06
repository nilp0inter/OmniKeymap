//! macOS layout extraction using Carbon's `UCKeyTranslate`.
//!
//! On non-macOS hosts this module reports an unsupported error.

use anyhow::{Result, anyhow};

/// Extract a macOS keyboard layout.
pub fn extract(_layout: &str, _variant: Option<&str>) -> Result<omni_keymap_core::LayoutFile> {
    let _ = (_layout, _variant);
    Err(anyhow!("macos extraction is only supported on macOS hosts"))
}