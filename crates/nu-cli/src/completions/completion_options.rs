use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use nu_parser::trim_quotes_str;
use nu_protocol::CompletionAlgorithm;
use nu_utils::IgnoreCaseExt;
use std::{borrow::Cow, fmt::Display};

use crate::SemanticSuggestion;

/// Describes how suggestions should be matched.
#[derive(Copy, Clone, Debug)]
pub enum MatchAlgorithm {
    /// Only show suggestions which begin with the given input
    ///
    /// Example:
    /// "git switch" is matched by "git sw"
    Prefix,

    /// Only show suggestions which contain the input chars at any place
    ///
    /// Example:
    /// "git checkout" is matched by "gco"
    Fuzzy,
}

impl From<CompletionAlgorithm> for MatchAlgorithm {
    fn from(value: CompletionAlgorithm) -> Self {
        match value {
            CompletionAlgorithm::Prefix => MatchAlgorithm::Prefix,
            CompletionAlgorithm::Fuzzy => MatchAlgorithm::Fuzzy,
        }
    }
}

impl TryFrom<String> for MatchAlgorithm {
    type Error = InvalidMatchAlgorithm;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "prefix" => Ok(Self::Prefix),
            "fuzzy" => Ok(Self::Fuzzy),
            _ => Err(InvalidMatchAlgorithm::Unknown),
        }
    }
}

pub struct NuMatcher<T> {
    needle: String,
    case_sensitive: bool,
    positional: bool,
    state: State<T>,
}

enum State<T> {
    Prefix { items: Vec<(String, T)> },
    Fuzzy { items: Vec<(i64, T)> },
}

impl<T> NuMatcher<T> {
    pub fn new(needle: impl AsRef<str>, options: &CompletionOptions) -> NuMatcher<T> {
        let needle = trim_quotes_str(needle.as_ref()).to_string();

        match &options.match_algorithm {
            MatchAlgorithm::Prefix => {
                let needle = if options.case_sensitive {
                    needle
                } else {
                    needle.to_folded_case()
                };
                NuMatcher {
                    needle,
                    case_sensitive: options.case_sensitive,
                    positional: options.positional,
                    state: State::Prefix { items: Vec::new() },
                }
            }
            MatchAlgorithm::Fuzzy => NuMatcher {
                needle,
                case_sensitive: options.case_sensitive,
                positional: options.positional,
                state: State::Fuzzy { items: Vec::new() },
            },
        }
    }

    pub fn add(&mut self, haystack: impl AsRef<str>, item: T) -> bool {
        let haystack = trim_quotes_str(haystack.as_ref());

        match &mut self.state {
            State::Prefix { items } => {
                let haystack = if self.case_sensitive {
                    Cow::Borrowed(haystack)
                } else {
                    Cow::Owned(haystack.to_folded_case())
                };
                let matches = if self.positional {
                    haystack.starts_with(&self.needle)
                } else {
                    haystack.contains(&self.needle)
                };
                if !matches {
                    return false;
                }

                let insert_ind =
                    match items.binary_search_by(|(other, _)| other.as_str().cmp(&haystack)) {
                        Ok(i) => i,
                        Err(i) => i,
                    };
                items.insert(insert_ind, (haystack.to_string(), item));

                true
            }
            State::Fuzzy { items } => {
                let mut matcher = SkimMatcherV2::default();
                if self.case_sensitive {
                    matcher = matcher.respect_case();
                } else {
                    matcher = matcher.ignore_case();
                }
                let Some(score) = matcher.fuzzy_match(haystack, &self.needle) else {
                    return false;
                };

                let insert_ind =
                    match items.binary_search_by(|(other_score, _)| other_score.cmp(&score)) {
                        Ok(i) => i,
                        Err(i) => i,
                    };
                items.insert(insert_ind, (score, item));

                true
            }
        }
    }

    pub fn results(self) -> Vec<T> {
        match self.state {
            State::Prefix { items } => items.into_iter().map(|(_, item)| item).collect(),
            State::Fuzzy { items } => items.into_iter().map(|(_, item)| item).collect(),
        }
    }
}

impl NuMatcher<SemanticSuggestion> {
    pub fn add_semantic_suggestion(&mut self, suggestion: SemanticSuggestion) -> bool {
        self.add(suggestion.suggestion.value.clone(), suggestion)
    }
}

#[derive(Debug)]
pub enum InvalidMatchAlgorithm {
    Unknown,
}

impl Display for InvalidMatchAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            InvalidMatchAlgorithm::Unknown => write!(f, "unknown match algorithm"),
        }
    }
}

impl std::error::Error for InvalidMatchAlgorithm {}

#[derive(Clone)]
pub struct CompletionOptions {
    pub case_sensitive: bool,
    pub positional: bool,
    pub match_algorithm: MatchAlgorithm,
}

impl Default for CompletionOptions {
    fn default() -> Self {
        Self {
            case_sensitive: true,
            positional: true,
            match_algorithm: MatchAlgorithm::Prefix,
        }
    }
}

#[cfg(test)]
mod test {
    use crate::completions::completion_options::NuMatcher;
    use rstest::rstest;

    use super::{CompletionOptions, MatchAlgorithm};

    fn run_match_algorithm_test(
        needle: &str,
        options: &CompletionOptions,
        haystacks: &[&str],
        expected: &[&str],
    ) {
        let mut matcher = NuMatcher::new(needle, options);

        for haystack in haystacks {
            matcher.add(haystack, *haystack);
        }

        assert_eq!(expected, matcher.results());
    }

    #[rstest]
    #[case("", &["foo", "bar", "baz"], &["bar", "baz", "foo"])]
    #[case("ba", &["foo", "bar", "bleh", "baz"], &["bar", "baz"])]
    #[case("bars", &["foo", "bar", "baz"], &[])]
    fn prefix_match(#[case] needle: &str, #[case] haystacks: &[&str], #[case] expected: &[&str]) {
        run_match_algorithm_test(
            needle,
            &CompletionOptions {
                case_sensitive: false,
                positional: false,
                match_algorithm: MatchAlgorithm::Prefix,
            },
            haystacks,
            expected,
        );
    }

    #[rstest]
    #[case("", &["foo", "bar", "baz"], &["foo", "bar", "baz"])]
    #[case("ba", &["foo", "bar", "blah", "baz"], &["bar", "baz", "blah"])]
    #[case("f8l", &["from_utf8_lossy", "String::from_utf8", "blehf8l"], &["blehf8l", "from_utf8_lossy"])]
    fn fuzzy_match(#[case] needle: &str, #[case] haystacks: &[&str], #[case] expected: &[&str]) {
        run_match_algorithm_test(
            needle,
            &CompletionOptions {
                case_sensitive: false,
                positional: false,
                match_algorithm: MatchAlgorithm::Fuzzy,
            },
            haystacks,
            expected,
        );
    }

    #[rstest]
    #[case(MatchAlgorithm::Prefix, true)]
    #[case(MatchAlgorithm::Prefix, false)]
    #[case(MatchAlgorithm::Fuzzy, true)]
    #[case(MatchAlgorithm::Fuzzy, false)]
    fn case_insensitive_sort(#[case] match_algorithm: MatchAlgorithm, #[case] positional: bool) {
        // B comes before b in ASCII, but they should be treated as the same letter
        run_match_algorithm_test(
            "b",
            &CompletionOptions {
                case_sensitive: false,
                positional,
                match_algorithm,
            },
            &["Buppercase", "blowercase"],
            &["blowercase", "Buppercase"],
        );
    }
}
