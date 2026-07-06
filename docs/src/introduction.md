# Introduction

OmniKeymap is a cross-platform library and database for **layout-aware keystroke translation**.

## Motivation

Software that synthesizes or remaps keyboard input — automation tools, accessibility software,
remote-desktop clients, IME testing harnesses — needs to answer a single question:

> *Given a target character and a keyboard layout, which physical keys and modifiers must be
> pressed to produce that character?*

Every operating system answers this differently: Windows uses `ToUnicodeEx`, macOS uses
`UCKeyTranslate` via Carbon, Linux uses `libxkbcommon`, and Android ships `.kl`/`.kcm` text files
in AOSP. OmniKeymap unifies these into a single JSON database and a Rust library that performs the
reverse lookup, including dead-key compositions.

## Architecture

The repository has three layers:

```
┌──────────────────────────────────────────────────────────────────┐
│ omni-keymap-extract (CLI)                                        │
│   windows.rs  macos.rs  linux.rs  android.rs   -> JSON layout     │
└──────────────────────────────────────────────────────────────────┘
                              │ writes
                              ▼
┌──────────────────────────────────────────────────────────────────┐
│ database/  (JSON layout files, one per platform layout)          │
│   windows/  macos/  linux/  android/                              │
└──────────────────────────────────────────────────────────────────┘
                              │ read by
                              ▼
┌──────────────────────────────────────────────────────────────────┐
│ omni-keymap-core (library)                                       │
│   LayoutFile  KeymapDb  Keystroke  KeyPress   -> char lookup      │
└──────────────────────────────────────────────────────────────────┘
```

1. **`omni-keymap-extract`** runs on a target operating system (or parses Android files offline)
   and queries the native keyboard-mapping APIs under a matrix of modifier states. It emits one
   normalized JSON file per layout into `database/`.
2. **`database/`** holds the extracted layouts, committed to version control so consumers don't
   need to run extraction themselves.
3. **`omni-keymap-core`** loads those JSON files into a `KeymapDb` and provides
   `lookup(platform, layout, character) -> &[Keystroke]`.

## W3C key codes

Physical keys are identified by [W3C `KeyboardEvent.code`][w3c] names (`KeyA`, `Digit1`, `Quote`,
…). The extractor maintains a static table mapping each W3C code to the platform-specific scan
code / keycode used to query the native APIs.

[w3c]: https://www.w3.org/TR/uievents-code/

## Dead keys

A dead key is a modifier key that does not produce a character on its own but instead alters the
next keypress (e.g. `´` + `a` → `á`). OmniKeymap represents a dead-key composition as a
multi-element `Keystroke` sequence: the first element is the dead-modifier key, the second is the
base key. See [JSON Schema](./schema.md) for the exact representation.