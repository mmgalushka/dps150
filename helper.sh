#!/usr/bin/env bash
# Development entry point: a thin dispatcher over cargo (DEVOPS.md §4.0).
set -euo pipefail

cd "$(dirname "$0")"

usage() {
    cat <<USAGE
Usage: ./helper.sh <command>

Commands:
  init    Check the prerequisites and build once (run this after cloning)
  run     Run the app: ./helper.sh run [--port <path>]
  build   Build every crate in the workspace
  test    Run every test in the workspace
  lint    Check formatting, run clippy and audit dependencies
  deb     Build the .deb package into target/debian/
  dist-plan  Show what a release would build
USAGE
}

require() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "helper.sh: missing tool: $1 ($2)" >&2
        exit 1
    fi
}

# Explains how to get cargo on the PATH, then stops.
missing_cargo() {
    echo "helper.sh: cargo not found. Install Rust with rustup:" >&2
    echo "  macOS (Homebrew): brew install rustup && rustup default stable" >&2
    echo "  any system:       https://rustup.rs" >&2
    if [ -x /opt/homebrew/opt/rustup/bin/cargo ] || [ -x /usr/local/opt/rustup/bin/cargo ]; then
        echo "Homebrew's rustup is installed but not on your PATH. Add this to ~/.zshrc and open a new terminal:" >&2
        echo '  export PATH="$(brew --prefix rustup)/bin:$PATH"' >&2
    fi
    exit 1
}

command_name="${1:-}"
case "$command_name" in
    init)
        command -v cargo >/dev/null 2>&1 || missing_cargo
        # Linux needs libudev and pkg-config for serial port enumeration.
        if [ "$(uname -s)" = "Linux" ]; then
            if ! command -v pkg-config >/dev/null 2>&1 || ! pkg-config --exists libudev; then
                echo "helper.sh: missing libudev (Debian/Ubuntu: sudo apt install libudev-dev pkg-config)" >&2
                exit 1
            fi
        fi
        # The first cargo call installs the toolchain pinned in rust-toolchain.toml.
        echo "==> Toolchain"
        cargo --version
        echo "==> Build"
        cargo build --workspace --locked
        if ! command -v cargo-deny >/dev/null 2>&1; then
            echo "Note: ./helper.sh lint needs cargo-deny: cargo install cargo-deny --locked"
        fi
        echo
        echo "Ready. Connect the DPS-150 with its USB cable and run: ./helper.sh run"
        if [ "$(uname -s)" = "Linux" ]; then
            echo "Linux: your user needs the serial port group: sudo usermod -aG dialout \$USER (then log in again)"
        fi
        ;;
    run)
        require cargo "run ./helper.sh init for setup help"
        shift
        cargo run --locked -- "$@"
        ;;
    build)
        require cargo "install with rustup"
        cargo build --workspace --locked
        ;;
    test)
        require cargo "install with rustup"
        cargo test --workspace --locked
        ;;
    lint)
        require cargo "install with rustup"
        require cargo-deny "install with: cargo install cargo-deny --locked"
        cargo fmt --all --check
        cargo clippy --workspace --all-targets --locked -- -D warnings
        cargo deny --locked check
        ;;
    deb)
        require cargo-deb "install with: cargo install cargo-deb --locked"
        cargo deb --locked
        ;;
    dist-plan)
        require dist "install with: cargo install cargo-dist --locked"
        dist plan
        ;;
    *)
        usage
        if [ -n "$command_name" ]; then
            echo "helper.sh: unknown command: $command_name" >&2
        fi
        exit 2
        ;;
esac
