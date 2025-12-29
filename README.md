# Sotto

Local speech-to-text transcription for Linux/Wayland using Whisper models.

Sotto runs entirely offline — your voice never leaves your machine. It uses [whisper.cpp](https://github.com/ggerganov/whisper.cpp) for fast, local transcription with a simple GTK4/libadwaita interface.

## Features

- **Click-to-record** — visual feedback while recording
- **Fully local** — no cloud services, no API keys, no internet required
- **12 Whisper models to download** — from Tiny (78 MB) to Large-v3 (3.1 GB)
- **Clipboard integration** — one-click copy via wl-clipboard
- **Device selection** — choose your input microphone

## 📦 Installation

**Arch Linux (AUR)**

```sh
paru -S sotto
```

**From source**

```sh
cargo build --release
```

## 🔧 Dependencies

| Runtime | Purpose |
|---------|---------|
| gtk4, libadwaita | GUI |
| pipewire | Audio capture (pw-record) |
| wl-clipboard | Clipboard (wl-copy) |

## 🚀 Usage

1. Launch Sotto
2. Open Settings and download a model
3. Click the record button, speak, click stop
4. Copy the transcription to clipboard

## 📝 Models

Models are downloaded from HuggingFace via Settings and stored in `~/.local/share/sotto/models/`.
<https://huggingface.co/ggerganov/whisper.cpp/tree/main>

| Model | Size | Notes |
|-------|------|-------|
| Tiny / Tiny (EN) | 78 MB | Fastest, lower accuracy |
| Base / Base (EN) | 148 MB | Good balance (default) |
| Small / Small (EN) | 488 MB | Better accuracy |
| Medium / Medium (EN) | 1.5 GB | High accuracy |
| Large v1/v2/v3 | 3.1 GB | Best accuracy, slower |
| Large v3 Turbo | 1.6 GB | Fast + accurate |

English-only models (EN) are smaller and optimized for English speech.

## 📄 License

MIT
