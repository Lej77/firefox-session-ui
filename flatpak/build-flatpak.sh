#!/bin/bash
set -e

# 1. Set working directory to the script's directory
pushd "$(dirname "$(readlink -f "$0")")" > /dev/null
trap 'popd > /dev/null 2>&1' EXIT

echo "Current directory inside script: $(pwd)"

# Define fixed tool versions
TRUNK_VERSION="0.21.14"
TAURI_CLI_VERSION="2.11.4"
WASM_BINDGEN_VERSION="0.2.127"

# Check if running inside a Dev Container or Docker container
if [ -n "$REMOTE_CONTAINERS" ] || [ -f "/.dockerenv" ] || [ -f "/run/.containerenv" ]; then
  echo "WARNING: Container environment detected, building a flatpak inside a container will likely fail..."
fi

# Check if required tools are installed
if ! command -v flatpak-cargo-generator >/dev/null 2>&1; then
    echo "=========================================================================="
    echo " ERROR: 'flatpak-cargo-generator' is not installed or not in PATH."
    echo ""
    echo " To install it, run:"
    echo "   pipx install flatpak-cargo-generator"
    echo ""
    echo " Ensure pipx binaries are available in your PATH:"
    echo "   export PATH=\"\$PATH:\$HOME/.local/bin\""
    echo "=========================================================================="
    exit 1
fi

if ! command -v flatpak-builder >/dev/null 2>&1; then
    echo "=========================================================================="
    echo " ERROR: 'flatpak-builder' is not installed."
    echo ""
    echo " To install it, run the command for your distribution:"
    echo "   Fedora:        sudo dnf install flatpak-builder"
    echo "   Ubuntu/Debian: sudo apt install flatpak-builder"
    echo "   Arch Linux:    sudo pacman -S flatpak-builder"
    echo "=========================================================================="
    exit 1
fi

# Downloads the real Cargo.lock file directly for a specific crate version
get_cargo_lock() {
  local crate="$1"
  local version="$2"
  local dir="$3"
  if [ -z "$crate" ] || [ -z "$version" ] || [ -z "$dir" ]; then
    echo "Usage: get_cargo_lock <crate_name> <version> <target_dir>" >&2
    return 1
  fi

  echo "Fetching Cargo.lock for ${crate} v${version} from crates.io..."

  # Extract Cargo.lock directly from the specific crates.io release tarball into target dir
  if curl -sL "https://static.crates.io/crates/${crate}/${crate}-${version}.crate" | \
     tar -xz -C "$dir" --strip-components=1 "${crate}-${version}/Cargo.lock" 2>/dev/null; then
    echo "Saved ${dir}/Cargo.lock."
    return 0
  else
    echo "Error: ${crate} v${version} does not package a Cargo.lock file on crates.io." >&2
    return 1
  fi
}

# Generates a synthetic lockfile containing crates.io checksums for a specific crate version
generate_fake_cargo_lock() {
  local crate="$1"
  local version="$2"
  local dir="$3"
  if [ -z "$crate" ] || [ -z "$version" ] || [ -z "$dir" ]; then
    echo "Usage: generate_fake_cargo_lock <crate_name> <version> <target_dir>" >&2
    return 1
  fi

  local lock_file="${dir}/FakeToolsCargo.lock"
  echo 'version = 3' > "$lock_file"

  echo "Fetching metadata and checksum for ${crate} v${version} from crates.io..."

  # Query crates.io API for crate metadata
  local meta
  meta=$(curl -sH "User-Agent: fedora-script" "https://crates.io/api/v1/crates/${crate}/${version}")

  # Extract SHA-256 checksum for the specified version
  local checksum
  checksum=$(echo "$meta" | jq -r '.version.checksum // empty')

  if [ -z "$checksum" ] || [ "$checksum" = "null" ]; then
    echo "Error: Could not retrieve checksum for ${crate} v${version}." >&2
    return 1
  fi

  # Append crate block to the tool's fake Cargo.lock file
  cat <<EOF >> "$lock_file"

[[package]]
name = "${crate}"
version = "${version}"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "${checksum}"
EOF

  echo "==> Successfully created ${lock_file}"
}

