# Desktop app

Tauri 2 + React frontend for the prober. The probes themselves live in
`crates/prober_core`; this is only the UI plus a thin command layer in `src-tauri`.

```bash
npm install
npm run tauri dev      # dev build with hot reload
npm run tauri build    # bundle for the current platform
```

Recommended setup: VS Code with the [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode)
and [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer) extensions.
