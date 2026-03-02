[demo-img]: https://img.shields.io/badge/Web--App-krokett_editor-blue?logo=kitsu
[demo-url]: https://jasminfragnaud.github.io/krokett_editor/
[![Web-app][demo-img]][demo-url]

### 1) Build release for musl

```bash
RUSTFLAGS="-C target-feature=-crt-static" cargo build --release --target x86_64-unknown-linux-musl
```

Note: On Fedora musl-gcc is needed

```bash
sudo dnf install musl-gcc
```

### 2) Launch the app with the key

From the workspace root:

```bash
IGN_API_KEY="your_ign_key" cargo run -p krokett_editor_native
```
