# Screen 13

[![Crates.io](https://img.shields.io/crates/v/screen-13.svg)](https://crates.io/crates/screen-13)
[![Docs.rs](https://docs.rs/screen-13/badge.svg)](https://docs.rs/screen-13)

Screen 13 is an easy-to-use 3D game engine in the spirit of QBasic.

## Overview

Games made using Screen 13 are built as regular executables using a design-time asset baking process. Screen 13 provides all asset-baking logic and aims to, but currently does not, provide wide support for texture formats, vertex formats, and other associated data. Baked assets are stored in `.pak` files.

## Asset Baking

Asset baking is the process of converting files from their native file formats into a runtime-ready format that is optimized for both speed and size. Currently Screen 13 uses a single file (or single HTTP/S endpoint) for all runtime assets. Assets are baked from `.toml` files which you can find examples of in the `examples/content` directory.

## Quick Start

Included is example you might find helpful:

- `basic.rs` - Displays 'Hello, World!' on the screen. Please start here.

The example requires an associated asset `.pak` file in order to run, so you will need to run the examples like so:

```bash
cargo run examples/content/basic.toml
cargo run --example basic
```

These commands do the following:

- Build the Screen 13 engine (_runtime_) and executable code (_design-time_)
- Bake the assets from `basic.toml` into `basic.pak`
- Runs the `basic` example (Press ESC to exit)

## Roadmap/Status/Notes

This engine is very young and is likely to change as development continues.

- Requires Rust 1.45 or later
- Asset .pak file baking: Needs work, currently written in a script-like or procedural style and should be refactored to become much more general purpose
- Asset .pak file runtime: 75% complete. Needs implemetation of HTTP/S support.
- Debug names should maybe be a Cargo.toml "feature" for games that aren't attempting to support debuggability via graphics API capturing tools such as RenderDoc. The way it is right now lots of API calls require a string you must attribute with the debug-assertions if-config attribute.
- Drawing lines, bitmaps, 3D models, lights (and shadows): I recently ripped out all this code in order to add a compilation stage after you submit rendering commands. This allows for proper z-order painting and batching to reduce GPU resource-switching. It is not complete yet and requires more work. Update: The design of this section is really coming along and likely to remain somewhat stable as it scrolls towards dev-complete.
- Input: Keyboard has been started but the design is not very good. Mouse input is to-do. Game controllers and joysticks are planned.

## Content Baking Procedures

### Brotli Compression

Higher compression ratio and somewhat slow during compression. *Currently does not read properly*

```toml
[content]
compression = 'brotli'
buf_size = 4096
quality = 10
window_size = 20
```

### Snap Compression

Faster during compression and lower compression ratio compared to Brotli.

```toml
[content]
compression = 'snap'
```

## History

As a child I was given access to a computer that had GW-Basic; and later one with QBasic. All of my favorite programs started with:

```basic
CLS
SCREEN 13
```

These commands cleared the screen of text and setup a 320x200 256-color paletized color video mode. There were other video modes available, but none of them had the 'magic' of 256 colors.

Additional commands QBasic offered, such as `DRAW`, allowed you to build very simple games incredibly quickly because you didn't have to grok the enirety of linking and compiling in order get things done. I think we should have options like this today, and this project aims to allow future developers to have the same ability to get things done quickly while using modern tools.

## Notes

- Run your game with the `RUST_LOG` environment variable set to `screen_13=trace` for detailed debugging messages
- Make all panics/todos/unreachables and others only have messages in debug builds?
- Consider removing the extra derived things
- Create new BMFont files on Windows using [this](http://www.angelcode.com/products/bmfont/)
- Regenerate files by cd'ing to correct directory and run this:
  - "c:\Program Files (x86)\AngelCode\BMFont\bmfont.com" -c SmallFonts-12px.bmfc -o SmallFonts-12px.fnt
  - "c:\Program Files (x86)\AngelCode\BMFont\bmfont.com" -c SmallFonts-10px.bmfc -o SmallFonts-10px.fnt
