use rustyline::completion::{Completer, Pair};
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::{Context as RlContext, Helper};

use super::COMMANDS;

pub struct ReplHelper;

impl Completer for ReplHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &RlContext,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let prefix = line.get(..pos).unwrap_or("");
        let start = prefix.rfind(' ').map_or(0, |i| i + 1);
        let word = &prefix[start..];

        let source: &[&str] = if start == 0 {
            COMMANDS
        } else if word.starts_with("--") {
            &["--device", "--port"]
        } else {
            &[]
        };

        let candidates = source
            .iter()
            .filter(|c| c.starts_with(word))
            .map(|c| Pair {
                display: (*c).to_string(),
                replacement: (*c).to_string(),
            })
            .collect();

        Ok((start, candidates))
    }
}

impl Hinter for ReplHelper {
    type Hint = String;
}

impl Highlighter for ReplHelper {}

impl Validator for ReplHelper {}

impl Helper for ReplHelper {}