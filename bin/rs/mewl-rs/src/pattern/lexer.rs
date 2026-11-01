//! Lexical analyzer of pick patterns.

use std::ops::Range;

use super::{Cmp, PatternError, Result};
use crate::compat::BUFSIZ;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Token {
    /// `str`, `"str"`, `\"`
    Str(Vec<u8>),
    Cmp(Cmp),
    /// `&`, `&&`
    And,
    /// `|`, `||`
    Or,
    /// `!`
    Not,
    /// `(`
    LParen,
    /// `)`
    RParen,
}

/// A token and where it is in the pattern.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spanned {
    pub token: Token,
    pub span: Range<usize>,
}

/// The kind of a token being read.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    None,
    Str,
    Cmp(Cmp),
    And,
    Or,
    Not,
}

pub struct Lexer<'a> {
    input: &'a [u8],
    pos: usize,
    /// The token just read ended at a blank, which is consumed.
    blank_ended: bool,
    /// Without the bugs of mewl in reading (see the comparisons in
    /// [`Lexer::read_token`]) and its limit (see [`Lexer::push`]).
    strict: bool,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a [u8], strict: bool) -> Self {
        Lexer { input, pos: 0, blank_ended: false, strict }
    }

    /// The next character, `None` at the end.
    fn getc(&mut self) -> Option<u8> {
        let c = *self.input.get(self.pos)?;
        self.pos += 1;
        Some(c)
    }

    /// Appends `c` to the string of a token. mewl keeps it in a buffer
    /// of `BUFSIZ` bytes with NUL at the end, and fails for more; not
    /// so when strict.
    fn push(&self, s: &mut Vec<u8>, c: u8) -> Result<()> {
        if !self.strict && s.len() >= BUFSIZ - 1 {
            return Err(PatternError::TooLong);
        }
        s.push(c);
        Ok(())
    }

    /// Steps back even after the end, as `Ungetc()` does.
    fn ungetc(&mut self) -> Result<()> {
        if self.pos == 0 {
            return Err(PatternError::UngetcImpossible);
        }
        self.pos -= 1;
        Ok(())
    }

    fn escaped(&mut self) -> Result<u8> {
        self.getc().ok_or(PatternError::BrokenEscape)
    }

    /// The next token. `None` at the end of the pattern.
    pub fn next_token(&mut self) -> Result<Option<Spanned>> {
        let mut start = self.pos;
        self.blank_ended = false;
        let token = self.read_token(&mut start)?;
        let end = self.pos - usize::from(self.blank_ended);
        Ok(token.map(|token| Spanned { token, span: start..end }))
    }

    /// Reads a token, setting `start` to where it begins.
    fn read_token(&mut self, start: &mut usize) -> Result<Option<Token>> {
        let mut kind = Kind::None;
        let mut s = Vec::new();

        /* The first character decides the kind. */
        while let Some(c) = self.getc() {
            match c {
                b' ' | b'\t' => {
                    *start = self.pos;
                    continue;
                }
                b'&' => kind = Kind::And,
                b'|' => kind = Kind::Or,
                b'!' => kind = Kind::Not,
                b'=' => kind = Kind::Cmp(Cmp::Eq),
                b'(' => return Ok(Some(Token::LParen)),
                b')' => return Ok(Some(Token::RParen)),
                b'"' => return self.quoted().map(Some),
                b'\\' => {
                    kind = Kind::Str;
                    let c = self.escaped()?;
                    self.push(&mut s, c)?;
                }
                _ => {
                    kind = Kind::Str;
                    self.push(&mut s, c)?;
                }
            }
            break;
        }

        /* The rest of the token. */
        while let Some(c) = self.getc() {
            match c {
                b' ' | b'\t' => self.blank_ended = true,
                b'&' if kind == Kind::And => {}
                b'|' if kind == Kind::Or => {}
                b'=' if kind == Kind::Cmp(Cmp::Eq) => kind = Kind::Cmp(Cmp::EqCase),
                b'=' if kind == Kind::Not => match self.getc() {
                    Some(b'=') => kind = Kind::Cmp(Cmp::NeCase),
                    /* mewl steps back even at the end. */
                    None if self.strict => kind = Kind::Cmp(Cmp::Ne),
                    _ => {
                        kind = Kind::Cmp(Cmp::Ne);
                        self.ungetc()?;
                    }
                },
                b'&' | b'|' | b'!' | b'=' | b'(' | b')' => self.ungetc()?,
                _ if kind != Kind::Str => self.ungetc()?,
                b'\\' => {
                    let c = self.escaped()?;
                    self.push(&mut s, c)?;
                    continue;
                }
                _ => {
                    self.push(&mut s, c)?;
                    continue;
                }
            }
            break;
        }

        Ok(match kind {
            Kind::None => None,
            Kind::Str => Some(Token::Str(s)),
            Kind::Cmp(cmp) => Some(Token::Cmp(cmp)),
            Kind::And => Some(Token::And),
            Kind::Or => Some(Token::Or),
            Kind::Not => Some(Token::Not),
        })
    }

    /// `"..."` with `\` escapes.
    fn quoted(&mut self) -> Result<Token> {
        let mut s = Vec::new();
        while let Some(c) = self.getc() {
            match c {
                b'\\' => {
                    let c = self.escaped()?;
                    self.push(&mut s, c)?;
                }
                b'"' => return Ok(Token::Str(s)),
                _ => self.push(&mut s, c)?,
            }
        }
        Err(PatternError::BrokenQuotedString)
    }
}

