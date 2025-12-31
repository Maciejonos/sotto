# Sotto

Local speech-to-text transcription for Linux/Wayland using Whisper models.

Sotto runs entirely offline — your voice never leaves your machine. It uses [whisper.cpp](https://github.com/ggerganov/whisper.cpp) for fast, local transcription with a simple GTK4/libadwaita interface.

![demo](https://github.com/user-attachments/assets/0b2f7929-e9de-4e58-8c72-236f27409680)

## Features

- **GUI + Daemon modes** — use the app or a global hotkey
- **Fully local** — no cloud services, no API keys, no internet required
- **GPU accelerated** — Vulkan support for NVIDIA, AMD, and Intel GPUs
- **Voice activity detection** — automatically filters silence
- **12 Whisper models** — from Tiny (78 MB) to Large-v3 (3.1 GB)
- **Clipboard integration** — one-click copy via wl-clipboard
- **Auto-paste** — daemon mode types transcription directly via wtype

## Installation

**Arch Linux (AUR)**

```sh
paru -S sotto-bin
```

**AppImage**

Download from [Releases](https://github.com/Maciejonos/sotto/releases), make executable and run:

```sh
chmod +x Sotto-x86_64.AppImage
./Sotto-x86_64.AppImage
```

**From source**

```sh
# Install dependencies (Arch)
sudo pacman -S gtk4 libadwaita pipewire wl-clipboard wtype vulkan-headers

# Build
cargo build --release

# Run
./target/release/sotto
```

## Quick Start

### GUI Mode

1. Launch `sotto`
2. Open Settings and download a model
3. Click record, speak, click stop
4. Copy the transcription

### Daemon Mode (Global Hotkey)

Run the daemon:

```sh
sotto daemon
```

Configure your compositor to toggle recording with a hotkey:

**Hyprland** (`~/.config/hypr/hyprland.conf`):
```
bind = $mainMod, V, exec, pkill -USR1 sotto
```

**Niri** (`~/.config/niri/config.kdl`):
```kdl
binds {
    Mod+V { spawn "pkill" "-USR1" "sotto"; }
}
```

**Sway** (`~/.config/sway/config`):
```
bindsym $mod+v exec pkill -USR1 sotto
```

Press the hotkey to start recording, press again to stop — transcription is auto-pasted at cursor.

### Autostart Daemon

```sh
sotto enable   # Enable systemd user service
sotto disable  # Disable it
```

## Dependencies

| Runtime | Purpose |
|---------|---------|
| gtk4, libadwaita | GUI |
| pipewire | Audio capture |
| wl-clipboard | Clipboard (wl-copy) |
| wtype | Auto-paste in daemon mode |
| vulkan-icd-loader | GPU acceleration |

## Models

Models are downloaded from HuggingFace via Settings and stored in `~/.local/share/sotto/models/`.

| Model | Size | Notes |
|-------|------|-------|
| Tiny / Tiny (EN) | 78 MB | Fastest, lower accuracy |
| Base / Base (EN) | 148 MB | Good balance (default) |
| Small / Small (EN) | 488 MB | Better accuracy |
| Medium / Medium (EN) | 1.5 GB | High accuracy |
| Large v1/v2/v3 | 3.1 GB | Best accuracy, slower |
| Large v3 Turbo | 1.6 GB | Fast + accurate |

English-only models (EN) are smaller and optimized for English speech.

## License

MIT
