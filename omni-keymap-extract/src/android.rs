//! Offline parser for Android `.kl` (key layout) and `.kcm` (key character map) files.
//!
//! These are plain-text files shipped with AOSP. `.kl` maps Linux evdev keycodes to Android
//! keycode names (`key 30 A`); `.kcm` maps Android keycode names under modifier states to
//! Unicode characters (and dead-key markers).
//!
//! The extractor takes direct file paths (no device access) and emits an OmniKeymap JSON layout
//! by reversing the mapping through the [`crate::w3c_keys`] table (Linux evdev keycode ≡ W3C
//! keycode on Android).
//!
//! This module uses a hand-written parser rather than `nom`: the grammar is line-oriented and
//! small, and a direct parser avoids borrow-checker friction with streaming combinators.

use anyhow::{Context, Result, anyhow};
use std::collections::HashMap;
use std::path::Path;

use crate::w3c_keys::{W3C_KEYS, W3cKey};

/// One row of a `.kl` file: `key <linux_keycode> <android_keycode_name>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KlEntry {
    /// Linux evdev keycode, e.g. `30` for `A`.
    pub linux_keycode: u32,
    /// Android keycode name, e.g. `A`, `KEYCODE_A`. We keep the raw token.
    pub android_name: String,
}

/// A character produced by a `.kcm` key block under a particular modifier set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KcmMapping {
    /// The Unicode character (or string for non-BMP/composed output). Raw text between quotes,
    /// with `\uXXXX` escapes resolved.
    pub output: String,
    /// Whether this output is a dead key.
    pub dead: bool,
}

/// One `.kcm` key block: keyed by Android keycode name, with a set of modifier-state -> mapping
/// rows.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct KcmKey {
    /// `label:` value if present.
    pub label: Option<String>,
    /// Modifier-state name (e.g. `base`, `shift`, `shift, capslock`, `alt`) -> mapping.
    pub rows: HashMap<String, KcmMapping>,
}

/// The fully-parsed `.kcm` file: Android keycode name -> key block.
pub type KcmFile = HashMap<String, KcmKey>;

/// The fully-parsed `.kl` file.
pub type KlFile = Vec<KlEntry>;

/// Parse a `.kl` file body into a [`KlFile`].
pub fn parse_kl(input: &str) -> Result<KlFile> {
    let mut entries = Vec::new();
    for (lineno, raw_line) in input.lines().enumerate() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut it = line.split_whitespace();
        let kw = it.next().unwrap_or("");
        if kw != "key" {
            continue;
        }
        let num_str = it
            .next()
            .ok_or_else(|| anyhow!("kl line {}: missing keycode", lineno + 1))?;
        let linux_keycode: u32 = num_str
            .parse()
            .with_context(|| format!("kl line {}: bad keycode `{}`", lineno + 1, num_str))?;
        let android_name = it
            .next()
            .ok_or_else(|| anyhow!("kl line {}: missing android name", lineno + 1))?
            .to_string();
        entries.push(KlEntry {
            linux_keycode,
            android_name,
        });
    }
    Ok(entries)
}

/// Resolve a `.kcm` escape sequence within a quoted character literal: `\uXXXX` (4 or 6 hex
/// digits) or `\\`, `\'`, `\n`, `\t`. Returns the literal decoded string.
fn decode_char_literal(raw: &str) -> Result<String> {
    if raw.len() < 2 || !raw.starts_with('\'') || !raw.ends_with('\'') {
        return Err(anyhow!("bad char literal `{}`", raw));
    }
    let inner = &raw[1..raw.len() - 1];
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('u') => {
                let hex: String = chars.by_ref().take(4).collect();
                let cp = u32::from_str_radix(&hex, 16)
                    .with_context(|| format!("bad \\u escape in `{}`", raw))?;
                out.push(
                    char::from_u32(cp)
                        .ok_or_else(|| anyhow!("bad codepoint U+{:04X}", cp))?,
                );
            }
            Some('U') => {
                let hex: String = chars.by_ref().take(6).collect();
                let cp = u32::from_str_radix(&hex, 16)
                    .with_context(|| format!("bad \\U escape in `{}`", raw))?;
                out.push(
                    char::from_u32(cp)
                        .ok_or_else(|| anyhow!("bad codepoint U+{:08X}", cp))?,
                );
            }
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            Some('\'') => out.push('\''),
            Some('"') => out.push('"'),
            Some(other) => out.push(other),
            None => return Err(anyhow!("trailing backslash in `{}`", raw)),
        }
    }
    Ok(out)
}

