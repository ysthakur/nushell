use crate::completions::{
    completion_common::{adjust_if_intermediate, complete_item, AdjustView},
    Completer, CompletionOptions, SortBy,
};
use nu_ansi_term::Style;
use nu_protocol::{
    engine::{EngineState, Stack, StateWorkingSet},
    levenshtein_distance, Span,
};
use reedline::Suggestion;
use std::path::{Path, MAIN_SEPARATOR as SEP};

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
        let output: Vec<_> = directory_completion(
            span,
            &prefix,
            &working_set.permanent_state.current_work_dir(),
            options,
            self.get_sort_by(),
            working_set.permanent_state,
            stack,
        )
        .into_iter()
        .map(move |x| SemanticSuggestion {
            suggestion: Suggestion {
                value: x.1,
                description: None,
                style: x.2,
                extra: None,
                span: reedline::Span {
                    start: x.0.start - offset,
                    end: x.0.end - offset,
                },
                append_whitespace: false,
            },
            // TODO????
            kind: None,
        })
        .collect();

        output
    }

    // Sort results prioritizing the non hidden folders
    fn sort(&self, items: Vec<SemanticSuggestion>, prefix: Vec<u8>) -> Vec<SemanticSuggestion> {
        let prefix_str = String::from_utf8_lossy(&prefix).to_string();

        // Sort items
        let mut sorted_items = items;

        match self.get_sort_by() {
            SortBy::Ascending => {
                sorted_items.sort_by(|a, b| {
                    // Ignore trailing slashes in folder names when sorting
                    a.suggestion
                        .value
                        .trim_end_matches(SEP)
                        .cmp(b.suggestion.value.trim_end_matches(SEP))
                });
            }
            SortBy::LevenshteinDistance => {
                sorted_items.sort_by(|a, b| {
                    let a_distance = levenshtein_distance(&prefix_str, &a.suggestion.value);
                    let b_distance = levenshtein_distance(&prefix_str, &b.suggestion.value);
                    a_distance.cmp(&b_distance)
                });
            }
            _ => (),
        }

        // Separate the results between hidden and non hidden
        let mut hidden: Vec<SemanticSuggestion> = vec![];
        let mut non_hidden: Vec<SemanticSuggestion> = vec![];

        for item in sorted_items.into_iter() {
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

pub fn directory_completion(
    span: nu_protocol::Span,
    partial: &str,
    cwd: &str,
    options: &CompletionOptions,
    sort_by: SortBy,
    engine_state: &EngineState,
    stack: &Stack,
) -> Vec<(nu_protocol::Span, String, Option<Style>)> {
    complete_item(
        true,
        span,
        partial,
        cwd,
        options,
        sort_by,
        engine_state,
        stack,
    )
}
