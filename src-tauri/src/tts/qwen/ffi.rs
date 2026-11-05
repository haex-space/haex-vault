//! The only `unsafe` code of spec 081 (research R1): owners for the llama.cpp handles Qwen3-TTS
//! needs. `llama-cpp-2` keeps the raw pointers of its model and context private and does not wrap
//! llama.cpp's audio generation helper, so this module calls `llama-cpp-sys-2` directly. Every
//! handle has one owner that frees it on drop; [`Handles`] drops them in the reverse order of
//! creation. The process backend is the chat engine's (`llm::llamacpp::model::backend`).

use std::ffi::{c_char, CString};
use std::path::Path;
use std::ptr::NonNull;

use llama_cpp_sys_2 as sys;

use super::super::TtsError;

fn unavailable(reason: impl Into<String>) -> TtsError {
    TtsError::Unavailable {
        reason: reason.into(),
    }
}

fn failed(reason: impl Into<String>) -> TtsError {
    TtsError::Failed {
        reason: reason.into(),
    }
}

fn c_path(path: &Path) -> Result<CString, TtsError> {
    let text = path
        .to_str()
        .ok_or_else(|| unavailable(format!("model path {} is not UTF-8", path.display())))?;
    CString::new(text).map_err(|_| unavailable(format!("model path {} has a NUL", path.display())))
}

/// How the model is placed: all layers on the graphics card, or none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    Gpu,
    Cpu,
}

/// What [`Handles::open`] needs.
pub struct OpenOptions<'a> {
    pub backbone: &'a Path,
    pub audio: &'a Path,
    pub placement: Placement,
    pub threads: i32,
}

/// The backbone, its context, the audio part and the generation helper of one loaded model.
/// Field order is drop order: the helper first, the backbone last.
pub struct Handles {
    gen: NonNull<sys::mtmd_helper_gen_audio>,
    mtmd: NonNull<sys::mtmd_context>,
    context: NonNull<sys::llama_context>,
    model: NonNull<sys::llama_model>,
    sample_rate: u32,
    reference_rate: u32,
}

// SAFETY: the handles are only used through `&mut Handles` (the caller keeps them behind a
// `Mutex`), and llama.cpp's model, context and mtmd context may move between threads as long as
// one thread uses them at a time.
unsafe impl Send for Handles {}

impl Handles {
    /// Loads the backbone and the audio part and checks that they generate Qwen3-TTS audio.
    pub fn open(options: &OpenOptions<'_>) -> Result<Self, TtsError> {
        crate::llm::llamacpp::model::backend()
            .map_err(|e| unavailable(format!("start llama.cpp: {e}")))?;
        let backbone = c_path(options.backbone)?;
        let audio = c_path(options.audio)?;
        let on_gpu = options.placement == Placement::Gpu;
        // SAFETY: the paths are valid C strings for the duration of each call; every returned
        // pointer is checked for null before it is owned, and owned pointers are freed exactly
        // once in `Drop` (or here on a later failure, by dropping the partly built owners).
        unsafe {
            let mut model_params = sys::llama_model_default_params();
            model_params.n_gpu_layers = if on_gpu { i32::MAX } else { 0 };
            let model = NonNull::new(sys::llama_model_load_from_file(
                backbone.as_ptr(),
                model_params,
            ))
            .ok_or_else(|| unavailable("load the Qwen3-TTS backbone"))?;
            let model = ModelOwner(model);

            let mut context_params = sys::llama_context_default_params();
            context_params.n_ctx = 4096;
            context_params.n_batch = 512;
            context_params.n_ubatch = 512;
            context_params.n_threads = options.threads;
            context_params.n_threads_batch = options.threads;
            // The helper reads the backbone's hidden state after every frame.
            context_params.embeddings = true;
            let context =
                NonNull::new(sys::llama_init_from_model(model.0.as_ptr(), context_params))
                    .ok_or_else(|| unavailable("create the Qwen3-TTS context"))?;
            let context = ContextOwner(context);

            let mut mtmd_params = sys::mtmd_context_params_default();
            mtmd_params.use_gpu = on_gpu;
            mtmd_params.n_threads = options.threads;
            mtmd_params.print_timings = false;
            let mtmd = NonNull::new(sys::mtmd_init_from_file(
                audio.as_ptr(),
                model.0.as_ptr(),
                mtmd_params,
            ))
            .ok_or_else(|| unavailable("load the Qwen3-TTS audio part"))?;
            let mtmd = MtmdOwner(mtmd);

            let info = sys::mtmd_gen_audio_get_info(mtmd.0.as_ptr());
            if info.type_ != sys::MTMD_GEN_AUDIO_TYPE_QWEN3TTS {
                return Err(unavailable(
                    "the audio part does not generate Qwen3-TTS audio",
                ));
            }
            let sample_rate = u32::try_from(info.sample_rate)
                .ok()
                .filter(|rate| *rate > 0)
                .ok_or_else(|| unavailable("the audio part reports no sample rate"))?;
            let reference_rate = u32::try_from(sys::mtmd_get_audio_sample_rate(mtmd.0.as_ptr()))
                .ok()
                .filter(|rate| *rate > 0)
                .ok_or_else(|| unavailable("the audio part reads no reference audio"))?;

            let gen = NonNull::new(sys::mtmd_helper_gen_audio_init(
                context.0.as_ptr(),
                mtmd.0.as_ptr(),
            ))
            .ok_or_else(|| unavailable("start the Qwen3-TTS audio generation"))?;

            let handles = Self {
                gen,
                mtmd: mtmd.0,
                context: context.0,
                model: model.0,
                sample_rate,
                reference_rate,
            };
            // `handles` owns them now.
            std::mem::forget(mtmd);
            std::mem::forget(context);
            std::mem::forget(model);
            Ok(handles)
        }
    }

