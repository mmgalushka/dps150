#!/usr/bin/env bash
# Sandbox side. Exposes the host bridge (scripts/hw-bridge.sh) as a local
# pseudo-terminal, so the app can use it like a real serial port:
#
#   scripts/hw-attach.sh &
#   dps150 --port /tmp/ttyDPS0
#
# Env: HW_HOST     (default host.docker.internal)
#      HW_PORT_TCP (default 5150)
#      HW_LINK     (default /tmp/ttyDPS0)
#
# The host must allow the port first:  sbx policy allow network localhost:5150
set -euo pipefail

host="${HW_HOST:-host.docker.internal}"
tcp_port="${HW_PORT_TCP:-5150}"
link="${HW_LINK:-/tmp/ttyDPS0}"

echo "Attaching ${link} <-> ${host}:${tcp_port} (Ctrl-C to stop)"
exec socat "pty,link=${link},raw,echo=0" "TCP:${host}:${tcp_port}"
