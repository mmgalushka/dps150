# dps150
Terminal interface for FNIRSI DPS-150

## Documents
- [spec/MAIN.md](spec/MAIN.md): goals, scope, architecture, decision log (start here)
- [spec/PROTOCOL.md](spec/PROTOCOL.md): serial protocol of the device, verified on hardware
- [spec/DESIGN.md](spec/DESIGN.md): UI design: layout, colours, screens, interaction
- [spec/SAFETY.md](spec/SAFETY.md): safety and behaviour rules: limits, confirmations, link loss, quit
- [spec/APP.md](spec/APP.md): data binding, polling and update model, configuration files
- [spec/SIMULATOR.md](spec/SIMULATOR.md): the device simulator: purpose, behaviour, assumed behaviour, fault-injection commands
- [spec/REQUIREMENTS.md](spec/REQUIREMENTS.md): register of testable requirements with stable IDs, status and verification level
- [spec/PLAN.md](spec/PLAN.md): implementation plan: stages, milestones, hardware session, requirement coverage
- [DEVOPS.md](DEVOPS.md): toolchain, testing, CI, release and installation
- [CONTRIBUTING.md](CONTRIBUTING.md): repository conventions (git ignore allowlist)

## Develop
Cloned the repository? Run `./helper.sh init`, then `./helper.sh run`. Full steps: [CONTRIBUTING.md](CONTRIBUTING.md#getting-started).

## Install
```
# macOS and Linux with Homebrew
brew install mmgalushka/tap/dps150

# Debian and Ubuntu: download the .deb from the latest release
sudo apt install ./dps150_<version>_<arch>.deb
```
On Linux your user needs access to the serial port: `sudo usermod -aG dialout $USER`, then log in again (the `.deb` prints this note). macOS needs no setup.

## Usage
```
dps150 [--port <path>] [--help] [--version]
```
Connect the DPS-150 with its USB cable and run `dps150`. The port is found by USB ID (`2E3C:5740`); `--port /dev/cu.usbmodemXXXX` (macOS) or `--port /dev/ttyACM0` (Linux) overrides it.

Main keys: `←→` move the digit cursor, `↑↓` change the digit, `⏎` or `v` / `i` type a value, `Tab` switch V/I, `Space` output on/off, `Ctrl-X` output off, `m 1-6` recall a preset, `p` presets, `o` protection, `,` settings, `?` help, `q` quit. Press `?` on any screen for that screen's keys. Settings are saved in `~/.config/dps150/config.toml`, preset names in `presets.toml`.
