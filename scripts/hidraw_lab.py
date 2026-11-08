#!/usr/bin/env python3
"""
Laboratorio hidraw para ingeniería inversa y verificación del protocolo Saitek/Logitech X56.

Permite:
1. Descubrir e inspeccionar todos los nodos /dev/hidraw* asociados al X56 (Stick y Throttle),
   identificando la interfaz USB (0 = Input, 2 = Vendor 0xFF00), permisos y descriptores HID
   desde sysfs sin requerir librerías externas.
2. Consultar la configuración de un eje (Packet 7 feature set -> Packet 8 interrupt in) o
   Packet 6 (feature get) mostrando decodificación en Little-Endian y Big-Endian para
   verificar endianness, Report IDs y rangos (0..1000 vs 0..100).
3. Leer paquetes de entrada (interfaz 0) en modo solo lectura sin hacer detach del driver
   del kernel.
4. Probar el cambio de color RGB (Packet 5 + Packet 4) sobre la interfaz 2.
"""

from __future__ import annotations

import argparse
import fcntl
import os
import select
import stat
import struct
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Dict, List, Optional, Tuple

# Tabla de dispositivos X56 conocidos (VID, PID) -> (Modelo, Rol)
KNOWN_DEVICES: Dict[Tuple[int, int], Tuple[str, str]] = {
    (0x0738, 0x2221): ("Saitek Pro Flight X-56 Rhino", "Stick"),
    (0x0738, 0xA221): ("Saitek Pro Flight X-56 Rhino", "Throttle"),
    (0x046D, 0xC22B): ("Logitech G X56 RGB", "Stick (por verificar)"),
    (0x046D, 0xC22A): ("Logitech G X56 RGB", "Throttle (por verificar)"),
}

# Constantes ioctl para Linux hidraw (<linux/hidraw.h> y <asm-generic/ioctl.h>)
_IOC_NRBITS = 8
_IOC_TYPEBITS = 8
_IOC_SIZEBITS = 14
_IOC_NRSHIFT = 0
_IOC_TYPESHIFT = _IOC_NRSHIFT + _IOC_NRBITS
_IOC_SIZESHIFT = _IOC_TYPESHIFT + _IOC_TYPEBITS
_IOC_DIRSHIFT = _IOC_SIZESHIFT + _IOC_SIZEBITS

_IOC_WRITE = 1
_IOC_READ = 2


def _IOC(direction: int, ioc_type: str, nr: int, size: int) -> int:
    return (
        (direction << _IOC_DIRSHIFT)
        | (ord(ioc_type) << _IOC_TYPESHIFT)
        | (nr << _IOC_NRSHIFT)
        | (size << _IOC_SIZESHIFT)
    )


def HIDIOCSFEATURE(length: int) -> int:
    return _IOC(_IOC_WRITE | _IOC_READ, "H", 0x06, length)


def HIDIOCGFEATURE(length: int) -> int:
    return _IOC(_IOC_WRITE | _IOC_READ, "H", 0x07, length)


@dataclass
class HidrawNode:
    name: str
    dev_path: Path
    sys_path: Path
    vid: int
    pid: int
    hid_name: str
    hid_phys: str
    interface_num: Optional[int]
    report_descriptor: bytes
    can_read: bool
    can_write: bool
    mode_str: str

    @property
    def model_info(self) -> Tuple[str, str]:
        return KNOWN_DEVICES.get((self.vid, self.pid), ("Unknown", "Unknown"))

    @property
    def is_vendor_interface(self) -> bool:
        if self.interface_num == 2:
            return True
        # Comprobar Usage Page 0xFF00 (0x06, 0x00, 0xFF) al inicio del report descriptor
        return self.report_descriptor.startswith(b"\x06\x00\xff")


def parse_uevent(uevent_path: Path) -> Dict[str, str]:
    data: Dict[str, str] = {}
    if not uevent_path.is_file():
        return data
    for line in uevent_path.read_text(errors="replace").splitlines():
        if "=" in line:
            k, v = line.split("=", 1)
            data[k.strip()] = v.strip()
    return data


def find_interface_number(hidraw_sys: Path) -> Optional[int]:
    try:
        real_dev = (hidraw_sys / "device").resolve()
        for parent in [real_dev, *real_dev.parents]:
            b_iface = parent / "bInterfaceNumber"
            if b_iface.is_file():
                return int(b_iface.read_text().strip(), 16)
    except (OSError, ValueError):
        pass
    return None


