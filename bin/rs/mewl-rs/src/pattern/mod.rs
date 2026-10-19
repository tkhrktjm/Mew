//! Pick patterns.
//!
//! ```text
//! <pattern> = 'key=val' | 'key!=val' | '!<pattern>' | '(<pattern>)'
//!           | '<pattern>&<pattern>' | '<pattern>|<pattern>'
//! ```
//!
//! `==` and `!==` are case-sensitive.

mod lexer;
mod parser;

use std::fmt;


/// Errors of a pattern, with the messages of mewl.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatternError {
    UngetcImpossible,
    BrokenEscape,
    BrokenQuotedString,
    BrokenComparison,
    Empty,
    TooManyClosing,
    EmptyParentheses,
    TooManyOpening,
    BrokenNot,
    BrokenAndOr,
    /// Found only when evaluated.
    BrokenTree,
    /// Not one expression, in the strict syntax.
    Broken,
}

impl PatternError {
    pub fn message(self) -> &'static str {
        match self {
            PatternError::UngetcImpossible => "ungetc is impossible.",
            PatternError::BrokenEscape => "broken escape.",
            PatternError::BrokenQuotedString => "broken quoted string.",
            PatternError::BrokenComparison => "broken eq/neq.",
            PatternError::Empty => "invalid pattern.",
            PatternError::TooManyClosing => "too many ')'.",
            PatternError::EmptyParentheses => "empty '()'.",
            PatternError::TooManyOpening => "too many '('.",
            PatternError::BrokenNot => "broken not.",
            PatternError::BrokenAndOr => "broken and/or.",
            PatternError::BrokenTree => "broken tree.",
            PatternError::Broken => "broken pattern.",
        }
    }
}

impl fmt::Display for PatternError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

impl std::error::Error for PatternError {}

type Result<T> = std::result::Result<T, PatternError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmp {
    /// `=`, case-insensitive
    Eq,
    /// `==`, case-sensitive
    EqCase,
    /// `!=`, case-insensitive
    Ne,
    /// `!==`, case-sensitive
    NeCase,
}

impl Cmp {
    fn case_sensitive(self) -> bool {
        matches!(self, Cmp::EqCase | Cmp::NeCase)
    }

    fn negated(self) -> bool {
        matches!(self, Cmp::Ne | Cmp::NeCase)
    }
}

#[derive(Debug)]
pub enum Expr {
    Not(Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Cmp { cmp: Cmp, key: Vec<u8>, value: Vec<u8> },
    /// A part which the parser accepted but cannot be evaluated.
    /// mewl reports it only when it is evaluated.
    Broken,
}

pub struct Pattern(Expr);

impl Pattern {
    /// Parses a pattern as mewl does.
    pub fn parse(pattern: &[u8]) -> Result<Self> {
        parser::parse(pattern, false).map(Pattern)
    }

    /// Parses a pattern more strictly (see [`parser::parse`]).
    pub fn parse_strict(pattern: &[u8]) -> Result<Self> {
        parser::parse(pattern, true).map(Pattern)
    }

    /// Evaluates the pattern from left to right with short circuit.
    /// `search(key, value, case_sensitive)` tells whether the field
    /// `key` contains `value`.
    pub fn matches<F>(&self, search: &mut F) -> Result<bool>
    where
        F: FnMut(&[u8], &[u8], bool) -> bool,
    {
        self.0.eval(search)
    }
}

impl Expr {
    fn eval<F>(&self, search: &mut F) -> Result<bool>
    where
        F: FnMut(&[u8], &[u8], bool) -> bool,
    {
        Ok(match self {
            Expr::Not(e) => !e.eval(search)?,
            Expr::And(a, b) => a.eval(search)? && b.eval(search)?,
            Expr::Or(a, b) => a.eval(search)? || b.eval(search)?,
            Expr::Cmp { cmp, key, value } => search(key, value, cmp.case_sensitive()) != cmp.negated(),
            Expr::Broken => return Err(PatternError::BrokenTree),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Evaluates `pattern` where a field `k` contains `v` if `v` is
    /// in `k` (e.g. the field `abc` contains `b`).
    fn eval(pattern: &[u8]) -> Result<bool> {
        Pattern::parse(pattern)?.matches(&mut |k, v, _| k.windows(v.len()).any(|w| w == v))
    }

    #[test]
    fn evaluation() {
        assert_eq!(eval(b"abc=b"), Ok(true));
        assert_eq!(eval(b"abc!=b"), Ok(false));
        assert_eq!(eval(b"abc=x | abc=c"), Ok(true));
        assert_eq!(eval(b"abc=x | abc=c & abc=y"), Ok(false));
        assert_eq!(eval(b"!(abc=x | abc=c) | abc=a"), Ok(true));
    }

    fn eval_strict(pattern: &[u8]) -> Result<bool> {
        /* The field "abc def" contains its substrings. */
        Pattern::parse_strict(pattern)?.matches(&mut |_, v, _| b"abc  def".windows(v.len()).any(|w| w == v))
    }

    #[test]
    fn strict() {
        /* Words after a value are a part of it, with the blanks. */
        assert_eq!(eval_strict(b"subject=abc  def"), Ok(true));
        assert_eq!(eval_strict(b"subject=abc def"), Ok(false));
        assert_eq!(eval_strict(b"subject=abc  def | x=zzz"), Ok(true));
        assert_eq!(eval_strict(b"(subject=abc  def)"), Ok(true));
        /* but a key is not. */
        assert_eq!(eval_strict(b"a=abc c=zzz"), Err(PatternError::Broken));
        assert_eq!(eval_strict(b"(a=abc c=zzz)"), Err(PatternError::Broken));
        assert_eq!(eval_strict(b"abc"), Err(PatternError::Broken));
        assert_eq!(eval_strict(b"a=abc !def"), Err(PatternError::BrokenNot));
        /* mewl evaluates the first part only. */
        assert_eq!(eval(b"abc=a abc=z"), Ok(true));
    }

    #[test]
    fn errors() {
        assert_eq!(eval(b""), Err(PatternError::Empty));
        assert_eq!(eval(b"a="), Err(PatternError::BrokenComparison));
        assert_eq!(eval(b"(a=b"), Err(PatternError::TooManyOpening));
        assert_eq!(eval(b"a=b)"), Err(PatternError::TooManyClosing));
        assert_eq!(eval(b"()"), Err(PatternError::EmptyParentheses));
        assert_eq!(eval(b"!a"), Err(PatternError::BrokenNot));
        assert_eq!(eval(b"a=b &"), Err(PatternError::BrokenAndOr));
        assert_eq!(eval(b"abc"), Err(PatternError::BrokenTree));
        /* Only the first one is evaluated. */
        assert_eq!(eval(b"abc=a abc"), Ok(true));
    }
}
