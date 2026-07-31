# Code Visualisation Experiments

The server scans a Rust project into nested folders, files, and semantic code
elements. It builds two deterministic layouts: a compact structure map with
header-only code cards, and a source map where every file flows in byte order
through 80-character columns capped at 100 visual lines. Minimal declaration
titles such as `struct Cell` and `fn run` use twice the source font size.

## Build and run

```bash
cd frontend
npm install
npm run build
cd ..
cargo run -- /path/to/rust/project
```

Open the tokenized URL printed by the server.

## Navigation

- Wheel or pinch to zoom around the pointer.
- Drag to pan.
- Double-click a district or source card to focus it.
- In structure mode, hover a leaf or `impl` card for 250ms to open its full,
  scrollable source preview.
- Use `Show source` / `Show structure` to switch layouts; each switch fits the
  selected map to the viewport.
- Use `Public only` to retain declarations with any `pub(*)` visibility,
  `#[macro_export]` macros, public-trait members, and the ancestor containers
  needed to show them. The filtered layouts load on first use and are cached.
- Trait implementations are included with a syntax-based approximation: their
  target type's final name must match a public type in the scanned project.
- Press `+`, `-`, or `0` to zoom or reset.
- Hold Shift while dragging to select source text.
