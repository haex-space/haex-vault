//! How much of the model's context one tool result may fill, for the tools that can give less or
//! more (a browser page, a file listing). A turn of a model with a small context sets it around
//! its tool calls; outside a turn, and for a model with a large context, there is none and a tool
//! gives its usual amount. Beside it, `chat::turn::fit` shrinks what is still too large before a
//! request goes out.

use std::future::Future;

tokio::task_local! {
    static RESULT_CHARS: usize;
}

/// How many characters of the context a tool result may fill, at most a quarter of it.
pub const SHARE: usize = 4;
/// Characters per token, as in `chat::turn::fit`.
const CHARS_PER_TOKEN: usize = 3;

/// Runs `work` with a budget for one result derived from the model's context of `limit_tokens`;
/// without a limit `work` runs as it is.
pub async fn within<F: Future>(limit_tokens: Option<usize>, work: F) -> F::Output {
    match limit_tokens {
        Some(limit) => {
            RESULT_CHARS
                .scope(limit / SHARE * CHARS_PER_TOKEN, work)
                .await
        }
        None => work.await,
    }
}

/// The characters one tool result may fill; `None` when the model's context is large or not known.
pub fn result_chars() -> Option<usize> {
    RESULT_CHARS.try_with(|chars| *chars).ok()
}
