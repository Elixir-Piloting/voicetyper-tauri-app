#!/usr/bin/env bash
set -euo pipefail

APP_NAME="voicetyper"
BUNDLE_DIR="src-tauri/target/release/bundle"
ICON_SRC="src-tauri/icons/icon.png"

if [ -f "$BUNDLE_DIR/deb/${APP_NAME}_0.1.0_amd64.deb" ]; then
    echo "Installing .deb package..."
    sudo dpkg -i "$BUNDLE_DIR/deb/${APP_NAME}_0.1.0_amd64.deb"
elif [ -f "$BUNDLE_DIR/rpm/${APP_NAME}-0.1.0-1.x86_64.rpm" ]; then
    echo "Installing .rpm package..."
    sudo rpm -i "$BUNDLE_DIR/rpm/${APP_NAME}-0.1.0-1.x86_64.rpm"
elif [ -f "$BUNDLE_DIR/appimage/${APP_NAME}_0.1.0_amd64.AppImage" ]; then
    echo "Installing AppImage..."
    mkdir -p "$HOME/.local/bin"
    cp "$BUNDLE_DIR/appimage/${APP_NAME}_0.1.0_amd64.AppImage" "$HOME/.local/bin/${APP_NAME}"
    chmod +x "$HOME/.local/bin/${APP_NAME}"
else
    echo "No pre-built bundle found. Building from source..."
    cd src-tauri
    cargo build --release
    cd ..

    mkdir -p "$HOME/.local/bin"
    cp "src-tauri/target/release/${APP_NAME}" "$HOME/.local/bin/"
fi

mkdir -p "$HOME/.local/share/applications"
cp "voicetyper.desktop" "$HOME/.local/share/applications/"

mkdir -p "$HOME/.local/share/icons/hicolor/256x256/apps"
cp "$ICON_SRC" "$HOME/.local/share/icons/hicolor/256x256/apps/${APP_NAME}.png"

update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
gtk-update-icon-cache "$HOME/.local/share/icons/hicolor" 2>/dev/null || true

echo "✓ ${APP_NAME} installed. Launch from your application menu or run: ${APP_NAME}"
