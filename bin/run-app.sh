#!/usr/bin/env bash
# Launch the desktop app with diagnostics. Output goes to /tmp/lam.log.
set -u
LOG=/tmp/lam.log
: > "$LOG"
export RUST_BACKTRACE=full
export RUST_LOG="${RUST_LOG:-info,tauri=debug,wry=debug}"
export LIBGL_ALWAYS_SOFTWARE=1
export WEBKIT_DISABLE_DMABUF_RENDERER=1
export WEBKIT_DISABLE_COMPOSITING_MODE=1
export WEBKIT_FORCE_SANDBOX=0
export G_MESSAGES_DEBUG=all

cd "$(dirname "$0")/.."
echo "log -> $LOG"
./src-tauri/target/debug/nolost 2>&1 | tee -a "$LOG"
echo "--- exit: $? ---" >> "$LOG"
