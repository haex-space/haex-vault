# Research: spike results (spec 070, US1/US2/US4)

**Date**: 2026-10-29 · **Branch**: `070-b-spike` · **Binding**: `llama-cpp-2` 0.1.160 (bundles llama.cpp)

The spike was `src-tauri/src/llm/local/llamacpp_spike_tests.rs` (ignored tests, feature `llm-llamacpp`,
CUDA through `llm-llamacpp-cuda`; removed in PR C, its checks live on in `tests/llamacpp_inference.rs`). Every number is one run, greedy decoding, 200 new tokens, the same
prompt, on the development machine: RTX A2000 8 GB laptop GPU, 24 threads, Linux.

## Verdict

llama.cpp through `llama-cpp-2` **loads Qwen3.5 and Gemma 4, is faster than mistralrs on Qwen3 4B, needs
less VRAM, and with a prototype chat-template renderer and tool-call parser it calls holzi's tools at
least as well as mistralrs** (spec 032 evaluation: 55 against 52 of 83 sentences with Qwen3 4B, 60 with
Qwen3.5). It builds for `aarch64-linux-android`. The renderer and parsers are not part of the binding;
they would be ours to build and keep. iOS could not be tested here, and the Android run on an emulator did
not happen (see Build).

## Results

| Engine / model               | Device | Load  | Generation | Prompt  | VRAM / RAM peak |
| ---------------------------- | ------ | ----- | ---------- | ------- | --------------- |
| llama.cpp, Qwen3 4B Q4_K_M   | GPU    | 3.5 s | 62.5 t/s   | 273 t/s | 2.5 GB / 2.8 GB |
| mistralrs, Qwen3 4B Q4_K_M   | GPU    | 7.3 s | 50.4 t/s   | 311 t/s | 4.6 GB / 1.3 GB |
| llama.cpp, Qwen3.5 4B Q4_K_M | GPU    | 4.3 s | 51.7 t/s   | 75 t/s  | 2.8 GB / 3.4 GB |
| llama.cpp, Qwen3.5 4B Q4_K_M | CPU    | 1.7 s | 2.0 t/s    | 4 t/s   | – / 3.1 GB      |

- mistralrs had a warm-up run before the timed one, llama.cpp did not. Its VRAM figure probably includes
  the paged-attention pool, so it is not like for like.
- The CPU row is **not representative**: the binding builds ggml without AVX2/FMA (`GGML_NATIVE=OFF`,
  its default unless `target-cpu=native`), and the machine had a load of 13 on 24 cores. SC-003 was
  checked on the GPU only.
- Qwen3.5 prompt processing is slow (75 t/s against 273 t/s for Qwen3) on a 23-token prompt; the 151-token
  tool prompt ran at several thousand t/s, so the first figure is probably start-up cost. Not investigated.
- Gemma 4 E4B Q4_K_M (5.3 GB, `lmstudio-community`): loads through llama.cpp in 5.8 s, all 43 layers on
  the GPU, about 3.0 GB of VRAM and 5.6 GB of RAM at peak (the per-layer embeddings stay in RAM), and answers
  the plain prompt coherently (29 tokens in 0.85 s).

## Tool calls (US2)

- With a Hermes-style prompt (JSON inside `<tool_call>`), both Qwen3 and Qwen3.5 produced a correct
  call as text, e.g. `{"name": "open_settings", "arguments": {"page": "sync"}}`. Reading it back out is
  string work, not an engine feature.
