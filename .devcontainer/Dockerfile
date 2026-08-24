# Devcontainer base image is Rust on Debian Bookworm
ARG BASE_IMAGE=mcr.microsoft.com/devcontainers/rust:2-1-trixie
FROM ${BASE_IMAGE}

# Install Tauri v2 System Dependencies & Tools
# Need at least: https://tauri.app/start/prerequisites/
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
       git \
       curl \
       sudo \
       bash \
       ca-certificates \
       wget \
       file \
       build-essential \
       libxdo-dev \
       libssl-dev \
       libayatana-appindicator3-dev \
       librsvg2-dev \
       javascriptcoregtk-4.1 \
       libsoup-3.0-dev \
       libwebkit2gtk-4.1-dev \
       dbus \
       dbus-x11 \
       clang \
       pipx \
       flatpak \
       flatpak-builder \
    && rm -rf /var/lib/apt/lists/*

# Install cargo-binstall, see https://crates.io/crates/cargo-binstall
RUN curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash

# Use cargo-binstall to install tools:
RUN cargo binstall --no-confirm \
  wild-linker \
  dioxus-cli \
  trunk \
  tauri-cli

# Setup Wild linker, see https://github.com/wild-linker/wild and https://github.com/wild-linker/action/blob/dc926c45399e1842cf81e604f52bdf0ad86a0c09/action.yml
RUN mkdir -p "/.cargo" \
  && echo "\n[target.x86_64-unknown-linux-gnu]\nlinker = \"clang\"\nrustflags = [\"-Clink-arg=--ld-path=wild\"]\n" >>"/.cargo/config.toml" \
  && echo "\n[target.x86_64-unknown-linux-musl]\nlinker = \"clang\"\nrustflags = [\"-Clink-arg=--ld-path=wild\"]\n" >>"/.cargo/config.toml" \
  && echo "\n[target.aarch64-unknown-linux-gnu]\nlinker = \"clang\"\nrustflags = [\"-Clink-arg=--ld-path=wild\"]\n" >>"/.cargo/config.toml" \
  && echo "\n[target.aarch64-unknown-linux-musl]\nlinker = \"clang\"\nrustflags = [\"-Clink-arg=--ld-path=wild\"]\n" >>"/.cargo/config.toml"

# Install wasm target for rust
RUN rustup target add wasm32-unknown-unknown

# Prepare for building flatpak, see https://v2.tauri.app/distribute/flatpak/
# USER vscode
# RUN pipx install flatpak-cargo-generator
# RUN flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo

# Install flatpak GNOME runtime, required for building flatpak (very large, several GB):
# RUN flatpak install --user -y flathub org.gnome.Platform//50 org.gnome.Sdk//50 org.freedesktop.Sdk.Extension.rust-stable//24.08