impl Iterator for Lexer<'_> {
    type Item = Result<Spanned>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_token().transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(p: &[u8]) -> Vec<Token> {
        Lexer::new(p, false).map(|t| t.map(|t| t.token)).collect::<Result<_>>().unwrap()
    }

    #[test]
    fn operators() {
        use Token::*;
        let s = |x: &[u8]| Str(x.to_vec());
        assert_eq!(tokens(b"a=b"), [s(b"a"), Cmp(super::Cmp::Eq), s(b"b")]);
        assert_eq!(tokens(b"a == b"), [s(b"a"), Cmp(super::Cmp::EqCase), s(b"b")]);
        assert_eq!(tokens(b"a!=b"), [s(b"a"), Cmp(super::Cmp::Ne), s(b"b")]);
        assert_eq!(tokens(b"a!==b"), [s(b"a"), Cmp(super::Cmp::NeCase), s(b"b")]);
        assert_eq!(tokens(b"!(x)&&y||z"), [Not, LParen, s(b"x"), RParen, And, s(b"y"), Or, s(b"z")]);
        assert_eq!(tokens(br#""a b"\(c"#), [s(b"a b"), s(b"(c")]);
    }

    #[test]
    fn strict() {
        let strict = |p: &[u8]| Lexer::new(p, true).map(|t| t.map(|t| t.token)).collect::<Result<Vec<_>>>().unwrap();
        /* 0xff is a byte, not the end. */
        let ff = [Token::Str(b"a".to_vec()), Token::Cmp(Cmp::Eq), Token::Str(b"\xffb".to_vec())];
        assert_eq!(strict(b"a=\xffb"), ff);
        assert_eq!(tokens(b"a=\xffb"), ff);
        /* "!=" at the end. */
        assert_eq!(strict(b"a!="), [Token::Str(b"a".to_vec()), Token::Cmp(Cmp::Ne)]);
        assert_eq!(tokens(b"a!="), [Token::Str(b"a".to_vec()), Token::Cmp(Cmp::Ne), Token::Cmp(Cmp::Eq)]);
    }

    #[test]
    fn too_long() {
        let lex = |p: &[u8], strict| Lexer::new(p, strict).map(|t| t.map(|t| t.token)).collect::<Result<Vec<_>>>();
        /* BUFSIZ - 1 bytes at most, an escaped one ("\\x") included */
        for (pre, post, kept) in [("", "", 0), ("\"", "\"", 0), ("a=", "", 0), ("\\x", "", 1)] {
            let max = [pre.as_bytes(), &vec![b'x'; BUFSIZ - 1 - kept], post.as_bytes()].concat();
            assert!(lex(&max, false).is_ok(), "{pre}");
            let over = [pre.as_bytes(), &vec![b'x'; BUFSIZ - kept], post.as_bytes()].concat();
            assert_eq!(lex(&over, false), Err(PatternError::TooLong), "{pre}");
            assert!(lex(&over, true).is_ok(), "{pre}");
        }
    }

    #[test]
    fn spans() {
        let spans: Vec<_> = Lexer::new(br#" key = "v w"  x&y"#, false).map(|t| t.unwrap().span).collect();
        assert_eq!(spans, [1..4, 5..6, 7..12, 14..15, 15..16, 16..17]);
    }
}