    /// The rate of the PCM the model writes.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// The rate a reference recording must have.
    pub fn reference_rate(&self) -> u32 {
        self.reference_rate
    }

    /// Forgets the previous part: the backbone's memory and the helper's output.
    pub fn reset(&mut self) {
        // SAFETY: both handles are live and used by this thread only (`&mut self`).
        unsafe {
            sys::llama_memory_clear(sys::llama_get_memory(self.context.as_ptr()), true);
            sys::mtmd_helper_gen_audio_reset(self.gen.as_ptr());
        }
    }

    /// Hands the helper the text of one part, the voice and the language.
    pub fn set_input(&mut self, input: &Input<'_>) -> Result<(), TtsError> {
        let prompt = CString::new(input.text).map_err(|_| failed("the part has a NUL"))?;
        let language = CString::new(input.language).map_err(|_| failed("bad language code"))?;
        // SAFETY: `inp` is fully initialised (zeroed, then every field the helper reads is set);
        // the strings and the bitmap outlive the call, which copies what it keeps.
        unsafe {
            let mut inp: sys::mtmd_helper_gen_audio_inp = std::mem::zeroed();
            inp.seq_id = 0;
            inp.prompt = prompt.as_ptr();
            inp.prompt_len = input.text.len();
            inp.speaker_ref = input.voice.0.as_ptr();
            inp.lang = language.as_ptr();
            inp.top_k = TOP_K;
            inp.top_p = TOP_P;
            inp.seed = input.seed;
            inp.out_type = sys::MTMD_HELPER_GEN_AUDIO_OUTTYPE_PCM;
            if sys::mtmd_helper_gen_audio_set_input(self.gen.as_ptr(), &inp) != 0 {
                return Err(failed("Qwen3-TTS refused the part"));
            }
        }
        Ok(())
    }

    /// Reads up to `batch` prompt tokens; `Ok(true)` once the whole prompt is read.
    pub fn step_prompt(&mut self, batch: i32) -> Result<bool, TtsError> {
        // SAFETY: the helper is live and owned by this thread (`&mut self`).
        let left = unsafe { sys::mtmd_helper_gen_audio_step_prompt(self.gen.as_ptr(), batch) };
        if left < 0 {
            return Err(failed("Qwen3-TTS could not read the part"));
        }
        Ok(left == 0)
    }

    /// The backbone's hidden state after the prompt, the start of the frame loop. An error when
    /// llama.cpp has none, so the helper is never handed a null state.
    pub fn first_state(&mut self) -> Result<*const f32, TtsError> {
        // SAFETY: the context is live; -1 asks for the last output, which the prompt step wrote.
        let state = unsafe { sys::llama_get_embeddings_ith(self.context.as_ptr(), -1) };
        if state.is_null() {
            return Err(failed("Qwen3-TTS has no state after reading the part"));
        }
        Ok(state)
    }

