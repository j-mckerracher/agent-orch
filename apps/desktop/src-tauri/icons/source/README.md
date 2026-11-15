# App icon source

`app-icon.svg` is the editable source for the Agent Orchestrator (MCAO) desktop icon on macOS and Windows. `app-icon-1024.png` is the 1024 x 1024 master rendered from it. `tauri icon` generates every platform file in `../` from that master.

The concept comparison and the small-size checks are in `docs/evidence/DPES-1105-app-icon/`.

## Regenerate

Run from `apps/desktop` after `npm ci`:

```sh
node src-tauri/icons/source/render-master.mjs
npx tauri icon src-tauri/icons/source/app-icon-1024.png --output src-tauri/icons
rm -r src-tauri/icons/ios src-tauri/icons/android src-tauri/icons/64x64.png
```

- `render-master.mjs` uses the Playwright Chromium from devDependencies. If the browser is missing, run `npx playwright install chromium` once.
- The `rm` line removes the iOS, Android and 64 px outputs. The repo has no mobile target and `bundle.icon` does not use them.
- `bundle.icon` in `src-tauri/tauri.conf.json` lists `32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.icns` and `icon.ico`. Update that list if the generator changes its output names.
- The `Square*.png` and `StoreLogo.png` files are the Windows Store and AppX logos. They are generated but not referenced by the NSIS bundle.
