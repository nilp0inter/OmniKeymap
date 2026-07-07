//! macOS layout extraction using Carbon's `UCKeyTranslate`.
//!
//! For each W3C key, we map the macOS virtual keycode to a character via `UCKeyTranslate` under
//! four modifier states by setting the `modifierKeyState` bitmask:
//!
//! 1. None
//! 2. Shift (`shiftKey`)
//! 3. AltGr/Option (`optionKey`)
//! 4. Shift + Option
//!
//! `UCKeyTranslate` takes a `*const UCKeyboardLayout` obtained from the active input source via
//! `TISGetInputSourceProperty(kTISPropertyUnicodeKeyLayoutData)`, and a `deadKeyState` pointer
//! that is non-zero when a dead key is pending. We detect dead keys (return value
//! `kUCKeyOutputNoKey`/dead state) and simulate the dead key followed by every other standard
//! key to discover the composed character.
//!
//! `--all` enumerates installed keyboard layouts via `TISCreateInputSourceList` filtered to
//! `kTISTypeKeyboardLayout`, extracting each by activating it as the current input source.

use anyhow::{Result, anyhow};

#[cfg(target_os = "macos")]
use anyhow::Context;
#[cfg(target_os = "macos")]
use std::collections::HashMap;

#[cfg(target_os = "macos")]
use crate::w3c_keys::{W3C_KEYS, W3cKey};

/// The four modifier states we probe. The bitmask values match Carbon's `modifierKeyState`
/// argument to `UCKeyTranslate`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(any(target_os = "macos", test))]
enum ModState {
    None,
    Shift,
    Option,
    ShiftOption,
}

#[cfg(any(target_os = "macos", test))]
impl ModState {
    #[cfg(any(target_os = "macos", test))]
    fn carbon_modifier_mask(self) -> u32 {
        // Carbon modifier key bits (from Events.h / HIToolbox/UCKeyTranslate.h):
        //   activeModifiers bit 0 = alphaShift (caps lock)
        //   bit 1 = shiftKey
        //   bit 2 = controlKey
        //   bit 3 = optionKey (Alt/Option)
        //   bit 4 = cmdKey
        match self {
            ModState::None => 0,
            ModState::Shift => 1 << 1,       // shiftKey
            ModState::Option => 1 << 3,      // optionKey
            ModState::ShiftOption => (1 << 1) | (1 << 3),
        }
    }

