//! Static mapping table from W3C `KeyboardEvent.code` values to platform-specific scan codes
//! and keycodes.
//!
//! Sources:
//! - Windows scancodes: USB HID/PS2 Set 1 scancodes (the values returned by
//!   `MapVirtualKeyExW(..., MAPVK_VSC_TO_VK_EX, ...)`).
//! - macOS keycodes: Apple Carbon `kVK_...` virtual keycodes.
//! - Linux keycodes: the Linux input subsystem `input-event-codes.h` `KEY_*` values, which
//!   match the X11 keysym column used by `xkbcommon` when configured with
//!   `XKB_KEY_*` derived from the evdev keycodes.

/// A single row of the W3C-to-platform mapping table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct W3cKey {
    /// W3C `KeyboardEvent.code` string, e.g. `KeyA`, `Digit1`.
    pub code: &'static str,
    /// Windows Set 1 scan code (hex).
    pub windows_scancode: u32,
    /// macOS virtual keycode.
    pub macos_keycode: u32,
    /// Linux evdev keycode (also the X11 keycode used by xkbcommon on Linux).
    pub linux_keycode: u32,
}

/// The full static table. Sorted by `code` for binary search; kept in W3C logical order.
pub static W3C_KEYS: &[W3cKey] = &[
    W3cKey { code: "Backquote", windows_scancode: 0x29, macos_keycode: 50, linux_keycode: 41 },
    W3cKey { code: "Digit1", windows_scancode: 0x02, macos_keycode: 18, linux_keycode: 2 },
    W3cKey { code: "Digit2", windows_scancode: 0x03, macos_keycode: 19, linux_keycode: 3 },
    W3cKey { code: "Digit3", windows_scancode: 0x04, macos_keycode: 20, linux_keycode: 4 },
    W3cKey { code: "Digit4", windows_scancode: 0x05, macos_keycode: 21, linux_keycode: 5 },
    W3cKey { code: "Digit5", windows_scancode: 0x06, macos_keycode: 23, linux_keycode: 6 },
    W3cKey { code: "Digit6", windows_scancode: 0x07, macos_keycode: 22, linux_keycode: 7 },
    W3cKey { code: "Digit7", windows_scancode: 0x08, macos_keycode: 26, linux_keycode: 8 },
    W3cKey { code: "Digit8", windows_scancode: 0x09, macos_keycode: 28, linux_keycode: 9 },
    W3cKey { code: "Digit9", windows_scancode: 0x0A, macos_keycode: 25, linux_keycode: 10 },
    W3cKey { code: "Digit0", windows_scancode: 0x0B, macos_keycode: 29, linux_keycode: 11 },
    W3cKey { code: "Minus", windows_scancode: 0x0C, macos_keycode: 27, linux_keycode: 12 },
    W3cKey { code: "Equal", windows_scancode: 0x0D, macos_keycode: 24, linux_keycode: 13 },
    W3cKey { code: "KeyQ", windows_scancode: 0x10, macos_keycode: 12, linux_keycode: 16 },
    W3cKey { code: "KeyW", windows_scancode: 0x11, macos_keycode: 13, linux_keycode: 17 },
    W3cKey { code: "KeyE", windows_scancode: 0x12, macos_keycode: 14, linux_keycode: 18 },
    W3cKey { code: "KeyR", windows_scancode: 0x13, macos_keycode: 15, linux_keycode: 19 },
    W3cKey { code: "KeyT", windows_scancode: 0x14, macos_keycode: 17, linux_keycode: 20 },
    W3cKey { code: "KeyY", windows_scancode: 0x15, macos_keycode: 16, linux_keycode: 21 },
    W3cKey { code: "KeyU", windows_scancode: 0x16, macos_keycode: 32, linux_keycode: 22 },
    W3cKey { code: "KeyI", windows_scancode: 0x17, macos_keycode: 34, linux_keycode: 23 },
    W3cKey { code: "KeyO", windows_scancode: 0x18, macos_keycode: 31, linux_keycode: 24 },
    W3cKey { code: "KeyP", windows_scancode: 0x19, macos_keycode: 35, linux_keycode: 25 },
    W3cKey { code: "BracketLeft", windows_scancode: 0x1A, macos_keycode: 33, linux_keycode: 26 },
    W3cKey { code: "BracketRight", windows_scancode: 0x1B, macos_keycode: 30, linux_keycode: 27 },
    W3cKey { code: "Backslash", windows_scancode: 0x2B, macos_keycode: 42, linux_keycode: 43 },
    W3cKey { code: "KeyA", windows_scancode: 0x1E, macos_keycode: 0, linux_keycode: 30 },
    W3cKey { code: "KeyS", windows_scancode: 0x1F, macos_keycode: 1, linux_keycode: 31 },
    W3cKey { code: "KeyD", windows_scancode: 0x20, macos_keycode: 2, linux_keycode: 32 },
    W3cKey { code: "KeyF", windows_scancode: 0x21, macos_keycode: 3, linux_keycode: 33 },
    W3cKey { code: "KeyG", windows_scancode: 0x22, macos_keycode: 5, linux_keycode: 34 },
    W3cKey { code: "KeyH", windows_scancode: 0x23, macos_keycode: 4, linux_keycode: 35 },
    W3cKey { code: "KeyJ", windows_scancode: 0x24, macos_keycode: 38, linux_keycode: 36 },
    W3cKey { code: "KeyK", windows_scancode: 0x25, macos_keycode: 40, linux_keycode: 37 },
    W3cKey { code: "KeyL", windows_scancode: 0x26, macos_keycode: 37, linux_keycode: 38 },
    W3cKey { code: "Semicolon", windows_scancode: 0x27, macos_keycode: 41, linux_keycode: 39 },
    W3cKey { code: "Quote", windows_scancode: 0x28, macos_keycode: 39, linux_keycode: 40 },
    W3cKey { code: "KeyZ", windows_scancode: 0x2C, macos_keycode: 6, linux_keycode: 44 },
    W3cKey { code: "KeyX", windows_scancode: 0x2D, macos_keycode: 7, linux_keycode: 45 },
    W3cKey { code: "KeyC", windows_scancode: 0x2E, macos_keycode: 8, linux_keycode: 46 },
    W3cKey { code: "KeyV", windows_scancode: 0x2F, macos_keycode: 9, linux_keycode: 47 },
    W3cKey { code: "KeyB", windows_scancode: 0x30, macos_keycode: 11, linux_keycode: 48 },
    W3cKey { code: "KeyN", windows_scancode: 0x31, macos_keycode: 45, linux_keycode: 49 },
    W3cKey { code: "KeyM", windows_scancode: 0x32, macos_keycode: 46, linux_keycode: 50 },
    W3cKey { code: "Comma", windows_scancode: 0x33, macos_keycode: 43, linux_keycode: 51 },
    W3cKey { code: "Period", windows_scancode: 0x34, macos_keycode: 47, linux_keycode: 52 },
    W3cKey { code: "Slash", windows_scancode: 0x35, macos_keycode: 44, linux_keycode: 53 },
    W3cKey { code: "IntlBackslash", windows_scancode: 0x56, macos_keycode: 10, linux_keycode: 86 },
    W3cKey { code: "IntlRo", windows_scancode: 0x73, macos_keycode: 94, linux_keycode: 89 },
    W3cKey { code: "IntlYen", windows_scancode: 0x7D, macos_keycode: 93, linux_keycode: 124 },
    W3cKey { code: "Space", windows_scancode: 0x39, macos_keycode: 49, linux_keycode: 57 },
];

/// Look up a [`W3cKey`] by its W3C `code` string.
#[allow(dead_code)]
pub fn lookup(code: &str) -> Option<&'static W3cKey> {
    W3C_KEYS.iter().find(|k| k.code == code)
}

#[allow(dead_code)]
pub fn all_codes() -> Vec<&'static str> {
    W3C_KEYS.iter().map(|k| k.code).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_known_code() {
        let k = lookup("KeyA").unwrap();
        assert_eq!(k.windows_scancode, 0x1E);
        assert_eq!(k.macos_keycode, 0);
        assert_eq!(k.linux_keycode, 30);
    }

    #[test]
    fn lookup_unknown_code_returns_none() {
        assert!(lookup("Nope").is_none());
    }

    #[test]
    fn table_covers_expected_count() {
        // The rows explicitly enumerated in the plan table, including the Intl* keys.
        assert_eq!(W3C_KEYS.len(), 51);
    }

    #[test]
    fn codes_are_unique() {
        let codes = all_codes();
        let mut sorted = codes.clone();
        sorted.sort();
        let mut deduped = sorted.clone();
        deduped.dedup();
        assert_eq!(sorted.len(), deduped.len(), "duplicate W3C codes in table");
    }
}