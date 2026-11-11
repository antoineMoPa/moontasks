#!/bin/sh
# moon_core and moon_tui must build without anything that draws a window: no egui, eframe,
# wgpu, winit, wry, and nothing of X11 or Wayland. Fails, naming what crept in, if any did.
set -eu
cd "$(dirname "$0")/.."

forbidden='^(egui|eframe|egui_[a-z_]+|epaint|emath|ecolor|wgpu[a-z_-]*|winit|wry|glow|glutin|x11rb|x11-dl|wayland-[a-z-]+|minerve-gui|muda|objc2-app-kit)$'
status=0
for crate in moon_core moon_tui; do
    found=$(cargo tree -p "$crate" -e normal --prefix none 2>/dev/null \
        | sed 's/ .*//' | sort -u | grep -E "$forbidden" || true)
    if [ -n "$found" ]; then
        echo "$crate depends on a GUI crate:" >&2
        echo "$found" | sed 's/^/  /' >&2
        status=1
    fi
done
[ "$status" -eq 0 ] && echo "moon_core and moon_tui draw no windows"
exit "$status"
