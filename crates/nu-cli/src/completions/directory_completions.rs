use crate::completions::{
    completion_common::{adjust_if_intermediate, complete_item, AdjustView},
    Completer, CompletionOptions,
};
use nu_protocol::{
    engine::{Stack, StateWorkingSet},
    Span,
};
use reedline::Suggestion;
use std::path::Path;

use super::SemanticSuggestion;

#[derive(Clone, Default)]
pub struct DirectoryCompletion {}

impl DirectoryCompletion {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Completer for DirectoryCompletion {
    fn fetch(
        &mut self,
        working_set: &StateWorkingSet,
        stack: &Stack,
        prefix: Vec<u8>,
        span: Span,
        offset: usize,
        _pos: usize,
        options: &CompletionOptions,
    ) -> Vec<SemanticSuggestion> {
        let AdjustView { prefix, span, .. } = adjust_if_intermediate(&prefix, working_set, span);

        // Filter only the folders
        #[allow(deprecated)]
        let items: Vec<_> = complete_item(
            true,
            span,
            &prefix,
            &[&working_set.permanent_state.current_work_dir()],
            options,
            working_set.permanent_state,
            stack,
        )
        .into_iter()
        .map(move |(span, _, value, style)| SemanticSuggestion {
            suggestion: Suggestion {
                value,
                description: None,
                style,
                extra: None,
                span: reedline::Span {
                    start: span.start - offset,
                    end: span.end - offset,
                },
                append_whitespace: false,
            },
            // TODO????
            kind: None,
        })
        .collect();

        // Separate the results between hidden and non hidden
        let mut hidden: Vec<SemanticSuggestion> = vec![];
        let mut non_hidden: Vec<SemanticSuggestion> = vec![];

        for item in items.into_iter() {
            let item_path = Path::new(&item.suggestion.value);

            if let Some(value) = item_path.file_name() {
                if let Some(value) = value.to_str() {
                    if value.starts_with('.') {
                        hidden.push(item);
                    } else {
                        non_hidden.push(item);
                    }
                }
            }
        }

        // Append the hidden folders to the non hidden vec to avoid creating a new vec
        non_hidden.append(&mut hidden);

        non_hidden
    }
}
