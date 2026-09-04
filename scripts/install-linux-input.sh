#!/usr/bin/env bash
set -euo pipefail

if [[ "${EUID}" -eq 0 ]]; then
  echo "Run this as your normal user. It will sudo when needed."
  exit 1
fi

USER_NAME="${USER}"
RULE_PATH="/etc/udev/rules.d/99-scribe-input.rules"

echo "Adding ${USER_NAME} to the input group…"
sudo usermod -aG input "${USER_NAME}"

echo "Installing udev rules for /dev/input and /dev/uinput…"
sudo tee "${RULE_PATH}" >/dev/null <<'EOF'
# Scribe dictation: global shortcuts (evdev) and virtual keyboard (uinput)
KERNEL=="uinput", GROUP="input", MODE="0660", OPTIONS+="static_node=uinput"
KERNEL=="event*", SUBSYSTEM=="input", GROUP="input", MODE="0660"
EOF

sudo udevadm control --reload-rules
sudo udevadm trigger

if [[ ! -e /dev/uinput ]]; then
  sudo modprobe uinput || true
fi

echo
echo "Done. Log out and back in so the input group applies."
echo "Scribe can then register push-to-talk on X11, Wayland, and every compositor"
echo "without extra window-manager bind files."
