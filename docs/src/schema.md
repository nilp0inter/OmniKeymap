# JSON Schema

Each file under `database/<platform>/` is a single JSON object conforming to the following
schema.

## Top-level object

```json
{
  "metadata": { ... },
  "mappings": { ... }
}
```

## `metadata`

| Field            | Type                | Required | Description                                                    |
|------------------|---------------------|----------|----------------------------------------------------------------|
| `platform`       | string              | yes      | One of `windows`, `macos`, `linux`, `android`.                  |
| `layout_name`    | string              | yes      | Layout identifier, e.g. `us`, `fr`, `de`.                      |
| `layout_variant` | string \| null      | no       | Variant, e.g. `intl`, `nodeadkeys`. `null` means no variant.    |
| `extracted_on`   | string              | yes      | ISO-8601 UTC timestamp of extraction, e.g. `2026-07-06T22:42:04Z`. |

```json
{
  "platform": "linux",
  "layout_name": "us",
  "layout_variant": "intl",
  "extracted_on": "2026-07-06T22:42:04Z"
}
```

## `mappings`

A JSON object whose keys are the **target character or short string** and whose values are arrays
of alternative [`Keystroke`](#keystroke) sequences that produce that target on the layout.

```json
{
  "a":  [ { "sequence": [ { "key": "KeyA", "modifiers": [] } ] } ],
  "A":  [ { "sequence": [ { "key": "KeyA", "modifiers": ["Shift"] } ] } ],
  "á":  [ { "sequence": [
              { "key": "Quote", "modifiers": [] },
              { "key": "KeyA",  "modifiers": [] }
          ] } ]
}
```

- A key can map to multiple alternatives; the first listed is the preferred sequence.
- A target may also be a short string (e.g. a dead-key composition result such as `"´a"`), though
  single characters are the common case.

## `Keystroke`

```json
{ "sequence": [ KeyPress, KeyPress, ... ] }
```

`sequence` is an ordered list of [`KeyPress`](#keypress) events.

- **Single-element sequence** — a direct key press.
- **Multi-element sequence** — a dead-key composition. The first element is the dead-modifier
  key (e.g. `Quote` on a US-International layout); subsequent elements are the base keys pressed
  while the dead state is active.

`sequence` MUST be non-empty.

## `KeyPress`

```json
{ "key": "KeyA", "modifiers": ["Shift"] }
```

| Field        | Type          | Required | Description                                                            |
|--------------|---------------|----------|------------------------------------------------------------------------|
| `key`        | string        | yes      | A W3C `KeyboardEvent.code` value, e.g. `KeyA`, `Digit1`, `Quote`.      |
| `modifiers`  | array&lt;string&gt; | yes      | Zero or more W3C modifier names (see below). May be empty `[]`.       |

## Modifier names

The modifier vocabulary is the W3C modifier set:

| Name        | Meaning                                                                 |
|-------------|-------------------------------------------------------------------------|
| `Shift`     | Left or right Shift.                                                    |
| `Control`   | Left or right Control.                                                  |
| `Alt`       | Left or right Alt (Option on macOS).                                    |
| `AltGraph`  | AltGr / ISO Level 3 Shift. On Windows this is physically Ctrl+Alt.       |
| `Meta`      | Left or right Meta (Super/Windows/Command).                             |

> **Windows AltGr note.** Windows sends AltGr as Ctrl+Alt at the hardware level. The extractor
> simulates Ctrl+Alt when calling `ToUnicodeEx` but records the result under the single modifier
> name `AltGraph` to remain platform-neutral in the JSON output.

## W3C key code representation

`key` values are strings from the [W3C `KeyboardEvent.code` specification][w3c-code]. The static
mapping from each W3C code to platform-specific scan codes / keycodes lives in
`omni-keymap-extract/src/w3c_keys.rs`.

[w3c-code]: https://www.w3.org/TR/uievents-code/

## Dead-key multi-step sequences

A dead-key composition is encoded as a two-element `sequence`:

```json
{
  "sequence": [
    { "key": "Quote", "modifiers": [] },
    { "key": "KeyA",  "modifiers": [] }
  ]
}
```

Read as: "press `Quote` (the dead acute on a US-International layout), then press `KeyA`". The
extractor discovers these by simulating the dead key and then pressing every other standard key
under the same modifier combination, capturing the resulting character.

## Validation rules

`omni-keymap-core` enforces the following at load time:

- `metadata.platform`, `metadata.layout_name`, `metadata.extracted_on` are non-empty.
- No `mappings` key is the empty string.
- No alternatives list is empty.
- Every `Keystroke.sequence` is non-empty.
- Every `KeyPress.modifiers` entry is one of the five W3C modifier names above.