    fn w3c_modifiers(self) -> Vec<String> {
        match self {
            ModState::None => vec![],
            ModState::Shift => vec!["Shift".to_string()],
            // macOS "Option" maps to the W3C "Alt" modifier (Option is the macOS Alt key).
            ModState::Option => vec!["Alt".to_string()],
            ModState::ShiftOption => {
                vec!["Shift".to_string(), "Alt".to_string()]
            }
        }
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::*;
    use core_foundation::{
        base::{CFRelease, TCFType},
        string::{CFString, CFStringRef},
    };
    use std::ffi::c_void;
    use std::os::raw::{c_uint, c_ulong, c_void as c_void_t};
    use std::ptr::null;

    // ---- Raw Carbon / HIToolbox FFI -------------------------------------------

    // UCKeyTranslate return constants (from HIToolbox/UnicodeUtilities.h).
    const K_UC_KEY_OUTPUT_NO_KEY: u32 = 0xFFFFFFFE;
    const K_UC_KEY_OUTPUT_DEAD_KEY: u32 = 0xFFFFFFFF;


    // Carbon InputSource type for keyboard layouts.
    const K_TIS_TYPE_KEYBOARD_LAYOUT: &str = "TISKeyboardLayout";

    type TISInputSourceRef = *mut c_void;
    type TISInputSourceIteratorRef = *mut c_void;

    #[repr(C)]
    struct CFRange {
        location: c_ulong,
        length: c_ulong,
    }

    // UCKeyboardLayout is an opaque struct; we only need a pointer to it.
    #[allow(non_camel_case_types)]
    type UCKeyboardLayout = c_void_t;

    #[allow(non_camel_case_types)]
    type OptionBits = u32;

    // extern "C" bindings into HIToolbox.framework.
    #[link(name = "Carbon", kind = "framework")]
    extern "C" {
        fn UCKeyTranslate(
            key_layout_ptr: *const UCKeyboardLayout,
            key_code: u16,
            key_action: u16,
            modifier_key_state: u32,
            keyboard_type: u32,
            key_translate_options: OptionBits,
            dead_key_state: *mut u32,
            max_string_length: c_uint,
            actual_string_length: *mut c_uint,
            unicode_string: *mut u16,
        ) -> u32;

        fn LMGetKbdType() -> u8;

        fn TISGetInputSourceProperty(
            source: TISInputSourceRef,
            property_key: CFStringRef,
        ) -> *mut c_void;

        fn TISCreateInputSourceList(
            filter: CFDictionaryRef,
            include_all: Boolean,
        ) -> TISInputSourceIteratorRef;

        fn TISSelectInputSource(source: TISInputSourceRef) -> u32;

        fn TISGetInputSourceProperty_CFString(
            source: TISInputSourceRef,
            property_key: CFStringRef,
        ) -> CFStringRef;

        fn TISCopyCurrentKeyboardLayoutInputSource() -> TISInputSourceRef;

        fn TISEnableInputSourceProperty(
            source: TISInputSourceRef,
            property_key: CFStringRef,
        ) -> u32;

        // Exported CFString constants from HIToolbox.framework. These are the actual
        // constant objects TISGetInputSourceProperty compares against; building a CFString
        // with the same text content is NOT equivalent (property lookup is by identity).
        static kTISPropertyInputSourceID: CFStringRef;
        static kTISPropertyLocalizedName: CFStringRef;
        static kTISPropertyUnicodeKeyLayoutData: CFStringRef;

        fn CFArrayGetCount(the_array: *const c_void) -> c_ulong;
        fn CFArrayGetValueAtIndex(
            the_array: *const c_void,
            idx: c_ulong,
        ) -> *const c_void;
        fn CFDataGetBytePtr(the_data: *const c_void) -> *const u8;
        fn CFDataGetLength(the_data: *const c_void) -> c_ulong;
    }

    type Boolean = u8;
    type CFDictionaryRef = *const c_void;


    /// Get the `UCKeyboardLayout*` for the given input source, if it has one.
    fn keyboard_layout_data(source: TISInputSourceRef) -> Option<*const UCKeyboardLayout> {
        unsafe {
            let raw = TISGetInputSourceProperty(source, kTISPropertyUnicodeKeyLayoutData);
            if raw.is_null() {
                return None;
            }
            let byte_ptr = CFDataGetBytePtr(raw as *const c_void);
            if byte_ptr.is_null() {
                return None;
            }
            Some(byte_ptr as *const UCKeyboardLayout)
        }
    }

    /// Translate a single key under a modifier state via `UCKeyTranslate`.
    /// Returns `None` if no character is produced.
    fn translate(
        layout: *const UCKeyboardLayout,
        keycode: u16,
        mstate: ModState,
        dead_state: &mut u32,
    ) -> Option<String> {
        let mut buf = [0u16; 16];
        let mut actual_len: c_uint = 0;
        let kbd_type = unsafe { LMGetKbdType() } as u32;
        let ret = unsafe {
            UCKeyTranslate(
                layout,
                keycode,
                0, // kUCKeyActionDisplay = 0
                mstate.carbon_modifier_mask(),
                kbd_type,
                0,
                dead_state,
                buf.len() as c_uint,
                &mut actual_len as *mut c_uint,
                buf.as_mut_ptr(),
            )
        };
        if ret != 0 {
            return None;
        }
        if actual_len == 0 {
            return None;
        }
        let s = String::from_utf16_lossy(&buf[..actual_len as usize]);
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    }

    /// Detect a dead key: `UCKeyTranslate` returns `kUCKeyOutputNoKey` and the dead state
    /// pointer is set to a non-zero value.
    fn is_dead_key(
        layout: *const UCKeyboardLayout,
        keycode: u16,
        mstate: ModState,
    ) -> bool {
        let mut dead_state: u32 = 0;
        let mut buf = [0u16; 16];
        let mut actual_len: c_uint = 0;
        let kbd_type = unsafe { LMGetKbdType() } as u32;
        let _ret = unsafe {
            UCKeyTranslate(
                layout,
                keycode,
                0,
                mstate.carbon_modifier_mask(),
                kbd_type,
                0,
                &mut dead_state as *mut u32,
                buf.len() as c_uint,
                &mut actual_len as *mut c_uint,
                buf.as_mut_ptr(),
            )
        };
        // A dead key sets the dead state and produces no output string.
        dead_state != 0 && actual_len == 0
    }

    /// Resolve a dead-key composition: feed the dead key, then press `base_keycode`, and return
    /// the composed UTF-16 string.
    fn compose_dead(
        layout: *const UCKeyboardLayout,
        dead_keycode: u16,
        dead_mstate: ModState,
        base_keycode: u16,
    ) -> Option<String> {
        // Enter the dead state.
        let mut dead_state: u32 = 0;
        let mut buf1 = [0u16; 16];
        let mut len1: c_uint = 0;
        let kbd_type = unsafe { LMGetKbdType() } as u32;
        unsafe {
            UCKeyTranslate(
                layout,
                dead_keycode,
                0,
                dead_mstate.carbon_modifier_mask(),
                kbd_type,
                0,
                &mut dead_state as *mut u32,
                buf1.len() as c_uint,
                &mut len1 as *mut c_uint,
                buf1.as_mut_ptr(),
            );
        }
        if dead_state == 0 {
            return None; // Not actually a dead key; nothing to compose.
        }
        // Press the base key under the same modifiers, feeding the dead state.
        let mut out_buf = [0u16; 16];
        let mut actual_len: c_uint = 0;
        let ret = unsafe {
            UCKeyTranslate(
                layout,
                base_keycode,
                0,
                dead_mstate.carbon_modifier_mask(),
                kbd_type,
                0,
                &mut dead_state as *mut u32,
                out_buf.len() as c_uint,
                &mut actual_len as *mut c_uint,
                out_buf.as_mut_ptr(),
            )
        };
        let _ = ret;
        if actual_len == 0 {
            return None;
        }
        let s = String::from_utf16_lossy(&out_buf[..actual_len as usize]);
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    }

    /// Extract a single macOS layout identified by its input-source ID string
    /// (e.g. `com.apple.keylayout.US`, `com.apple.keylayout.French`).
    pub fn extract(layout: &str, variant: Option<&str>) -> Result<omni_keymap_core::LayoutFile> {
        let source = find_input_source_by_id(layout)
            .with_context(|| format!("finding macOS input source `{}`", layout))?;
        let display_name = read_localized_name(source);
        extract_from_source(source, layout, variant, display_name)
    }

    /// Find a TIS input source by its `TISPropertyInputSourceID`.
    fn find_input_source_by_id(source_id: &str) -> Result<TISInputSourceRef> {
        let list = enumerate_input_sources()?;
        unsafe {
            let count = CFArrayGetCount(list as *const c_void);
            for i in 0..count {
                let source = CFArrayGetValueAtIndex(list as *const c_void, i) as TISInputSourceRef;
                let id_ref = TISGetInputSourceProperty(source, kTISPropertyInputSourceID);
                if id_ref.is_null() {
                    continue;
                }
                let cfstr = CFString::wrap_under_get_rule(id_ref as CFStringRef);
                if cfstr.to_string() == source_id {
                    return Ok(source);
                }
            }
            CFRelease(list as *const c_void);
        }
        Err(anyhow!("no input source with ID `{}` found", source_id))
    }

    /// Enumerate installed keyboard-layout input sources.
    fn enumerate_input_sources() -> Result<TISInputSourceIteratorRef> {
        // Passing NULL asks HIToolbox for all input sources. Filtering by a
        // CFDictionary is brittle across macOS runner images because the
        // kTIS* constants are exported CFString objects, not just their
        // string contents. We filter usable keyboard layouts later by checking
        // for Unicode key-layout data.
        let list = unsafe { TISCreateInputSourceList(null(), 1u8) };
        if list.is_null() {
            Err(anyhow!("TISCreateInputSourceList returned null"))
        } else {
            Ok(list)
        }
    }

    /// Read the `kTISPropertyLocalizedName` of a TIS input source.
    fn read_localized_name(source: TISInputSourceRef) -> Option<String> {
        unsafe {
            let name_ref = TISGetInputSourceProperty(source, kTISPropertyLocalizedName);
            if name_ref.is_null() {
                return None;
            }
            let cfstr = CFString::wrap_under_get_rule(name_ref as CFStringRef);
            let s = cfstr.to_string();
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        }
    }

    /// Extract a layout from a `TISInputSourceRef`.
    fn extract_from_source(
        source: TISInputSourceRef,
        layout: &str,
        variant: Option<&str>,
        display_name: Option<String>,
    ) -> Result<omni_keymap_core::LayoutFile> {
        let layout_ptr = keyboard_layout_data(source)
            .ok_or_else(|| anyhow!("input source `{}` has no UCKeyboardLayout data", layout))?;

        let mut mappings: HashMap<String, Vec<omni_keymap_core::Keystroke>> = HashMap::new();
        let mut dead_sources: Vec<(u16, ModState)> = Vec::new();

        for W3cKey { code, macos_keycode, .. } in W3C_KEYS {
            let kc = *macos_keycode as u16;
            for mstate in [ModState::None, ModState::Shift, ModState::Option, ModState::ShiftOption]
            {
                let mut dead_state = 0u32;
                if let Some(out) = translate(layout_ptr, kc, mstate, &mut dead_state) {
                    let mods = mstate.w3c_modifiers();
                    mappings
                        .entry(out)
                        .or_default()
                        .push(omni_keymap_core::Keystroke::single(*code, mods));
                }
                if is_dead_key(layout_ptr, kc, mstate) {
                    dead_sources.push((kc, mstate));
                }
            }
        }

        // Second stage: simulate dead-key compositions.
        for (dead_kc, dead_mstate) in &dead_sources {
            let dead_code = W3C_KEYS
                .iter()
                .find(|k| k.macos_keycode == *dead_kc as u32)
                .map(|k| k.code)
                .unwrap_or("");
            if dead_code.is_empty() {
                continue;
            }
            for W3cKey { code: base_code, macos_keycode: base_kc, .. } in W3C_KEYS {
                if *base_kc as u16 == *dead_kc {
                    continue;
                }
                if let Some(out) = compose_dead(
                    layout_ptr,
                    *dead_kc,
                    *dead_mstate,
                    *base_kc as u16,
                ) {
                    if out.is_empty() {
                        continue;
                    }
                    mappings
                        .entry(out)
                        .or_default()
                        .push(omni_keymap_core::Keystroke::dead_key(
                            dead_code,
                            dead_mstate.w3c_modifiers(),
                            *base_code,
                            Vec::new(),
                        ));
                }
            }
        }

        Ok(omni_keymap_core::LayoutFile {
            metadata: omni_keymap_core::LayoutMetadata {
                platform: "macos".to_string(),
                layout_name: layout.to_string(),
                layout_variant: variant.map(|s| s.to_string()),
                display_name,
                extracted_on: crate::now_iso8601(),
            },
            mappings,
        })
    }

    /// Enumerate every installed macOS keyboard layout and extract each.
    pub fn extract_all(out_dir: &std::path::Path) -> Result<crate::batch::BatchSummary> {
        use std::path::PathBuf;
        let list = enumerate_input_sources()?;
        std::fs::create_dir_all(out_dir)
            .with_context(|| format!("creating output directory {}", out_dir.display()))?;
        let mut summary = crate::batch::BatchSummary::default();

        unsafe {
            let count = CFArrayGetCount(list as *const c_void);
            for i in 0..count {
                let source =
                    CFArrayGetValueAtIndex(list as *const c_void, i) as TISInputSourceRef;
                // Get the input source ID to use as the layout name.
                let id_ref = TISGetInputSourceProperty(source, kTISPropertyInputSourceID);
                let source_id = if id_ref.is_null() {
                    format!("unknown-{}", i)
                } else {
                    let cfstr = CFString::wrap_under_get_rule(id_ref as CFStringRef);
                    cfstr.to_string()
                };
                // Strip the `com.apple.keylayout.` prefix for a cleaner file name.
                let stem = source_id
                    .strip_prefix("com.apple.keylayout.")
                    .unwrap_or(&source_id)
                    .to_string();

                let display_name = read_localized_name(source);
                let file = extract_from_source(source, &stem, None, display_name);
                match file {
                    Ok(f) => {
                        let n = f.mappings.len();
                        let path: PathBuf = out_dir.join(format!("{}.json", stem));
                        let raw = match serde_json::to_string_pretty(&f) {
                            Ok(s) => s,
                            Err(e) => {
                                summary.failures.push(crate::batch::BatchItem {
                                    layout: stem.clone(),
                                    variant: None,
                                    status: crate::batch::BatchStatus::Skipped {
                                        error: format!("serialize: {}", e),
                                    },
                                });
                                summary.skipped += 1;
                                continue;
                            }
                        };
                        if let Err(e) = std::fs::write(&path, raw) {
                            summary.failures.push(crate::batch::BatchItem {
                                layout: stem.clone(),
                                variant: None,
                                status: crate::batch::BatchStatus::Skipped {
                                    error: format!("write {}: {}", path.display(), e),
                                },
                            });
                            summary.skipped += 1;
                            continue;
                        }
                        summary.ok += 1;
                        summary.total_mappings += n;
                    }
                    Err(e) => {
                        summary.failures.push(crate::batch::BatchItem {
                            layout: stem,
                            variant: None,
                            status: crate::batch::BatchStatus::Skipped {
                                error: e.to_string(),
                            },
                        });
                        summary.skipped += 1;
                    }
                }
            }
            CFRelease(list as *const c_void);
        }
        Ok(summary)
    }

}

#[cfg(not(target_os = "macos"))]
mod imp {
    use std::path::Path;
    use super::*;
    pub fn extract(_layout: &str, _variant: Option<&str>) -> Result<omni_keymap_core::LayoutFile> {
        Err(anyhow!("macos extraction is only supported on macOS hosts"))
    }
    #[allow(dead_code)]
    pub fn extract_all(_out_dir: &Path) -> Result<crate::batch::BatchSummary> {
        Err(anyhow!("macos extraction is only supported on macOS hosts"))
    }
}

/// Public entry: extract a single macOS keyboard layout by input-source ID.
pub fn extract(layout: &str, variant: Option<&str>) -> Result<omni_keymap_core::LayoutFile> {
    imp::extract(layout, variant)
}

/// Public entry: enumerate and extract every installed macOS keyboard layout.
#[allow(dead_code)]
pub fn extract_all(out_dir: &std::path::Path) -> Result<crate::batch::BatchSummary> {
    imp::extract_all(out_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modstate_modifiers_correct() {
        assert_eq!(ModState::None.w3c_modifiers(), Vec::<String>::new());
        assert_eq!(ModState::Shift.w3c_modifiers(), vec!["Shift".to_string()]);
        assert_eq!(ModState::Option.w3c_modifiers(), vec!["Alt".to_string()]);
        assert_eq!(
            ModState::ShiftOption.w3c_modifiers(),
            vec!["Shift".to_string(), "Alt".to_string()]
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn carbon_modifier_masks() {
        assert_eq!(ModState::None.carbon_modifier_mask(), 0);
        assert_eq!(ModState::Shift.carbon_modifier_mask(), 1 << 1);
        assert_eq!(ModState::Option.carbon_modifier_mask(), 1 << 3);
        assert_eq!(
            ModState::ShiftOption.carbon_modifier_mask(),
            (1 << 1) | (1 << 3)
        );
    }
}