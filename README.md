[demo-img]: https://img.shields.io/badge/Web--App-krokett_editor-blue?logo=kitsu
[demo-url]: https://jasminfragnaud.github.io/krokett_editor/
[![Web-app][demo-img]][demo-url]

### 1) Build path for android apks

```bash
./krokett_editor_android/java/app/build/outputs/apk/release/app-release-unsigned.apk
./krokett_editor_android/java/app/build/outputs/apk/debug/app-debug.apk
```

### 2) Launch the app with the key

From the workspace root:

```bash
IGN_API_KEY="your_ign_key" cargo run -p krokett_editor_native
```
