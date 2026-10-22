#!/usr/bin/env bash
# One shard of the e2e suite on the emulator the workflow started (spec 043, contracts/ci.md).
# Usage: scripts/ci/android-e2e.sh <shard> <shards> <apk>
# One script, because the emulator action runs every line of its `script:` in a shell of its own.
set -euo pipefail
shard="$1"
shards="$2"
apk="$3"

# chromedriver must have the major version of the emulator's web view (Chrome for Testing).
major="$(adb shell dumpsys package com.google.android.webview \
  | sed -n 's/.*versionName=\([0-9][0-9]*\)\..*/\1/p' | head -1)"
if [ -z "$major" ]; then
  echo "the emulator reports no web view version" >&2
  exit 1
fi
url="$(curl -fsSL https://googlechromelabs.github.io/chrome-for-testing/latest-versions-per-milestone-with-downloads.json \
  | node -e '
    let text = ""
    process.stdin.on("data", (chunk) => (text += chunk))
    process.stdin.on("end", () => {
      const milestone = JSON.parse(text).milestones[process.argv[1]]
      const download = milestone?.downloads?.chromedriver?.find((d) => d.platform === "linux64")
      if (!download) process.exit(1)
      console.log(download.url)
    })' "$major")"
curl -fsSL -o "$RUNNER_TEMP/chromedriver.zip" "$url"
unzip -q -o "$RUNNER_TEMP/chromedriver.zip" -d "$RUNNER_TEMP"
export E2E_CHROMEDRIVER="$RUNNER_TEMP/chromedriver-linux64/chromedriver"

# The emulator's GPU is SwiftShader on the runner's CPUs: every new document compiled some 150
# shader programs, frames took seconds and held up the app's main thread (and with it the answers
# to the page), and a film on top made the emulator drop adb. The web view rasters on the CPU
# instead; it reads the flags of a debuggable app from this file (the first word is a stand-in for
# the program name). `pm clear` before each scenario leaves /data/local/tmp alone.
adb shell 'echo "_ --disable-gpu-rasterization" > /data/local/tmp/webview-command-line'

# The emulator's own state, written on this machine for the whole run: when the emulator stops
# answering (adb "device offline", a web view renderer lost under load), a scenario's material can no
# longer read logcat from the phone. Uploaded with the failure material.
health="src-tauri/target/e2e/android-shard-$shard"
mkdir -p "$health"
(
  # A new logcat after a reconnect starts at the newest line instead of repeating the buffer.
  since=()
  while true; do
    adb logcat -v time "${since[@]}" '*:I' || true
    echo "== logcat ended $(date -u +%FT%TZ)"
    since=(-T 1)
    sleep 2
  done
) >"$health/logcat.txt" 2>&1 &
logcat_loop=$!
(
  while true; do
    echo "== $(date -u +%FT%TZ)"
    timeout 10 adb shell 'head -3 /proc/meminfo; cat /proc/pressure/memory /proc/loadavg 2>/dev/null; ps -A -o RSS,NAME | sort -rn | head -5' 2>&1 ||
      echo "adb: no answer"
    sleep 5
  done
) >"$health/emulator-health.txt" 2>&1 &
health_loop=$!
# The adb server's own log tells transport drops (adb "device offline") apart from the app's faults.
finish() {
  pkill -P "$logcat_loop" 2>/dev/null
  kill "$logcat_loop" "$health_loop" 2>/dev/null
  cp "${TMPDIR:-/tmp}/adb.$(id -u).log" "$health/adb-server.log" 2>/dev/null
  # One line per shard in the job log, to compare runs without opening the material.
  echo "emulator summary: adb transport drops $(grep -c 'connection terminated' "$health/adb-server.log" 2>/dev/null)"
  # How hard the emulator's GPU worked: frames of a second or more, and shader programs linked.
  echo "rendering summary: frames of 1 s or more $(grep -cE 'Davey! duration=[0-9]{4,}ms' "$health/logcat.txt" 2>/dev/null), shader programs bound $(grep -c 's_glBindAttribLocation' "$health/logcat.txt" 2>/dev/null)"
}
trap 'finish || true' EXIT

corepack pnpm test:e2e --platform android --apk "$apk" --shard "$shard/$shards"
