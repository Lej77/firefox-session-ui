#!/bin/bash
set -e

# Set working directory to the script's directory
pushd "$(dirname "$(readlink -f "$0")")" > /dev/null
trap 'popd > /dev/null 2>&1' EXIT

echo "Current directory inside script: $(pwd)"

# Define fixed tool versions
TRUNK_VERSION="0.21.14"
TAURI_CLI_VERSION="2.11.4"
WASM_BINDGEN_VERSION="0.2.127"

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

  local meta
  meta=$(curl -sH "User-Agent: fedora-script" "https://crates.io/api/v1/crates/${crate}/${version}")

  local checksum
  checksum=$(echo "$meta" | jq -r '.version.checksum // empty')

  if [ -z "$checksum" ] || [ "$checksum" = "null" ]; then
    echo "Error: Could not retrieve checksum for ${crate} v${version}." >&2
    return 1
  fi

  cat <<EOF >> "$lock_file"

[[package]]
name = "${crate}"
version = "${version}"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "${checksum}"
EOF

  echo "==> Successfully created ${lock_file}"
}

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

echo "==> Preparation complete!"