# Rust source map

Checkpoint 1 packs every Rust file in a project into a single DOM-based,
viewport-sized treemap.

## Build and run

```bash
cd frontend
npm install
npm run build
cd ..
cargo run -- /path/to/rust/project
```

Open the tokenized URL printed by the server. To configure the port or line
wrapping:

```bash
cargo run -- /path/to/project --port 3000 --wrap-column 120
```

## Navigation

- Wheel or pinch to zoom around the pointer.
- Drag to pan.
- Double-click a file or directory to fit it.
- Press `+`, `-`, or `0` to zoom or reset.
- Hold Shift while dragging to select source text.