- **Qwen3.5's own template asks for another format**: XML, `<tool_call><function=name><parameter=key>value
</parameter></function></tool_call>` (read from the file's `tokenizer.chat_template`). The spike did not
  test that format, only that the model can call a tool.
- **The binding has no Jinja and no tool support.** It exposes `llama_chat_apply_template` only. For Qwen3.5
  it reported success but rendered plain ChatML, without the model's thinking handling and without tools.
  The `common` feature of the sys crate wraps grammar, fit and speculative decoding, not
  `common_chat_*`. Rendering the real template needs `minijinja`; parsing needs one parser per family.
- **Gemma 4 uses a third format**, with its own string delimiter:
  `<|tool_call>call:open_settings{page:<|"|>sync<|"|>}<tool_call|>`. The prototype has no parser for it, so
  Gemma 4 was loaded and asked but not evaluated.
- Not done: the chat-template probe of spec 032 R8.

## Evaluation of spec 032 (SC-002)

`src-tauri/tests/model_tool_eval.rs` with `HOLZI_EVAL_ENGINE=llamacpp`, through the prototype
`LlamaCppSpikeAdapter` (`llamacpp_spike_adapter.rs`: the file's own Jinja template through `minijinja`,
thinking split off, `<tool_call>` blocks read as JSON or XML, greedy decoding). The example set is now
**83 sentences** (set version 5), not the 30 the spec first named. All three runs on the GPU, deterministic,
one run each. mistralrs was measured again on the same machine because the figure in spec 032
(16 of 30) is a CPU run of an older set.

| Engine / model        | Pass (of 83) | Reached | Extra steps | Spurious calls | Time  |
| --------------------- | ------------ | ------- | ----------- | -------------- | ----- |
| mistralrs, Qwen3 4B   | 52 (62.7 %)  | 0.857   | 4           | 0              | 112 s |
| llama.cpp, Qwen3 4B   | 55 (66.3 %)  | 0.844   | 3           | 0              | 100 s |
| llama.cpp, Qwen3.5 4B | 60 (72.3 %)  | 0.883   | 6           | 1              | 151 s |

- With the same model llama.cpp is not below mistralrs, so the prototype's template rendering and parsing
  are at least as good as mistralrs' for Qwen3. The gap of three sentences is small and one run each;
  read it as "no regression", not as "better".
- Qwen3.5 gains most on the extension sentences (32 of 40 against 27 of 40 for Qwen3 on mistralrs).
- Qwen3.5's one spurious call was a `find_actions` search for small talk about the weather (harmless);
  its one `run_command` call was `echo "Küchen-Laptop"` for an alias change. Nothing is executed in the
  evaluation, but `run_command` is the kind of call the approval rules must catch in production.
- The prototype also needed what mistralrs does for the same templates: its own `tojson` filter, Python
  string methods (`minijinja-contrib` `pycompat`) and `raise_exception`; mistralrs additionally rewrites
  `[::-1]` and descending `range` calls and special-cases several model families. Expect that tail.

## Build (US4 and FR-005)

- **cmake is missing from the Nix devshell.** The spike took it from `nixpkgs#cmake` outside the shell
  (`nix shell` inside `nix develop` fails on a glibc mismatch).
- **CUDA** (first spike): the Nix CUDA toolkit has no `lib64` and no static cuBLAS in its merged output, so
  the crate's default static linking failed (`cudart_static` / `cublas_static` not found). The spike set
  `CUDA_LIBRARY_PATH` to a directory with a `lib64` link and used the `dynamic-link` feature, which needs the
  shared llama.cpp libraries on `LD_LIBRARY_PATH` at run time. `CUDAARCHS=86` limited the kernels to this
  card. A cold build took 15 min. R1 below replaces this with a static link.
- **Android**: `llama-cpp-2` builds for `aarch64-linux-android` with NDK 28.2 in 1.5 min (isolated crate,
  NDK clang set through `CC_aarch64_linux_android`). It was not run on a device or emulator. The check
  through the whole holzi crate failed earlier, in `aws-lc-sys` (host gcc instead of the NDK compiler),
  which is unrelated to llama.cpp but shows the cross-build environment needs the same variables.
- **iOS**: not buildable on this Linux machine; the sys crate's `build.rs` has no iOS-specific branch.
- **Android run on an emulator: not done.** The x86_64 probe build and the emulator were started, then the
  `/tmp` tmpfs (32 GB) filled up and the run was abandoned; the leftovers were cleaned up afterwards. The
  `aarch64-linux-android` build is the only Android evidence.
