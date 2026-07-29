# Blabber

Blabber is a peer-to-peer chat and voice application developed as for the course Distributed Programming and Internet Architecture. Instead of relying on a central server, identities, spaces, rooms, and messages are stored locally and synchronized directly between peers using [Iroh](https://iroh.computer/). The desktop application is built with [Tauri](https://tauri.app/) and uses a Vue 3 + TypeScript frontend together with a Rust backend.

## TODO
- View image 
- optimize image and message loading
- Voice channel view 
- voice channel list include members that are in call
- File sharing
- Updater/CICD

- Wordle

- Identity customization (profile picture)
- Different sounds
- Notifications -> (push?)
- Member permissions
- Associated roles
- mute rooms
- potentially encrypt files at rest

- Mobile APP

## Project layout

The project is organized as a Cargo workspace consisting of two Rust crates and the desktop application:

```text
blabber/
├── blabber-core/          Rust library containing identities, spaces, rooms,
│                          invites, voice channels, and all Iroh networking logic
├── blabber-app/
│   ├── src/               Vue 3 + TypeScript frontend (Vite, Pinia, Tailwind)
│   └── src-tauri/         Tauri backend connecting the frontend with
│                          blabber-core through Tauri commands and events
├── blabber-root/          Headless root node for permanently seeding/relaying
│                          spaces from a server: see blabber-root/README.md
├── Cargo.toml             Workspace manifest
└── flake.nix              Optional Nix development shell
```

`blabber-core` contains the main application logic and can be built and tested independently. `blabber-app` contains the desktop application.

## Prerequisites

Before building the project, make sure the following software is installed.

### 1. Rust


Install or update Rust using their official [installer](https://rust-lang.org/tools/install/):


### 2. Node.js

The project was developed with **Node 22**, but **Node 18 or newer** should also work.

Using `nvm`:

```bash
nvm install 22
nvm use 22
```

`npm` comes with Node.js and is used to install the frontend dependencies and run the Tauri CLI.

### 3. Native/system dependencies (Tauri + audio)

Since Blabber uses Tauri and voice communication through `cpal`, a few platform-specific libraries are required.

**Linux (Debian/Ubuntu):**

```bash
sudo apt update
sudo apt install -y \
  libwebkit2gtk-4.1-dev \
  libgtk-3-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev \
  patchelf \
  build-essential \
  curl wget file \
  libssl-dev \
  libasound2-dev \
  pkg-config
```

**macOS:**
MacOS should work out of the box

**Windows:**
We haven't tested windows

### Optional: Nix flake

`flake.nix` provides a development shell containing the Rust toolchain together with `pkg-config`, `openssl`, and `alsa-lib`.

```bash
nix develop
```

This shell is enough to build and test `blabber-core`, but Node.js and the Tauri system dependencies still need to be installed separately if you want to run the desktop application.

## Installation

Clone the repository and install the frontend dependencies:

```bash
git clone https://github.com/bkarli/blabber.git
cd blabber/blabber-app
npm install
```

## Running the app (development)

From the `blabber-app` directory run:

```bash
npm run tauri dev
```

This starts the Vite development server, builds the Rust backend, and launches the application.

The first build can take a while because Cargo has to compile Iroh and all other dependencies. Later builds are much faster.

## Building a release bundle

From `blabber-app` run:

```bash
npm run tauri build
```

This bundles the frontend, builds an optimized Rust binary, and creates platform-specific installers such as `.deb`, `.AppImage`, `.dmg`, or `.msi`.

The generated files can be found under:

```
blabber-app/src-tauri/target/release/bundle/
```


## First run

1. Start the application.
2. Create a new identity by choosing a display name and password.
3. Create a new space or join an existing one using an invite code.
4. Inside a space you can create rooms for text chat and channels for voice calls.
5. Invite other users by sharing the generated invite code.

On Linux, the first time you join a voice channel your desktop environment may ask for microphone permission.

## Where data is stored

Each identity has its own directory inside the operating system's application data folder.

For each identity, Blabber stores:

- `identities/<name>.bin` – encrypted identity information
- `blobs/<name>/` – Iroh blob storage
- `spaces/<name>/<space-uuid>/` – synchronized space and room data

Deleting an identity removes the encrypted identity file. The remaining synchronized data stays on disk.

## Troubleshooting

### Peers cannot connect / messages are not synchronized

Blabber uses Iroh's relay and discovery services to establish peer-to-peer connections, so both peers need an internet connection.

### No sound or missing audio devices

Make sure the required audio libraries are installed and restart the application so the audio devices are detected correctly.

### `tauri dev` cannot find GTK or WebKit libraries

Double-check that all required Linux dependencies listed above are installed. Tauri v2 requires `webkit2gtk-4.1`.
