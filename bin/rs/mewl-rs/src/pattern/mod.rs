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

use crate::error::{Result, fatal};

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
    pub fn parse(pattern: &[u8]) -> Result<Self> {
        parser::parse(pattern).map(Pattern)
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
            Expr::Broken => return Err(fatal!("broken tree.")),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Error;

    /// Evaluates `pattern` where a field `k` contains `v` if `v` is
    /// in `k` (e.g. the field `abc` contains `b`).
    fn eval(pattern: &[u8]) -> std::result::Result<bool, Vec<u8>> {
        let msg = |e| match e {
            Error::Fatal(m) => m,
            Error::Usage => b"usage".to_vec(),
        };
        let p = Pattern::parse(pattern).map_err(msg)?;
        p.matches(&mut |k, v, _| k.windows(v.len()).any(|w| w == v)).map_err(msg)
    }

    #[test]
    fn evaluation() {
        assert_eq!(eval(b"abc=b"), Ok(true));
        assert_eq!(eval(b"abc!=b"), Ok(false));
        assert_eq!(eval(b"abc=x | abc=c"), Ok(true));
        assert_eq!(eval(b"abc=x | abc=c & abc=y"), Ok(false));
        assert_eq!(eval(b"!(abc=x | abc=c) | abc=a"), Ok(true));
    }

    #[test]
    fn errors() {
        assert_eq!(eval(b""), Err(b"invalid pattern.".to_vec()));
        assert_eq!(eval(b"a="), Err(b"broken eq/neq.".to_vec()));
        assert_eq!(eval(b"(a=b"), Err(b"too many '('.".to_vec()));
        assert_eq!(eval(b"a=b)"), Err(b"too many ')'.".to_vec()));
        assert_eq!(eval(b"()"), Err(b"empty '()'.".to_vec()));
        assert_eq!(eval(b"!a"), Err(b"broken not.".to_vec()));
        assert_eq!(eval(b"a=b &"), Err(b"broken and/or.".to_vec()));
        assert_eq!(eval(b"abc"), Err(b"broken tree.".to_vec()));
        /* Only the first one is evaluated. */
        assert_eq!(eval(b"abc=a abc"), Ok(true));
    }
}
