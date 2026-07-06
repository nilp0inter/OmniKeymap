//! Batch extraction: enumerate every layout and variant declared in the XKB `evdev.lst` rules
//! file and extract each into the output directory.
//!
//! The `evdev.lst` file lives under `$XKB_CONFIG_ROOT/rules/evdev.lst` (or the standard
//! `/usr/share/X11/xkb/rules/evdev.lst`). It has two sections we care about:
//!
//! ```text
//! ! layout
//!   us  English (US)
//!   fr  French
//!   ...
//! ! variant
//!   nodeadkeys  fr: French (no dead keys)
//!   intl        us: English (US, intl., with dead keys)
//!   ...
//! ```
//!
//! We emit one JSON file per layout (`<layout>.json`) and one per variant
//! (`<layout>+<variant>.json`). Layouts/variants that fail to compile are reported and skipped so
//! a single broken entry does not abort the whole run.

use anyhow::{Context, Result, anyhow};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// One `(layout, description)` row from the `! layout` section of `evdev.lst`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutEntry {
    pub name: String,
    pub description: String,
}

/// One `(variant, parent_layout, description)` row from the `! variant` section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantEntry {
    pub variant: String,
    pub layout: String,
    pub description: String,
}

/// The parsed contents of `evdev.lst`: the layout list and the variant list.
#[derive(Debug, Clone, Default)]
pub struct EvdevList {
    pub layouts: Vec<LayoutEntry>,
    pub variants: Vec<VariantEntry>,
}

impl EvdevList {
    /// Parse the raw text of an `evdev.lst` (or `base.lst`) rules file.
    pub fn parse(raw: &str) -> Result<Self> {
        let mut section = String::new();
        let mut layouts = Vec::new();
        let mut variants = Vec::new();
        for line in raw.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('!') {
                section = trimmed.trim_start_matches('!').trim().to_string();
                continue;
            }
            if trimmed.is_empty() {
                continue;
            }
            // Each data row is `<token(s)> <description>`; tokens are whitespace-separated and
            // the description is the trailing free text. For the variant section the second
            // token is `<layout>:` (with a trailing colon).
            let mut parts = trimmed.split_whitespace();
            match section.as_str() {
                "layout" => {
                    let name = parts.next().unwrap_or("");
                    if name.is_empty() {
                        continue;
                    }
                    let description = trimmed
                        .trim_start_matches(|c: char| !c.is_whitespace())
                        .trim_start()
                        .to_string();
                    layouts.push(LayoutEntry {
                        name: name.to_string(),
                        description,
                    });
                }
                "variant" => {
                    let variant = parts.next().unwrap_or("");
                    let layout_colon = parts.next().unwrap_or("");
                    if variant.is_empty() || layout_colon.is_empty() {
                        continue;
                    }
                    let layout = layout_colon.trim_end_matches(':').to_string();
                    if layout.is_empty() {
                        continue;
                    }
                    // Description is the remainder of the line after the two tokens.
                    let mut rest = trimmed;
                    for _ in 0..2 {
                        rest = rest.trim_start();
                        rest = rest.trim_start_matches(|c: char| !c.is_whitespace());
                    }
                    let description = rest.trim_start().to_string();
                    variants.push(VariantEntry {
                        variant: variant.to_string(),
                        layout,
                        description,
                    });
                }
                _ => {}
            }
        }
        if layouts.is_empty() {
            return Err(anyhow!(
                "evdev.lst parse: no `! layout` section found (or empty)"
            ));
        }
        Ok(EvdevList { layouts, variants })
    }

    /// Group variants by their parent layout, returning a sorted map `layout -> [variants]`.
    pub fn variants_by_layout(&self) -> BTreeMap<String, Vec<&VariantEntry>> {
        let mut map: BTreeMap<String, Vec<&VariantEntry>> = BTreeMap::new();
        for v in &self.variants {
            map.entry(v.layout.clone()).or_default().push(v);
        }
        map
    }
}

