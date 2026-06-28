#!/usr/bin/env bash
# install-uc.sh — Install Uten Core into JRE-like directory structure.
#
# Produces:
#   <prefix>/uc/
#   ├── bin/
#   │   ├── uc         VM launcher
#   │   ├── ucc        compiler
#   │   ├── ucw        windowed launcher
#   │   ├── ucdump     disassembler
#   │   └── utencore   all-in-one CLI
#   ├── lib/
#   │   ├── libutencore.so    core library
#   │   ├── python/           py2uc stdlib .uclib files
#   │   └── ucif/             CIB interface definitions (.ucif)
#   └── conf/
#       └── uc.conf

set -euo pipefail

PREFIX="${1:-/usr/local}"
UC_HOME="$PREFIX/uc"
BUILD_DIR="$(dirname "$0")/target/release"

echo "Installing Uten Core to $UC_HOME"

# Create directories
mkdir -p "$UC_HOME/bin"
mkdir -p "$UC_HOME/lib/python"
mkdir -p "$UC_HOME/lib/ucif"
mkdir -p "$UC_HOME/conf"

# Install binaries
for bin in uc ucc ucw ucdump utencore; do
    if [ -f "$BUILD_DIR/$bin" ]; then
        cp "$BUILD_DIR/$bin" "$UC_HOME/bin/$bin"
        chmod 755 "$UC_HOME/bin/$bin"
        echo "  bin/$bin"
    else
        echo "  WARNING: $bin not found at $BUILD_DIR/$bin"
    fi
done

# Install core shared library
if [ -f "$BUILD_DIR/libutencore.so" ]; then
    cp "$BUILD_DIR/libutencore.so" "$UC_HOME/lib/"
    echo "  lib/libutencore.so"
fi
if [ -f "$BUILD_DIR/libutencore.dll" ]; then
    cp "$BUILD_DIR/libutencore.dll" "$UC_HOME/lib/"
    echo "  lib/libutencore.dll"
fi

# Install stdlib
if [ -d "compilers/py2uc/lib/python" ]; then
    cp -r compilers/py2uc/lib/python/*.py "$UC_HOME/lib/python/" 2>/dev/null || true
    echo "  lib/python/ (stdlib sources)"
fi
# Copy compiled .uclib stdlib from build output
STDLIB_OUT="$BUILD_DIR/build/compilers/py2uc/out/lib/python"
if [ -d "$STDLIB_OUT" ]; then
    cp "$STDLIB_OUT"/*.uclib "$UC_HOME/lib/python/" 2>/dev/null || true
fi

# Install UCIF interface definitions
if [ -d "ucif" ]; then
    cp ucif/*.ucif "$UC_HOME/lib/ucif/" 2>/dev/null || true
    cp ucif/*.yaml "$UC_HOME/lib/ucif/" 2>/dev/null || true
    echo "  lib/ucif/"
fi

# Install config
cat > "$UC_HOME/conf/uc.conf" << 'CONF'
# Uten Core configuration
# uc.home: path to installation (auto-detected from binary location)
uc.lib: ${uc.home}/lib
uc.stdlib.python: ${uc.lib}/python
uc.ucif.path: ${uc.lib}/ucif
CONF
echo "  conf/uc.conf"

# Create symlinks in $PREFIX/bin for convenience
for bin in uc ucc ucw ucdump utencore; do
    if [ -f "$UC_HOME/bin/$bin" ]; then
        ln -sf "$UC_HOME/bin/$bin" "$PREFIX/bin/$bin" 2>/dev/null || true
    fi
done

echo ""
echo "Done. Add $PREFIX/bin to your PATH if not already."
echo "Try: uc hello.uclib"
