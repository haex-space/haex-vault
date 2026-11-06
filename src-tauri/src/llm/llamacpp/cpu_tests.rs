use super::default_threads;

/// Guards `.cargo/config.toml`: without the GGML_* switches ggml builds its plain C kernels and nothing
/// fails, it is only several times slower. A target dir built before the switches keeps the old ggml until
/// `cargo clean -p llama-cpp-sys-2`.
#[cfg(target_arch = "x86_64")]
#[test]
fn ggml_is_built_for_the_avx2_baseline() {
    use llama_cpp_sys_2 as sys;
    // SAFETY: constants compiled into ggml-cpu.
    let built = unsafe {
        [
            sys::ggml_cpu_has_avx(),
            sys::ggml_cpu_has_avx2(),
            sys::ggml_cpu_has_fma(),
            sys::ggml_cpu_has_f16c(),
            sys::ggml_cpu_has_bmi2(),
        ]
    };
    assert_eq!(built, [1; 5], "avx, avx2, fma, f16c, bmi2");
}

/// Guards `.cargo/llama-cpp-android.cmake` the same way for Android on ARM (operator 2026-11-06): without
/// it ggml-cpu builds for plain armv8-a and reads a prompt several times slower.
#[cfg(all(target_os = "android", target_arch = "aarch64"))]
#[test]
fn ggml_is_built_for_the_android_arm_baseline() {
    use llama_cpp_sys_2 as sys;
    // SAFETY: constants compiled into ggml-cpu.
    let built = unsafe { [sys::ggml_cpu_has_dotprod(), sys::ggml_cpu_has_fp16_va()] };
    assert_eq!(built, [1; 2], "dotprod, fp16");
}

#[test]
fn a_generation_gets_at_least_one_thread() {
    assert!(default_threads() >= 1);
}
