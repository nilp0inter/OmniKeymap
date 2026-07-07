//! omni-keymap-core: layout-aware keystroke lookup over the OmniKeymap JSON database.
//!
//! The database is a directory of JSON files (one per platform layout) conforming to the
//! [`LayoutFile`] schema. Each file maps a target character (or short string) to a list of
//! alternative [`Keystroke`] sequences that produce it on the target platform.
//!
//! Dead-key compositions are represented as multi-element [`Keystroke`] sequences: the first
//! element is the dead-modifier key (e.g. `Quote` on a US layout), the second is the base key.

#![deny(missing_docs)]

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A single key press within a keystroke sequence.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeyPress {
    /// W3C `KeyboardEvent.code` value, e.g. `KeyA`, `Digit1`, `Quote`.
    pub key: String,
    /// Modifier names from the W3C modifier set: `Shift`, `Control`, `Alt`, `AltGraph`, `Meta`.
    pub modifiers: Vec<String>,
}

/// A sequence of [`KeyPress`] events that together produce a target character.
///
/// Single-element sequences are direct key presses; multi-element sequences model dead-key
/// compositions (the first element is the dead modifier key, subsequent elements are the base
/// keys pressed while the dead state is active).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Keystroke {
    /// The ordered list of key presses forming this keystroke.
    pub sequence: Vec<KeyPress>,
}

/// The alternatives list for a single target character/string in a layout file.
pub type Alternatives = Vec<Keystroke>;

/// Top-level metadata block of a layout JSON file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutMetadata {
    /// Origin platform: `windows`, `macos`, `linux`, or `android`.
    pub platform: String,
    /// Layout identifier, e.g. `us`, `fr`, `de` on Linux. On Windows this is the
    /// 8-digit KLID (e.g. `00000409`); on macOS the input-source ID (e.g.
    /// `com.apple.keylayout.US`). The identifier is the canonical, locale-independent
    /// name accepted by the platform's native loader.
    pub layout_name: String,
    /// Optional layout variant, e.g. `nodeadkeys`, `intl`. `null`/absent means no variant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout_variant: Option<String>,
    /// Optional human-readable display name sourced from the platform. On Windows this
    /// is the registry `Layout Text` value (e.g. `US`, `Arabic (101)`, `German`).
    /// `null`/absent when the platform does not provide one or extraction could not
    /// resolve it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// ISO-8601 timestamp marking when the layout was extracted.
    pub extracted_on: String,
}


/// The full in-memory representation of a single layout JSON file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutFile {
    /// File metadata.
    pub metadata: LayoutMetadata,
    /// Character/string -> alternative keystroke sequences.
    pub mappings: HashMap<String, Alternatives>,
}

/// The set of W3C modifier names recognized by this library.
pub const MODIFIERS: &[&str] = &["Shift", "Control", "Alt", "AltGraph", "Meta"];

impl KeyPress {
    /// Create a bare key press with no modifiers.
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            modifiers: Vec::new(),
        }
    }

    /// Create a key press with the given modifiers.
    pub fn with_modifiers(key: impl Into<String>, modifiers: Vec<String>) -> Self {
        Self {
            key: key.into(),
            modifiers,
        }
    }

    /// Validate that all modifiers are drawn from [`MODIFIERS`].
    pub fn validate(&self) -> Result<()> {
        for m in &self.modifiers {
            if !MODIFIERS.contains(&m.as_str()) {
                return Err(anyhow!(
                    "invalid modifier `{}` for key `{}`; expected one of {:?}",
                    m,
                    self.key,
                    MODIFIERS
                ));
            }
        }
        Ok(())
    }
}

impl Keystroke {
    /// Build a single-press keystroke.
    pub fn single(key: impl Into<String>, modifiers: Vec<String>) -> Self {
        Self {
            sequence: vec![KeyPress::with_modifiers(key, modifiers)],
        }
    }

    /// Build a dead-key composition: dead key first, then the base key (no modifiers on the
    /// dead element by convention; the base element carries its own modifiers).
    pub fn dead_key(
        dead_key: impl Into<String>,
        dead_modifiers: Vec<String>,
        base_key: impl Into<String>,
        base_modifiers: Vec<String>,
    ) -> Self {
        Self {
            sequence: vec![
                KeyPress::with_modifiers(dead_key, dead_modifiers),
                KeyPress::with_modifiers(base_key, base_modifiers),
            ],
        }
    }

