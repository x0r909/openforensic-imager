#!/bin/sh
set -eu

export LD_LIBRARY_PATH="/app/lib/openforensic-imager${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export QT_QUICK_CONTROLS_STYLE=Basic

exec /app/bin/openforensic-imager.real "$@"
