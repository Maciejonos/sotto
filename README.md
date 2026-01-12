# Sotto

Local speech-to-text transcription for Linux/Wayland using Whisper models.

Sotto runs entirely offline — your voice never leaves your machine. It uses [whisper.cpp](https://github.com/ggerganov/whisper.cpp) for fast, local transcription.

### Demo
https://github.com/user-attachments/assets/e8480c00-81dc-45d3-8369-d4e12b32ea1d

## Features

- **Fully local** — no cloud services, no API keys, no internet required
- **GPU accelerated** — Vulkan support for NVIDIA, AMD, and Intel GPUs
- **Voice activity detection** — automatically filters silence
- **Auto-paste** — transcription typed directly at cursor via wtype
- **Push-to-talk mode** — hold a key to record, release to transcribe (requires input group)
- **Spoken punctuation** — say "period", "comma", "question mark" etc. to insert symbols
- **Visual indicator** — layer shell overlay shows recording time and status
- **12 Whisper models** — from Tiny (78 MB) to Large-v3 (3.1 GB)

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
sudo pacman -S gtk4 libadwaita gtk4-layer-shell pipewire wl-clipboard wtype vulkan-headers
cargo build --release
./target/release/sotto
```

## Quick Start

1. Launch `sotto` to open the control panel
2. Download a model via "Manage Models"
3. Select your input device and language
4. Choose activation mode (Toggle or Push-to-talk)
5. Enable the daemon toggle
6. Configure your hotkey (see below)
7. Press the hotkey to record, speak, then release/press again to transcribe

## Activation Modes

Sotto supports two activation modes, configurable in the control panel:

### Toggle Mode (default)

Uses compositor keybindings to send a signal. Press once to start recording, press again to transcribe.

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

### Push-to-Talk Mode

Hold a key to record, release to transcribe. No compositor configuration needed. Requires user in `input` group:

```sh
sudo usermod -aG input $USER
```

Log out and back in for changes to take effect. Available hotkeys: INSERT (default), SCROLLLOCK, PAUSE, F13-F24, RIGHTALT, or any custom evdev key name.

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
| gtk4-layer-shell | Visual indicator overlay |
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

## Spoken Punctuation

Say punctuation out loud and it will be converted to symbols:

| Say | Insert |
|-----|--------|
| period, comma, colon, semicolon | `.` `,` `:` `;` |
| question mark, exclamation mark | `?` `!` |
| open/close paren, bracket, brace | `()` `[]` `{}` |
| new line, new paragraph, tab | newlines, tabs |
| dash, hyphen, underscore | `-` `_` |
| hash, asterisk, slash, pipe | `#` `*` `/` `\|` |

## License

MIT