/// Extract the first single-quoted character literal from a line, returning the decoded string,
/// whether it is marked `dead`, and the remainder of the line after the literal (and optional
/// trailing `dead` keyword). Comments (`#...`) are stripped first.
fn parse_char_literal(line: &str) -> Option<(String, bool, &str)> {
    let line = line.split('#').next().unwrap_or(line).trim_start();
    if !line.starts_with('\'') {
        return None;
    }
    let bytes = line.as_bytes();
    let mut end = 1usize;
    let mut escape = false;
    for (i, &b) in bytes.iter().enumerate().skip(1) {
        if escape {
            escape = false;
            continue;
        }
        if b == b'\\' {
            escape = true;
            continue;
        }
        if b == b'\'' {
            end = i + 1;
            break;
        }
    }
    let raw = &line[..end];
    let rest = line[end..].trim_start();
    let (rest, dead) = if let Some(stripped) = rest.strip_prefix("dead") {
        let after = stripped.trim_start();
        // Require a word boundary so we don't strip `deadline`.
        if after.is_empty() || after.starts_with([' ', '\t', '\r', '\n']) {
            (after, true)
        } else {
            (rest, false)
        }
    } else {
        (rest, false)
    };
    let decoded = decode_char_literal(raw).ok()?;
    Some((decoded, dead, rest))
}

/// Parse a `.kcm` file body into a [`KcmFile`].
///
/// Grammar (per AOSP `KeyCharacterMap`):
/// ```text
/// [NAME] key
///     label: 'x'
///     state, state: 'x' [dead]
/// ```
/// Blank lines separate blocks. `#` starts a line comment.
pub fn parse_kcm(input: &str) -> Result<KcmFile> {
    let mut map: KcmFile = HashMap::new();
    let mut lines = input.lines().peekable();
    while let Some(line) = lines.next() {
        let trimmed = line.split('#').next().unwrap_or(line).trim();
        if trimmed.is_empty() {
            continue;
        }
        // Header: `[NAME] key` (the trailing `key` is optional).
        let header = trimmed
            .strip_prefix('[')
            .and_then(|s| s.split(']').next())
            .map(|s| s.trim())
            .ok_or_else(|| anyhow!("kcm: expected `[NAME]` header, found `{}`", trimmed))?;
        let name = header.to_string();
        let mut key = KcmKey::default();
        // Collect body lines until a blank line or a new `[` header.
        while let Some(&peek) = lines.peek() {
            let pt = peek.split('#').next().unwrap_or(peek).trim();
            if pt.is_empty() || pt.starts_with('[') {
                break;
            }
            let body_line = lines.next().unwrap();
            let bt = body_line.split('#').next().unwrap_or(body_line).trim();
            if bt.is_empty() {
                continue;
            }
            // `label: 'x'`
            if let Some(rest) = bt.strip_prefix("label") {
                let rest = rest.trim_start();
                let rest = rest
                    .strip_prefix(':')
                    .ok_or_else(|| anyhow!("kcm block `{}`: bad label line", name))?
                    .trim_start();
                if let Some((lit, _, _)) = parse_char_literal(rest) {
                    key.label = Some(lit);
                }
                continue;
            }
            // `state: 'x' [dead]`
            let (state, after) = bt
                .split_once(':')
                .ok_or_else(|| anyhow!("kcm block `{}`: bad row `{}`", name, bt))?;
            let state_label = state.trim().to_string();
            if let Some((lit, dead, _)) = parse_char_literal(after.trim_start()) {
                key.rows.insert(
                    state_label,
                    KcmMapping {
                        output: lit,
                        dead,
                    },
                );
            }
        }
        map.insert(name, key);
    }
    Ok(map)
}

/// The modifier state names we map to OmniMap modifier lists.
fn modifiers_for_state(state: &str) -> Vec<String> {
    let parts: Vec<&str> = state.split(',').map(|s| s.trim()).collect();
    let mut mods = Vec::new();
    if parts.iter().any(|&s| s == "shift" || s == "capslock") {
        mods.push("Shift".to_string());
    }
    if parts.iter().any(|&s| s == "alt") {
        mods.push("Alt".to_string());
    }
    mods
}

