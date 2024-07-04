use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use nu_parser::trim_quotes_str;
use nu_protocol::CompletionAlgorithm;
use nu_utils::IgnoreCaseExt;
use std::fmt::Display;

#[derive(Copy, Clone)]
pub enum SortBy {
    LevenshteinDistance,
    Ascending,
    None,
}

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

impl MatchAlgorithm {
    /// Returns whether the `needle` search text matches the given `haystack`.
    pub fn matches_str(&self, haystack: &str, needle: &str) -> bool {
        let haystack = trim_quotes_str(haystack);
        let needle = trim_quotes_str(needle);
        match *self {
            MatchAlgorithm::Prefix => haystack.starts_with(needle),
            MatchAlgorithm::Fuzzy => {
                let matcher = SkimMatcherV2::default();
                matcher.fuzzy_match(haystack, needle).is_some()
            }
        }
    }

    /// Returns whether the `needle` search text matches the given `haystack`.
    pub fn matches_u8(&self, haystack: &[u8], needle: &[u8]) -> bool {
        match *self {
            MatchAlgorithm::Prefix => haystack.starts_with(needle),
            MatchAlgorithm::Fuzzy => {
                let haystack_str = String::from_utf8_lossy(haystack);
                let needle_str = String::from_utf8_lossy(needle);

                let matcher = SkimMatcherV2::default();
                matcher.fuzzy_match(&haystack_str, &needle_str).is_some()
            }
        }
    }
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
    options: CompletionOptions,
    sort: bool,
    state: State<T>,
}

enum State<T> {
    Prefix { items: Vec<(String, T)> },
    Fuzzy { items: Vec<(i64, String, T)> },
}

impl<T> NuMatcher<T> {
    pub fn new(needle: impl AsRef<str>, options: CompletionOptions, sort: bool) -> NuMatcher<T> {
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
                    options,
                    sort,
                    state: State::Prefix { items: Vec::new() },
                }
            }
            MatchAlgorithm::Fuzzy => NuMatcher {
                needle,
                options,
                sort,
                state: State::Fuzzy { items: Vec::new() },
            },
        }
    }

    fn add(&mut self, haystack: impl AsRef<str>, item: T) -> bool {
        let haystack = trim_quotes_str(haystack.as_ref());

        match &mut self.state {
            State::Prefix { items } => {
                let matches = if self.options.positional {
                    haystack.starts_with(&self.needle)
                } else {
                    haystack.contains(&self.needle)
                };
                if !matches {
                    return false;
                }

                if self.sort {
                    let insert_ind =
                        match items.binary_search_by(|(other, _)| other.as_str().cmp(haystack)) {
                            Ok(i) => i,
                            Err(i) => i,
                        };
                    items.insert(insert_ind, (haystack.to_string(), item));
                } else {
                    items.push((haystack.to_string(), item))
                }

                true
            }
            State::Fuzzy { items } => {
                let mut matcher = SkimMatcherV2::default();
                if self.options.case_sensitive {
                    matcher = matcher.respect_case();
                } else {
                    matcher = matcher.ignore_case();
                }
                let Some(score) = matcher.fuzzy_match(haystack, &self.needle) else {
                    return false;
                };

                if self.sort {
                    let insert_ind = match items
                        .binary_search_by(|(other_score, _, _)| other_score.cmp(&score))
                    {
                        Ok(i) => i,
                        Err(i) => i,
                    };
                    items.insert(insert_ind, (score, haystack.to_string(), item));
                } else {
                    items.push((score, haystack.to_string(), item))
                }

                true
            }
        }
    }

    fn results(self) -> Vec<T> {
        match self.state {
            State::Prefix { items } => items.into_iter().map(|(_, item)| item).collect(),
            State::Fuzzy { items } => items.into_iter().map(|(_, _, item)| item).collect(),
        }
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
    use super::MatchAlgorithm;

    #[test]
    fn match_algorithm_prefix() {
        let algorithm = MatchAlgorithm::Prefix;

        assert!(algorithm.matches_str("example text", ""));
        assert!(algorithm.matches_str("example text", "examp"));
        assert!(!algorithm.matches_str("example text", "text"));

        assert!(algorithm.matches_u8(&[1, 2, 3], &[]));
        assert!(algorithm.matches_u8(&[1, 2, 3], &[1, 2]));
        assert!(!algorithm.matches_u8(&[1, 2, 3], &[2, 3]));
    }

    #[test]
    fn match_algorithm_fuzzy() {
        let algorithm = MatchAlgorithm::Fuzzy;

        assert!(algorithm.matches_str("example text", ""));
        assert!(algorithm.matches_str("example text", "examp"));
        assert!(algorithm.matches_str("example text", "ext"));
        assert!(algorithm.matches_str("example text", "mplxt"));
        assert!(!algorithm.matches_str("example text", "mpp"));

        assert!(algorithm.matches_u8(&[1, 2, 3], &[]));
        assert!(algorithm.matches_u8(&[1, 2, 3], &[1, 2]));
        assert!(algorithm.matches_u8(&[1, 2, 3], &[2, 3]));
        assert!(algorithm.matches_u8(&[1, 2, 3], &[1, 3]));
        assert!(!algorithm.matches_u8(&[1, 2, 3], &[2, 2]));
    }
}
