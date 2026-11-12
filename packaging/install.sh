#!/usr/bin/env bash
set -euo pipefail

# Script de instalación universal para x56linux
PREFIX="${PREFIX:-/usr/local}"
UDEV_DIR="${UDEV_DIR:-/etc/udev/rules.d}"
DESKTOP_DIR="${DESKTOP_DIR:-/usr/local/share/applications}"
ICONS_DIR="${ICONS_DIR:-/usr/local/share/icons/hicolor/scalable/apps}"
SHARE_DIR="${SHARE_DIR:-/usr/local/share/x56linux}"

if [[ $EUID -ne 0 ]]; then
   echo "Este script requiere permisos de administrador para instalar en el sistema."
   echo "Ejecute con: sudo $0"
   exit 1
fi

echo "==> Instalando binarios en ${PREFIX}/bin..."
install -d "${PREFIX}/bin"
if [[ -f "./x56" ]]; then
    install -m 755 "./x56" "${PREFIX}/bin/x56"
elif [[ -f "./target/release/x56" ]]; then
    install -m 755 "./target/release/x56" "${PREFIX}/bin/x56"
fi

if [[ -f "./x56-cli" ]]; then
    install -m 755 "./x56-cli" "${PREFIX}/bin/x56-cli"
elif [[ -f "./target/release/x56-cli" ]]; then
    install -m 755 "./target/release/x56-cli" "${PREFIX}/bin/x56-cli"
elif [[ -f "${PREFIX}/bin/x56" ]]; then
    ln -sf "x56" "${PREFIX}/bin/x56-cli"
fi

if [[ ! -f "${PREFIX}/bin/x56" && -f "${PREFIX}/bin/x56-cli" ]]; then
    ln -sf "x56-cli" "${PREFIX}/bin/x56"
fi

if [[ -f "./x56-deck" ]]; then
    install -m 755 "./x56-deck" "${PREFIX}/bin/x56-deck"
elif [[ -f "./target/release/x56-deck" ]]; then
    install -m 755 "./target/release/x56-deck" "${PREFIX}/bin/x56-deck"
fi

if [[ -f "./x56-gui" ]]; then
    install -m 755 "./x56-gui" "${PREFIX}/bin/x56-gui"
elif [[ -f "./target/release/x56-gui" ]]; then
    install -m 755 "./target/release/x56-gui" "${PREFIX}/bin/x56-gui"
elif [[ -f "${PREFIX}/bin/x56-deck" ]]; then
    ln -sf "x56-deck" "${PREFIX}/bin/x56-gui"
fi

if [[ ! -f "${PREFIX}/bin/x56-deck" && -f "${PREFIX}/bin/x56-gui" ]]; then
    ln -sf "x56-gui" "${PREFIX}/bin/x56-deck"
fi

echo "==> Instalando reglas udev en ${UDEV_DIR}..."
install -d "${UDEV_DIR}"
RULES_SRC=""
for cand in "./70-x56.rules" "./packaging/70-x56.rules"; do
    if [[ -f "$cand" ]]; then
        RULES_SRC="$cand"
        break
    fi
done
if [[ -n "$RULES_SRC" ]]; then
    install -m 644 "$RULES_SRC" "${UDEV_DIR}/70-x56.rules"
    if command -v udevadm >/dev/null 2>&1; then
        echo "==> Recargando reglas de udev (udevadm control --reload-rules)..."
        udevadm control --reload-rules || true
        udevadm trigger --subsystem-match=hidraw || true
    fi
fi

echo "==> Instalando integración de escritorio XDG..."
install -d "${DESKTOP_DIR}"
for cand in "./com.saitek.x56.desktop" "./packaging/com.saitek.x56.desktop"; do
    if [[ -f "$cand" ]]; then
        install -m 644 "$cand" "${DESKTOP_DIR}/com.saitek.x56.desktop"
        break
    fi
done

install -d "${ICONS_DIR}"
for cand in "./x56linux.svg" "./packaging/x56linux.svg"; do
    if [[ -f "$cand" ]]; then
        install -m 644 "$cand" "${ICONS_DIR}/x56linux.svg"
        break
    fi
done

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "${DESKTOP_DIR}" 2>/dev/null || true
fi

echo "==> Instalando perfiles de ejemplo..."
install -d "${SHARE_DIR}/examples"
for cand in "./examples/combat_profile.toml" "./combat_profile.toml"; do
    if [[ -f "$cand" ]]; then
        install -m 644 "$cand" "${SHARE_DIR}/examples/combat_profile.toml"
        break
    fi
done

echo ""
echo "✅ Instalación completada exitosamente."
echo "   - CLI: ${PREFIX}/bin/x56, ${PREFIX}/bin/x56-cli"
echo "   - GUI: ${PREFIX}/bin/x56-deck, ${PREFIX}/bin/x56-gui"
echo "   - Udev: ${UDEV_DIR}/70-x56.rules"
echo "   (Si los dispositivos ya están conectados, reconéctalos o ejecuta: sudo udevadm trigger)"