def discover_x56_nodes(include_all: bool = False) -> List[HidrawNode]:
    sys_hidraw = Path("/sys/class/hidraw")
    if not sys_hidraw.is_dir():
        return []

    nodes: List[HidrawNode] = []
    for entry in sorted(sys_hidraw.iterdir(), key=lambda p: p.name):
        uevent = parse_uevent(entry / "device" / "uevent")
        hid_id = uevent.get("HID_ID", "")
        # Formato esperado: BBBB:VVVVVVVV:PPPPPPPP
        parts = hid_id.split(":")
        if len(parts) != 3:
            continue
        try:
            vid = int(parts[1], 16)
            pid = int(parts[2], 16)
        except ValueError:
            continue

        if not include_all and (vid, pid) not in KNOWN_DEVICES:
            continue

        dev_path = Path("/dev") / entry.name
        rdesc_path = entry / "device" / "report_descriptor"
        try:
            rdesc = rdesc_path.read_bytes() if rdesc_path.is_file() else b""
        except OSError:
            rdesc = b""

        try:
            st = dev_path.stat()
            mode_str = stat.filemode(st.st_mode)
        except OSError:
            mode_str = "??????????"

        nodes.append(
            HidrawNode(
                name=entry.name,
                dev_path=dev_path,
                sys_path=entry,
                vid=vid,
                pid=pid,
                hid_name=uevent.get("HID_NAME", "Unknown"),
                hid_phys=uevent.get("HID_PHYS", ""),
                interface_num=find_interface_number(entry),
                report_descriptor=rdesc,
                can_read=os.access(dev_path, os.R_OK),
                can_write=os.access(dev_path, os.W_OK),
                mode_str=mode_str,
            )
        )
    return nodes


def parse_rdesc_summary(rdesc: bytes) -> str:
    """Extrae Usage Page inicial y los Report IDs declarados en el descriptor HID."""
    if not rdesc:
        return "vacío/no disponible"
    usage_page = "desconocido"
    if len(rdesc) >= 3 and rdesc[0] == 0x06:
        up = struct.unpack_from("<H", rdesc, 1)[0]
        usage_page = f"0x{up:04X}"
    elif len(rdesc) >= 2 and rdesc[0] == 0x05:
        usage_page = f"0x{rdesc[1]:02X}"

    report_ids: List[int] = []
    i = 0
    while i < len(rdesc):
        b = rdesc[i]
        if b == 0xFE:  # Long item
            size = rdesc[i + 1] if i + 1 < len(rdesc) else 0
            i += 3 + size
            continue
        size_code = b & 0x03
        size = 4 if size_code == 3 else size_code
        tag_type = b & 0xFC
        if tag_type == 0x84 and size == 1 and i + 1 < len(rdesc):  # Report ID (0x85)
            rid = rdesc[i + 1]
            if rid not in report_ids:
                report_ids.append(rid)
        i += 1 + size

    rids_str = ", ".join(f"0x{r:02x}" for r in report_ids) if report_ids else "ninguno"
    return f"UsagePage={usage_page}, len={len(rdesc)}B, ReportIDs=[{rids_str}]"


def format_hexdump(data: bytes) -> str:
    lines: List[str] = []
    for offset in range(0, len(data), 16):
        chunk = data[offset : offset + 16]
        hex_part = " ".join(f"{b:02x}" for b in chunk)
        lines.append(f"  {offset:04x}: {hex_part}")
    return "\n".join(lines)


def cmd_inspect(args: argparse.Namespace) -> int:
    nodes = discover_x56_nodes(include_all=args.all)
    if not nodes:
        print("No se detectaron nodos hidraw del X56 en /sys/class/hidraw.")
        return 0

    print(f"Encontrados {len(nodes)} nodo(s) hidraw:\n")
    for n in nodes:
        model, role = n.model_info
        iface_role = (
            "Vendor Config (0xFF00)"
            if n.is_vendor_interface
            else "HID Input (Joystick/Botones)"
        )
        rw_status = (
            "OK (R/W)"
            if (n.can_read and n.can_write)
            else "SIN PERMISOS (requiere regla udev 70-x56.rules o sudo)"
        )
        print(f"=== {n.dev_path} ({n.vid:04x}:{n.pid:04x}) — {model} [{role}] ===")
        print(f"  Nombre HID : {n.hid_name}")
        print(f"  Ruta física: {n.hid_phys}")
        print(f"  Interfaz   : {n.interface_num} -> {iface_role}")
        print(f"  Permisos   : {n.mode_str} -> {rw_status}")
        print(f"  Descriptor : {parse_rdesc_summary(n.report_descriptor)}")
        if args.dump_rdesc and n.report_descriptor:
            print("  Report Descriptor (hex):")
            print(format_hexdump(n.report_descriptor))
        print()
    return 0


