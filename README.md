# Focus Flow

[简体中文](README_CN.md)

Focus Flow is a small Windows desktop monitor for API account quota usage. It combines a Vue 3 + Vite renderer, an Electron shell, and a native Rust backend service.

## What it does

- Tracks multiple API sessions from JWT, `sess-`, or `auth.json` style credentials.
- Polls quota and billing endpoints through a local Rust HTTP service.
- Shows active status, usage, reset windows, and manual refresh controls in a desktop UI.
- Stores local account and settings data outside the repository.

## Project structure

```text
.
├── backend/          # Rust backend service, listens on 127.0.0.1:48123
├── electron/         # Electron main and preload processes
├── src/              # Vue renderer UI
├── index.html        # Vite entry
├── package.json      # npm scripts and Electron Builder config
└── vite.config.ts    # Vite + Electron plugin config
```

## Requirements

- Node.js 20 or newer
- npm
- Rust stable toolchain
- Windows for the packaged `.exe` build

## Development

Install frontend dependencies:

```powershell
npm install
```

Start the Rust backend in one terminal:

```powershell
cd backend
cargo run
```

Start the Electron/Vite development app in another terminal:

```powershell
npm run dev
```

In development, the Electron main process does not start the backend automatically. The UI expects the backend API at `http://127.0.0.1:48123`.

## Build

Build the desktop app from the project root:

```powershell
npm run build
```

The `build` script compiles the Rust backend in release mode before running the Vue/Vite and Electron Builder steps.

The packaged app expects `backend/target/release/focus-flow-backend.exe`, which is copied into Electron resources by the `extraResources` setting in `package.json`.

## Local data

The backend reads and writes:

- `accounts.json`
- `settings.json`

These files can contain credentials or personal settings and are intentionally ignored by git. Build outputs, dependency folders, Rust target artifacts, installer files, and local tool binaries are also ignored so the repository stays source-only.

## Useful scripts

```powershell
npm run dev      # Start Vite/Electron development mode
npm run build:backend # Build the Rust backend release binary
npm run build    # Build backend, type-check, build renderer/Electron output, and package with electron-builder
npm run preview  # Preview the Vite build
```