- Two engines in one binary built and ran (mistralrs and llama.cpp in the CUDA test binary). Each test ran
  in its own process, so both in one process at once is not shown.

## Consequences for the spec

- US1 is answered yes; SC-001 and SC-003 hold on the development machine.
- US2 is answered for Qwen3 and Qwen3.5: with our own template rendering and parsers (Hermes JSON and the
  Qwen3.5 XML) the evaluation does not fall below mistralrs (SC-002). Gemma 4 needs a third parser.
- The devshell needs cmake and static cuBLAS from the Nix CUDA package, exposed through the
  `CUDA_LIBRARY_PATH` layout documented in R1 (task T003 is handled in the atoms repo).
- Still to do before a plan: an Android run on the emulator, an iOS build on a Mac, a parser for Gemma 4's
  call format and its evaluation, the chat-template probe, CPU speed with `GGML_NATIVE`.

## Promotion to production modules (PR B)

The spike's template renderer and parsers moved into `llm/llamacpp/` (`template.rs`, `formats/`) with unit
tests and the three real chat templates as fixtures, and the spike adapter now uses them. The 83-sentence
evaluation through the promoted code gives the same figures as before: **55 of 83 for Qwen3 4B and 60 of 83
for Qwen3.5 4B**, identical in reach (0.844 and 0.883), extra steps (3 and 6) and spurious calls (0 and 1).

## Adapter results (PR C, 2026-10-30)

`LlamaCppAdapter` (`llm/llamacpp/adapter.rs`) with streaming, cancelling and the per-model gate, on the
development machine with the CUDA build and Qwen3 4B and Qwen3.5 4B (both Q4_K_M):

- The ignored tests of `tests/llamacpp_inference.rs` pass for both files: the answer arrives in pieces
  (`finish_reason` `length` at the cap, token count exact), a tool call arrives as a `ToolCall` and no call
  markup reaches the text, two requests at once both finish, greedy decoding repeats itself, and
  **a cancel ends the generation 24 ms (Qwen3) and 34 ms (Qwen3.5) after the call** (US3 scenario 4 asks for
  one second). Both engines answer in one process on one GPU (Qwen3: mistralrs and llama.cpp).
- The 83-sentence evaluation through the adapter, streaming included, gives the same figures as the spike
  and as PR B: **55 of 83 (Qwen3 4B) and 60 of 83 (Qwen3.5 4B)**. Streaming, the holdback buffer and the
  cancel flag change nothing about the quality.

## Engine choice (PR D, 2026-10-31)

`llm::engine::load_local` reads `general.architecture` from the file and picks the engine: every
architecture mistralrs knows stays with mistralrs, the rest goes to llama.cpp where it is compiled in, and
otherwise the load fails at once with a message. With the CUDA build on the development machine,
`HOLZI_TEST_GGUF` set to the Qwen3 4B file loads through mistralrs and the Qwen3.5 4B file through
llama.cpp, and both answer (`load_local_picks_the_engine_from_the_file_and_answers`).

The catalog carries an `architecture` per entry, used only as a gate (FR-007): `entries()` hides a model no
compiled engine loads, so a build without llama.cpp does not offer the Qwen3.5 download. The `llamacpp` CI
leg runs the `catalog::` tests, because a default build never sees Qwen3.5 and could not catch a mistake
there. One consequence worth knowing: on a roomy desktop the tier suggestion is the largest model that fits,
which in a build with llama.cpp is Qwen3.5 4B (3.0 GB) and not the Qwen3 4B profile. The profile itself
stays Qwen3 4B until the decision of US5.

## Both engines on every catalog model (T042, 2026-10-31)

Release builds on the development machine (i7-12800HX, 16 cores and 24 threads; RTX A2000 8 GB laptop
GPU), a quiet machine (load about 3), one run each, greedy decoding:

