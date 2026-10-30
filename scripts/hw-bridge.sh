#!/usr/bin/env bash
# Host side (run on the Mac, outside the sandbox).
# Shares the DPS-150 serial port over TCP on 127.0.0.1 so a sandbox can reach it.
#
#   scripts/hw-bridge.sh [serial-port]
#
# Env: HW_PORT_TCP (default 5150)
#      HW_DEBUG    (optional) set to 1 for socat -d -d diagnostics.
#      HW_RECORD   (optional) file prefix: writes <prefix>.dev-to-host and
#                  <prefix>.host-to-dev raw dumps, usable as test fixtures.
#
# Safety: bound to loopback, one client at a time. The bridge does not
# filter anything: whoever connects can drive the supply. Use a resistive
# load only, and stop the bridge (Ctrl-C) when the session is over.
set -euo pipefail

tcp_port="${HW_PORT_TCP:-5150}"

if [[ $# -ge 1 ]]; then
  dev="$1"
else
  shopt -s nullglob
  cands=(/dev/cu.usbmodem*)
  if [[ ${#cands[@]} -ne 1 ]]; then
    echo "Expected exactly one /dev/cu.usbmodem*, found ${#cands[@]}: ${cands[*]:-none}" >&2
    echo "Pass the port explicitly: $0 /dev/cu.usbmodemXXXX" >&2
    exit 1
  fi
  dev="${cands[0]}"
fi

command -v socat >/dev/null || { echo "socat not found (brew install socat)" >&2; exit 1; }

rec=()
if [[ -n "${HW_DEBUG:-}" ]]; then rec+=(-d -d); fi
if [[ -n "${HW_RECORD:-}" ]]; then
  rec+=(-r "${HW_RECORD}.dev-to-host" -R "${HW_RECORD}.host-to-dev")
  echo "Recording to ${HW_RECORD}.{dev-to-host,host-to-dev}"
fi

echo "Bridging ${dev} <-> 127.0.0.1:${tcp_port} (Ctrl-C to stop)"
# No 'fork': a single client at a time. Loop so the sandbox can reconnect.
while true; do
  # Baud via stty: socat's b115200 option is unknown in the macOS build. CDC-ACM
  # probably ignores the rate (spec/PROTOCOL.md §1), so a failure here is not fatal.
  stty -f "${dev}" 115200 2>/dev/null || stty -F "${dev}" 115200 2>/dev/null || true
  # ${rec[@]+...} keeps an empty array from tripping 'set -u' on macOS bash 3.2
  socat ${rec[@]+"${rec[@]}"} "${dev},raw,echo=0" "TCP-LISTEN:${tcp_port},bind=127.0.0.1,reuseaddr" || true
  sleep 0.5
done
