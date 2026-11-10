//! The owners of the llama.cpp handles free them in the reverse order of creation (spec 081
//! T014, research R1): the generation helper reads the audio part and the context, and both read
//! the backbone, so freeing one before its readers would leave them a dangling pointer. The
//! pointers here are dangling; `freed::recorded` keeps them from reaching llama.cpp.

use std::ptr::NonNull;

use super::{freed, ContextOwner, Handles, ModelOwner, MtmdOwner};

#[test]
fn loaded_handles_free_the_helper_first_and_the_backbone_last() {
    let order = freed::recorded(|| {
        drop(Handles {
            gen: NonNull::dangling(),
            mtmd: NonNull::dangling(),
            context: NonNull::dangling(),
            model: NonNull::dangling(),
            sample_rate: 24_000,
            reference_rate: 16_000,
        });
    });
    assert_eq!(order, ["gen", "mtmd", "context", "model"]);
}

#[test]
fn a_load_that_fails_halfway_frees_what_it_built_in_reverse_order() {
    // As in `Handles::open`: backbone, context, audio part, then a failure before the helper.
    let order = freed::recorded(|| {
        let _model = ModelOwner(NonNull::dangling());
        let _context = ContextOwner(NonNull::dangling());
        let _mtmd = MtmdOwner(NonNull::dangling());
    });
    assert_eq!(order, ["mtmd", "context", "model"]);
}
