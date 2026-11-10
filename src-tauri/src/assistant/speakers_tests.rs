//! Spec 066 FR-016a: voice prints are compared by cosine similarity; an enrolment of sentences
//! that disagree is refused; prints of another model are never compared.

use uuid::Uuid;

use super::speakers::{
    best_match, consistent, cosine, mean, normalized, MATCH_THRESHOLD, MODEL_ID,
};
use crate::storage::assistant_speakers::Speaker;

fn person(name: &str, embedding: Vec<f32>, model: &str) -> Speaker {
    Speaker {
        id: Uuid::new_v4(),
        name: name.to_owned(),
        model: model.to_owned(),
        embedding,
        utterances: 3,
        created_at: 0,
    }
}

#[test]
fn similarity_ignores_length() {
    assert!((cosine(&[1.0, 0.0], &[5.0, 0.0]) - 1.0).abs() < 1e-6);
    assert!(cosine(&[1.0, 0.0], &[0.0, 2.0]).abs() < 1e-6);
    assert!((cosine(&[1.0, 0.0], &[-1.0, 0.0]) + 1.0).abs() < 1e-6);
    assert_eq!(
        cosine(&[1.0], &[1.0, 0.0]),
        0.0,
        "different lengths never match"
    );
    assert_eq!(cosine(&[0.0, 0.0], &[1.0, 0.0]), 0.0);
    let n = normalized(&[3.0, 4.0]);
    assert!((n[0] - 0.6).abs() < 1e-6 && (n[1] - 0.8).abs() < 1e-6);
}

#[test]
fn the_print_is_the_mean_direction_of_the_sentences() {
    let print = mean(&[vec![1.0, 0.0], vec![0.0, 10.0]]);
    let half = std::f32::consts::FRAC_1_SQRT_2;
    assert!((print[0] - half).abs() < 1e-6 && (print[1] - half).abs() < 1e-6);
    assert!(mean(&[]).is_empty());
}

#[test]
fn sentences_of_one_voice_agree_and_a_stranger_among_them_does_not() {
    let voice = [
        vec![1.0, 0.1, 0.0],
        vec![0.9, 0.2, 0.1],
        vec![1.0, 0.0, 0.1],
    ];
    assert!(consistent(&voice));
    let with_stranger = [voice[0].clone(), voice[1].clone(), vec![-0.2, 0.0, 1.0]];
    assert!(!consistent(&with_stranger));
}

#[test]
fn an_utterance_belongs_to_the_most_similar_enrolled_person() {
    let anna = person("Anna", vec![1.0, 0.0, 0.0], MODEL_ID);
    let ben = person("Ben", vec![0.0, 1.0, 0.0], MODEL_ID);
    let old = person("Old", vec![0.0, 0.0, 1.0], "another-model");
    let prints = [anna.clone(), ben.clone(), old];
    assert_eq!(
        best_match(&[0.9, 0.3, 0.0], &prints, MATCH_THRESHOLD).map(|m| m.0),
        Some(anna.id)
    );
    assert_eq!(
        best_match(&[0.2, 0.95, 0.0], &prints, MATCH_THRESHOLD).map(|m| m.0),
        Some(ben.id)
    );
    // Nobody is close enough; a print of another model is never compared.
    assert_eq!(best_match(&[0.5, 0.5, 2.0], &prints, MATCH_THRESHOLD), None);
    assert_eq!(best_match(&[0.0, 0.0, 1.0], &prints, MATCH_THRESHOLD), None);
}
