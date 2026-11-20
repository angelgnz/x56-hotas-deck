#!/usr/bin/env bash
set -euo pipefail

DIST_DIR="${1:?usage: publish-repo.sh DIST_DIR REPO_DIR GPG_KEY_ID}"
REPO_DIR="${2:?usage: publish-repo.sh DIST_DIR REPO_DIR GPG_KEY_ID}"
GPG_KEY_ID="${3:?usage: publish-repo.sh DIST_DIR REPO_DIR GPG_KEY_ID}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if [ -n "${GNUPGHOME:-}" ]; then
  mkdir -p "${GNUPGHOME}"
  if ! grep -qxF 'pinentry-mode loopback' "${GNUPGHOME}/gpg.conf" 2>/dev/null; then
    printf 'batch\npinentry-mode loopback\n' >> "${GNUPGHOME}/gpg.conf"
  fi
fi

mkdir -p \
  "${REPO_DIR}" \
  "${REPO_DIR}/apt/pool/main/x/x56-hotas-deck" \
  "${REPO_DIR}/apt/dists/stable/main/binary-amd64" \
  "${REPO_DIR}/rpm/Packages" \
  "${REPO_DIR}/arch/x86_64"

touch "${REPO_DIR}/.nojekyll"
if [ -f "${SCRIPT_DIR}/index.html" ] && [ ! -f "${REPO_DIR}/index.html" ]; then
  install -m644 "${SCRIPT_DIR}/index.html" "${REPO_DIR}/index.html"
fi

gpg --armor --export "${GPG_KEY_ID}" > "${REPO_DIR}/gpg.key"
gpg --batch --yes --dearmor -o "${REPO_DIR}/gpg.gpg" "${REPO_DIR}/gpg.key"

# 1. APT repository
shopt -s nullglob
debs=("${DIST_DIR}"/x56-hotas-deck_*_amd64.deb)
if [ "${#debs[@]}" -eq 0 ]; then
  echo "error: no x56-hotas-deck .deb packages found in ${DIST_DIR}" >&2
  exit 1
fi
rm -f "${REPO_DIR}/apt/pool/main/x/x56-hotas-deck/x56-hotas-deck_"*.deb
cp -f "${debs[@]}" "${REPO_DIR}/apt/pool/main/x/x56-hotas-deck/"

(
  cd "${REPO_DIR}/apt"
  dpkg-scanpackages -m pool/ > dists/stable/main/binary-amd64/Packages
  gzip -kf dists/stable/main/binary-amd64/Packages

  APT_CONF="$(mktemp)"
  trap 'rm -f "$APT_CONF"' EXIT
  cat > "$APT_CONF" <<'EOF'
APT::FTPArchive::Release {
  Origin "angelgnz";
  Label "angelgnz // pkg";
  Suite "stable";
  Codename "stable";
  Architectures "amd64";
  Components "main";
  Description "Central signed APT repository for angelgnz Linux packages";
};
EOF
  apt-ftparchive -c "$APT_CONF" release dists/stable > dists/stable/Release
)

gpg --batch --yes --pinentry-mode loopback --local-user "${GPG_KEY_ID}" \
  --armor --detach-sign -o "${REPO_DIR}/apt/dists/stable/Release.gpg" \
  "${REPO_DIR}/apt/dists/stable/Release"
gpg --batch --yes --pinentry-mode loopback --local-user "${GPG_KEY_ID}" \
  --clearsign -o "${REPO_DIR}/apt/dists/stable/InRelease" \
  "${REPO_DIR}/apt/dists/stable/Release"

# 2. RPM repository (Fedora, Silverblue rpm-ostree, openSUSE)
rpms=("${DIST_DIR}"/x56-*.x86_64.rpm)
if [ "${#rpms[@]}" -eq 0 ]; then
  echo "error: no x56-*.x86_64.rpm packages found in ${DIST_DIR}" >&2
  exit 1
fi
rm -f "${REPO_DIR}/rpm/Packages/x56-hotas-deck-"*.rpm "${REPO_DIR}/rpm/Packages/x56-cli-"*.rpm
cp -f "${rpms[@]}" "${REPO_DIR}/rpm/Packages/"
for dest_rpm in "${REPO_DIR}/rpm/Packages/"*.rpm; do
  rpmsign \
    --define "_gpg_name ${GPG_KEY_ID}" \
    --define "__gpg $(command -v gpg)" \
    --addsign "${dest_rpm}"
done

createrepo_c --update "${REPO_DIR}/rpm"
gpg --batch --yes --pinentry-mode loopback --local-user "${GPG_KEY_ID}" \
  --armor --detach-sign -o "${REPO_DIR}/rpm/repodata/repomd.xml.asc" \
  "${REPO_DIR}/rpm/repodata/repomd.xml"
install -m644 "${REPO_DIR}/gpg.key" "${REPO_DIR}/rpm/repodata/repomd.xml.key"

cat > "${REPO_DIR}/rpm/angelgnz-packages.repo" <<'EOF'
[angelgnz-packages]
name=angelgnz // pkg
baseurl=https://angelgnz.github.io/packages/rpm
enabled=1
gpgcheck=1
repo_gpgcheck=1
gpgkey=https://angelgnz.github.io/packages/gpg.key
EOF

# 3. Arch Linux Pacman repository (skip -debug packages)
arch_pkgs=()
for pkg in "${DIST_DIR}"/x56-hotas-deck-*-x86_64.pkg.tar.zst; do
  if [[ "$(basename "${pkg}")" != *-debug-* ]]; then
    arch_pkgs+=("${pkg}")
  fi
done
if [ "${#arch_pkgs[@]}" -eq 0 ]; then
  echo "error: no non-debug Arch .pkg.tar.zst packages found in ${DIST_DIR}" >&2
  exit 1
fi
rm -f "${REPO_DIR}/arch/x86_64/x56-hotas-deck-"*.pkg.tar.zst*
cp -f "${arch_pkgs[@]}" "${REPO_DIR}/arch/x86_64/"
python3 "${SCRIPT_DIR}/build_pacman_db.py" "${REPO_DIR}/arch/x86_64" "angelgnz" "${GPG_KEY_ID}"

echo "Published x56-hotas-deck APT, RPM, and Arch repository metadata in ${REPO_DIR}"
