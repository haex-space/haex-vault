//! Proves that the feature builds llama.cpp and links it: the CI leg `llamacpp` would otherwise only show
//! that the Rust side compiles. These calls need no model and no GPU.

#[test]
fn llama_cpp_is_linked_and_answers_capability_questions() {
    // `max_devices` reads a constant of the C++ side; a link or ABI mistake fails here, not at a model load.
    assert!(llama_cpp_2::max_devices() >= 1);
    // The answer depends on the platform; only that the call returns matters.
    let _ = llama_cpp_2::mmap_supported();
}
