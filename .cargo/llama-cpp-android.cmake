# Spec 070 R3 (operator 2026-11-06): ggml for the Android ARM baseline. Without it the sys crate builds
# ggml-cpu for plain armv8-a, whose kernels read a prompt several times slower. A phone without these
# extensions gets no llama.cpp (`llm/llamacpp/cpu.rs`): no local chat model is better than a slow one.
# dotprod (armv8.2-a, phones from about 2018) carries nearly all of the gain; i8mm (armv8.6-a) added 13 %
# to reading the prompt on the Pixel 11 and would leave out phones up to 2022 (`research.md` R3).
# `.cargo/config.toml` hands this file to llama.cpp's CMake as CMAKE_PROJECT_INCLUDE; it acts on Android
# ARM only; the Mac and Linux on ARM keep their own baselines.
if(ANDROID AND CMAKE_SYSTEM_PROCESSOR MATCHES "^(aarch64|arm64)")
  set(GGML_CPU_ARM_ARCH "armv8.2-a+dotprod+fp16" CACHE STRING "ggml ARM baseline (spec 070 R3)" FORCE)
endif()
