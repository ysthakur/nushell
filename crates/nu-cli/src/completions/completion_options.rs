use nu_parser::trim_quotes_str;
use nu_protocol::CompletionAlgorithm;
use nu_utils::IgnoreCaseExt;
use nucleo_matcher::{
    pattern::{AtomKind, CaseMatching, Normalization, Pattern},
    Config, Matcher, Utf32Str,
};
use std::{borrow::Cow, fmt::Display};

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
    // / Keeps only items that match the given `needle`
    // /
    // / # Arguments
    // /
    // / * `items` - A list of haystacks and their corresponding items
    // / * `needle` - The text to search for
    // / * `case_sensitive` - true to respect case, false to ignore it
    // /
    // / # Returns
    // /
    // / A list of matching items, as well as the indices in their haystacks that matched
}

pub struct NuMatcher<T> {
    state: State<T>,
    sort_by: SortBy,
}

enum State<T> {
    Prefix {
        needle: Vec<u8>,
        case_sensitive: bool,
        items: Vec<T>,
    },
    Nucleo {
        matcher: Matcher,
        pat: Pattern,
        items: Vec<(u32, T, Vec<usize>)>,
    },
}

impl<T> NuMatcher<T> {
    pub fn from_str(
        needle: impl AsRef<str>,
        options: &CompletionOptions,
        sort_by: SortBy,
    ) -> NuMatcher<T> {
        let needle = trim_quotes_str(needle.as_ref());
        let state = match options.match_algorithm {
            MatchAlgorithm::Prefix => {
                let needle = if options.case_sensitive {
                    Cow::Borrowed(needle)
                } else {
                    Cow::Owned(needle.to_folded_case())
                };
                State::Prefix {
                    needle: needle.as_bytes().to_vec(),
                    case_sensitive: options.case_sensitive,
                    items: Vec::new(),
                }
            }
            MatchAlgorithm::Fuzzy => {
                let matcher = Matcher::new(Config::DEFAULT);
                let pat = Pattern::new(
                    needle,
                    if options.case_sensitive {
                        CaseMatching::Respect
                    } else {
                        CaseMatching::Ignore
                    },
                    Normalization::Smart,
                    AtomKind::Fuzzy,
                );
                State::Nucleo {
                    matcher,
                    pat,
                    items: Vec::new(),
                }
            }
        };
        NuMatcher { state, sort_by }
    }

    pub fn from_u8(
        needle: impl AsRef<[u8]>,
        options: &CompletionOptions,
        sort_by: SortBy,
    ) -> NuMatcher<T> {
        let needle = needle.as_ref();
        match options.match_algorithm {
            MatchAlgorithm::Prefix => {
                let needle = if options.case_sensitive {
                    needle.to_owned()
                } else {
                    needle.to_ascii_lowercase()
                };
                NuMatcher {
                    state: State::Prefix {
                        needle,
                        case_sensitive: options.case_sensitive,
                        items: Vec::new(),
                    },
                    sort_by,
                }
            }
            MatchAlgorithm::Fuzzy => {
                Self::from_str(String::from_utf8_lossy(needle), options, sort_by)
            }
        }
    }

    pub fn add_str(&mut self, haystack: impl AsRef<str>, item: T) -> bool {
        match &mut self.state {
            State::Prefix {
                needle,
                case_sensitive,
                items,
            } => {
                let haystack = trim_quotes_str(haystack.as_ref());
                let haystack = if *case_sensitive {
                    Cow::Borrowed(haystack)
                } else {
                    Cow::Owned(haystack.to_folded_case())
                };
                if haystack.as_bytes().starts_with(needle) {
                    items.push(item);
                    true
                } else {
                    false
                }
            }
            State::Nucleo {
                matcher,
                pat,
                items,
            } => {
                let mut haystack_buf = Vec::new();
                let haystack = Utf32Str::new(trim_quotes_str(haystack.as_ref()), &mut haystack_buf);
                // todo find out why nucleo uses u32 instead of usize for indices
                let mut indices = Vec::new();
                match pat.indices(haystack, matcher, &mut indices) {
                    Some(score) => {
                        indices.sort_unstable();
                        indices.dedup();
                        items.push((
                            score,
                            item,
                            indices.into_iter().map(|i| i as usize).collect(),
                        ));
                        true
                    }
                    None => false,
                }
            }
        }
    }