# ------------------------------------------------------------------------------
# Process each tool inside its own isolated subdirectory with explicitly pinned versions
# ------------------------------------------------------------------------------

# Process trunk
echo "==> Collecting lock file for trunk..."
mkdir -p trunk
get_cargo_lock "trunk" "$TRUNK_VERSION" "trunk"
generate_fake_cargo_lock "trunk" "$TRUNK_VERSION" "trunk"

echo "==> Generating sources for trunk..."
flatpak-cargo-generator -o "cargo-sources-trunk.json" "trunk/Cargo.lock"
flatpak-cargo-generator -o "cargo-sources-trunk-tool.json" "trunk/FakeToolsCargo.lock"

# Process tauri-cli
echo "==> Collecting lock file for tauri-cli..."
mkdir -p tauri-cli
get_cargo_lock "tauri-cli" "$TAURI_CLI_VERSION" "tauri-cli"
generate_fake_cargo_lock "tauri-cli" "$TAURI_CLI_VERSION" "tauri-cli"

echo "==> Generating sources for tauri-cli..."
flatpak-cargo-generator -o "cargo-sources-tauri-cli.json" "tauri-cli/Cargo.lock"
flatpak-cargo-generator -o "cargo-sources-tauri-cli-tool.json" "tauri-cli/FakeToolsCargo.lock"

# Process wasm-bindgen
echo "==> Collecting lock file for wasm-bindgen-cli..."
mkdir -p wasm-bindgen-cli
get_cargo_lock "wasm-bindgen-cli" "$WASM_BINDGEN_VERSION" "wasm-bindgen-cli"
generate_fake_cargo_lock "wasm-bindgen-cli" "$WASM_BINDGEN_VERSION" "wasm-bindgen-cli"

echo "==> Generating sources for wasm-bindgen-cli..."
flatpak-cargo-generator -o "cargo-sources-wasm-bindgen-cli.json" "wasm-bindgen-cli/Cargo.lock"
flatpak-cargo-generator -o "cargo-sources-wasm-bindgen-cli-tool.json" "wasm-bindgen-cli/FakeToolsCargo.lock"

# Generate cargo-sources for the main project
echo "==> Generating project cargo-sources.json..."
flatpak-cargo-generator -o cargo-sources-project.json ../Cargo.lock

# Check for required Flatpak runtimes & SDKs
echo "==> Checking required Flatpak SDKs..."
REQUIRED_REFS="
org.gnome.Platform//50
org.gnome.Sdk//50
org.freedesktop.Sdk.Extension.rust-stable//25.08
"

MISSING_REFS=""
for ref in $REQUIRED_REFS; do
    if ! flatpak info "$ref" >/dev/null 2>&1; then
        MISSING_REFS="$MISSING_REFS $ref"
    fi
done

# If dependencies are missing, guide the user without forcing system modifications
if [ -n "$MISSING_REFS" ]; then
    echo "=========================================================================="
    echo " ERROR: Missing required Flatpak runtimes/SDKs:"
    for ref in $MISSING_REFS; do
        echo "   - $ref"
    done
    echo ""
    echo " To INSTALL them, run:"

    # Check if flathub remote is configured anywhere on the system
    if ! flatpak remotes | grep -q "flathub"; then
        echo "   flatpak remote-add --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo"
    fi

    echo "   flatpak install $MISSING_REFS"
    echo ""
    echo " To REMOVE them later when done with development, run:"
    echo "   flatpak uninstall --user $MISSING_REFS"
    echo "=========================================================================="
    exit 1
fi

echo "  All required Flatpak dependencies are installed."

# Run flatpak-builder
echo "==> Building Flatpak..."
cd ..
flatpak-builder --force-clean --user --disable-cache --repo flatpak/flatpak-repo flatpak/flatpak flatpak/flatpak-builder.yaml
cd flatpak
echo "==> Build complete!"


echo "==> Packaging into .flatpak"

flatpak build-bundle flatpak-repo firefox-session-data-utility.flatpak com.lej77.firefox-session-data-utility

echo "==> Finished"
