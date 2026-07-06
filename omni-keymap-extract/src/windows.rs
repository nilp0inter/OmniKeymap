//! Windows layout extraction using `ToUnicodeEx` / `MapVirtualKeyExW`.
//!
//! For each W3C key, we map the Windows scan code to a virtual key via
//! `MapVirtualKeyExW(scancode, MAPVK_VSC_TO_VK_EX, hkl)`, then call `ToUnicodeEx` under four
//! modifier states by populating a 256-byte `kbdState` array:
//!
//! 1. None
//! 2. Shift (`VK_SHIFT` down)
//! 3. AltGr (`VK_CONTROL` + `VK_MENU` down — Windows sends AltGr as Ctrl+Alt)
//! 4. Shift + AltGr
//!
//! `ToUnicodeEx` returns a negative count when the key is a dead key. We detect that, then
//! simulate the dead key followed by every other standard key to discover the composed
//! character, recording it as a two-element [`Keystroke`] sequence. After each dead-key probe
//! we feed the dead key once more (with a space character) to clear the lingering dead state.
//!
//! `--all` enumerates installed keyboard layouts via `GetKeyboardLayoutList` and maps each
//! `HKL` to its KLID string via `GetKeyboardLayoutNameW`.

use anyhow::{Result, anyhow};

#[cfg(target_os = "windows")]
use anyhow::Context;
#[cfg(target_os = "windows")]
use std::collections::HashMap;
/// The four modifier states we probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(any(target_os = "windows", test))]
enum ModState {
    None,
    Shift,
    AltGr,
    ShiftAltGr,
}

#[cfg(any(target_os = "windows", test))]
impl ModState {
    fn w3c_modifiers(self) -> Vec<String> {
        match self {
            ModState::None => vec![],
            ModState::Shift => vec!["Shift".to_string()],
            ModState::AltGr => vec!["AltGraph".to_string()],
            ModState::ShiftAltGr => {
                vec!["Shift".to_string(), "AltGraph".to_string()]
            }
        }
    }
}