    /// Validate every element of the sequence.
    pub fn validate(&self) -> Result<()> {
        if self.sequence.is_empty() {
            return Err(anyhow!("keystroke sequence must be non-empty"));
        }
        for kp in &self.sequence {
            kp.validate()?;
        }
        Ok(())
    }
}

impl LayoutFile {
    /// Validate metadata and every keystroke sequence.
    pub fn validate(&self) -> Result<()> {
        if self.metadata.platform.is_empty() {
            return Err(anyhow!("metadata.platform must be non-empty"));
        }
        if self.metadata.layout_name.is_empty() {
            return Err(anyhow!("metadata.layout_name must be non-empty"));
        }
        if self.metadata.extracted_on.is_empty() {
            return Err(anyhow!("metadata.extracted_on must be non-empty"));
        }
        for (target, alts) in &self.mappings {
            if target.is_empty() {
                return Err(anyhow!("mappings has an empty target string"));
            }
            if alts.is_empty() {
                return Err(anyhow!(
                    "mappings[`{}`] has an empty alternatives list",
                    target
                ));
            }
            for ks in alts {
                ks.validate()?;
            }
        }
        Ok(())
    }
}

/// The loaded database: a collection of layout files indexed by platform/layout name.
#[derive(Debug, Clone, Default)]
pub struct KeymapDb {
    layouts: HashMap<String, LayoutFile>,
}

impl KeymapDb {
    /// Create an empty database.
    pub fn new() -> Self {
        Self::default()
    }

    /// A canonical index key for a platform/layout pair: `platform/layout_name` (with variant
    /// appended as `+variant` when present).
    pub fn index_key(platform: &str, name: &str, variant: Option<&str>) -> String {
        match variant {
            Some(v) if !v.is_empty() => format!("{}/{}+{}", platform, name, v),
            _ => format!("{}/{}", platform, name),
        }
    }

    /// Insert a layout file, validating it first. Replaces any existing entry with the same
    /// index key.
    pub fn insert(&mut self, layout: LayoutFile) -> Result<String> {
        layout.validate()?;
        let key = Self::index_key(
            &layout.metadata.platform,
            &layout.metadata.layout_name,
            layout.metadata.layout_variant.as_deref(),
        );
        self.layouts.insert(key.clone(), layout);
        Ok(key)
    }

    /// Look up a layout by platform and name (no variant).
    pub fn get(&self, platform: &str, name: &str) -> Option<&LayoutFile> {
        self.layouts.get(&Self::index_key(platform, name, None))
    }

    /// Look up a layout including a variant.
    pub fn get_variant(
        &self,
        platform: &str,
        name: &str,
        variant: Option<&str>,
    ) -> Option<&LayoutFile> {
        self.layouts
            .get(&Self::index_key(platform, name, variant))
    }

    /// Iterate over all loaded layout index keys.
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.layouts.keys()
    }

    /// Number of loaded layouts.
    pub fn len(&self) -> usize {
        self.layouts.len()
    }

    /// Whether the database is empty.
    pub fn is_empty(&self) -> bool {
        self.layouts.is_empty()
    }

    /// Load a single layout file from disk, validating its schema.
    pub fn load_file(path: impl AsRef<Path>) -> Result<LayoutFile> {
        let path = path.as_ref();
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read layout file {}", path.display()))?;
        let layout: LayoutFile = serde_json::from_str(&raw)
            .with_context(|| format!("failed to parse layout file {}", path.display()))?;
        layout.validate()?;
        Ok(layout)
    }

    /// Load every JSON file under a directory tree into a new [`KeymapDb`].
    ///
    /// Each file must parse as a [`LayoutFile`]; files failing validation are reported with their
    /// path and the first error short-circuits.
    pub fn load_dir(dir: impl AsRef<Path>) -> Result<KeymapDb> {
        let dir = dir.as_ref();
        let mut db = KeymapDb::new();
        let mut paths: Vec<PathBuf> = Vec::new();
        collect_json_files(dir, &mut paths)?;
        paths.sort();
        if paths.is_empty() {
            return Err(anyhow!(
                "no layout JSON files found under {}",
                dir.display()
            ));
        }
        for p in paths {
            let layout = Self::load_file(&p)?;
            db.insert(layout)
                .with_context(|| format!("inserting layout from {}", p.display()))?;
        }
        Ok(db)
    }

    /// Look up the alternative keystroke sequences for a single character on a platform/layout.
    pub fn lookup(
        &self,
        platform: &str,
        name: &str,
        ch: &str,
    ) -> Option<&Alternatives> {
        self.get(platform, name).and_then(|l| l.mappings.get(ch))
    }

    /// Look up alternatives including an optional layout variant.
    pub fn lookup_variant(
        &self,
        platform: &str,
        name: &str,
        variant: Option<&str>,
        ch: &str,
    ) -> Option<&Alternatives> {
        self.get_variant(platform, name, variant)
            .and_then(|l| l.mappings.get(ch))
    }
}

