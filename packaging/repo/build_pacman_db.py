#!/usr/bin/env python3
"""Build GPG-signed Pacman repository databases (db and files) without symlink dependence."""

from __future__ import annotations

import base64
import glob
import hashlib
import io
import os
import shutil
import subprocess
import sys
import tarfile
import time


PKGINFO_MULTI_MAP = {
    "license": "LICENSE",
    "replaces": "REPLACES",
    "conflict": "CONFLICTS",
    "provides": "PROVIDES",
    "depend": "DEPENDS",
    "optdepend": "OPTDEPENDS",
    "makedepend": "MAKEDEPENDS",
    "checkdepend": "CHECKDEPENDS",
    "group": "GROUPS",
}

PKGINFO_SINGLE_MAP = {
    "pkgname": "NAME",
    "pkgbase": "BASE",
    "pkgver": "VERSION",
    "pkgdesc": "DESC",
    "url": "URL",
    "arch": "ARCH",
    "builddate": "BUILDDATE",
    "packager": "PACKAGER",
    "size": "ISIZE",
}


def inspect_zst_pkg(pkg_path: str) -> tuple[dict[str, list[str]], list[str]]:
    proc = subprocess.Popen(["zstd", "-dc", pkg_path], stdout=subprocess.PIPE)
    assert proc.stdout is not None
    pkginfo_text = ""
    files: list[str] = []
    try:
        with tarfile.open(fileobj=proc.stdout, mode="r|") as tf:
            for member in tf:
                clean_name = member.name.lstrip("./")
                if clean_name == ".PKGINFO":
                    extracted = tf.extractfile(member)
                    if extracted is not None:
                        pkginfo_text = extracted.read().decode("utf-8", errors="replace")
                    continue
                if not clean_name or clean_name in {".MTREE", ".BUILDINFO", ".INSTALL"}:
                    continue
                if member.isdir() and not clean_name.endswith("/"):
                    clean_name += "/"
                files.append(clean_name)
    finally:
        proc.wait()
        if proc.returncode != 0:
            raise RuntimeError(f"zstd decompression failed for {pkg_path}")

    fields: dict[str, list[str]] = {}
    for line in pkginfo_text.splitlines():
        line = line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = (part.strip() for part in line.split("=", 1))
        fields.setdefault(key, []).append(value)
    return fields, files


def ensure_pkg_sig(pkg_path: str, gpg_key_id: str) -> bytes:
    sig_path = f"{pkg_path}.sig"
    subprocess.run(
        [
            "gpg",
            "--batch",
            "--yes",
            "--pinentry-mode",
            "loopback",
            "--detach-sign",
            "--local-user",
            gpg_key_id,
            "-o",
            sig_path,
            pkg_path,
        ],
        check=True,
    )
    with open(sig_path, "rb") as fh:
        return fh.read()


def format_desc(pkg_path: str, fields: dict[str, list[str]], sig_bytes: bytes) -> bytes:
    with open(pkg_path, "rb") as fh:
        data = fh.read()

    csize = len(data)
    md5sum = hashlib.md5(data).hexdigest()
    sha256sum = hashlib.sha256(data).hexdigest()
    pgpsig = base64.b64encode(sig_bytes).decode("ascii")

    sections: list[tuple[str, list[str]]] = [("FILENAME", [os.path.basename(pkg_path)])]
    for src_key, dst_tag in [
        ("pkgname", "NAME"),
        ("pkgbase", "BASE"),
        ("pkgver", "VERSION"),
        ("pkgdesc", "DESC"),
    ]:
        vals = fields.get(src_key)
        if vals:
            sections.append((dst_tag, [vals[0]]))
        elif src_key == "pkgbase" and fields.get("pkgname"):
            sections.append(("BASE", [fields["pkgname"][0]]))

    sections.append(("CSIZE", [str(csize)]))
    if fields.get("size"):
        sections.append(("ISIZE", [fields["size"][0]]))
    sections.append(("MD5SUM", [md5sum]))
    sections.append(("SHA256SUM", [sha256sum]))
    sections.append(("PGPSIG", [pgpsig]))

    for src_key in ["url", "license", "arch", "builddate", "packager"]:
        vals = fields.get(src_key)
        if not vals:
            continue
        tag = PKGINFO_SINGLE_MAP.get(src_key) or PKGINFO_MULTI_MAP[src_key]
        sections.append((tag, vals))

    for src_key in [
        "replaces",
        "group",
        "conflict",
        "provides",
        "depend",
        "optdepend",
        "makedepend",
        "checkdepend",
    ]:
        vals = fields.get(src_key)
        if vals:
            sections.append((PKGINFO_MULTI_MAP[src_key], vals))

    out = io.StringIO()
    for tag, vals in sections:
        out.write(f"%{tag}%\n")
        for val in vals:
            out.write(f"{val}\n")
        out.write("\n")
    return out.getvalue().encode("utf-8")


