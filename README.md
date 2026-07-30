# Rust source map

The server scans a Rust project, flattens its tree in preorder, and lays out
fixed-size blocks along a Morton (Z-order) curve. Each block contains a
vertical page of wrapped folder, file, and source-chunk names.

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
- Double-click a node to focus it.
- Press `+`, `-`, or `0` to zoom or reset.
- Hold Shift while dragging to select source text.
