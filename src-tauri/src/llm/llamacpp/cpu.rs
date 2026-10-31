//! The CPU side of llama.cpp (spec 070 R3): which processors run the ggml this build carries, and how many
//! threads a generation uses.
//!
//! Desktop builds compile ggml for AVX2 (`.cargo/config.toml`), because without it ggml falls back to its
//! plain C kernels and answers several times slower. A processor without those extensions would stop at the
//! first such instruction, so it gets no llama.cpp at all instead; mistralrs still runs there.

/// The x86 extensions ggml was compiled for that this processor lacks. Empty where it runs ggml, and always
/// empty off x86, where ggml is built for the platform's baseline.
pub fn missing_features() -> Vec<&'static str> {
    #[cfg(target_arch = "x86_64")]
    {
        use llama_cpp_sys_2 as sys;
        // SAFETY: each returns a constant compiled into ggml-cpu and touches no state.
        let checks = unsafe {
            [
                (
                    "avx",
                    sys::ggml_cpu_has_avx(),
                    is_x86_feature_detected!("avx"),
                ),
                (
                    "avx2",
                    sys::ggml_cpu_has_avx2(),
                    is_x86_feature_detected!("avx2"),
                ),
                (
                    "fma",
                    sys::ggml_cpu_has_fma(),
                    is_x86_feature_detected!("fma"),
                ),
                (
                    "f16c",
                    sys::ggml_cpu_has_f16c(),
                    is_x86_feature_detected!("f16c"),
                ),
                (
                    "bmi2",
                    sys::ggml_cpu_has_bmi2(),
                    is_x86_feature_detected!("bmi2"),
                ),
            ]
        };
        checks
            .into_iter()
            .filter(|&(_, built, present)| built != 0 && !present)
            .map(|(name, _, _)| name)
            .collect()
    }
    #[cfg(not(target_arch = "x86_64"))]
    Vec::new()
}

/// Threads for a generation: one per physical core. Reading the prompt scales with them; generating stops
/// gaining after about eight, where memory is the limit (`research.md` R3). llama.cpp's own default is 4.
pub fn default_threads() -> u32 {
    sysinfo::System::physical_core_count()
        .or_else(|| std::thread::available_parallelism().ok().map(|n| n.get()))
        .unwrap_or(4)
        .max(1) as u32
}

#[cfg(test)]
#[path = "cpu_tests.rs"]
mod tests;