def add_tar_bytes(tf: tarfile.TarFile, arcname: str, payload: bytes, mtime: int) -> None:
    info = tarfile.TarInfo(name=arcname)
    info.size = len(payload)
    info.mode = 0o644
    info.mtime = mtime
    tf.addfile(info, io.BytesIO(payload))


def add_tar_dir(tf: tarfile.TarFile, arcname: str, mtime: int) -> None:
    info = tarfile.TarInfo(name=arcname)
    info.type = tarfile.DIRTYPE
    info.mode = 0o755
    info.mtime = mtime
    tf.addfile(info)


def build_repo(arch_dir: str, repo_name: str, gpg_key_id: str) -> None:
    pkgs = [
        p
        for p in sorted(glob.glob(os.path.join(arch_dir, "*.pkg.tar.zst")))
        if "-debug-" not in os.path.basename(p)
    ]
    if not pkgs:
        raise RuntimeError(f"no .pkg.tar.zst packages found in {arch_dir}")

    now = int(time.time())
    db_tar = os.path.join(arch_dir, f"{repo_name}.db.tar.gz")
    files_tar = os.path.join(arch_dir, f"{repo_name}.files.tar.gz")

    with tarfile.open(db_tar, "w:gz") as db_tf, tarfile.open(files_tar, "w:gz") as files_tf:
        for pkg in pkgs:
            fields, file_list = inspect_zst_pkg(pkg)
            pkgname = fields["pkgname"][0]
            pkgver = fields["pkgver"][0]
            sig_bytes = ensure_pkg_sig(pkg, gpg_key_id)
            desc_bytes = format_desc(pkg, fields, sig_bytes)
            files_bytes = ("%FILES%\n" + "\n".join(sorted(file_list)) + "\n").encode("utf-8")

            entry_dir = f"{pkgname}-{pkgver}"
            for tf in (db_tf, files_tf):
                add_tar_dir(tf, entry_dir, now)
                add_tar_bytes(tf, f"{entry_dir}/desc", desc_bytes, now)
            add_tar_bytes(files_tf, f"{entry_dir}/files", files_bytes, now)

    for archive_path, link_path in [
        (db_tar, os.path.join(arch_dir, f"{repo_name}.db")),
        (files_tar, os.path.join(arch_dir, f"{repo_name}.files")),
    ]:
        subprocess.run(
            [
                "gpg",
                "--batch",
                "--yes",
                "--pinentry-mode",
                "loopback",
                "--detach-sign",
                "--local-user",
                gpg_key_id,
                "-o",
                f"{archive_path}.sig",
                archive_path,
            ],
            check=True,
        )
        if os.path.islink(link_path) or os.path.exists(link_path):
            os.remove(link_path)
        shutil.copy2(archive_path, link_path)
        sig_link = f"{link_path}.sig"
        if os.path.islink(sig_link) or os.path.exists(sig_link):
            os.remove(sig_link)
        shutil.copy2(f"{archive_path}.sig", sig_link)


def main() -> int:
    if len(sys.argv) != 4:
        print(
            f"usage: {sys.argv[0]} ARCH_DIR REPO_NAME GPG_KEY_ID",
            file=sys.stderr,
        )
        return 2
    build_repo(sys.argv[1], sys.argv[2], sys.argv[3])
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
