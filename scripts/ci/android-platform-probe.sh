#!/usr/bin/env bash
# The platform probe on the emulator the workflow started (spec 044, T007, research R4).
# Usage: scripts/ci/android-platform-probe.sh <apk built with the feature platform-probe>
# One script, because the emulator action runs every line of its `script:` in a shell of its own.
# Two runs (T090): without "Zugriff auf alle Dateien" the step all-files reports `granted: false`;
# with the grant from `appops` it lists the shared storage and sees a file adb creates there.
set -euo pipefail
apk="$1"
package=com.haex.holzi
result="$RUNNER_TEMP/platform-probe-result.txt"
touched=/storage/emulated/0/Download/probe.txt

echo "web view: $(adb shell dumpsys package com.google.android.webview \
  | sed -n 's/.*versionName=\([^ ]*\).*/\1/p' | head -1)"

adb install -r "$apk"

# probe <expected start of the all-files detail> [touch]: one run of the probe, judged.
probe() {
  local expect="$1" touch="${2:-}" touched_yet=""
  # The generated activity is singleTask. Stop any instance restored with the emulator snapshot so
  # the next launch creates a fresh Rust process that observes the probe property and starts a new
  # listener instead of reusing a stale WebView/process from an earlier run.
  adb shell am force-stop "$package"
  # A rerun must not mistake a result left by an older process for this run's result.
  adb shell run-as "$package" rm -f cache/platform-probe-result.txt \
    cache/platform-probe-started.txt cache/platform-probe-watching.txt
  # Read by `platform_probe::requested` through `getprop`; the shell user may set `debug.*`.
  adb shell setprop debug.holzi.probe 1
  echo "debug.holzi.probe=$(adb shell getprop debug.holzi.probe), expecting all-files=$expect"
  adb shell am start -n "$package/.MainActivity"

  # The probe writes its line into holzi's cache directory (`platform_probe::RESULT_FILE`); a debug
  # APK lets the shell read it with `run-as`. The probe gives up after 90 s itself; the step
  # all-files waits up to 60 s for the file created once its watch stands.
  : > "$result"
  for _ in $(seq 1 60); do
    if [ -n "$touch" ] && [ -z "$touched_yet" ] \
      && adb shell run-as "$package" cat cache/platform-probe-watching.txt > /dev/null 2>&1; then
      adb shell touch "$touched"
      touched_yet=1
      echo "created $touched"
    fi
    if adb shell run-as "$package" cat cache/platform-probe-result.txt > "$result" 2>/dev/null \
      && grep -q HOLZI_PROBE_RESULT "$result"; then
      break
    fi
    sleep 3
  done
  adb shell setprop debug.holzi.probe 0

  if ! grep -q HOLZI_PROBE_RESULT "$result"; then
    echo "no result; started marker: $(adb shell run-as "$package" cat cache/platform-probe-started.txt 2>&1 || true)"
    echo "process: $(adb shell pidof "$package" || echo 'not running')"
    adb logcat -d -s holzi-probe || true
    adb logcat -d -b all | grep -iE "holzi|tauri|rust|chromium|AndroidRuntime" | tail -n 200 || true
  fi
  cat "$result"
  if ! node scripts/run-platform-probe.ts --judge "$result" --expect "all-files=$expect"; then
    # The web view's own reasons (policy, cleartext, network access) are only in logcat.
    adb logcat -d -s chromium:* cr_*:* | tail -n 100 || true
    # Console messages of the web view reach logcat under Tauri's own tags.
    adb logcat -d | grep -iE "127\.0\.0\.1|fetch|cors|private network|console|cleartext" | tail -n 100 || true
    exit 1
  fi
}

# The emulator snapshot may hold an earlier grant.
adb shell appops set "$package" MANAGE_EXTERNAL_STORAGE default
probe "granted: false"

adb shell rm -f "$touched"
adb shell appops set "$package" MANAGE_EXTERNAL_STORAGE allow
probe "granted: true" touch