def send_feature_report(fd: int, payload: bytes, size: int = 64) -> None:
    buf = bytearray(size)
    buf[: len(payload)] = payload
    fcntl.ioctl(fd, HIDIOCSFEATURE(size), buf, True)


def get_feature_report(fd: int, report_id: int, size: int = 64) -> bytes:
    buf = bytearray(size)
    buf[0] = report_id
    fcntl.ioctl(fd, HIDIOCGFEATURE(size), buf, True)
    return bytes(buf)


def cmd_query_axis(args: argparse.Namespace) -> int:
    dev_path = Path(args.device)
    axis_id = args.axis
    # Packet 7 (request current configuration): [0x0b, 0x02, 0x00, axis_id, ...]
    pkt7 = bytes([0x0B, 0x02, 0x00, axis_id])
    print(f"Enviando Packet 7 (request config) para axis_id={axis_id} a {dev_path}...")
    print(format_hexdump(pkt7))

    if args.dry_run:
        print("[dry-run] No se abrió el dispositivo.")
        return 0

    fd = os.open(dev_path, os.O_RDWR | os.O_NONBLOCK)
    try:
        send_feature_report(fd, pkt7, size=args.size)
        rlist, _, _ = select.select([fd], [], [], args.timeout)
        if not rlist:
            print(f"Timeout ({args.timeout}s) esperando Packet 8 (interrupt in).")
            return 1
        resp = os.read(fd, 64)
        print(f"\nRespuesta Packet 8 recibida ({len(resp)} bytes):")
        print(format_hexdump(resp))
        if len(resp) >= 20:
            # Según Packets.md (1-indexed):
            # byte 1 (idx 0) = ff, bytes 2-4 (idx 1..3) = 0b 02 00, byte 5 (idx 4) = axis_id
            # bytes 10-11 (idx 9..11) = xsat, 12-13 (idx 11..13) = ysat,
            # 14-15 (idx 13..15) = deadband, 16-17 (idx 15..17) = curvature,
            # byte 18 (idx 17) = profile, bytes 19-20 (idx 18..20) = hall zero
            le_vals = struct.unpack_from("<HHHHBH", resp, 9)
            be_vals = struct.unpack_from(">HHHHBH", resp, 9)
            print("\nDecodificación candidata (offset 9..20):")
            print(
                f"  Little-Endian: xsat={le_vals[0]}, ysat={le_vals[1]}, "
                f"deadband={le_vals[2]}, curve={le_vals[3]}, "
                f"profile={le_vals[4]}, hall_zero=0x{le_vals[5]:04x}"
            )
            print(
                f"  Big-Endian   : xsat={be_vals[0]}, ysat={be_vals[1]}, "
                f"deadband={be_vals[2]}, curve={be_vals[3]}, "
                f"profile={be_vals[4]}, hall_zero=0x{be_vals[5]:04x}"
            )
    finally:
        os.close(fd)
    return 0


def cmd_get_feature(args: argparse.Namespace) -> int:
    dev_path = Path(args.device)
    print(
        f"Solicitando Feature Report (ID=0x{args.report_id:02x}, len={args.size}) "
        f"en {dev_path}..."
    )
    if args.dry_run:
        return 0
    fd = os.open(dev_path, os.O_RDWR)
    try:
        data = get_feature_report(fd, args.report_id, size=args.size)
        print(format_hexdump(data))
    finally:
        os.close(fd)
    return 0


def cmd_set_rgb(args: argparse.Namespace) -> int:
    dev_path = Path(args.device)
    color_hex = args.color.lstrip("#")
    if len(color_hex) != 6:
        print("Error: el color debe tener formato RRGGBB (ej. ff8800)", file=sys.stderr)
        return 1
    r, g, b = int(color_hex[0:2], 16), int(color_hex[2:4], 16), int(color_hex[4:6], 16)

    # Packet 5: [0x09, 0x00, 0x03, R, G, B]
    pkt5 = bytes([0x09, 0x00, 0x03, r, g, b])
    # Packet 4: [0x01, 0x01]
    pkt4 = bytes([0x01, 0x01])

    print(f"Packet 5 (RGB #{color_hex}):\n{format_hexdump(pkt5)}")
    print(f"Packet 4 (Commit/End):\n{format_hexdump(pkt4)}")

    if args.dry_run:
        print("[dry-run] No se escribieron bytes al dispositivo.")
        return 0

    fd = os.open(dev_path, os.O_RDWR)
    try:
        send_feature_report(fd, pkt5, size=args.size)
        send_feature_report(fd, pkt4, size=args.size)
        print("Color RGB enviado correctamente.")
    finally:
        os.close(fd)
    return 0