    pub fn add_u8(&mut self, haystack: impl AsRef<[u8]>, item: T) -> bool {
        let haystack = haystack.as_ref();
        match &mut self.state {
            State::Prefix {
                needle,
                case_sensitive,
                items,
            } => {
                let haystack = if *case_sensitive {
                    Cow::Borrowed(haystack)
                } else {
                    Cow::Owned(haystack.to_ascii_lowercase())
                };
                if haystack.starts_with(needle) {
                    items.push(item);
                    true
                } else {
                    false
                }
            }
            State::Nucleo { .. } => self.add_str(String::from_utf8_lossy(haystack), item),
        }
    }

    pub fn get_results(self) -> Vec<T> {
        match self.state {
            State::Prefix { items, .. } => items,
            State::Nucleo { .. } => {
                let (results, _): (Vec<_>, Vec<_>) =
                    self.get_results_with_inds().into_iter().unzip();
                results
            }
        }
    }

    pub fn get_results_with_inds(self) -> Vec<(T, Vec<usize>)> {
        match self.state {
            State::Prefix { needle, items, .. } => items
                .into_iter()
                .map(|item| (item, (0..needle.len()).collect()))
                .collect(),
            State::Nucleo { items, .. } => items
                .into_iter()
                .map(|(_, items, indices)| (items, indices))
                .collect(),
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
    use super::{CompletionOptions, MatchAlgorithm, NuMatcher, SortBy};

    fn test_match_str(options: &CompletionOptions, haystack: &str, needle: &str) {
        let mut matcher = NuMatcher::from_str(needle, options, SortBy::None);
        matcher.add_str(haystack, haystack);
        assert_eq!(vec![haystack], matcher.get_results());
    }

    fn test_not_match_str(options: &CompletionOptions, haystack: &str, needle: &str) {
        let mut matcher = NuMatcher::from_str(needle, options, SortBy::None);
        matcher.add_str(haystack, haystack);
        assert_ne!(vec![haystack], matcher.get_results());
    }

    fn test_match_u8(options: &CompletionOptions, haystack: &[u8], needle: &[u8]) {
        let mut matcher = NuMatcher::from_u8(needle, options, SortBy::None);
        matcher.add_u8(haystack, haystack);
        assert_eq!(vec![haystack], matcher.get_results());
    }

    fn test_not_match_u8(options: &CompletionOptions, haystack: &[u8], needle: &[u8]) {
        let mut matcher = NuMatcher::from_u8(needle, options, SortBy::None);
        matcher.add_u8(haystack, haystack);
        assert_ne!(vec![haystack], matcher.get_results());
    }

    #[test]
    fn match_algorithm_prefix() {
        let options = CompletionOptions {
            match_algorithm: MatchAlgorithm::Prefix,
            case_sensitive: true,
            positional: false,
        };

        test_match_str(&options, "example text", "");
        test_match_str(&options, "example text", "examp");
        test_not_match_str(&options, "example text", "text");

        test_match_u8(&options, &[1, 2, 3], &[]);
        test_match_u8(&options, &[1, 2, 3], &[1, 2]);
        test_not_match_u8(&options, &[1, 2, 3], &[2, 3]);
    }

    #[test]
    fn match_algorithm_fuzzy() {
        let options = CompletionOptions {
            match_algorithm: MatchAlgorithm::Fuzzy,
            case_sensitive: true,
            positional: false,
        };

        test_match_str(&options, "example text", "");
        test_match_str(&options, "example text", "examp");
        test_match_str(&options, "example text", "ext");
        test_match_str(&options, "example text", "mplxt");
        test_not_match_str(&options, "example text", "mpp");

        test_match_u8(&options, &[1, 2, 3], &[]);
        test_match_u8(&options, &[1, 2, 3], &[1, 2]);
        test_match_u8(&options, &[1, 2, 3], &[2, 3]);
        test_match_u8(&options, &[1, 2, 3], &[1, 3]);
        test_not_match_u8(&options, &[1, 2, 3], &[2, 2]);
    }

    #[test]
    fn match_algorithm_fuzzy_sort_score() {
        let options = CompletionOptions {
            match_algorithm: MatchAlgorithm::Fuzzy,
            case_sensitive: true,
            positional: false,
        };

        // Taken from the nucleo-matcher crate's examples
        // todo more thorough tests
        let mut matcher = NuMatcher::from_str("foo bar", &options, SortBy::None);
        matcher.add_str("foo/bar", "foo/bar");
        matcher.add_str("bar/foo", "bar/foo");
        matcher.add_str("foobar", "foobar");
        assert_eq!(vec!["foo/bar", "bar/foo", "foobar"], matcher.get_results());
    }
}
