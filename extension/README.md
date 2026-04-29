# rongnote — browser extension

Firefox/Chrome MV3 popup that surfaces rongnote secrets for the current
tab URL. Same E2E crypto as the SPA — passphrase derives the KEK on the
device, master key never leaves it.

## What v0.1 does

- Click the extension icon → popup
- First time: opens the options page, asks for the rongnote server URL
- Then a passphrase prompt; the extension does its own login (separate
  session from the SPA tab)
- Lists secrets whose `url` field matches the current tab's host. Empty
  match falls through to "show all"
- One click copies username, password, or current TOTP code; clipboard
  auto-clears after 30s
- Auto-locks after 15 min idle (alarm-driven). Vault is in
  `browser.storage.session`, so a browser restart re-locks regardless

## What v0.1 doesn't do

- No content-script form fill — copy-with-clipboard is the v0.1 UX
- No "save this to rongnote" capture from a login form
- No multi-account / multi-server. One server per browser profile.

## Building

```bash
cd extension
npm install
npm run build      # → extension/dist/
npm run watch      # rebuild on save
npm run check      # tsc --noEmit
```

## Loading in Firefox

1. `about:debugging` → **This Firefox**
2. **Load Temporary Add-on…** → pick `extension/dist/manifest.json`
3. The icon appears in the toolbar. Right-click → **Pin to Toolbar** to keep it.

For permanent installation, the extension needs to be signed by Mozilla
(`web-ext sign --api-key … --api-secret …`). Out of scope for v0.1 —
temporary loading is fine for personal use.

## Loading in Chrome

1. `chrome://extensions` → toggle **Developer mode** on
2. **Load unpacked** → pick the `extension/dist/` folder

## Files

```
extension/
├── package.json        # esbuild + libsodium + tsc
├── build.mjs           # bundles src/{popup,options,background}.ts → dist/
├── tsconfig.json
└── src/
    ├── manifest.json   # MV3 manifest, copied verbatim
    ├── popup.html
    ├── popup.ts        # main UI: login + secret list + copy
    ├── options.html
    ├── options.ts      # server URL + email
    ├── background.ts   # auto-lock alarm
    ├── icons/icon.svg  # same brand mark as the web favicon
    └── lib/
        ├── crypto.ts   # libsodium subset — KEK, secretbox open, sealed-box
        ├── api.ts      # fetch wrapper for /api/v1/*
        ├── items.ts    # decrypt secrets + URL matching
        ├── totp.ts     # RFC 6238 via Web Crypto
        └── store.ts    # browser.storage.session/local helpers
```