def cmd_sniff_input(args: argparse.Namespace) -> int:
    dev_path = Path(args.device)
    print(
        f"Leyendo {args.count} paquete(s) de entrada desde {dev_path} (solo lectura)..."
    )
    fd = os.open(dev_path, os.O_RDONLY)
    try:
        for i in range(args.count):
            data = os.read(fd, 64)
            print(f"[{i + 1}/{args.count}] ({len(data)}B): {data.hex(' ')}")
    finally:
        os.close(fd)
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Laboratorio hidraw para inspección y verificación del hardware Saitek/Logitech X56."
    )
    sub = parser.add_subparsers(dest="command")

    p_inspect = sub.add_parser(
        "inspect", help="Enumera e inspecciona los nodos hidraw del X56 (por defecto)"
    )
    p_inspect.add_argument(
        "--all", action="store_true", help="Mostrar todos los nodos hidraw del sistema"
    )
    p_inspect.add_argument(
        "--dump-rdesc",
        action="store_true",
        help="Volcar en hexadecimal el HID Report Descriptor completo",
    )

    p_query = sub.add_parser(
        "query-axis",
        help="Envía Packet 7 (feature) y lee Packet 8 (interrupt) para un eje",
    )
    p_query.add_argument("device", help="Ruta al nodo hidraw de interfaz 2 (ej. /dev/hidraw21)")
    p_query.add_argument("--axis", type=int, default=30, help="ID del eje (por defecto: 30)")
    p_query.add_argument("--size", type=int, default=64, help="Tamaño del feature report (64)")
    p_query.add_argument("--timeout", type=float, default=2.0, help="Timeout de lectura en seg")
    p_query.add_argument("--dry-run", action="store_true", help="Solo mostrar los bytes a enviar")

    p_feat = sub.add_parser(
        "get-feature", help="Solicita un Feature Report (ej. Packet 6, report ID 0x08)"
    )
    p_feat.add_argument("device", help="Ruta al nodo hidraw de interfaz 2")
    p_feat.add_argument(
        "--report-id",
        type=lambda x: int(x, 0),
        default=0x08,
        help="Report ID a solicitar (por defecto: 0x08)",
    )
    p_feat.add_argument("--size", type=int, default=64, help="Tamaño del buffer (64)")
    p_feat.add_argument("--dry-run", action="store_true", help="No abrir el dispositivo")

    p_rgb = sub.add_parser(
        "set-rgb", help="Envía Packet 5 (RGB) + Packet 4 sobre la interfaz 2"
    )
    p_rgb.add_argument("device", help="Ruta al nodo hidraw de interfaz 2")
    p_rgb.add_argument("color", help="Color en formato RRGGBB (ej. ff8800)")
    p_rgb.add_argument("--size", type=int, default=64, help="Tamaño del feature report (64)")
    p_rgb.add_argument("--dry-run", action="store_true", help="Solo mostrar los bytes a enviar")

    p_sniff = sub.add_parser(
        "sniff-input", help="Lee paquetes de entrada desde el nodo hidraw de interfaz 0"
    )
    p_sniff.add_argument("device", help="Ruta al nodo hidraw de interfaz 0")
    p_sniff.add_argument("--count", type=int, default=10, help="Número de paquetes a leer")

    return parser


def main() -> int:
    parser = build_parser()
    args = parser.parse_args()
    if args.command in (None, "inspect"):
        if not hasattr(args, "all"):
            args.all = False
            args.dump_rdesc = True
        return cmd_inspect(args)
    if args.command == "query-axis":
        return cmd_query_axis(args)
    if args.command == "get-feature":
        return cmd_get_feature(args)
    if args.command == "set-rgb":
        return cmd_set_rgb(args)
    if args.command == "sniff-input":
        return cmd_sniff_input(args)
    return 1


if __name__ == "__main__":
    sys.exit(main())
