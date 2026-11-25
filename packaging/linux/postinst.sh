#!/bin/sh
set -e

if command -v udevadm >/dev/null 2>&1; then
  udevadm control --reload-rules >/dev/null 2>&1 || true
  udevadm trigger --subsystem-match=hwmon --action=change >/dev/null 2>&1 || true
fi