/// Locate the `evdev.lst` rules file.
///
/// Search order:
/// 1. `$XKB_CONFIG_ROOT/rules/evdev.lst`
/// 2. `$XDG_DATA_DIRS` colon-separated entries, each `<dir>/X11/xkb/rules/evdev.lst`
/// 3. `/usr/share/X11/xkb/rules/evdev.lst`
///
/// Returns the first path that exists and is readable.
pub fn find_evdev_lst() -> Result<PathBuf> {
    let candidates: Vec<PathBuf> = std::env::var("XKB_CONFIG_ROOT")
        .ok()
        .map(|r| Path::new(&r).join("rules").join("evdev.lst"))
        .into_iter()
        .chain(
            std::env::var("XDG_DATA_DIRS")
                .ok()
                .map(|d| {
                    d.split(':')
                        .map(|p| Path::new(p).join("X11").join("xkb").join("rules").join("evdev.lst"))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
        )
        .chain(std::iter::once(
            Path::new("/usr/share/X11/xkb/rules/evdev.lst").to_path_buf(),
        ))
        .collect();
    for c in candidates {
        if c.is_file() {
            return Ok(c);
        }
    }
    Err(anyhow!(
        "could not find evdev.lst; set XKB_CONFIG_ROOT to the XKB data directory \
         (e.g. .../share/xkeyboard-config-2 on NixOS)"
    ))
}

/// Outcome of extracting a single layout in a batch run.
#[derive(Debug, Clone)]
pub struct BatchItem {
    pub layout: String,
    pub variant: Option<String>,
    pub status: BatchStatus,
}

/// Status of one batch item.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum BatchStatus {
    /// Wrote `<out-dir>/<stem>.json` with N mappings.
    Ok { mappings: usize, path: PathBuf },
    /// The layout/variant failed to compile and was skipped.
    Skipped { error: String },
}

/// Summary of a batch run.
#[derive(Debug, Clone, Default)]
pub struct BatchSummary {
    pub ok: usize,
    pub skipped: usize,
    pub total_mappings: usize,
    pub failures: Vec<BatchItem>,
}

impl std::fmt::Display for BatchSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} ok, {} skipped, {} total mappings",
            self.ok, self.skipped, self.total_mappings
        )?;
        if !self.failures.is_empty() {
            write!(f, "\nfailed layouts:")?;
            for item in &self.failures {
                let stem = match &item.variant {
                    Some(v) => format!("{}+{}", item.layout, v),
                    None => item.layout.clone(),
                };
                if let BatchStatus::Skipped { error } = &item.status {
                    write!(f, "\n  {}: {}", stem, error)?;
                }
            }
        }
        Ok(())
    }
}

/// Extract every layout and variant listed in `evdev.lst` into `out_dir`.
///
/// Each layout produces `<out_dir>/<layout>.json`; each variant produces
/// `<out_dir>/<layout>+<variant>.json`. Failures are recorded and do not abort the run.
pub fn extract_all(out_dir: &Path) -> Result<BatchSummary> {
    let lst_path = find_evdev_lst()?;
    let raw = std::fs::read_to_string(&lst_path)
        .with_context(|| format!("reading {}", lst_path.display()))?;
    let evdev = EvdevList::parse(&raw)?;
    std::fs::create_dir_all(out_dir)
        .with_context(|| format!("creating output directory {}", out_dir.display()))?;

    let mut summary = BatchSummary::default();
    let variants_by_layout = evdev.variants_by_layout();

    for layout in &evdev.layouts {
        // Base layout (no variant).
        let item = extract_one(out_dir, &layout.name, None);
        record(&mut summary, &item);

        // Every variant registered against this layout.
        if let Some(vars) = variants_by_layout.get(&layout.name) {
            for v in vars {
                let item = extract_one(out_dir, &layout.name, Some(&v.variant));
                record(&mut summary, &item);
            }
        }
    }
    Ok(summary)
}

fn extract_one(out_dir: &Path, layout: &str, variant: Option<&str>) -> BatchItem {
    match crate::linux::extract(layout, variant) {
        Ok(file) => {
            let file_stem = match variant {
                Some(v) if !v.is_empty() => format!("{}+{}", layout, v),
                _ => layout.to_string(),
            };
            let path = out_dir.join(format!("{}.json", file_stem));
            let n = file.mappings.len();
            let raw = match serde_json::to_string_pretty(&file) {
                Ok(s) => s,
                Err(e) => {
                    return BatchItem {
                        layout: layout.to_string(),
                        variant: variant.map(|s| s.to_string()),
                        status: BatchStatus::Skipped {
                            error: format!("serialize: {}", e),
                        },
                    };
                }
            };
            if let Err(e) = std::fs::write(&path, raw) {
                return BatchItem {
                    layout: layout.to_string(),
                    variant: variant.map(|s| s.to_string()),
                    status: BatchStatus::Skipped {
                        error: format!("write {}: {}", path.display(), e),
                    },
                };
            }
            BatchItem {
                layout: layout.to_string(),
                variant: variant.map(|s| s.to_string()),
                status: BatchStatus::Ok { mappings: n, path },
            }
        }
        Err(e) => BatchItem {
            layout: layout.to_string(),
            variant: variant.map(|s| s.to_string()),
            status: BatchStatus::Skipped {
                error: e.to_string(),
            },
        },
    }
}