fn collect_json_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    if !dir.is_dir() {
        return Err(anyhow!("{} is not a directory", dir.display()));
    }
    for entry in std::fs::read_dir(dir)
        .with_context(|| format!("reading directory {}", dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_json_files(&path, out)?;
        } else if path.extension().and_then(|s| s.to_str()) == Some("json") {
            out.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn us_layout() -> LayoutFile {
        let mut mappings = HashMap::new();
        mappings.insert(
            "a".to_string(),
            vec![Keystroke::single("KeyA", vec![])],
        );
        mappings.insert(
            "A".to_string(),
            vec![Keystroke::single("KeyA", vec!["Shift".to_string()])],
        );
        mappings.insert(
            "1".to_string(),
            vec![Keystroke::single("Digit1", vec![])],
        );
        mappings.insert(
            "!".to_string(),
            vec![Keystroke::single("Digit1", vec!["Shift".to_string()])],
        );
        // Dead key: acute accent (`Quote` + base) on US-International.
        mappings.insert(
            "á".to_string(),
            vec![Keystroke::dead_key(
                "Quote",
                vec![],
                "KeyA",
                vec![],
            )],
        );
        LayoutFile {
            metadata: LayoutMetadata {
                platform: "linux".to_string(),
                layout_name: "us".to_string(),
                layout_variant: Some("intl".to_string()),
                display_name: Some("English (US, intl)".to_string()),
                extracted_on: "2026-07-06T22:42:04Z".to_string(),
            },
            mappings,
        }
    }

    #[test]
    fn keypress_validate_accepts_known_modifiers() {
        let kp = KeyPress::with_modifiers(
            "KeyA",
            vec!["Shift".to_string(), "AltGraph".to_string()],
        );
        assert!(kp.validate().is_ok());
    }

    #[test]
    fn keypress_validate_rejects_unknown_modifier() {
        let kp = KeyPress::with_modifiers("KeyA", vec!["Hyper".to_string()]);
        assert!(kp.validate().is_err());
    }

    #[test]
    fn keystroke_validate_rejects_empty_sequence() {
        let ks = Keystroke { sequence: vec![] };
        assert!(ks.validate().is_err());
    }

    #[test]
    fn layout_validate_rejects_empty_target() {
        let mut bad = us_layout();
        bad.mappings.insert("".to_string(), vec![Keystroke::single("KeyA", vec![])]);
        assert!(bad.validate().is_err());
    }

    #[test]
    fn layout_validate_rejects_empty_alternatives() {
        let mut bad = us_layout();
        bad.mappings.insert("z".to_string(), vec![]);
        assert!(bad.validate().is_err());
    }

    #[test]
    fn index_key_with_and_without_variant() {
        assert_eq!(KeymapDb::index_key("linux", "us", None), "linux/us");
        assert_eq!(
            KeymapDb::index_key("linux", "us", Some("intl")),
            "linux/us+intl"
        );
        assert_eq!(
            KeymapDb::index_key("linux", "us", Some("")),
            "linux/us"
        );
    }

    #[test]
    fn insert_and_lookup_with_variant() {
        let mut db = KeymapDb::new();
        let key = db.insert(us_layout()).unwrap();
        assert_eq!(key, "linux/us+intl");
        let alts = db.lookup_variant("linux", "us", Some("intl"), "a").unwrap();
        assert_eq!(alts.len(), 1);
        assert_eq!(
            alts[0],
            Keystroke::single("KeyA", vec![])
        );
    }

    #[test]
    fn lookup_returns_none_for_missing_character() {
        let mut db = KeymapDb::new();
        db.insert(us_layout()).unwrap();
        assert!(db.lookup_variant("linux", "us", Some("intl"), "Q").is_none());
    }

    #[test]
    fn dead_key_sequence_round_trips() {
        let layout = us_layout();
        let alts = layout.mappings.get("á").unwrap();
        assert_eq!(alts.len(), 1);
        assert_eq!(alts[0].sequence.len(), 2);
        assert_eq!(alts[0].sequence[0], KeyPress::new("Quote"));
        assert_eq!(alts[0].sequence[1], KeyPress::new("KeyA"));
    }

    #[test]
    fn load_dir_reads_nested_json_files() {
        let tmp = tempfile::tempdir().unwrap();
        let linux_dir = tmp.path().join("linux");
        std::fs::create_dir_all(&linux_dir).unwrap();
        let layout = us_layout();
        let raw = serde_json::to_string_pretty(&layout).unwrap();
        let path = linux_dir.join("us+intl.json");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(raw.as_bytes()).unwrap();

        let db = KeymapDb::load_dir(tmp.path()).unwrap();
        assert_eq!(db.len(), 1);
        assert!(db.get_variant("linux", "us", Some("intl")).is_some());
    }

    #[test]
    fn load_dir_errors_on_empty_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let res = KeymapDb::load_dir(tmp.path());
        assert!(res.is_err());
    }

    #[test]
    fn load_file_rejects_invalid_metadata() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("bad.json");
        let bad = serde_json::json!({
            "metadata": {
                "platform": "",
                "layout_name": "us",
                "extracted_on": "2026-07-06T22:42:04Z"
            },
            "mappings": {}
        });
        std::fs::write(&path, bad.to_string()).unwrap();
        assert!(KeymapDb::load_file(&path).is_err());
    }

    #[test]
    fn display_name_optional_field_round_trips() {
        let mut layout = us_layout();
        layout.metadata.display_name = None;
        let raw = serde_json::to_string(&layout).unwrap();
        assert!(
            !raw.contains("display_name"),
            "absent display_name should be skipped: {raw}"
        );
        let back: LayoutFile = serde_json::from_str(&raw).unwrap();
        assert_eq!(back.metadata.display_name, None);

        layout.metadata.display_name = Some("English (United States)".to_string());
        let raw2 = serde_json::to_string(&layout).unwrap();
        assert!(raw2.contains("\"display_name\":\"English (United States)\""));
        let back2: LayoutFile = serde_json::from_str(&raw2).unwrap();
        assert_eq!(
            back2.metadata.display_name.as_deref(),
            Some("English (United States)")
        );
    }

    #[test]
    fn serialize_deserialize_round_trip() {
        let layout = us_layout();
        let raw = serde_json::to_string(&layout).unwrap();
        let back: LayoutFile = serde_json::from_str(&raw).unwrap();
        assert_eq!(layout, back);
    }

    #[test]
    fn load_generated_linux_database() {
        // Loads the real database/linux/ directory produced by omni-keymap-extract.
        // Skipped when the database has not been generated yet (e.g. fresh checkout before
        // extraction).
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../database/linux");
        if !dir.is_dir() || std::fs::read_dir(&dir).ok().map_or(true, |mut r| r.next().is_none()) {
            eprintln!("skipping load_generated_linux_database: {} missing or empty", dir.display());
            return;
        }
        let db = KeymapDb::load_dir(&dir).expect("generated database must load");
        assert!(db.len() >= 1, "expected at least one linux layout");
        // The US layout's 'a' must map to a KeyA keystroke with no modifiers.
        let us = db.get("linux", "us").expect("linux/us layout present");
        let alts = us.mappings.get("a").expect("'a' mapping present");
        assert!(!alts.is_empty());
        let first = &alts[0].sequence[0];
        assert_eq!(first.key, "KeyA");
        assert!(first.modifiers.is_empty());
    }
}