#[cfg(target_os = "windows")]
mod imp {
    use super::*;
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetKeyboardLayout, GetKeyboardLayoutList, GetKeyboardLayoutNameW, LoadKeyboardLayoutW,
        MapVirtualKeyExW, ToUnicodeEx, UnloadKeyboardLayout, HKL, MAPVK_VSC_TO_VK_EX,
        VK_CONTROL, VK_MENU, VK_SHIFT,
    };
    use windows_sys::core::{PCWSTR, PWSTR};

    const KBD_STATE_LEN: usize = 256;

    /// Translate a KLID string like `"00000409"` into a loaded `HKL`.
    fn load_layout(klid: &str) -> Result<HKL> {
        // LoadKeyboardLayoutW expects a wide string KLID; the KLF_NOTELLSHELL flag (0x00000080)
        // prevents it from activating the layout in the shell.
        let wide: Vec<u16> = klid.encode_utf16().chain(std::iter::once(0)).collect();
        let hkl = unsafe {
            LoadKeyboardLayoutW(PCWSTR(wide.as_ptr()), 0x00000080)
        };
        if hkl.is_null() {
            Err(anyhow!("LoadKeyboardLayoutW failed for KLID `{}`", klid))
        } else {
            Ok(hkl)
        }
    }

    /// Map an `HKL` to its KLID string via `GetKeyboardLayoutNameW`.
    fn layout_name(hkl: HKL) -> Result<String> {
        let mut buf = [0u16; 16];
        let ok = unsafe { GetKeyboardLayoutNameW(PWSTR(buf.as_mut_ptr())) };
        if ok == 0 {
            return Err(anyhow!("GetKeyboardLayoutNameW failed"));
        }
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Ok(String::from_utf16_lossy(&buf[..len]))
    }

    /// Build a 256-byte keyboard state array for the given modifier combination.
    fn kbd_state_for(mstate: ModState) -> [u8; KBD_STATE_LEN] {
        let mut state = [0u8; KBD_STATE_LEN];
        let set_down = |state: &mut [u8; KBD_STATE_LEN], vk: u16| {
            // VK indices are 0..=255; the array is indexed by virtual-key code.
            state[vk as usize] = 0x80;
        };
        match mstate {
            ModState::None => {}
            ModState::Shift => {
                set_down(&mut state, VK_SHIFT);
            }
            ModState::AltGr => {
                set_down(&mut state, VK_CONTROL);
                set_down(&mut state, VK_MENU);
            }
            ModState::ShiftAltGr => {
                set_down(&mut state, VK_SHIFT);
                set_down(&mut state, VK_CONTROL);
                set_down(&mut state, VK_MENU);
            }
        }
        state
    }

    /// Call `ToUnicodeEx` for `vk` under `mstate` and return the decoded UTF-16 string.
    /// Returns `None` if no character is produced.
    fn translate(vk: u32, scancode: u32, hkl: HKL, mstate: ModState) -> Option<String> {
        let state = kbd_state_for(mstate);
        let mut buf = [0u16; 16];
        let ret = unsafe {
            ToUnicodeEx(
                vk,
                scancode,
                state.as_ptr(),
                PWSTR(buf.as_mut_ptr()),
                buf.len() as i32,
                0,
                hkl,
            )
        };
        if ret <= 0 {
            return None;
        }
        let s = String::from_utf16_lossy(&buf[..ret as usize]);
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    }

    /// Probe whether `vk` under `mstate` yields a dead key by checking the `ToUnicodeEx` return
    /// sign. A negative return means the key entered a dead state.
    fn is_dead_key(vk: u32, scancode: u32, hkl: HKL, mstate: ModState) -> bool {
        let state = kbd_state_for(mstate);
        let mut buf = [0u16; 16];
        let ret = unsafe {
            ToUnicodeEx(
                vk,
                scancode,
                state.as_ptr(),
                PWSTR(buf.as_mut_ptr()),
                buf.len() as i32,
                0,
                hkl,
            )
        };
        let dead = ret < 0;
        if dead {
            // Clear the dead state by pressing space.
            let mut clear_buf = [0u16; 16];
            let space_state = [0u8; KBD_STATE_LEN];
            unsafe {
                ToUnicodeEx(
                    0x20, // VK_SPACE
                    0,
                    space_state.as_ptr(),
                    PWSTR(clear_buf.as_mut_ptr()),
                    clear_buf.len() as i32,
                    0,
                    hkl,
                );
            }
        }
        dead
    }

    /// Resolve a dead-key composition: feed the dead key, then press `base_vk`, and return the
    /// composed UTF-16 string.
    fn compose_dead(
        dead_vk: u32,
        dead_scancode: u32,
        dead_mstate: ModState,
        base_vk: u32,
        base_scancode: u32,
        hkl: HKL,
    ) -> Option<String> {
        // Enter the dead state.
        let dead_state = kbd_state_for(dead_mstate);
        let mut dead_buf = [0u16; 16];
        unsafe {
            ToUnicodeEx(
                dead_vk,
                dead_scancode,
                dead_state.as_ptr(),
                PWSTR(dead_buf.as_mut_ptr()),
                dead_buf.len() as i32,
                0,
                hkl,
            );
        }
        // Press the base key under the same dead modifier combination.
        let base_state = kbd_state_for(dead_mstate);
        let mut out_buf = [0u16; 16];
        let ret = unsafe {
            ToUnicodeEx(
                base_vk,
                base_scancode,
                base_state.as_ptr(),
                PWSTR(out_buf.as_mut_ptr()),
                out_buf.len() as i32,
                0,
                hkl,
            )
        };
        // Clear any lingering dead state with a space.
        let mut clear_buf = [0u16; 16];
        let clear_state = [0u8; KBD_STATE_LEN];
        unsafe {
            ToUnicodeEx(
                0x20,
                0,
                clear_state.as_ptr(),
                PWSTR(clear_buf.as_mut_ptr()),
                clear_buf.len() as i32,
                0,
                hkl,
            );
        }
        if ret <= 0 {
            return None;
        }
        let s = String::from_utf16_lossy(&out_buf[..ret as usize]);
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    }

    /// Extract a single Windows layout by KLID (e.g. `"00000409"`) into a [`LayoutFile`].
    pub fn extract(layout: &str, variant: Option<&str>) -> Result<omni_keymap_core::LayoutFile> {
        let hkl = load_layout(layout)
            .with_context(|| format!("loading Windows keyboard layout KLID `{}`", layout))?;
        let result = extract_with_hkl(hkl, layout, variant);
        // Unload the layout we loaded. (The system default layout must not be unloaded, but
        // LoadKeyboardLayoutW always loads a *new* handle that is safe to unload.)
        unsafe { UnloadKeyboardLayout(hkl) };
        result
    }

    /// Extract using an already-loaded `HKL`.
    fn extract_with_hkl(
        hkl: HKL,
        layout: &str,
        variant: Option<&str>,
    ) -> Result<omni_keymap_core::LayoutFile> {
        let mut mappings: HashMap<String, Vec<omni_keymap_core::Keystroke>> = HashMap::new();
        let mut dead_sources: Vec<(u32, u32, ModState)> = Vec::new(); // (vk, scancode, mstate)

        for W3cKey { code, windows_scancode, .. } in W3C_KEYS {
            let sc = *windows_scancode;
            let vk = unsafe { MapVirtualKeyExW(sc, MAPVK_VSC_TO_VK_EX, hkl) };
            if vk == 0 {
                continue;
            }
            for mstate in [ModState::None, ModState::Shift, ModState::AltGr, ModState::ShiftAltGr]
            {
                if let Some(out) = translate(vk, sc, hkl, mstate) {
                    let mods = mstate.w3c_modifiers();
                    mappings
                        .entry(out)
                        .or_default()
                        .push(omni_keymap_core::Keystroke::single(*code, mods));
                }
                if is_dead_key(vk, sc, hkl, mstate) {
                    dead_sources.push((vk, sc, mstate));
                }
            }
        }

        // Second stage: simulate dead-key compositions.
        for (dead_vk, dead_sc, dead_mstate) in &dead_sources {
            let dead_code = W3C_KEYS
                .iter()
                .find(|k| k.windows_scancode == *dead_sc)
                .map(|k| k.code)
                .unwrap_or("");
            if dead_code.is_empty() {
                continue;
            }
            for W3cKey { code: base_code, windows_scancode: base_sc, .. } in W3C_KEYS {
                if *base_sc == *dead_sc {
                    continue;
                }
                let base_vk = unsafe { MapVirtualKeyExW(*base_sc, MAPVK_VSC_TO_VK_EX, hkl) };
                if base_vk == 0 {
                    continue;
                }
                if let Some(out) = compose_dead(
                    *dead_vk,
                    *dead_sc,
                    *dead_mstate,
                    base_vk,
                    *base_sc,
                    hkl,
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
                platform: "windows".to_string(),
                layout_name: layout.to_string(),
                layout_variant: variant.map(|s| s.to_string()),
                extracted_on: crate::now_iso8601(),
            },
            mappings,
        })
    }

    /// Enumerate every installed Windows keyboard layout and extract each.
    pub fn extract_all(out_dir: &std::path::Path) -> Result<crate::batch::BatchSummary> {
        use std::path::PathBuf;
        let mut layouts: Vec<HKL> = Vec::new();
        // First call with a null buffer returns the count.
        let count = unsafe { GetKeyboardLayoutList(0, std::ptr::null_mut()) };
        if count <= 0 {
            return Err(anyhow!("GetKeyboardLayoutList returned no layouts"));
        }
        layouts.resize_with(count as usize, || std::ptr::null_mut());
        let got = unsafe { GetKeyboardLayoutList(count, layouts.as_mut_ptr()) };
        if got <= 0 {
            return Err(anyhow!("GetKeyboardLayoutList failed on second call"));
        }
        layouts.truncate(got as usize);

        let mut summary = crate::batch::BatchSummary::default();
        std::fs::create_dir_all(out_dir)
            .with_context(|| format!("creating output directory {}", out_dir.display()))?;
        for hkl in layouts {
            let klid = match layout_name(hkl) {
                Ok(k) => k,
                Err(e) => {
                    summary.failures.push(crate::batch::BatchItem {
                        layout: format!("{:p}", hkl),
                        variant: None,
                        status: crate::batch::BatchStatus::Skipped {
                            error: format!("GetKeyboardLayoutNameW: {}", e),
                        },
                    });
                    summary.skipped += 1;
                    continue;
                }
            };
            let file = match extract_with_hkl(hkl, &klid, None) {
                Ok(f) => f,
                Err(e) => {
                    summary.failures.push(crate::batch::BatchItem {
                        layout: klid.clone(),
                        variant: None,
                        status: crate::batch::BatchStatus::Skipped {
                            error: e.to_string(),
                        },
                    });
                    summary.skipped += 1;
                    continue;
                }
            };
            let n = file.mappings.len();
            let path: PathBuf = out_dir.join(format!("{}.json", klid));
            let raw = serde_json::to_string_pretty(&file)
                .with_context(|| format!("serializing layout {}", klid))?;
            if let Err(e) = std::fs::write(&path, raw) {
                summary.failures.push(crate::batch::BatchItem {
                    layout: klid.clone(),
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
        let _ = CloseHandle; // referenced to ensure the Foundation import is used
        Ok(summary)
    }
}

#[cfg(not(target_os = "windows"))]
mod imp {
    use super::*;
    use std::path::Path;
    pub fn extract(_layout: &str, _variant: Option<&str>) -> Result<omni_keymap_core::LayoutFile> {
        Err(anyhow!("windows extraction is only supported on Windows hosts"))
    }
    #[allow(dead_code)]
    pub fn extract_all(_out_dir: &Path) -> Result<crate::batch::BatchSummary> {
        Err(anyhow!("windows extraction is only supported on Windows hosts"))
    }
}

/// Public entry: extract a single Windows keyboard layout by KLID.
pub fn extract(layout: &str, variant: Option<&str>) -> Result<omni_keymap_core::LayoutFile> {
    imp::extract(layout, variant)
}

/// Public entry: enumerate and extract every installed Windows keyboard layout.
pub fn extract_all(out_dir: &std::path::Path) -> Result<crate::batch::BatchSummary> {
    imp::extract_all(out_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modstate_modifiers_correct() {
        assert_eq!(ModState::None.w3c_modifiers(), Vec::<String>::new());
        assert_eq!(
            ModState::ShiftAltGr.w3c_modifiers(),
            vec!["Shift".to_string(), "AltGraph".to_string()]
        );
    }
}