/// Build the OmniKeymap reverse-mapping from parsed `.kl` and `.kcm` data.
///
/// For every W3C key whose `linux_keycode` appears in the `.kl` file, we look up the Android
/// keycode name in the `.kcm` file and emit one [`Keystroke`] alternative per modifier-state row.
/// Dead-key base rows produce a second-stage entry keyed under the composed string, using
/// [`Keystroke::dead_key`].
pub fn build_layout(
    kl: &KlFile,
    kcm: &KcmFile,
) -> Result<omni_keymap_core::LayoutFile> {
    use omni_keymap_core::Keystroke;

    let mut kl_map: HashMap<u32, &str> = HashMap::new();
    for e in kl {
        kl_map.insert(e.linux_keycode, e.android_name.as_str());
    }

    let mut mappings: HashMap<String, Vec<Keystroke>> = HashMap::new();
    let mut dead_keys: Vec<(String, String, Vec<String>)> = Vec::new();

    for W3cKey { code, linux_keycode, .. } in W3C_KEYS {
        let android_name = match kl_map.get(linux_keycode) {
            Some(n) => *n,
            None => continue,
        };
        let key = match kcm.get(android_name) {
            Some(k) => k,
            None => continue,
        };
        for (state, mapping) in &key.rows {
            if mapping.output.is_empty() {
                continue;
            }
            let mods = modifiers_for_state(state);
            if mapping.dead {
                dead_keys.push((mapping.output.clone(), code.to_string(), mods.clone()));
                mappings
                    .entry(mapping.output.clone())
                    .or_default()
                    .push(Keystroke::single(*code, mods.clone()));
                continue;
            }
            mappings
                .entry(mapping.output.clone())
                .or_default()
                .push(Keystroke::single(*code, mods.clone()));
        }
    }

    // Second stage: for each dead key D produced by (dead_code, dead_mods), compose with every
    // single-character base output to discover dead-key compositions (AOSP `.kcm` files do not
    // encode the composition table directly, so we follow the plan's "simulate combinations"
    // approach).
    if !dead_keys.is_empty() {
        let base_keys: Vec<(String, String, Vec<String>)> = mappings
            .iter()
            .flat_map(|(ch, alts)| {
                alts.iter().filter_map(|ks| {
                    if ks.sequence.len() == 1 {
                        let kp = &ks.sequence[0];
                        if kp.modifiers.is_empty() && ch.chars().count() == 1 {
                            return Some((
                                ch.clone(),
                                kp.key.clone(),
                                kp.modifiers.clone(),
                            ));
                        }
                    }
                    None
                })
            })
            .collect();
        for (dead_ch, dead_code, dead_mods) in &dead_keys {
            for (base_ch, base_code, base_mods) in &base_keys {
                if base_code == dead_code {
                    continue;
                }
                let composed = format!("{}{}", dead_ch, base_ch);
                mappings
                    .entry(composed)
                    .or_default()
                    .push(Keystroke::dead_key(
                        dead_code,
                        dead_mods.clone(),
                        base_code,
                        base_mods.clone(),
                    ));
            }
        }
    }

    Ok(omni_keymap_core::LayoutFile {
        metadata: omni_keymap_core::LayoutMetadata {
            platform: "android".to_string(),
            layout_name: "android".to_string(),
            layout_variant: None,
            extracted_on: crate::now_iso8601(),
        },
        mappings,
    })
}

/// Load `.kl` and `.kcm` files from disk and produce a [`omni_keymap_core::LayoutFile`].
pub fn extract_from_files(
    kl_path: &Path,
    kcm_path: &Path,
) -> Result<omni_keymap_core::LayoutFile> {
    let kl_raw = std::fs::read_to_string(kl_path)
        .with_context(|| format!("reading .kl file {}", kl_path.display()))?;
    let kcm_raw = std::fs::read_to_string(kcm_path)
        .with_context(|| format!("reading .kcm file {}", kcm_path.display()))?;
    let kl = parse_kl(&kl_raw)?;
    let kcm = parse_kcm(&kcm_raw)?;
    build_layout(&kl, &kcm)
}

#[cfg(test)]
mod tests {
    use super::*;
    use omni_keymap_core::Keystroke;

    const KL_SAMPLE: &str = r#"
# Test key layout file
key 30 A
key 48 B
key 46 C
key 40 APOSTROPHE
key 2 1
"#;

    const KCM_SAMPLE: &str = r#"
[A] key
    label: 'a'
    base: 'a'
    shift, capslock: 'A'
    alt: '\u00e4'
    shift+alt: '\u00c4'

[B] key
    label: 'b'
    base: 'b'
    shift, capslock: 'B'

[C] key
    label: 'c'
    base: 'c'
    shift, capslock: 'C'

[APOSTROPHE] key
    label: '\''
    base: '\''
    shift: '"'
    alt: '\u00b4' dead
"#;

