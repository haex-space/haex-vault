use llama_cpp_2::token::LlamaToken;

use super::kept_tokens;

const END: LlamaToken = LlamaToken(1);

fn tokens(ids: &[i32]) -> Vec<LlamaToken> {
    ids.iter().copied().map(LlamaToken).collect()
}

/// Runs the step against what the model would sample at each position; returns the kept tokens and how
/// many positions were sampled.
fn run(draft: &[i32], model: &[i32]) -> (Vec<LlamaToken>, usize) {
    let said = tokens(model);
    let mut asked = 0;
    let kept = kept_tokens(
        &tokens(draft),
        |i| {
            asked += 1;
            said[i]
        },
        |token| token == END,
    );
    (kept, asked)
}

#[test]
fn every_proposal_the_model_agrees_with_is_kept_plus_its_own_next_token() {
    let (kept, asked) = run(&[10, 11, 12], &[10, 11, 12, 13]);
    assert_eq!(kept, tokens(&[10, 11, 12, 13]));
    assert_eq!(asked, 4);
}

#[test]
fn a_first_proposal_the_model_rejects_keeps_only_the_models_token() {
    let (kept, asked) = run(&[10, 11, 12], &[20, 11, 12, 13]);
    assert_eq!(kept, tokens(&[20]));
    assert_eq!(asked, 1, "nothing after a rejection is sampled");
}

#[test]
fn a_rejection_in_the_middle_keeps_what_came_before() {
    let (kept, asked) = run(&[10, 11, 12], &[10, 21, 12, 13]);
    assert_eq!(kept, tokens(&[10, 21]));
    assert_eq!(asked, 2);
}

#[test]
fn an_empty_draft_samples_one_token() {
    let (kept, asked) = run(&[], &[30]);
    assert_eq!(kept, tokens(&[30]));
    assert_eq!(asked, 1);
}

#[test]
fn an_end_of_generation_inside_the_accepted_part_ends_there() {
    let (kept, asked) = run(&[10, 1, 12], &[10, 1, 12, 13]);
    assert_eq!(kept, tokens(&[10, 1]));
    assert_eq!(asked, 2);
}
