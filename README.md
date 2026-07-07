# OmniKeymap

A cross-platform library and database for **layout-aware keystroke translation**.

OmniKeymap maps Unicode characters to the keystroke sequences that produce them on a given
operating-system keyboard layout. It ships:

1. **`omni-keymap-core`** — a Rust library that loads the JSON layout database and performs
   character-to-keystroke lookups (including dead-key compositions).
2. **`omni-keymap-extract`** — a Rust CLI that queries native APIs on Windows, macOS, and Linux,
   and parses Android `.kl`/`.kcm` files offline, emitting normalized JSON layout files.
3. **`database/`** — the JSON layout database, organized by platform (`windows/`, `macos/`,
   `linux/`, `android/`).

## Repository layout

```
.
├── Cargo.toml                # Workspace manifest
├── Makefile                  # Build, test, extract, book targets
├── README.md
├── docs/                     # mdBook documentation
├── omni-keymap-core/         # Library crate: models, KeymapDb, lookup APIs
├── omni-keymap-extract/      # CLI extractor crate (Windows/macOS/Linux/Android)
└── database/                 # JSON layout files, one per platform layout
    ├── windows/
    ├── macos/
    ├── linux/
    └── android/
```

## JSON schema

Each layout file is a single JSON object:

```json
{
  "metadata": {
    "platform": "linux",
    "layout_name": "us",
    "layout_variant": "intl",
    "extracted_on": "2026-07-06T22:42:04Z"
  },
  "mappings": {
    "a": [
      { "sequence": [ { "key": "KeyA", "modifiers": [] } ] }
    ],
    "á": [
      { "sequence": [
          { "key": "Quote", "modifiers": [] },
          { "key": "KeyA", "modifiers": [] }
      ] }
    ]
  }
}
```

- `metadata.platform` — one of `windows`, `macos`, `linux`, `android`.
- `metadata.layout_name` — layout identifier (e.g. `us`, `fr`).
- `metadata.layout_variant` — optional variant (e.g. `intl`, `nodeadkeys`).
- `metadata.extracted_on` — ISO-8601 UTC timestamp.
- `mappings` — character (or short string) -> list of alternative keystroke sequences.
- Each `Keystroke.sequence` is an ordered list of `KeyPress` events. A single-element sequence
  is a direct key press; a multi-element sequence models a **dead-key composition** (first
  element is the dead-modifier key, subsequent elements are the base keys).
- `modifiers` values are drawn from the W3C set: `Shift`, `Control`, `Alt`, `AltGraph`, `Meta`.

## Build

The project is a Cargo workspace.

```sh
make build          # cargo build --workspace
make build -j8      # 8 parallel jobs
```

On NixOS, ad-hoc toolchains are available via `nix run`/`nix shell`, e.g.:

```sh
nix shell nixpkgs#cargo nixpkgs#rustc nixpkgs#gcc nixpkgs#libxkbcommon \
  --command cargo build --workspace
```

## Test

```sh
make test           # cargo test --workspace
pytest -v           # if Python glue tests are present
```

## Extract a layout

Run the extractor for your current operating system and submit the resulting JSON file in a pull
request.

```sh
# Linux (XKB)
cargo run -p omni-keymap-extract -- \
    --platform linux --layout us --out-dir database/linux

# Android (offline .kl/.kcm parsing)
cargo run -p omni-keymap-extract -- \
    --platform android --layout generic \
    --android-kl /path/to/Generic.kl --android-kcm /path/to/Generic.kcm \
    --out-dir database/android
```

### Extract every layout at once (`--all`)

`--all` extracts every layout available on the current host into `database/<platform>/`:

```sh
# Linux: every layout/variant from evdev.lst (xkbcommon)
cargo run -p omni-keymap-extract -- --platform linux --all --out-dir database/linux

# Windows: every registered KLID under HKLM Keyboard Layouts
cargo run -p omni-keymap-extract -- --platform windows --all --out-dir database/windows

# macOS: every installed layout (TISCreateInputSourceList)
cargo run -p omni-keymap-extract -- --platform macos --all --out-dir database/macos
```

On NixOS for Linux, set `XKB_CONFIG_ROOT` to the `xkeyboard-config` data directory (see
[`docs/src/extraction.md`](docs/src/extraction.md) for the full invocation).

### Regenerate all databases via CI

The `Regenerate layout databases` GitHub Actions workflow runs `--all` on `ubuntu-latest`,
`windows-latest`, and `macos-latest` runners in parallel and opens a PR with the merged result.
Trigger it from the **Actions** tab → **Run workflow**. See
[`docs/src/extraction.md`](docs/src/extraction.md) for details.

## Documentation

The full documentation is an mdBook under `docs/`. Build and open it with:

```sh
make book           # nix run nixpkgs#mdbook -- build docs
```

The generated HTML is written to `docs/book/`.

## Contributing a layout

1. Build the extractor: `make build`.
2. Run it for your platform and layout (see above).
3. Verify the output JSON under `database/<platform>/` parses with
   `cargo run -p omni-keymap-extract -- --help`.
4. Commit the new layout file and open a pull request.
5. Pull requests require at least one reviewer; CI runs build, test, and lint.

## Release

Tags follow Semantic Versioning:

```sh
git tag -a v1.2.3 -m 'Release v1.2.3'
git push origin master --tags
```

## License

Dual-licensed under MIT OR Apache-2.0.