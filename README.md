# Sotto

Local speech-to-text transcription for Linux/Wayland using Whisper models.

Sotto runs entirely offline — your voice never leaves your machine. It uses [whisper.cpp](https://github.com/ggerganov/whisper.cpp) for fast, local transcription.

### Demo
![demo-low-res](https://github.com/user-attachments/assets/01c34927-3d0f-4bad-955b-988c1b19cdb0)

### Settings panel
<img width="500" alt="settings" src="https://github.com/user-attachments/assets/42d6844e-0e08-41e6-bd75-a28f06d32311" />


## Features

- **Fully local** — no cloud services, no API keys, no internet required
- **GPU accelerated** — Vulkan support for NVIDIA, AMD, and Intel GPUs
- **Voice activity detection** — automatically filters silence
- **Auto-paste** — transcription typed directly at cursor via wtype
- **12 Whisper models** — from Tiny (78 MB) to Large-v3 (3.1 GB)
- **Desktop notifications** — recording status feedback

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
sudo pacman -S gtk4 libadwaita pipewire wl-clipboard wtype vulkan-headers
cargo build --release
./target/release/sotto
```

## Quick Start

1. Launch `sotto` to open the control panel
2. Download a model via "Manage Models"
3. Select your input device and language
4. Enable the daemon toggle
5. Add a keybinding to your compositor (see below)
6. Press the hotkey to start recording, speak, press again to transcribe and paste

## Compositor Keybindings

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

## CLI Usage

```sh
sotto              # Open control panel
sotto daemon       # Run daemon directly
sotto enable       # Enable systemd user service
sotto disable      # Disable systemd user service
```

## Dependencies

| Runtime | Purpose |
|---------|---------|
| gtk4, libadwaita | Control panel |
| pipewire | Audio capture |
| wtype | Auto-paste transcription |
| vulkan-icd-loader | GPU acceleration |

## Models

Models are downloaded via the control panel and stored in `~/.local/share/sotto/models/`.

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
