# Extraction

`omni-keymap-extract` is the CLI that produces the JSON layout files under `database/`. It
queries native keyboard APIs on the host operating system (or parses Android files offline) and
emits a normalized [`LayoutFile`](./schema.md).

## Common arguments

```
omni-keymap-extract --platform <windows|macos|linux|android> \
    --out-dir <dir> \
    [--layout <name>] [--layout-variant <variant>] \
    [--all] \
    [--android-kl <path>] [--android-kcm <path>]
```

| Flag              | Required for         | Description                                                  |
|-------------------|----------------------|--------------------------------------------------------------|
| `--platform`      | all                  | Target platform.                                             |
| `--out-dir`       | all                  | Directory to write the JSON file into.                       |
| `--layout`        | all (unless `--all`) | Layout name (e.g. `us`, `fr`).                              |
| `--layout-variant`| optional             | Variant (e.g. `intl`, `nodeadkeys`).                         |
| `--all`           | optional (Linux)     | Extract every layout/variant from `evdev.lst` into `--out-dir`. |
| `--android-kl`    | `android`            | Path to an Android `.kl` file.                              |
| `--android-kcm`   | `android`            | Path to an Android `.kcm` file.                             |

The output file is named `<layout>[+<variant>].json` inside `--out-dir`.

## Linux

Linux extraction uses [`libxkbcommon`][xkbcommon] to compile an XKB keymap for the requested
layout/variant and query `xkb_state_key_get_utf8` under four modifier states:

1. None
2. Shift
3. AltGr (`ISO_Level3_Shift`)
4. Shift + AltGr

Dead keys are detected by a keysym name starting with `dead_`. For each dead key, the extractor
re-feeds the dead state and presses every other standard key to discover the resulting composed
character, emitting it as a two-element `Keystroke` sequence.

### Run on Linux

```sh
cargo run -p omni-keymap-extract -- \
    --platform linux --layout us --out-dir database/linux
# -> database/linux/us.json

cargo run -p omni-keymap-extract -- \
    --platform linux --layout us --layout-variant intl --out-dir database/linux
# -> database/linux/us+intl.json
```

`libxkbcommon` must be installed and discoverable by the linker. On NixOS:

```sh
nix shell nixpkgs#cargo nixpkgs#rustc nixpkgs#gcc nixpkgs#pkg-config nixpkgs#libxkbcommon \
  --command cargo run -p omni-keymap-extract -- --platform linux --layout us --out-dir database/linux
```

### Extract every layout (`--all`)

`--all` parses the XKB `evdev.lst` rules file and extracts every declared layout and variant
into `--out-dir`, one JSON file each. Layouts that fail to compile (e.g. `custom`, which has no
symbols file) are skipped and reported; the run does not abort.

```sh
cargo run -p omni-keymap-extract -- --platform linux --all --out-dir database/linux
```

The rules file is located via `$XKB_CONFIG_ROOT/rules/evdev.lst`, then `$XDG_DATA_DIRS`, then
`/usr/share/X11/xkb/rules/evdev.lst`. On NixOS, point `XKB_CONFIG_ROOT` at the `xkeyboard-config`
data directory:

```sh
nix shell nixpkgs#cargo nixpkgs#rustc nixpkgs#gcc nixpkgs#libxkbcommon nixpkgs#xkeyboard_config \
  --command env XKB_CONFIG_ROOT=$(nix path-info nixpkgs#xkeyboard_config)/share/xkeyboard-config-2 \
  cargo run -p omni-keymap-extract -- --platform linux --all --out-dir database/linux
```

## Windows

Windows extraction uses [`windows-sys`][windows-sys] to call `MapVirtualKeyExW` (scancode → VK) and
`ToUnicodeEx` (VK + modifiers → character) for each W3C key under four modifier states:

1. None
2. Shift (`VK_SHIFT`)
3. AltGraph (`VK_CONTROL` + `VK_MENU` — Windows sends AltGr as Ctrl+Alt)
4. Shift + AltGraph

Dead keys are detected when `ToUnicodeEx` returns a negative result; the extractor then simulates
combinations of the dead key with all other standard keys.

Windows extraction only runs on a Windows host. Build with:

```powershell
cargo run -p omni-keymap-extract -- --platform windows --layout 00000409 --out-dir database\windows
```

## macOS

macOS extraction uses Carbon's `UCKeyTranslate` via the current keyboard layout input source. For
each W3C keycode it queries under Shift, Option, and Shift+Option, tracking the
`deadKeyState` output parameter to drive second-stage composition.

macOS extraction only runs on a macOS host. Build with:

```sh
cargo run -p omni-keymap-extract -- --platform macos --layout U.S. --out-dir database/macos
```

## Android (offline)

Android extraction is **offline**: it parses AOSP `.kl` (key layout) and `.kcm` (key character
map) text files directly, with no device access required. The `.kl` parser maps Linux evdev
keycodes to Android keycode names; the `.kcm` parser resolves per-modifier-state character
mappings and dead-key markers. Because Android's evdev keycodes correspond 1:1 to the W3C
keycode column in [`w3c_keys.rs`][w3c], the reverse mapping is direct.

```sh
cargo run -p omni-keymap-extract -- \
    --platform android --layout generic \
    --android-kl /path/to/Generic.kl \
    --android-kcm /path/to/Generic.kcm \
    --out-dir database/android
# -> database/android/generic.json
```

`.kl` and `.kcm` files can be obtained from the [AOSP `frameworks/base`][aosp-kbd] repository
(`data/keyboards/`).

## Contributing a layout

1. Build the workspace: `make build`.
2. Run the extractor for your platform and layout (see above).
3. Verify the generated JSON file appears under `database/<platform>/`.
4. Commit it and open a pull request. CI will run `cargo build`, `cargo test`, and `cargo clippy`.
5. Pull requests require at least one reviewer approval.

[xkbcommon]: https://github.com/xkbcommon/libxkbcommon
[windows-sys]: https://crates.io/crates/windows-sys
[aosp-kbd]: https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/data/keyboards/
[w3c]: https://www.w3.org/TR/uievents-code/