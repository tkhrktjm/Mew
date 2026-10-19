//! Lexical analyzer of pick patterns.

use crate::compat::CHAR_IS_SIGNED;
use crate::error::{Result, fatal};

use super::Cmp;

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
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a [u8]) -> Self {
        Lexer { input, pos: 0 }
    }

    /// The next character, `None` at the end.
    ///
    /// When `char` is signed, the byte 0xff is `EOF` (-1) in mewl.
    /// It is consumed nevertheless, so reading can go on after it.
    fn getc(&mut self) -> Option<u8> {
        let c = *self.input.get(self.pos)?;
        self.pos += 1;
        (!(CHAR_IS_SIGNED && c == 0xff)).then_some(c)
    }

    /// Steps back even after the end, as `Ungetc()` does.
    fn ungetc(&mut self) -> Result<()> {
        if self.pos == 0 {
            return Err(fatal!("ungetc is impossible."));
        }
        self.pos -= 1;
        Ok(())
    }

    fn escaped(&mut self) -> Result<u8> {
        self.getc().ok_or_else(|| fatal!("broken escape."))
    }

    /// The next token. `None` at the end of the pattern (or at 0xff,
    /// see [`Lexer::getc`]).
    pub fn next_token(&mut self) -> Result<Option<Token>> {
        let mut kind = Kind::None;
        let mut s = Vec::new();

        /* The first character decides the kind. */
        while let Some(c) = self.getc() {
            match c {
                b' ' | b'\t' => continue,
                b'&' => kind = Kind::And,
                b'|' => kind = Kind::Or,
                b'!' => kind = Kind::Not,
                b'=' => kind = Kind::Cmp(Cmp::Eq),
                b'(' => return Ok(Some(Token::LParen)),
                b')' => return Ok(Some(Token::RParen)),
                b'"' => return self.quoted().map(Some),
                b'\\' => {
                    kind = Kind::Str;
                    s.push(self.escaped()?);
                }
                _ => {
                    kind = Kind::Str;
                    s.push(c);
                }
            }
            break;
        }

        /* The rest of the token. */
        while let Some(c) = self.getc() {
            match c {
                b' ' | b'\t' => {}
                b'&' if kind == Kind::And => {}
                b'|' if kind == Kind::Or => {}
                b'=' if kind == Kind::Cmp(Cmp::Eq) => kind = Kind::Cmp(Cmp::EqCase),
                b'=' if kind == Kind::Not => {
                    if self.getc() == Some(b'=') {
                        kind = Kind::Cmp(Cmp::NeCase);
                    } else {
                        kind = Kind::Cmp(Cmp::Ne);
                        self.ungetc()?;
                    }
                }
                b'&' | b'|' | b'!' | b'=' | b'(' | b')' => self.ungetc()?,
                _ if kind != Kind::Str => self.ungetc()?,
                b'\\' => {
                    s.push(self.escaped()?);
                    continue;
                }
                _ => {
                    s.push(c);
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
                b'\\' => s.push(self.escaped()?),
                b'"' => return Ok(Token::Str(s)),
                _ => s.push(c),
            }
        }
        Err(fatal!("broken quoted string."))
    }
}

impl Iterator for Lexer<'_> {
    type Item = Result<Token>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_token().transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(p: &[u8]) -> Vec<Token> {
        Lexer::new(p).collect::<Result<_>>().unwrap()
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
}
