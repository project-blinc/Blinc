# blinc_noto_emoji

Drop-in NotoColorEmoji fallback for [`blinc_text`](../blinc_text).
Add this crate to your `Cargo.toml` and color emoji work everywhere
— wasm, headless Linux, minimal Android images, Windows without
the optional Segoe UI Emoji pack — with **zero source changes**.

```toml
[dependencies]
blinc_noto_emoji = "0.6"
```

That's the whole integration. No `use blinc_noto_emoji;` in your
source, no `init()` call, no build script. A `#[ctor]` function in
this crate runs at binary initialisation and registers the bundled
font with `blinc_text::global_font_registry()` before `main` is
ever called.

## What's in the box

A ~142 KB subset of Google's
[NotoColorEmoji](https://github.com/googlefonts/noto-emoji) (CBDT/CBLC
color bitmap format) that covers **90 codepoints** commonly used
across the Blinc example gallery and typical app chrome:

- Latin-1 Supplement punctuation — `°`, `±`, `×`
- General Punctuation — `–`, `—`, `…`
- Arrows — `←`, `↑`, `→`, `↓`, `↻`, `⇧`
- Misc Technical — `⌘`, `⏳`
- Dingbats — `✅`, `✊`, `✌`, `✓`, `✨`, `❄`, `❌`, `❤`
- Emoticons — `😀`–`😆`, `🙌`
- Misc & Supplemental Symbols and Pictographs — food, animals,
  weather, celebration glyphs

If your app needs glyphs outside that set, either rebuild the
subset locally (see [Regenerating the subset](#regenerating-the-subset))
or wait for the lazy per-codepoint loader on the roadmap (4.4).

## How it works

1. This crate holds the subset bytes in
   `assets/NotoColorEmoji-subset.ttf` and exposes them as
   `pub const NOTO_COLOR_EMOJI_SUBSET: &[u8]`.
2. A `#[ctor::ctor]` function calls `register()` at binary init:
   ```rust
   #[ctor::ctor]
   fn auto_register() {
       register();
   }
   ```
3. `register()` acquires the global `blinc_text::FontRegistry` via
   `blinc_text::global_font_registry()` and calls
   `load_font_data(NOTO_COLOR_EMOJI_SUBSET.to_vec())`.
4. When the user's code later renders text containing an emoji,
   `blinc_text::FontRegistry::load_emoji_font` walks its fallback
   chain and finds the "Noto Color Emoji" family we just loaded.

Because `ctor` marks its functions `#[used]`, the dead-code
eliminator can't strip them even if no code in the user's crate
ever references this crate by name — adding the dependency to
`Cargo.toml` really is enough.

### `ctor` platform support

| Platform                | Mechanism                |
|-------------------------|--------------------------|
| Linux / Android / BSDs  | `.init_array` section    |
| macOS / iOS             | `__DATA,__mod_init_func` |
| Windows                 | TLS callback             |
| wasm32                  | `linker_init`            |

## Where it fits

`blinc_text` itself does not bundle any emoji font — it tries the
platform's system fonts (Apple Color Emoji, Segoe UI Emoji, Noto
Color Emoji, etc.) and falls back to a `FontLoadError` if none are
present. On wasm there are no system fonts at all, so the error
path is the default, and this crate is the recommended way to
guarantee a color-emoji fallback.

Apple platforms ship Apple Color Emoji at a well-known system
path, so on macOS and iOS `blinc_text`'s system chain succeeds
first and the bundled subset is effectively unused (but still
loaded, at ~142 KB cost). If you want to skip the cost on Apple
targets, use a cfg-gated dependency:

```toml
[target.'cfg(not(any(target_os = "macos", target_os = "ios")))'.dependencies]
blinc_noto_emoji = "0.6"
```

## `blinc_<font>_emoji` naming convention

This is the first entry in a family of optional emoji add-on
crates. The convention is `blinc_<font>_emoji`, each one:

- Bundles a pre-subsetted font as a `const &[u8]`.
- Exposes a public `register()` function that pushes the bytes into
  `blinc_text::global_font_registry()` via
  `FontRegistry::load_font_data`.
- Declares a `#[ctor::ctor]` wrapper so depending on the crate is
  the only user-visible step.

Planned siblings:

| Crate                   | Source                                   | License |
|-------------------------|------------------------------------------|---------|
| `blinc_noto_emoji`      | Google NotoColorEmoji                    | OFL 1.1 |
| `blinc_twemoji_emoji`   | Twitter Twemoji (COLRv0)                 | CC-BY 4.0 |
| `blinc_fluent_emoji`    | Microsoft Fluent Emoji                   | MIT     |
| `blinc_openmoji`        | OpenMoji                                 | CC-BY-SA 4.0 |

Applications depend on exactly one add-on. Switching between them
is a single-line `Cargo.toml` edit.

## Regenerating the subset

The committed subset is generated from the Blinc workspace
examples. If your app needs glyphs that aren't covered, clone the
Blinc repo and run the regeneration script against *your own*
source tree:

```bash
./scripts/regen-emoji-subset.sh /path/to/your/app/src
```

The script runs `blinc-emoji-scan` (AST walk that extracts every
non-ASCII codepoint from Rust string / char literals), pipes the
result into `hb-subset`, and writes the updated subset into
`crates/blinc_noto_emoji/assets/NotoColorEmoji-subset.ttf`.

Then publish a fork of this crate with the regenerated bundle.

Requires `hb-subset` in PATH (`brew install harfbuzz` on macOS,
`apt install harfbuzz-utils` on Debian/Ubuntu).

## Licensing

The bundled subset is SIL OFL 1.1 licensed; the license text is
included verbatim at `assets/NotoColorEmoji-OFL.txt`. Anyone
redistributing this crate or a binary that depends on it should
keep that file alongside the distribution to honour OFL
attribution.
