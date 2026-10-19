//! Parser of pick patterns.
//!
//! This follows pattern.c of mewl: tokens are put in a doubly linked
//! list which is rewritten step by step (comparisons, parentheses,
//! `!`, `&`, and then `|`). It is kept as is, instead of a usual
//! recursive descent parser, so that broken patterns are rejected or
//! accepted in the same way. The nodes live in an arena and links are
//! indices. The result is converted to [`Expr`].

use crate::error::{Result, fatal};

use super::lexer::{Lexer, Token};
use super::{Cmp, Expr};

type Link = Option<usize>;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// An abstracted expression whose content is `down`.
    Exp,
    Str,
    /// A comparison before or after `exp_eq` binds its operands.
    Cmp(Cmp),
    And,
    Or,
    Not,
    LParen,
    RParen,
}

struct Node {
    kind: Kind,
    prev: Link,
    next: Link,
    down: Link,
    /// The string of [`Kind::Str`] or the key of [`Kind::Cmp`].
    key: Vec<u8>,
    /// The value of [`Kind::Cmp`].
    value: Vec<u8>,
}

#[derive(Default)]
struct Parser {
    nodes: Vec<Node>,
}

pub fn parse(pattern: &[u8]) -> Result<Expr> {
    let mut parser = Parser::default();
    let mut top = None;
    let mut last: Link = None;
    for token in Lexer::new(pattern) {
        let p = parser.push_token(token?);
        match last {
            None => top = Some(p),
            Some(q) => parser.link(q, p),
        }
        last = Some(p);
    }

    parser.bind_comparisons(top)?;
    let top = parser.expression(top)?;
    Ok(parser.to_expr(Some(top)))
}

impl Parser {
    fn push(&mut self, kind: Kind) -> usize {
        self.nodes.push(Node {
            kind,
            prev: None,
            next: None,
            down: None,
            key: Vec::new(),
            value: Vec::new(),
        });
        self.nodes.len() - 1
    }

    fn push_token(&mut self, token: Token) -> usize {
        let kind = match token {
            Token::Str(s) => {
                let p = self.push(Kind::Str);
                self.nodes[p].key = s;
                return p;
            }
            Token::Cmp(c) => Kind::Cmp(c),
            Token::And => Kind::And,
            Token::Or => Kind::Or,
            Token::Not => Kind::Not,
            Token::LParen => Kind::LParen,
            Token::RParen => Kind::RParen,
        };
        self.push(kind)
    }

    fn link(&mut self, prev: usize, next: usize) {
        self.nodes[prev].next = Some(next);
        self.nodes[next].prev = Some(prev);
    }

    fn kind(&self, link: Link) -> Option<Kind> {
        link.map(|i| self.nodes[i].kind)
    }

    /*
     * Both key = val and key != val is the most strong.
     * This is called just once as pre-process before expression().
     *
     *  Str Cmp Str  =>  Exp
     *  key     val       v
     *                   Cmp
     *                 key val
     */
    fn bind_comparisons(&mut self, top: Link) -> Result<()> {
        let mut q = top;
        while let Some(qi) = q {
            let Kind::Cmp(cmp) = self.nodes[qi].kind else {
                q = self.nodes[qi].next;
                continue;
            };
            let (p, r) = (self.nodes[qi].prev, self.nodes[qi].next);
            if self.kind(p) != Some(Kind::Str) || self.kind(r) != Some(Kind::Str) {
                return Err(fatal!("broken eq/neq."));
            }
            let (p, r) = (p.expect("checked"), r.expect("checked"));
            let n = self.push(Kind::Cmp(cmp));
            self.nodes[n].key = std::mem::take(&mut self.nodes[p].key);
            self.nodes[n].value = self.nodes[r].key.clone();
            self.nodes[p].kind = Kind::Exp;
            self.nodes[p].down = Some(n);
            q = self.nodes[r].next;
            if let Some(qn) = q {
                self.nodes[qn].prev = Some(p);
            }
            self.nodes[p].next = q;
        }
        Ok(())
    }

    /// Creates an expression tree.
    fn expression(&mut self, top: Link) -> Result<usize> {
        let Some(top) = top else {
            return Err(fatal!("invalid pattern."));
        };
        let top = self.parentheses(top)?;
        let top = self.negations(top)?;
        let top = self.binary(top, Kind::And)?;
        self.binary(top, Kind::Or)
    }