- the GPU build `llm-cuda,llm-llamacpp-cuda`, and the CPU build `llm-llamacpp` (no CUDA, so mistralrs runs
  on the CPU too; ggml with the AVX2 baseline of R3, threads = physical cores, still with OpenMP, which
  R3's last paragraph drops);
- the 83 sentences of spec 032 (set version 5) on the GPU, `model_tool_eval`;
- the speed with `measures_the_speed` (now also through mistralrs, `HOLZI_TEST_ENGINE=mistralrs`): a prompt
  of about 560 tokens, 128 new tokens, the second of two rounds. mistralrs closes the stream at the token
  limit without its final frame and so without a token count; the test then counts the pieces, one per
  token.

Qwen3.5 4B runs only through llama.cpp (mistralrs 0.8.1 panics on `qwen35`).

**Evaluation (GPU, pass of 83):**

| Model      | mistralrs | llama.cpp | Reached (m / l) | Extra steps (m / l) | Spurious calls |
| ---------- | --------- | --------- | --------------- | ------------------- | -------------- |
| Qwen3 0.6B | 14        | 12        | 0.831 / 0.831   | 0 / 0               | 0 / 0          |
| Qwen3 1.7B | 48        | 48        | 0.831 / 0.831   | 0 / 0               | 0 / 0          |
| Qwen3 4B   | 52        | 55        | 0.857 / 0.844   | 4 / 3               | 0 / 0          |
| Qwen3.5 4B | –         | 60        | – / 0.883       | – / 6               | – / 1          |

**Speed, GPU** (first text after the prompt, then tokens per second; load time; the process' peak resident
memory, where the weights in video memory do not count):

| Model      | mistralrs                        | llama.cpp                        |
| ---------- | -------------------------------- | -------------------------------- |
| Qwen3 0.6B | 0.03 s, 155 t/s; 2.8 s; 1134 MiB | 0.05 s, 226 t/s; 0.4 s; 977 MiB  |
| Qwen3 1.7B | 0.05 s, 100 t/s; 3.7 s; 1259 MiB | 0.10 s, 125 t/s; 0.4 s; 1739 MiB |
| Qwen3 4B   | 0.10 s, 55 t/s; 1.7 s; 1319 MiB  | 0.22 s, 61 t/s; 0.6 s; 2897 MiB  |
| Qwen3.5 4B | –                                | 0.30 s, 51 t/s; 0.8 s; 3426 MiB  |

**Speed, CPU only:**

| Model      | mistralrs                        | llama.cpp                      |
| ---------- | -------------------------------- | ------------------------------ |
| Qwen3 0.6B | 12.3 s, 21 t/s; 1.8 s; 1430 MiB  | 0.9 s, 27 t/s; 0.2 s; 864 MiB  |
| Qwen3 1.7B | 22.0 s, 13 t/s; 2.5 s; 2714 MiB  | 1.6 s, 32 t/s; 0.3 s; 2085 MiB |
| Qwen3 4B   | 51.6 s, 6.2 t/s; 2.1 s; 4420 MiB | 4.1 s, 16 t/s; 0.6 s; 4321 MiB |
| Qwen3.5 4B | –                                | 4.8 s, 13 t/s; 0.6 s; 4296 MiB |

What the figures say:

- **Quality:** the same model gives the same result through both engines within one or three sentences, in
  both directions (0.6B: 14 against 12, 1.7B: equal, 4B: 52 against 55). Neither engine reads calls
  better. The real gain is the model: Qwen3.5 4B reaches 60, and only llama.cpp loads it.
- **GPU:** llama.cpp generates faster at every size (+11 % to +46 %); mistralrs reads the prompt faster,
  but both start within a third of a second, which nobody notices. Loading is three to eight times faster
  through llama.cpp, on the CPU as well.
- **CPU:** on the measured x86 machine, llama.cpp starts answering 12 to 14 times sooner (4B: 4 s against
  52 s) and generates 1.3 to 2.5 times faster. The phone result is still unmeasured: the phone check
  belongs to the ARM validation in T040.
- **Memory:** on the CPU about equal, llama.cpp lower with the small models. On the GPU llama.cpp's
  resident memory is higher because the file is mapped into the process and counted there; it is page
  cache, not a copy.
- The CPU figures come from x86. Phones run ARM kernels in both engines, which this machine does not
  measure; the phone check belongs to the switch (T040).

## R3 – CPU kernels (settled, 2026-10-31)

Question: how does a release build get fast ggml kernels without being built for the build machine's CPU?

**Finding.** Without `target-cpu`, the sys crate sets `GGML_NATIVE=OFF` and turns on only the extensions Rust's
target has, which for `x86_64-unknown-linux-gnu` is SSE2. ggml then has no x86 kernel for the K-quants at all:
in such a binary `ggml_vec_dot_q4_K_q8_K` is a jump to `ggml_vec_dot_q4_K_q8_K_generic`, plain C. Every
build so far (CI, the evaluations' engine, a release) ran like that on the CPU. A second finding: the engine
set no thread count, so llama.cpp used its default of 4.

**Measurement** (T038). Qwen3 4B Q4_K_M on the CPU only, i7-12800HX (8 performance and 8 efficiency cores,
24 threads), a prompt of about 560 tokens and 128 new tokens, `measures_the_speed` in
`tests/llamacpp_inference.rs`, second of two rounds. Each variant was built cold in its own target dir with
`LLAMA_LIB_PROFILE=Release`, and the kernel was checked in the binary (60 `ymm`/FMA instructions in the
AVX2 build, a jump to the generic kernel in the other). The machine was not quiet (load 11 to 39 from
other builds), so the figures are rough; the gap is not.

| ggml build      | Threads | First text after | Tokens/s after it |
| --------------- | ------- | ---------------- | ----------------- |
| as before (C)   | 4       | 94.3 s           | 1.5               |
| as before (C)   | 8       | 45.8 s           | 4.2               |
| as before (C)   | 16      | 51.6 s           | 5.6               |
| as before (C)   | 24      | 52.6 s           | 3.2               |
| AVX2, FMA, F16C | 4       | 11.7 s           | 9.6               |
| AVX2, FMA, F16C | 8       | 5.3 s            | 11.1              |
| AVX2, FMA, F16C | 16      | 4.8 s            | 11.4              |
| AVX2, FMA, F16C | 24      | 4.4 s            | 11.2              |

AVX2 reads the prompt 8 to 18 times faster and generates 2 to 6 times faster. Reading the prompt scales
with threads; generating stops gaining after about eight, where memory bandwidth is the limit.

**Decision (operator, 2026-10-31): a fixed AVX2 baseline, not runtime dispatch.** `.cargo/config.toml`
passes `GGML_AVX`, `GGML_AVX2`, `GGML_FMA`, `GGML_F16C` and `GGML_BMI2` to the sys crate, which forwards
`GGML_*` variables to CMake; ggml reads them only for x86, so Android and Apple builds on ARM are untouched.
Runtime dispatch (`dynamic-backends`) would cover every CPU and use AVX-512 where it exists, but it needs
`dynamic-link`: llama.cpp as shared libraries next to the binary in every installer, which undoes R1's one
static binary. The price of the baseline is processors without AVX2 (Intel before Haswell, 2013; AMD before
Excavator, 2015): `llm/llamacpp/cpu.rs` compares what ggml was built with (`ggml_cpu_has_*`) against the
processor, and where something is missing the engine choice refuses llama.cpp, so the catalog hides
Qwen3.5 there and mistralrs models keep working. A unit test pins the baseline, because a build without
the switches fails nothing and is only slow.

Threads now default to one per physical core (`sysinfo`), for both the prompt and the generation.

Two build traps found on the way: the sys crate prints `rerun-if-env-changed` only for the `GGML_*` variables
present at its last run, and CMake keeps an option in its cache once set. A target dir built before the
switches keeps the C kernels until `cargo clean -p llama-cpp-sys-2`; the `llamacpp` CI leg got a new cache
key for that reason, and the unit test fails on such a stale build instead of letting it pass slowly.

**OpenMP (2026-10-31, PR G).** The R3 builds used OpenMP on Linux and Windows. With llama.cpp in the
default features, OpenMP would add a runtime library the installers do not carry: `vcomp140.dll` next to
Tauri's static VC runtime on Windows (the runners have Visual Studio and would not notice), `libgomp.so.1` on
Linux. Without it ggml uses its own thread pool, as on Android and the Mac. The two release builds were
measured in turns (CPU only, the R3 prompt, two rounds each, a machine at load 4 to 10):

| Model      | With OpenMP              | Without OpenMP           |
| ---------- | ------------------------ | ------------------------ |
| Qwen3 1.7B | 1.5–1.6 s, 20–27 t/s     | 1.4–1.6 s, 38.5–38.6 t/s |
| Qwen3 4B   | 3.7–3.9 s, 14.8–15.0 t/s | 4.2–4.4 s, 17.1–17.8 t/s |

The prompt reads a little slower, the answer comes faster; OpenMP is off on every platform.

## R4 – Gemma 4 call syntax (settled, 2026-10-31)

Read from the template in `gemma-4-E4B-it-Q4_K_M.gguf` (fixture `gemma4.jinja`) and llama.cpp's reader
(`common/parsers/gemma4.cpp`, `chat-peg-parser.cpp` `gemma4_to_json`) in the pinned `llama-cpp-sys-2`
0.1.160. The full syntax with its cases is in `contracts/tool-call-formats.md`; what mattered beyond the
spike's one example:

- Thinking is not `<think>`: it sits in `<|channel>thought…<channel|>`, and the model may add an empty
  thought block or a lone `<channel|>` after its answer. A format now names its own thinking tags, and the
  streamer holds text back at these stray tags as at a call marker.
- The vocabulary marks `<|tool_response>` and `<turn|>` as end of generation, so the model stops after its
  calls without a stop string.
- Tool results belong in the model turn that made the calls. The current template reads OpenAI-style `tool`
  messages, older ones only `tool_responses` on the assistant message; the format sends the merged shape,
  which both read (llama.cpp does the same for older templates).
- llama.cpp holds the model to the syntax with a lazy grammar once `<|tool_call>` appears; holzi has no
  grammar, so the reader also takes what the model may write without one: quoted keys, bare words as strings,
  calls without braces.

With the CUDA build, the Gemma 4 E4B file loads with all 43 layers on the GPU at 8192 tokens, answers in
pieces, returns `open_settings{page: "sync"}` as a call, stops 49 ms after a cancel and repeats itself under
greedy decoding. The evaluation (T033) gave 53 of 83 (`specs/032-model-operates-holzi/eval-results.md`), so
the catalog leaves Gemma 4 out and the desktop profile stays Qwen3 4B (T034).

## R2 – GPU layer fit (settled, 2026-10-30)

Question: does `llama_rs_fit_params` give a safe GPU layer count, or does the engine need its own plan?

**Decision: the engine plans itself (`llm/llamacpp/memory.rs`).** `fit_params` sits behind the `common`
feature of the binding, which builds a second library (`libcommon`) into every platform, Android and iOS
included, to compute two numbers. The inputs are few and cheap: the file size, the layer count, the KV heads
and the head size from the GGUF header (`gguf_header::read_summary`, a read of the metadata, no tensors), and
the free memory of the best GPU (llama.cpp's device list) and of the host. The plan is a pure function, so
it is tested without a model and on machines that have no 4 GB or 24 GB card
(`memory_tests.rs`). Rules: everything on the GPU when file, KV cache and a margin of 768 MiB fit; the
context is halved twice before layers move to the host; then as many layers stay on the GPU as fit; the
context shrinks further only when the host cannot take the rest; otherwise the load is refused with the
numbers instead of letting ggml abort.

Measured on the RTX A2000 8 GB (7.9 GiB usable), context 8192:

| Situation                                         | Plan                        | Result                        |
| ------------------------------------------------- | --------------------------- | ----------------------------- |
| Qwen3 4B, GPU empty                               | all 37 (36 layers + output) | answers, 12.9 s for six tests |
| Qwen3.5 4B, GPU empty                             | all 34                      | answers                       |
| Qwen3 4B, mistralrs already holds its copy        | 25 of 37 layers             | both engines answer           |
| Qwen3 4B, GPU still held after the mistralrs test | all 37, context cut to 2048 | answers                       |
| Qwen3 4B, layers forced to 12                     | 12 of 37                    | answers, tool call works      |

What this does not show: only one card was measured. The 4 GB and 24 GB cases rest on the unit tests. The
KV estimate counts every layer as an attention layer, so it over-counts hybrid models such as Qwen3.5, where
only some layers keep a cache; that errs towards putting fewer layers on the GPU, never more. If a model
needs a finer figure later, `memory.rs` is the one place to change.

## R1 – CUDA linking (settled, 2026-10-30)

Question: can the CUDA build link cudart and cuBLAS statically, so one binary needs only the driver?

**Yes.** The pinned nixpkgs has an output `static` for cuBLAS (`cudaPackages.libcublas.static`: `libcublas_static.a`
165 MB and `libcublasLt_static.a` 1.1 GB). The sys crate's `find_cuda_helper` reads only `CUDA_LIBRARY_PATH` on
Linux (not `CUDA_PATH`) and only looks in `<dir>/lib64`, so a directory has to be laid out for it:

```text
$CUDA_LIBRARY_PATH/lib64/
├── libcudart_static.a      (from the merged CUDA toolkit)
├── libculibos.a            (from the merged CUDA toolkit)
├── libcublas_static.a      (from cudaPackages.libcublas.static)
├── libcublasLt_static.a    (from cudaPackages.libcublas.static)
└── stubs/                  (libcuda.so stub, from the merged CUDA toolkit)
```

Build with `CUDAARCHS=86` (this card; without it ggml compiles every architecture) and `cmake` on `PATH`. The
feature `llm-llamacpp-cuda` no longer enables `dynamic-link`.

| Check                                                         | Result                                                                         |
| ------------------------------------------------------------- | ------------------------------------------------------------------------------ |
| Isolated probe crate (`llama-cpp-2` with `cuda`), debug build | 5 min 23 s; dynamic dependencies: only `libcuda.so.1` (and libgomp, libstdc++) |
| Probe run                                                     | finds the RTX A2000 (7893 MiB), `max_devices` 16                               |
| holzi `--features llm-cuda,llm-llamacpp-cuda`, test build     | 9 min 14 s from a reflinked cache                                              |
| llama.cpp and mistralrs CUDA tests in **one process**         | both pass; Qwen3 4B 65.2 t/s (llama.cpp) and 54.2 t/s (mistralrs)              |

- A first run of the two-engine test failed with "cudaMalloc failed: out of memory" and a mistralrs "device
  mismatch": another process (the dev app) held 4.6 GB of the 8 GB card. After it was closed the run passed,
  so the failure was memory, not the static link.
- A binary that links cudart statically needs no `libcudart.so` at run time, but still needs the driver. A
  plain binary has to find `libcuda.so.1` itself; holzi's `build.rs` adds the rpath for the app (and the host
  bridge script covers the rest), a standalone probe needs `LD_LIBRARY_PATH` set to the host's driver directory.
- **What it does not show**: Windows (the crate reads `CUDA_PATH` there, with a different layout), more than one
  card or architecture, and a release build for all GPU architectures (`CUDAARCHS` choice, build time, binary
  size). A debug binary with `sm_86` only was 961 MiB unstripped.

What the devShell has to provide (task T003, atoms repo): `cmake`, `cudaPackages.libcublas.static`, and the
`CUDA_LIBRARY_PATH` directory above.
