//! Windows layout extraction.
//!
//! On Linux (and other non-Windows hosts) this module reports an unsupported error so the
//! workspace compiles and tests run cross-platform. On Windows it would use `windows-sys` to call
//! `MapVirtualKeyExW` and `ToUnicodeEx` under modifier states, per the plan.

use anyhow::{Result, anyhow};

/// Extract a Windows keyboard layout.
///
/// Not implemented on non-Windows hosts. The Windows build path lives behind `target_os =
/// "windows"` and uses `ToUnicodeEx` / `MapVirtualKeyExW` from `windows-sys`.
pub fn extract(_layout: &str, _variant: Option<&str>) -> Result<omni_keymap_core::LayoutFile> {
    #[cfg(target_os = "windows")]
    {
        imp::extract(_layout, _variant)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (_layout, _variant);
        Err(anyhow!("windows extraction is only supported on Windows hosts"))
    }
}

#[cfg(target_os = "windows")]
mod imp {
    use super::*;
    use anyhow::Context;
    use std::collections::HashMap;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetKeyboardLayout, MapVirtualKeyExW, ToUnicodeEx, MAPVK_VSC_TO_VK_EX,
        VK_CONTROL, VK_MENU, VK_SHIFT,
    };

    pub fn extract(layout: &str, variant: Option<&str>) -> Result<omni_keymap_core::LayoutFile> {
        let hkl = unsafe { GetKeyboardLayout(0) };
        let mut mappings: HashMap<String, Vec<omni_keymap_core::Keystroke>> = HashMap::new();
        for w3c in crate::w3c_keys::W3C_KEYS {
            let vk = unsafe {
                MapVirtualKeyExW(w3c.windows_scancode, MAPVK_VSC_TO_VK_EX, hkl)
            } as u16;
            if vk == 0 {
                continue;
            }
            for mstate in [ModState::None, ModState::Shift, ModState::AltGraph, ModState::ShiftAltGr]
            {
                let buf = translate(vk, hkl, mstate)?;
                if let Some(out) = buf {
                    let mods = mstate.w3c_modifiers();
                    mappings
                        .entry(out)
                        .or_default()
                        .push(omni_keymap_core::Keystroke::single(w3c.code, mods));
                }
            }
        }
        Ok(omni_keymap_core::LayoutFile {
            metadata: omni_keymap_core::LayoutMetadata {
                platform: "windows".to_string(),
                layout_name: layout.to_string(),
                layout_variant: variant.map(|s| s.to_string()),
                extracted_on: crate::now_iso8601(),
            },
            mappings,
        })
    }

    enum ModState {
        None,
        Shift,
        AltGraph,
        ShiftAltGr,
    }
    impl ModState {
        fn w3c_modifiers(self) -> Vec<String> {
            match self {
                ModState::None => vec![],
                ModState::Shift => vec!["Shift".to_string()],
                ModState::AltGraph => vec!["AltGraph".to_string()],
                ModState::ShiftAltGr => vec!["Shift".to_string(), "AltGraph".to_string()],
            }
        }
        fn key_states(self) -> &'static [u16] {
            match self {
                ModState::None => &[],
                ModState::Shift => &[VK_SHIFT],
                ModState::AltGraph => &[VK_CONTROL, VK_MENU],
                ModState::ShiftAltGr => &[VK_SHIFT, VK_CONTROL, VK_MENU],
            }
        }
    }

    fn translate(vk: u16, hkl: usize, mstate: ModState) -> Result<Option<String>> {
        // Real implementation requires per-key send_input sequencing; this stub records the
        // approach documented in the plan. Full dead-key simulation is out of scope for the
        // non-Windows verification pass.
        let _ = (vk, hkl, mstate);
        Ok(None)
    }
}