    /// Samples the backbone's next token with `sampler`.
    pub fn sample(&mut self, sampler: &mut Sampler) -> sys::llama_token {
        // SAFETY: both are live and used by this thread only.
        unsafe { sys::llama_sampler_sample(sampler.0.as_ptr(), self.context.as_ptr(), -1) }
    }

    /// Generates one frame. Returns the next hidden state, or `None` when the model is done.
    pub fn step_frame(
        &mut self,
        token: sys::llama_token,
        state: *const f32,
    ) -> Result<Option<*const f32>, TtsError> {
        let mut next: *const f32 = std::ptr::null();
        let mut stop = false;
        // SAFETY: `state` came from `first_state` or the previous `step_frame` and stays valid
        // until this call, as the helper documents; the out-pointers are valid locals.
        let ret = unsafe {
            sys::mtmd_helper_gen_audio_step_gen(
                self.gen.as_ptr(),
                token,
                state,
                &mut next,
                &mut stop,
            )
        };
        if ret != 0 {
            return Err(failed("Qwen3-TTS failed during a frame"));
        }
        Ok((!stop && !next.is_null()).then_some(next))
    }

    /// The PCM of every frame since [`Handles::set_input`].
    pub fn output(&mut self) -> Result<Vec<f32>, TtsError> {
        let mut rate = 0i32;
        let mut data: *const c_char = std::ptr::null();
        let mut len = 0usize;
        let mut samples = 0i64;
        // SAFETY: the out-pointers are valid locals; `data` holds `samples` f32 values (PCM out
        // type) and stays valid until the next `get_output` or `reset`, so it is copied at once.
        unsafe {
            if sys::mtmd_helper_gen_audio_get_output(
                self.gen.as_ptr(),
                &mut rate,
                &mut data,
                &mut len,
                &mut samples,
            ) != 0
            {
                return Err(failed("Qwen3-TTS returned no audio"));
            }
            let count = usize::try_from(samples).unwrap_or(0);
            if data.is_null() || count == 0 || len < count * std::mem::size_of::<f32>() {
                return Ok(Vec::new());
            }
            Ok(std::slice::from_raw_parts(data.cast::<f32>(), count).to_vec())
        }
    }
}

impl Drop for Handles {
    fn drop(&mut self) {
        // SAFETY: each pointer is owned here and freed once, helper first, backbone last.
        release("gen", || unsafe {
            sys::mtmd_helper_gen_audio_free(self.gen.as_ptr())
        });
        release("mtmd", || unsafe { sys::mtmd_free(self.mtmd.as_ptr()) });
        release("context", || unsafe {
            sys::llama_free(self.context.as_ptr())
        });
        release("model", || unsafe {
            sys::llama_model_free(self.model.as_ptr())
        });
    }
}

/// Frees the handle named `name` with `free`.
#[cfg(not(test))]
fn release(_name: &'static str, free: impl FnOnce()) {
    free();
}

/// Frees the handle named `name` with `free`, unless a test records the order instead
/// (`ffi_tests`); then `free` is not called, so a test can drop owners of dangling pointers.
#[cfg(test)]
fn release(name: &'static str, free: impl FnOnce()) {
    if !freed::note(name) {
        free();
    }
}

/// The names [`release`] saw while a test records them.
#[cfg(test)]
mod freed {
    use std::cell::RefCell;

    thread_local! {
        static NAMES: RefCell<Option<Vec<&'static str>>> = const { RefCell::new(None) };
    }

    /// Notes `name` and returns `true` while recording.
    pub(super) fn note(name: &'static str) -> bool {
        NAMES.with(|names| match &mut *names.borrow_mut() {
            Some(names) => {
                names.push(name);
                true
            }
            None => false,
        })
    }

    /// Runs `f` with recording on and returns what was freed, in order.
    pub(super) fn recorded(f: impl FnOnce()) -> Vec<&'static str> {
        NAMES.with(|names| *names.borrow_mut() = Some(Vec::new()));
        f();
        NAMES.with(|names| names.borrow_mut().take().unwrap_or_default())
    }
}

