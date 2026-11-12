#!/usr/bin/env bash
set -euo pipefail

PREFIX="${PREFIX:-/usr/local}"
UDEV_DIR="${UDEV_DIR:-/etc/udev/rules.d}"
DESKTOP_DIR="${DESKTOP_DIR:-/usr/local/share/applications}"
ICONS_DIR="${ICONS_DIR:-/usr/local/share/icons/hicolor/scalable/apps}"
SHARE_DIR="${SHARE_DIR:-/usr/local/share/x56linux}"

if [[ $EUID -ne 0 ]]; then
   echo "Este script requiere permisos de administrador."
   echo "Ejecute con: sudo $0"
   exit 1
fi

echo "==> Desinstalando X56 HOTAS Deck..."
rm -f "${PREFIX}/bin/x56"
rm -f "${PREFIX}/bin/x56-cli"
rm -f "${PREFIX}/bin/x56-deck"
rm -f "${PREFIX}/bin/x56-gui"
rm -f "${UDEV_DIR}/70-x56.rules"
rm -f "${DESKTOP_DIR}/com.saitek.x56.desktop"
rm -f "${ICONS_DIR}/x56linux.svg"
rm -rf "${SHARE_DIR}"

if command -v udevadm >/dev/null 2>&1; then
    udevadm control --reload-rules || true
fi

echo "✅ Desinstalación completada."