    /*
     * Making parentheses to one Exp.
     *
     * X ( Y Z ) Z => X Exp Z
     *                   v
     *                   Y Z   (parsed by expression())
     */
    fn parentheses(&mut self, top: usize) -> Result<usize> {
        let mut top = top;
        let mut open: Link = None;
        let mut q: Link = Some(top);
        let mut level = 0;
        while let Some(qi) = q {
            match self.nodes[qi].kind {
                Kind::LParen => {
                    if level == 0 {
                        open = Some(qi);
                    }
                    level += 1;
                }
                Kind::RParen => {
                    level -= 1;
                    if level < 0 {
                        return Err(fatal!("too many ')'."));
                    }
                    if level == 0 {
                        let p = open.expect("'(' precedes");
                        if self.nodes[p].next == Some(qi) {
                            return Err(fatal!("empty '()'."));
                        }
                        let inner = self.nodes[p].next.expect("not empty");
                        self.nodes[inner].prev = None;
                        let before_close = self.nodes[qi].prev.expect("not empty");
                        self.nodes[before_close].next = None;
                        let n = self.expression(Some(inner))?;
                        match self.nodes[p].prev {
                            None => top = n,
                            Some(pp) => self.link(pp, n),
                        }
                        if let Some(qn) = self.nodes[qi].next {
                            self.link(n, qn);
                        }
                        q = self.nodes[n].next;
                        continue;
                    }
                }
                _ => {}
            }
            q = self.nodes[qi].next;
        }

        if level > 0 {
            return Err(fatal!("too many '('."));
        }
        Ok(top)
    }

    /*
     * Binding ! to the next one.
     *
     *  ! Exp  =>  Exp'
     *              v
     *              !
     *              v
     *             Exp
     */
    fn negations(&mut self, top: usize) -> Result<usize> {
        let mut p: Link = Some(top);
        while let Some(pi) = p {
            if self.nodes[pi].kind != Kind::Not {
                p = self.nodes[pi].next;
                continue;
            }
            let q = self.nodes[pi].next;
            if self.kind(q) != Some(Kind::Exp) {
                return Err(fatal!("broken not."));
            }
            let q = q.expect("checked");
            let not = self.push(Kind::Not);
            let exp = self.push(Kind::Exp);
            self.nodes[pi].down = Some(not);
            self.nodes[not].down = Some(exp);
            self.nodes[exp].down = self.nodes[q].down;
            self.nodes[pi].kind = Kind::Exp;
            let after = self.nodes[q].next;
            if let Some(a) = after {
                self.nodes[a].prev = Some(pi);
            }
            self.nodes[pi].next = after;
            p = after;
        }
        Ok(top)
    }

    /* Binding & or | to the previous one and the next one.
     * This is processed backward from the tail so that the result
     * can be evaluated naturally.
     *
     * Exp1 &a Exp2 &b Exp3
     *
     *         v
     *
     * Exp5
     *  v
     * &a Exp1 Exp4
     *          v
     *         &b Exp2 Exp3
     */
    fn binary(&mut self, top: usize, op: Kind) -> Result<usize> {
        let mut last = top;
        let mut found = false;
        let mut p = Some(top);
        while let Some(pi) = p {
            found |= self.nodes[pi].kind == op;
            last = pi;
            p = self.nodes[pi].next;
        }
        if !found {
            return Ok(top);
        }

        let mut top = top;
        let mut q = Some(last);
        while let Some(qi) = q {
            if self.nodes[qi].kind != op {
                top = qi;
                q = self.nodes[qi].prev;
                continue;
            }
            let (p, r) = (self.nodes[qi].prev, self.nodes[qi].next);
            if self.kind(p) != Some(Kind::Exp) || self.kind(r) != Some(Kind::Exp) {
                return Err(fatal!("broken and/or."));
            }
            let (p, r) = (p.expect("checked"), r.expect("checked"));
            let n = self.push(Kind::Exp);
            self.nodes[n].prev = self.nodes[p].prev;
            self.nodes[n].next = self.nodes[r].next;
            self.nodes[n].down = Some(p);
            if let Some(pp) = self.nodes[p].prev {
                self.nodes[pp].next = Some(n);
            }
            if let Some(rn) = self.nodes[r].next {
                self.nodes[rn].prev = Some(n);
            }
            self.nodes[p].prev = None;
            self.nodes[r].next = None;
            /* swap p and q: p becomes the operator. */
            self.nodes[qi].kind = Kind::Exp;
            self.nodes[qi].down = self.nodes[p].down;
            self.nodes[p].kind = op;
            self.nodes[p].down = None;
            top = n;
            q = self.nodes[n].prev;
        }
        Ok(top)
    }

    /// Converts the list to a tree following the links in the same way
    /// as `eval_exp()` of mewl. What it would refuse to evaluate
    /// becomes [`Expr::Broken`].
    fn to_expr(&self, link: Link) -> Expr {
        let Some(i) = link else {
            return Expr::Broken;
        };
        let node = &self.nodes[i];
        let operands = || {
            let second = node.next.and_then(|n| self.nodes[n].next);
            (Box::new(self.to_expr(node.next)), Box::new(self.to_expr(second)))
        };
        match node.kind {
            Kind::Exp => self.to_expr(node.down),
            Kind::Not => Expr::Not(Box::new(self.to_expr(node.down))),
            Kind::And => {
                let (a, b) = operands();
                Expr::And(a, b)
            }
            Kind::Or => {
                let (a, b) = operands();
                Expr::Or(a, b)
            }
            Kind::Cmp(cmp) => Expr::Cmp { cmp, key: node.key.clone(), value: node.value.clone() },
            Kind::Str | Kind::LParen | Kind::RParen => Expr::Broken,
        }
    }
}