#[cfg(test)]
#[path = "ffi_tests.rs"]
mod ffi_tests;

/// Top-k of the backbone and the code predictor (research R4).
const TOP_K: i32 = 40;
/// Top-p of the backbone and the code predictor (research R4).
const TOP_P: f32 = 0.95;

/// What one part needs.
pub struct Input<'a> {
    pub text: &'a str,
    pub language: &'a str,
    pub voice: &'a VoiceBitmap,
    pub seed: u32,
}

/// A voice's reference recording as the audio part reads it.
pub struct VoiceBitmap(NonNull<sys::mtmd_bitmap>);

// SAFETY: a bitmap is plain data owned by this value; after creation it is only read (during
// `set_input`, under the model's lock), so it may move and be shared between threads.
unsafe impl Send for VoiceBitmap {}
unsafe impl Sync for VoiceBitmap {}

impl VoiceBitmap {
    /// A bitmap of `samples`, which must already be at [`Handles::reference_rate`].
    pub fn new(samples: &[f32]) -> Result<Self, TtsError> {
        if samples.is_empty() {
            return Err(failed("the voice has no recording"));
        }
        // SAFETY: `samples` is valid for `len` floats during the call, which copies them.
        let bitmap = unsafe { sys::mtmd_bitmap_init_from_audio(samples.len(), samples.as_ptr()) };
        NonNull::new(bitmap)
            .map(Self)
            .ok_or_else(|| failed("read the voice recording"))
    }
}

impl Drop for VoiceBitmap {
    fn drop(&mut self) {
        // SAFETY: owned here, freed once.
        unsafe { sys::mtmd_bitmap_free(self.0.as_ptr()) }
    }
}

/// The backbone's sampler chain for one part (research R4).
pub struct Sampler(NonNull<sys::llama_sampler>);

impl Sampler {
    pub fn new(seed: u32) -> Result<Self, TtsError> {
        // SAFETY: the chain takes ownership of every sampler added to it and is freed in `Drop`.
        unsafe {
            let chain = NonNull::new(sys::llama_sampler_chain_init(
                sys::llama_sampler_chain_default_params(),
            ))
            .ok_or_else(|| failed("create the sampler"))?;
            sys::llama_sampler_chain_add(chain.as_ptr(), sys::llama_sampler_init_top_k(TOP_K));
            sys::llama_sampler_chain_add(chain.as_ptr(), sys::llama_sampler_init_top_p(TOP_P, 1));
            sys::llama_sampler_chain_add(chain.as_ptr(), sys::llama_sampler_init_min_p(0.05, 1));
            sys::llama_sampler_chain_add(chain.as_ptr(), sys::llama_sampler_init_temp(0.8));
            sys::llama_sampler_chain_add(chain.as_ptr(), sys::llama_sampler_init_dist(seed));
            Ok(Self(chain))
        }
    }
}

impl Drop for Sampler {
    fn drop(&mut self) {
        // SAFETY: owned here, freed once; it frees the samplers in the chain.
        unsafe { sys::llama_sampler_free(self.0.as_ptr()) }
    }
}

/// Frees a backbone that [`Handles::open`] has not handed over yet.
struct ModelOwner(NonNull<sys::llama_model>);

impl Drop for ModelOwner {
    fn drop(&mut self) {
        // SAFETY: owned here until `open` forgets the owner.
        release("model", || unsafe {
            sys::llama_model_free(self.0.as_ptr())
        });
    }
}

/// Frees a context that [`Handles::open`] has not handed over yet.
struct ContextOwner(NonNull<sys::llama_context>);

impl Drop for ContextOwner {
    fn drop(&mut self) {
        // SAFETY: owned here until `open` forgets the owner.
        release("context", || unsafe { sys::llama_free(self.0.as_ptr()) });
    }
}

/// Frees an audio part that [`Handles::open`] has not handed over yet.
struct MtmdOwner(NonNull<sys::mtmd_context>);

impl Drop for MtmdOwner {
    fn drop(&mut self) {
        // SAFETY: owned here until `open` forgets the owner.
        release("mtmd", || unsafe { sys::mtmd_free(self.0.as_ptr()) });
    }
}
