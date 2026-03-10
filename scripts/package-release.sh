#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST_DIR="${DIST_DIR:-$ROOT_DIR/dist}"
OS_NAME="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH_NAME="$(uname -m)"

normalize_arch() {
    case "$1" in
        x86_64|amd64)
            printf '%s\n' "x86_64"
            ;;
        aarch64|arm64)
            printf '%s\n' "aarch64"
            ;;
        *)
            printf '%s\n' "$1"
            ;;
    esac
}

VERSION="$(python3 - <<'PY' "$ROOT_DIR/Cargo.toml"
import pathlib
import sys
import tomllib

with pathlib.Path(sys.argv[1]).open("rb") as fh:
    data = tomllib.load(fh)

print(data["package"]["version"])
PY
)"

ARCH_NAME="$(normalize_arch "$ARCH_NAME")"
PACKAGE_BASENAME="bluetodo-tui-v${VERSION}-${OS_NAME}-${ARCH_NAME}"
PACKAGE_DIR="$DIST_DIR/$PACKAGE_BASENAME"
ARCHIVE_PATH="$DIST_DIR/${PACKAGE_BASENAME}.tar.gz"
CHECKSUM_PATH="$DIST_DIR/${PACKAGE_BASENAME}.sha256"

mkdir -p "$DIST_DIR"
rm -rf "$PACKAGE_DIR" "$ARCHIVE_PATH" "$CHECKSUM_PATH"

echo "building release binary"
cargo build --release --manifest-path "$ROOT_DIR/Cargo.toml"

mkdir -p "$PACKAGE_DIR"
install -m 0755 "$ROOT_DIR/target/release/bluetodo-tui" "$PACKAGE_DIR/bluetodo-tui"
install -m 0644 "$ROOT_DIR/README.md" "$PACKAGE_DIR/README.md"
install -m 0644 "$ROOT_DIR/LICENSE" "$PACKAGE_DIR/LICENSE"

cat >"$PACKAGE_DIR/run-local.sh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec "$SCRIPT_DIR/bluetodo-tui" "$@"
EOF
chmod 0755 "$PACKAGE_DIR/run-local.sh"

tar -C "$DIST_DIR" -czf "$ARCHIVE_PATH" "$PACKAGE_BASENAME"
(
    cd "$DIST_DIR"
    sha256sum "$(basename "$ARCHIVE_PATH")" >"$(basename "$CHECKSUM_PATH")"
)

echo "created:"
echo "  $ARCHIVE_PATH"
echo "  $CHECKSUM_PATH"
