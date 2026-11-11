// Copyright 2026 Thomas Johannesson
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
// Whether a search pattern matches regardless of letter case.

use clap::ValueEnum;
use regex_syntax::ast::{self, Ast, ClassSetItem, Visitor};

/// How a search treats letter case (`--search-case`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum SearchCase {
    /// Ignore case unless the pattern has an uppercase letter of its own.
    #[default]
    Smart,
    /// Match case exactly.
    Sensitive,
    /// Ignore case always.
    Insensitive,
}

impl SearchCase {
    /// Whether the regex `pattern` matches regardless of case.
    pub fn ignores_case_in_regex(self, pattern: &str) -> bool {
        match self {
            SearchCase::Smart => !regex_has_uppercase_letter(pattern),
            SearchCase::Sensitive => false,
            SearchCase::Insensitive => true,
        }
    }

    /// Whether the path glob `pattern` matches regardless of case. Glob
    /// syntax has no letter escapes, so every letter in it is meant.
    pub fn ignores_case_in_glob(self, pattern: &str) -> bool {
        match self {
            SearchCase::Smart => !pattern.chars().any(char::is_uppercase),
            SearchCase::Sensitive => false,
            SearchCase::Insensitive => true,
        }
    }
}

/// Whether `pattern` spells out an uppercase letter to match, as opposed to
/// one that is syntax, like `\S` or `\p{Lu}`. A pattern that does not parse
/// counts as having one, so that it is not quietly widened.
fn regex_has_uppercase_letter(pattern: &str) -> bool {
    let Ok(parsed) = ast::parse::Parser::new().parse(pattern) else {
        return true;
    };
    ast::visit(&parsed, UppercaseLetters).is_err()
}

/// Stops at the first uppercase letter matched literally, as an error.
struct UppercaseLetters;

impl Visitor for UppercaseLetters {
    type Output = ();
    type Err = ();

    fn finish(self) -> Result<(), ()> {
        Ok(())
    }

    fn visit_pre(&mut self, ast: &Ast) -> Result<(), ()> {
        match ast {
            Ast::Literal(literal) if literal.c.is_uppercase() => Err(()),
            _ => Ok(()),
        }
    }

    fn visit_class_set_item_pre(&mut self, item: &ClassSetItem) -> Result<(), ()> {
        match item {
            ClassSetItem::Literal(literal) if literal.c.is_uppercase() => Err(()),
            ClassSetItem::Range(range)
                if range.start.c.is_uppercase() || range.end.c.is_uppercase() =>
            {
                Err(())
            }
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn smart(pattern: &str) -> bool {
        SearchCase::Smart.ignores_case_in_regex(pattern)
    }

    #[test]
    fn smart_case_ignores_case_without_an_uppercase_letter() {
        assert!(smart("fix"));
        assert!(!smart("Fix"));
        assert!(!smart("fi[A-Z]"));
        assert!(!smart("fi[xY]"));
    }

    #[test]
    fn uppercase_in_regex_syntax_does_not_count() {
        assert!(smart(r"\S+"));
        assert!(smart(r"\W\D"));
        assert!(smart(r"\bfoo\B"));
        assert!(smart(r"\p{Lu}x"));
        assert!(smart(r"[\S\d]"));
    }

    #[test]
    fn an_unparsable_pattern_is_case_sensitive() {
        assert!(!smart("fix("));
    }

    #[test]
    fn sensitive_and_insensitive_override_the_pattern() {
        assert!(!SearchCase::Sensitive.ignores_case_in_regex("fix"));
        assert!(SearchCase::Insensitive.ignores_case_in_regex("Fix"));
        assert!(!SearchCase::Sensitive.ignores_case_in_glob("*.md"));
        assert!(SearchCase::Insensitive.ignores_case_in_glob("README*"));
    }

    #[test]
    fn smart_case_in_a_glob_counts_every_uppercase_letter() {
        assert!(SearchCase::Smart.ignores_case_in_glob("readme*"));
        assert!(!SearchCase::Smart.ignores_case_in_glob("README*"));
    }
}