fn record(summary: &mut BatchSummary, item: &BatchItem) {
    match &item.status {
        BatchStatus::Ok { mappings, .. } => {
            summary.ok += 1;
            summary.total_mappings += mappings;
        }
        BatchStatus::Skipped { .. } => {
            summary.skipped += 1;
            summary.failures.push(item.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LST_SAMPLE: &str = "\
// comment
! layout
  us  English (US)
  fr  French
  de  German
! variant
  nodeadkeys  fr: French (no dead keys)
  intl        us: English (US, intl., with dead keys)
  deadacute   de: German (dead acute)
! option
  some:opt description
";

    #[test]
    fn parse_layouts_and_variants() {
        let ev = EvdevList::parse(LST_SAMPLE).unwrap();
        assert_eq!(ev.layouts.len(), 3);
        assert_eq!(ev.layouts[0].name, "us");
        assert_eq!(ev.layouts[1].name, "fr");
        assert_eq!(ev.variants.len(), 3);
        assert_eq!(ev.variants[0].variant, "nodeadkeys");
        assert_eq!(ev.variants[0].layout, "fr");
        assert_eq!(ev.variants[1].variant, "intl");
        assert_eq!(ev.variants[1].layout, "us");
        assert_eq!(ev.variants[2].layout, "de");
    }

    #[test]
    fn variants_by_layout_groups_correctly() {
        let ev = EvdevList::parse(LST_SAMPLE).unwrap();
        let map = ev.variants_by_layout();
        assert_eq!(map.get("fr").unwrap().len(), 1);
        assert_eq!(map.get("us").unwrap().len(), 1);
        assert_eq!(map.get("de").unwrap().len(), 1);
        assert!(map.get("jp").is_none());
    }

    #[test]
    fn parse_errors_on_missing_layout_section() {
        let bad = "! variant\n  foo  bar: desc\n";
        assert!(EvdevList::parse(bad).is_err());
    }

    #[test]
    fn parse_skips_unknown_sections() {
        let raw = "! layout\n  us  English (US)\n! option\n  foo:bar baz\n";
        let ev = EvdevList::parse(raw).unwrap();
        assert_eq!(ev.layouts.len(), 1);
        assert_eq!(ev.variants.len(), 0);
    }

    #[test]
    fn variant_description_is_captured() {
        let ev = EvdevList::parse(LST_SAMPLE).unwrap();
        assert_eq!(ev.variants[1].description, "English (US, intl., with dead keys)");
    }

    #[test]
    fn batch_summary_display_lists_failures() {
        let mut s = BatchSummary::default();
        s.ok = 5;
        s.skipped = 1;
        s.failures.push(BatchItem {
            layout: "xx".to_string(),
            variant: None,
            status: BatchStatus::Skipped {
                error: "bad".to_string(),
            },
        });
        let out = s.to_string();
        assert!(out.contains("5 ok"));
        assert!(out.contains("xx"));
        assert!(out.contains("bad"));
    }

    #[test]
    fn find_evdev_lst_returns_error_when_absent() {
        // Remove any XKB_CONFIG_ROOT/XDG_DATA_DIRS so we fall through to the hard-coded path,
        // which does not exist on this host.
        std::env::remove_var("XKB_CONFIG_ROOT");
        std::env::remove_var("XDG_DATA_DIRS");
        let res = find_evdev_lst();
        // This test is best-effort: on a system with /usr/share/X11/xkb it would succeed.
        if std::path::Path::new("/usr/share/X11/xkb/rules/evdev.lst").is_file() {
            assert!(res.is_ok());
        } else {
            assert!(res.is_err());
        }
    }
}