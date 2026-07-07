//! omni-keymap-extract: CLI that emits normalized OmniKeymap JSON layout files by querying native
//! platform APIs (Windows, macOS, Linux) or parsing Android `.kl`/`.kcm` files offline.
//!
//! Usage:
//! ```text
//! omni-keymap-extract --platform <windows|macos|linux|android> --out-dir <dir> \
//!     --layout <name> [--layout-variant <variant>] \
//!     [--android-kl <path>] [--android-kcm <path>]
//! omni-keymap-extract --platform linux --all --out-dir <dir>
//! ```

mod android;
mod batch;
mod linux;
mod macos;
mod w3c_keys;
mod windows;

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use omni_keymap_core::LayoutFile;

/// Target platform for extraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Platform {
    /// Microsoft Windows (uses `ToUnicodeEx`/`MapVirtualKeyExW`).
    Windows,
    /// Apple macOS (uses `UCKeyTranslate` via Carbon).
    Macos,
    /// X11/Linux via `libxkbcommon`.
    Linux,
    /// Android (offline `.kl`/`.kcm` parsing).
    Android,
}

/// CLI arguments.
#[derive(Debug, Parser)]
#[command(
    name = "omni-keymap-extract",
    version,
    about = "Extract a keyboard layout into the OmniKeymap JSON format.",
    long_about = None,
)]
pub struct Cli {
    /// Target platform.
    #[arg(long, value_enum)]
    pub platform: Platform,

    /// Output directory where the JSON layout file will be written.
    #[arg(long)]
    pub out_dir: PathBuf,
    /// Layout name (e.g. `us`, `fr`, `de`). For Android this is used only as the file stem.
    /// Required unless `--all` is set.
    #[arg(long)]
    pub layout: Option<String>,

    /// Optional layout variant (e.g. `nodeadkeys`, `intl`).
    #[arg(long)]
    pub layout_variant: Option<String>,

    /// Extract every layout and variant listed in the XKB `evdev.lst` rules file into `--out-dir`.
    /// Linux platform only. Implies batch mode; `--layout`/`--layout-variant` are ignored.
    #[arg(long, default_value_t = false)]
    pub all: bool,

    /// Path to an Android `.kl` file (Android platform only).
    #[arg(long)]
    pub android_kl: Option<PathBuf>,

    /// Path to an Android `.kcm` file (Android platform only).
    #[arg(long)]
    pub android_kcm: Option<PathBuf>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    if cli.all {
        let summary = match cli.platform {
            Platform::Linux => batch::extract_all(&cli.out_dir)?,
            Platform::Windows => windows::extract_all(&cli.out_dir)?,
            Platform::Macos => macos::extract_all(&cli.out_dir)?,
            Platform::Android => {
                return Err(anyhow::anyhow!(
                    "--all is not supported for --platform android (use --android-kl/--android-kcm)"
                ));
            }
        };
        eprintln!("{}", summary);
        return Ok(());
    }
    let layout_name = cli
        .layout
        .clone()
        .ok_or_else(|| anyhow::anyhow!("--layout is required (or use --all)"))?;
    let layout = run_extraction(&cli, &layout_name)?;
    write_layout(&cli, &layout_name, &layout)?;
    Ok(())
}

fn run_extraction(cli: &Cli, layout_name: &str) -> Result<LayoutFile> {
    match cli.platform {
        Platform::Windows => windows::extract(layout_name, cli.layout_variant.as_deref()),
        Platform::Macos => macos::extract(layout_name, cli.layout_variant.as_deref()),
        Platform::Linux => linux::extract(layout_name, cli.layout_variant.as_deref(), None),
        Platform::Android => {
            let kl = cli
                .android_kl
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("--android-kl is required for platform android"))?;
            let kcm = cli
                .android_kcm
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("--android-kcm is required for platform android"))?;
            let mut layout = android::extract_from_files(kl, kcm)?;
            // Override the auto-derived metadata with the user-supplied layout name/variant.
            layout.metadata.layout_name = layout_name.to_string();
            layout.metadata.layout_variant = cli.layout_variant.clone();
            Ok(layout)
        }
    }
}

fn write_layout(cli: &Cli, layout_name: &str, layout: &LayoutFile) -> Result<()> {
    std::fs::create_dir_all(&cli.out_dir)
        .with_context(|| format!("creating output directory {}", cli.out_dir.display()))?;
    layout.validate()?;
    let file_stem = match &cli.layout_variant {
        Some(v) if !v.is_empty() => format!("{}+{}", layout_name, v),
        _ => layout_name.to_string(),
    };
    let out_path = cli.out_dir.join(format!("{}.json", file_stem));
    let raw = serde_json::to_string_pretty(layout)?;
    std::fs::write(&out_path, raw)
        .with_context(|| format!("writing layout file {}", out_path.display()))?;
    eprintln!("wrote {}", out_path.display());
    Ok(())
}

/// Current UTC time as an ISO-8601 string with `Z` suffix, e.g. `2026-07-06T22:42:04Z`.
pub(crate) fn now_iso8601() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    iso8601_from_unix(secs)
}

/// Convert a Unix timestamp to an ISO-8601 UTC string.
fn iso8601_from_unix(secs: u64) -> String {
    // Calendar conversion without external crates. Algorithm: Howard Hinnant's
    // `civil_from_days`.
    let days = (secs / 86400) as i64;
    let rem = (secs % 86400) as i64;
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let (y, mo, d) = civil_from_days(days);
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, mo, d, h, m, s)
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as i64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (y + if m <= 2 { 1 } else { 0 }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso8601_from_unix_known_value() {
        // 2026-07-06T22:42:04Z = 1783377724 seconds since epoch.
        assert_eq!(iso8601_from_unix(1783377724), "2026-07-06T22:42:04Z");
    }

    #[test]
    fn civil_from_days_epoch() {
        // 1970-01-01 is day 0.
        let (y, m, d) = civil_from_days(0);
        assert_eq!((y, m, d), (1970, 1, 1));
    }

    #[test]
    fn civil_from_days_leap_year() {
        // 2024-02-29 = day 19782.
        let (y, m, d) = civil_from_days(19782);
        assert_eq!((y, m, d), (2024, 2, 29));
    }

    #[test]
    fn cli_platform_enum_parses() {
        let p = Platform::from_str("linux", true).unwrap();
        assert_eq!(p, Platform::Linux);
    }
}