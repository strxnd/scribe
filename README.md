# Scribe

System-wide AI dictation for Linux. Hold a key, speak, and the transcript is typed into whatever app is focused.

Speech recognition runs on this machine with **NVIDIA Parakeet** or **OpenAI Whisper** (whisper.cpp). Optional cleanup goes through **Pi**, which auto-detects whichever models you already configured. There is no other cloud path in v1.

Linux is the v1 target. X11, Wayland, and every compositor are supported. macOS and Windows are out of scope.

## What v1 does

- Global **toggle** (`Super+Shift+Space`) and **push-to-talk** (`Right Ctrl`) without writing Hyprland, Sway, i3, GNOME, or KDE bind files
- Local STT: Parakeet TDT 0.6B (int8 ONNX) and Whisper GGML
- Pi as the only LLM provider, with auto-detect from the `pi` CLI, `~/.pi/agent/auth.json`, `~/.pi/agent/models.json`, and Pi's provider environment variables
- Inserts text with a virtual keyboard when `/dev/uinput` is available, otherwise copies to the clipboard
- A textless monochrome pill at the bottom of the screen: vertical bars while you speak, then those same bars roll as a wave while it transcribes

## Run

Scribe needs a working GPU/Vulkan stack (the same as Zed) and a microphone.

```bash
# Debian/Ubuntu build dependencies
sudo apt install -y \
  build-essential clang cmake pkg-config \
  libfontconfig-dev libfreetype-dev \
  libwayland-dev wayland-protocols \
  libxkbcommon-dev libxkbcommon-x11-dev \
  libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
  libssl-dev libvulkan-dev \
  libasound2-dev libpulse-dev libclang-dev

cargo run --release
```

On first launch, open Scribe and download a speech model. Parakeet is the default; Whisper Base English is the smaller Whisper option.

## Global shortcuts

Scribe registers shortcuts itself:

| Action | Default |
| --- | --- |
| Push to talk | Hold `Right Ctrl` |
| Toggle listening | `Super+Shift+Space` |
| Cancel | `Super+Shift+Escape` |

Backends, in parallel:

1. **evdev** — reads `/dev/input` so shortcuts work on X11, Wayland, and TTY
2. **X11 GrabKey** — swallows the combo when `DISPLAY` is set
3. **XDG Global Shortcuts portal** — used when the compositor implements it (no extra WM config)

Grant input access once:

```bash
./scripts/install-linux-input.sh
```

Then log out and back in. The script adds your user to the `input` group and installs a udev rule for `/dev/uinput`.

## Pi

Scribe does not talk to OpenAI, Anthropic, or anyone else except through Pi's model list.

Detection order:

1. `pi --list-models` when the Pi CLI is on `PATH`
2. Custom providers in `~/.pi/agent/models.json` (Ollama, LM Studio, llama.cpp, …)
3. API keys in `~/.pi/agent/auth.json`
4. The same environment variables Pi documents (`OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `GROQ_API_KEY`, …)

Set the model to **Auto-detect** (default) or pick a `provider/id` in the settings window. If no Pi model is available, Scribe still types the raw transcript.

## Config

`~/.config/scribe/config.toml` is created on first run. Models live in `~/.local/share/scribe/models/`. History is `~/.local/share/scribe/history.jsonl`.

## License

Apache-2.0
