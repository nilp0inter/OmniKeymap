//! Linux extraction using `libxkbcommon`.
//!
//! Configures an `xkb_context`, loads the requested layout/variant, iterates the W3C keycode
//! table, and queries `xkb_keymap_key_get_syms_by_level` for each key under up to four layout
//! levels (0 = base, 1 = Shift, 2 = AltGr/ISO_Level3, 3 = Shift+AltGr). Each keysym is converted
//! to a UTF-8 string via `xkb_keysym_to_utf8`. Dead keys are detected by their keysym name prefix
//! (`dead_*`) and their second-stage compositions are simulated by re-feeding the dead state and
//! pressing every other standard key.

use anyhow::{Result, anyhow};

use crate::w3c_keys::{W3C_KEYS, W3cKey};

/// The layout levels we query. XKB levels are 0-indexed; for a typical 4-level layout these
/// correspond to: 0 = base, 1 = Shift, 2 = AltGr (ISO Level 3), 3 = Shift + AltGr.
const LEVELS: &[(u32, &[&str])] = &[
    (0, &[]),
    (1, &["Shift"]),
    (2, &["AltGraph"]),
    (3, &["Shift", "AltGraph"]),
];

#[cfg(target_os = "linux")]
mod imp {
    use super::*;
    use std::collections::HashMap;
    use xkbcommon::xkb::{self, Keysym};

    /// Extract a Linux layout to an [`omni_keymap_core::LayoutFile`].
    pub fn extract(
        layout: &str,
        variant: Option<&str>,
    ) -> Result<omni_keymap_core::LayoutFile> {
        let ctx = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let keymap = xkb::Keymap::new_from_names(
            &ctx,
            "",                    // rules
            "evdev",               // model
            layout,                // layout
            variant.unwrap_or(""), // variant
            None,                  // options
            xkb::COMPILE_NO_FLAGS,
        )
        .ok_or_else(|| {
            anyhow!(
                "failed to compile xkb keymap for layout `{}` variant `{:?}`",
                layout,
                variant
            )
        })?;

        let mut mappings: HashMap<String, Vec<omni_keymap_core::Keystroke>> = HashMap::new();
        let mut dead_sources: Vec<(u32, Vec<String>)> = Vec::new();

        // XKB layouts can have multiple groups; we use layout index 0 (the primary group).
        let layout_idx: xkb::LayoutIndex = 0;

        for W3cKey { code, linux_keycode, .. } in W3C_KEYS {
            let kc = xkb::Keycode::new(linux_keycode + 8);
            let num_levels = keymap.num_levels_for_key(kc, layout_idx);
            for (level, mods) in LEVELS.iter().copied() {
                if level >= num_levels {
                    break;
                }
                let syms: &[Keysym] =
                    keymap.key_get_syms_by_level(kc, layout_idx, level);
                if syms.is_empty() {
                    continue;
                }
                // Use the first keysym at this level (multi-sym keys are out of scope).
                let sym = syms[0];
                let sym_name = xkb::keysym_get_name(sym);
                let is_dead = sym_name.starts_with("dead_");
                let mods_vec: Vec<String> = mods.iter().map(|s| s.to_string()).collect();
                if is_dead {
                    if !dead_sources
                        .iter()
                        .any(|(k, m)| *k == *linux_keycode && *m == mods_vec)
                    {
                        dead_sources.push((*linux_keycode, mods_vec.clone()));
                    }
                    // Dead keys produce no direct character output; they only contribute via
                    // second-stage compositions below.
                    continue;
                }
                let out = xkb::keysym_to_utf8(sym);
                let out = out.trim_end_matches('\0').to_string();
                if out.is_empty() {
                    continue;
                }
                mappings
                    .entry(out)
                    .or_default()
                    .push(omni_keymap_core::Keystroke::single(*code, mods_vec));
            }
        }

        // Second stage: simulate dead-key compositions. For each dead source, feed the dead key
        // into a fresh state under its producing modifiers, then press every other standard key
        // and capture the resulting UTF-8 string. This mirrors how `xkb_state_key_get_utf8`
        // accumulates a dead-key state across two keypresses.
        for (dead_kc, dead_mods) in &dead_sources {
            let dead_code = W3C_KEYS
                .iter()
                .find(|k| k.linux_keycode == *dead_kc)
                .map(|k| k.code)
                .unwrap_or("");
            if dead_code.is_empty() {
                continue;
            }
            for W3cKey { code: base_code, linux_keycode: base_kc, .. } in W3C_KEYS {
                if *base_kc == *dead_kc {
                    continue;
                }
                let composed = simulate_dead_compose(&keymap, *dead_kc, dead_mods, *base_kc);
                if let Some(out) = composed {
                    if out.is_empty() {
                        continue;
                    }
                    mappings
                        .entry(out)
                        .or_default()
                        .push(omni_keymap_core::Keystroke::dead_key(
                            dead_code,
                            dead_mods.clone(),
                            *base_code,
                            Vec::new(),
                        ));
                }
            }
        }

        Ok(omni_keymap_core::LayoutFile {
            metadata: omni_keymap_core::LayoutMetadata {
                platform: "linux".to_string(),
                layout_name: layout.to_string(),
                layout_variant: variant.map(|s| s.to_string()),
                extracted_on: crate::now_iso8601(),
            },
            mappings,
        })
    }

    /// Simulate pressing a dead key (with its producing modifiers) followed by a base key, and
    /// return the resulting UTF-8 string from `xkb_state_key_get_utf8`.
    fn simulate_dead_compose(
        keymap: &xkb::Keymap,
        dead_kc: u32,
        dead_mods: &[String],
        base_kc: u32,
    ) -> Option<String> {
        use xkbcommon::xkb::keysyms::{KEY_Shift_L, KEY_ISO_Level3_Shift};
        let mut state = xkb::State::new(keymap);
        // Apply the modifiers that produced the dead key.
        if dead_mods.iter().any(|m| m == "Shift") {
            state.update_key(
                xkb::Keycode::new(KEY_Shift_L),
                xkb::KeyDirection::Down,
            );
        }
        if dead_mods.iter().any(|m| m == "AltGraph") {
            state.update_key(
                xkb::Keycode::new(KEY_ISO_Level3_Shift),
                xkb::KeyDirection::Down,
            );
        }
        // Feed the dead key down then up to enter the dead state.
        state.update_key(xkb::Keycode::new(dead_kc + 8), xkb::KeyDirection::Down);
        state.update_key(xkb::Keycode::new(dead_kc + 8), xkb::KeyDirection::Up);
        // Apply the base key's modifiers (we query the base level of the base key).
        let out = state.key_get_utf8(xkb::Keycode::new(base_kc + 8));
        if out.is_empty() {
            None
        } else {
            Some(out)
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod imp {
    use super::*;
    pub fn extract(
        _layout: &str,
        _variant: Option<&str>,
    ) -> Result<omni_keymap_core::LayoutFile> {
        Err(anyhow!("linux extraction is only supported on Linux hosts"))
    }
}

/// Public entry: extract a Linux XKB layout.
pub fn extract(
    layout: &str,
    variant: Option<&str>,
) -> Result<omni_keymap_core::LayoutFile> {
    imp::extract(layout, variant)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_table_modifiers_correct() {
        assert_eq!(LEVELS[0].1, &[] as &[&str]);
        assert_eq!(LEVELS[1].1, &["Shift"]);
        assert_eq!(LEVELS[2].1, &["AltGraph"]);
        assert_eq!(LEVELS[3].1, &["Shift", "AltGraph"]);
    }
}