    fn parse_ok() -> (KlFile, KcmFile) {
        let kl = parse_kl(KL_SAMPLE).unwrap();
        let kcm = parse_kcm(KCM_SAMPLE).unwrap();
        (kl, kcm)
    }

    #[test]
    fn parse_kl_skips_comments_and_unknown_directives() {
        let kl = parse_kl(KL_SAMPLE).unwrap();
        let codes: Vec<u32> = kl.iter().map(|e| e.linux_keycode).collect();
        assert_eq!(codes, vec![30, 48, 46, 40, 2]);
        assert_eq!(kl[0].android_name, "A");
    }

    #[test]
    fn parse_kl_errors_on_missing_keycode() {
        let bad = "key A";
        assert!(parse_kl(bad).is_err());
    }

    #[test]
    fn parse_kcm_reads_modifier_rows() {
        let (_, kcm) = parse_ok();
        let a = kcm.get("A").unwrap();
        assert_eq!(a.label.as_deref(), Some("a"));
        assert_eq!(a.rows.get("base").unwrap().output, "a");
        assert_eq!(a.rows.get("shift, capslock").unwrap().output, "A");
        assert_eq!(a.rows.get("alt").unwrap().output, "\u{00e4}");
    }

    #[test]
    fn parse_kcm_marks_dead_keys() {
        let (_, kcm) = parse_ok();
        let q = kcm.get("APOSTROPHE").unwrap();
        let alt = q.rows.get("alt").unwrap();
        assert!(alt.dead);
        assert_eq!(alt.output, "\u{00b4}");
    }

    #[test]
    fn build_layout_emits_base_mappings() {
        let (kl, kcm) = parse_ok();
        let layout = build_layout(&kl, &kcm).unwrap();
        let a = layout.mappings.get("a").unwrap();
        assert!(a.contains(&Keystroke::single("KeyA", vec![])));
        let cap = layout.mappings.get("A").unwrap();
        assert!(cap.contains(&Keystroke::single(
            "KeyA",
            vec!["Shift".to_string()]
        )));
    }

    #[test]
    fn build_layout_emits_alt_mapping() {
        let (kl, kcm) = parse_ok();
        let layout = build_layout(&kl, &kcm).unwrap();
        let alt = layout.mappings.get("\u{00e4}").unwrap();
        assert!(alt.contains(&Keystroke::single(
            "KeyA",
            vec!["Alt".to_string()]
        )));
    }

    #[test]
    fn build_layout_emits_dead_key_compositions() {
        let (kl, kcm) = parse_ok();
        let layout = build_layout(&kl, &kcm).unwrap();
        let composed = layout.mappings.get("\u{00b4}a");
        assert!(
            composed.is_some(),
            "missing dead-key composition for dead-accent + a"
        );
        let ks = composed
            .unwrap()
            .iter()
            .find(|k| k.sequence.len() == 2)
            .unwrap();
        // The dead source key for APOSTROPHE maps to linux keycode 40 -> W3C `Quote`.
        assert_eq!(ks.sequence[0].key, "Quote");
        assert_eq!(ks.sequence[1].key, "KeyA");
    }

    #[test]
    fn build_layout_metadata_is_android() {
        let (kl, kcm) = parse_ok();
        let layout = build_layout(&kl, &kcm).unwrap();
        assert_eq!(layout.metadata.platform, "android");
    }

    #[test]
    fn decode_char_literal_handles_unicode_escape() {
        let s = decode_char_literal("'\\u00e4'").unwrap();
        assert_eq!(s, "\u{00e4}");
    }

    #[test]
    fn decode_char_literal_handles_plain() {
        let s = decode_char_literal("'a'").unwrap();
        assert_eq!(s, "a");
    }

    #[test]
    fn decode_char_literal_rejects_unterminated() {
        assert!(decode_char_literal("'a").is_err());
    }

    #[test]
    fn parse_kcm_minimal_file() {
        let kcm = "[A] key\n    base: 'a'\n";
        let f = parse_kcm(kcm).unwrap();
        assert_eq!(f.get("A").unwrap().rows.get("base").unwrap().output, "a");
    }

    #[test]
    fn extract_from_files_reads_disk() {
        let tmp = tempfile::tempdir().unwrap();
        let kl = tmp.path().join("Generic.kl");
        let kcm = tmp.path().join("Generic.kcm");
        std::fs::write(&kl, KL_SAMPLE).unwrap();
        std::fs::write(&kcm, KCM_SAMPLE).unwrap();
        let layout = extract_from_files(&kl, &kcm).unwrap();
        assert!(layout.mappings.contains_key("a"));
    }
}