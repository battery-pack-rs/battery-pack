//! Parsed Cargo feature-list reference.
//!
//! Variants mirror Cargo's syntax for entries inside a `[features]` value list:
//! `foo`, `dep:foo`, `foo/bar`, `foo?/bar`.
//!
//! See `md/spec/feature-refs.md`.

use std::{fmt::Display, str::FromStr};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A parsed entry from a Cargo `[features]` value list.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FeatureRef {
    Feature(String),
    Dep(String),
    DepFeature {
        dep: String,
        feature: String,
        weak: bool,
    },
}

/// Failure to parse a feature reference string.
#[derive(Debug, Error, PartialEq, Eq)]
#[error("invalid feature reference: '{raw}'")]
pub struct FeatureParseError {
    pub raw: String,
}

impl FeatureRef {
    /// Parse one entry from a `[features]` value list.
    ///
    /// Rejects empty inputs and entries with empty halves: `""`, `"dep:"`, `"/"`,
    /// `"foo/"`, `"/bar"`, `"?/foo"`.
    ///
    /// # Examples
    /// ```
    /// use bphelper_manifest::FeatureRef;
    ///
    /// assert_eq!(FeatureRef::parse("clap").unwrap(), FeatureRef::Feature("clap".into()));
    ///
    /// assert_eq!(FeatureRef::parse("dep:clap").unwrap(), FeatureRef::Dep("clap".into()));
    ///
    /// assert_eq!(
    ///     FeatureRef::parse("serde?/derive").unwrap(),
    ///     FeatureRef::DepFeature { dep: "serde".into(), feature: "derive".into(), weak: true },
    /// );
    /// ```
    pub fn parse(raw: &str) -> Result<Self, FeatureParseError> {
        let invalid = || FeatureParseError {
            raw: raw.to_owned(),
        };

        let (had_dep_prefix, rest) = match raw.strip_prefix("dep:") {
            Some(stripped) => (true, stripped),
            None => (false, raw),
        };

        if let Some((dep_part, feature)) = rest.split_once('/') {
            let (dep, weak) = match dep_part.strip_suffix('?') {
                Some(stripped) => (stripped, true),
                None => (dep_part, false),
            };

            if dep.is_empty() || feature.is_empty() {
                return Err(invalid());
            }
            return Ok(Self::DepFeature {
                dep: dep.to_owned(),
                feature: feature.to_owned(),
                weak,
            });
        }

        if rest.is_empty() {
            return Err(invalid());
        }

        Ok(match had_dep_prefix {
            true => Self::Dep(rest.to_owned()),
            false => Self::Feature(rest.to_owned()),
        })
    }

    /// Name of the dep side of the reference: the bare name, the `dep:` target, or the dep half of `dep/feature`.
    pub fn dep_name(&self) -> &str {
        match self {
            Self::Feature(name) | Self::Dep(name) | Self::DepFeature { dep: name, .. } => name,
        }
    }
}

impl Display for FeatureRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Feature(name) => f.write_str(name),
            Self::Dep(name) => write!(f, "dep:{name}"),
            Self::DepFeature { dep, feature, weak } => {
                let sep = if *weak { "?/" } else { "/" };

                write!(f, "{dep}{sep}{feature}")
            }
        }
    }
}

impl FromStr for FeatureRef {
    type Err = FeatureParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

// Display-order Ord gives `BTreeSet<FeatureRef>` diff-stable iteration that
// matches the printed form.
impl Ord for FeatureRef {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.to_string().cmp(&other.to_string())
    }
}

impl PartialOrd for FeatureRef {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Serialize for FeatureRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for FeatureRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bare_feature() {
        assert_eq!(
            FeatureRef::parse("foo").unwrap(),
            FeatureRef::Feature("foo".into())
        );
    }

    #[test]
    fn parses_dep_prefix() {
        assert_eq!(
            FeatureRef::parse("dep:foo").unwrap(),
            FeatureRef::Dep("foo".into()),
        )
    }

    #[test]
    fn parses_strong_dep_feature() {
        assert_eq!(
            FeatureRef::parse("serde/derive").unwrap(),
            FeatureRef::DepFeature {
                dep: "serde".into(),
                feature: "derive".into(),
                weak: false,
            }
        )
    }

    #[test]
    fn parses_weak_dep_feature() {
        assert_eq!(
            FeatureRef::parse("serde?/derive").unwrap(),
            FeatureRef::DepFeature {
                dep: "serde".into(),
                feature: "derive".into(),
                weak: true
            }
        )
    }

    #[test]
    fn rejects_empty_halves() {
        assert!(FeatureRef::parse("").is_err());
        assert!(FeatureRef::parse("/").is_err());
        assert!(FeatureRef::parse("foo/").is_err());
        assert!(FeatureRef::parse("/bar").is_err());
        assert!(FeatureRef::parse("dep:").is_err());
        assert!(FeatureRef::parse("?/foo").is_err());
    }

    #[test]
    fn display_round_trips_parse() {
        for raw in ["foo", "dep:foo", "serde/derive", "serde?/derive"] {
            let parsed = FeatureRef::parse(raw).unwrap();
            assert_eq!(parsed.to_string(), raw);
            assert_eq!(parsed.to_string().parse::<FeatureRef>().unwrap(), parsed);
        }
    }

    #[test]
    fn dep_name_returns_dep_side() {
        assert_eq!(FeatureRef::parse("foo").unwrap().dep_name(), "foo");
        assert_eq!(FeatureRef::parse("dep:foo").unwrap().dep_name(), "foo");
        assert_eq!(FeatureRef::parse("foo/bar").unwrap().dep_name(), "foo");
        assert_eq!(FeatureRef::parse("foo?/bar").unwrap().dep_name(), "foo");
    }

    #[test]
    fn ord_matches_display_lex() {
        let mut refs = [
            FeatureRef::parse("serde/derive").unwrap(),
            FeatureRef::parse("dep:anyhow").unwrap(),
            FeatureRef::parse("clap").unwrap(),
            FeatureRef::parse("serde?/derive").unwrap(),
        ];
        refs.sort();

        let displayed = refs.iter().map(ToString::to_string).collect::<Vec<_>>();

        let mut sorted_strings = displayed.clone();
        sorted_strings.sort();

        assert_eq!(displayed, sorted_strings);
    }

    #[test]
    fn serde_round_trips_via_string() {
        let original = FeatureRef::parse("serde?/derive").unwrap();
        let json = serde_json::to_string(&original).unwrap();
        assert_eq!(json, "\"serde?/derive\"");

        let back: FeatureRef = serde_json::from_str(&json).unwrap();
        assert_eq!(back, original);
    }
}
