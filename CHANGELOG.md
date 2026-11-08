# Changelog

All notable changes are listed here, newest first. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow [Semantic Versioning](https://semver.org/) (while the version is 0.x, minor versions may change behaviour).

## [Unreleased]

## [1.0.0] - 2026-11-08

### Added
- Terminal interface for the FNIRSI DPS-150: main panel with digit-cursor editing, six presets (M1–M6, names kept by the app), protection settings, settings screen, help overlay.
- Output safety rules: confirmations, limit checks, link-loss handling, quit sequence (`spec/SAFETY.md`).
- Device simulator `dps150-sim` with fault injection.
- Releases for macOS and Linux (Homebrew, `.deb`).
