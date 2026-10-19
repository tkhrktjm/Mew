/*
 * pattern.rs -- counterpart of pattern.c
 *
 * Author:  Mew developing team
 *
 * Code:
 *
 * The parser keeps the doubly linked list manipulation of pattern.c
 * as is so that broken patterns are treated in the same way.
 * Nodes live in an arena and pointers are indices.
 */

use crate::util::*;

/****************************************************************
 *
 * Macro, structure, and tables
 *
 */

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PType {
    Nul, /* terminator */
    Exp, /* abstracted expression */
    Str, /* str, "str", "\"" */
    Eq,  /* =   case-insensitive */
    Ceq, /* ==  case-sensitive */
    Neq, /* !=  case-insensitive */
    Cne, /* !== case-sensitive */
    And, /* &, && */
    Or,  /* |, || */
    Not, /* ! */
    Lsq, /* ( */
    Rsq, /* ) */
}

type Link = Option<usize>;

struct Pat {
    typ: PType,
    prv: Link,           /* Doubly linked list */
    nxt: Link,           /* Doubly linked list */
    dwn: Link,           /* For P_EXP and P_NOT */
    key: Option<Vec<u8>>, /* For P_STR, P_EQ, P_NEQ */
    val: Option<Vec<u8>>, /* P_EQ, P_NEQ */
}

pub struct Pattern {
    nodes: Vec<Pat>,
    top: usize,
}

struct Parser {
    nodes: Vec<Pat>,
    pvec: Vec<u8>,
    ppos: usize,
}

impl Parser {
    /****************************************************************
     *
     * Input
     *
     */

    fn getc(&mut self) -> i32 {
        if self.ppos < self.pvec.len() {
            /* char may be signed in C. */
            let c = self.pvec[self.ppos] as std::ffi::c_char as i32;
            self.ppos += 1;
            c
        } else {
            EOF
        }
        /* Getc() is to be called even after Getc() returned EOF. */
    }

    fn ungetc(&mut self) {
        if self.ppos > 0 {
            self.ppos -= 1;
        } else {
            warn_exit(b"ungetc is impossible.");
        }
    }

    /****************************************************************
     *
     * Lexical analyzer
     *
     */

    fn token(&mut self) -> (PType, Option<Vec<u8>>) {
        let mut typ = PType::Nul;
        let mut keyval: Vec<u8> = Vec::new();
        let mut quoted = false;
        const C_SP: i32 = SP as i32;
        const C_TAB: i32 = TAB as i32;
        const C_AND: i32 = b'&' as i32;
        const C_OR: i32 = b'|' as i32;
        const C_NOT: i32 = b'!' as i32;
        const C_EQ: i32 = b'=' as i32;
        const C_LSQ: i32 = b'(' as i32;
        const C_RSQ: i32 = b')' as i32;
        const C_DQ: i32 = b'"' as i32;
        const C_BS: i32 = b'\\' as i32;

        loop {
            let c = self.getc();
            if c == EOF {
                break;
            }
            match c {
                C_SP | C_TAB => continue,
                C_AND => typ = PType::And,
                C_OR => typ = PType::Or,
                C_NOT => typ = PType::Not,
                C_EQ => typ = PType::Eq,
                C_LSQ => return (PType::Lsq, None),
                C_RSQ => return (PType::Rsq, None),
                C_DQ => {
                    typ = PType::Str;
                    quoted = true;
                }
                C_BS => {
                    let c = self.getc();
                    if c == EOF {
                        warn_exit(b"broken escape.");
                    }
                    typ = PType::Str;
                    keyval.push(c as u8);
                }
                _ => {
                    typ = PType::Str;
                    keyval.push(c as u8);
                }
            }
            break;
        }

        if quoted {
            loop {
                let c = self.getc();
                if c == EOF {
                    break;
                }
                match c {
                    C_BS => {
                        let c = self.getc();
                        if c == EOF {
                            warn_exit(b"broken escape.");
                        }
                        keyval.push(c as u8);
                    }
                    C_DQ => return (typ, Some(keyval)), /* allocstr */
                    _ => keyval.push(c as u8),
                }
            }
            warn_exit(b"broken quoted string.");
        }

        loop {
            let mut c = self.getc();
            if c == EOF {
                break;
            }
            match c {
                C_SP | C_TAB => {}
                C_AND => {
                    if typ != PType::And {
                        self.ungetc();
                    }
                }
                C_OR => {
                    if typ != PType::Or {
                        self.ungetc();
                    }
                }
                C_NOT => self.ungetc(),
                C_EQ => {
                    if typ == PType::Eq {
                        typ = PType::Ceq;
                    } else if typ == PType::Not {
                        if self.getc() == C_EQ {
                            typ = PType::Cne;
                        } else {
                            typ = PType::Neq;
                            self.ungetc();
                        }
                    } else {
                        self.ungetc();
                    }
                }
                C_LSQ | C_RSQ => self.ungetc(),
                _ => {
                    if typ != PType::Str {
                        self.ungetc();
                    } else {
                        if c == C_BS {
                            c = self.getc();
                            if c == EOF {
                                warn_exit(b"broken escape.");
                            }
                        }
                        keyval.push(c as u8);
                        continue;
                    }
                }
            }
            break;
        }

        if typ == PType::Str {
            (typ, Some(keyval))
        } else {
            (typ, None)
        }
    }

    /****************************************************************
     *
     * Utilities for parser
     *
     */

    fn get_pat(&mut self, typ: PType) -> usize {
        self.nodes.push(Pat {
            typ,
            prv: None,
            nxt: None,
            dwn: None,
            key: None,
            val: None,
        });
        self.nodes.len() - 1
    }

    fn n(&mut self, i: usize) -> &mut Pat {
        &mut self.nodes[i]
    }

    /****************************************************************
     *
     * Parser
     *
     */

    /*
     * Both key = val and key != val is the most strong.
     * This function assumes to be called just once as pre-process
     * before exp_exp().
     *
     *  P_STR P_EQ P_STR  =>  P_EXP
     *   key   val              v
     *                        P_EQ
     *                       key val
     */
    fn exp_eq(&mut self, top: Link) -> Link {
        let mut q = top;
        while let Some(qi) = q {
            let t = self.nodes[qi].typ;
            if t == PType::Eq || t == PType::Ceq || t == PType::Neq || t == PType::Cne {
                let (p, r) = match (self.nodes[qi].prv, self.nodes[qi].nxt) {
                    (Some(p), Some(r)) => (p, r),
                    _ => warn_exit(b"broken eq/neq."),
                };
                if self.nodes[p].typ != PType::Str || self.nodes[r].typ != PType::Str {
                    warn_exit(b"broken eq/neq.");
                }
                let n = self.get_pat(t);
                let key = self.n(p).key.take();
                let val = self.n(r).key.clone();
                self.n(n).key = key;
                self.n(n).val = val;
                self.n(p).typ = PType::Exp;
                self.n(p).dwn = Some(n);
                q = self.nodes[r].nxt;
                if let Some(qn) = q {
                    self.n(qn).prv = Some(p);
                }
                self.n(p).nxt = q;
            } else {
                q = self.nodes[qi].nxt;
            }
        }
        top
    }

    /*
     * Making square bracket to one P_EXP.
     *
     * X P_LSQ Y Z P_RSQ Z => X P_EXP Z
     *                            v
     *                            Y Z   (will be parsed by exp_exp())
     */
    fn exp_sq(&mut self, top: usize) -> usize {
        let mut top = top;
        let mut p: Link = None;
        let mut q: Link = Some(top);
        let mut level = 0;
        while let Some(qi) = q {
            match self.nodes[qi].typ {
                PType::Lsq => {
                    if level == 0 {
                        p = Some(qi);
                    }
                    level += 1;
                }
                PType::Rsq => {
                    level -= 1;
                    if level < 0 {
                        warn_exit(b"too many ')'.");
                    }
                    if level == 0 {
                        let pi = p.unwrap();
                        if self.nodes[pi].nxt == Some(qi) {
                            warn_exit(b"empty '()'.");
                        }
                        let n = self.nodes[pi].nxt.unwrap();
                        self.n(n).prv = None;
                        let qp = self.nodes[qi].prv.unwrap();
                        self.n(qp).nxt = None;
                        let n = self.exp_exp(Some(n));
                        match self.nodes[pi].prv {
                            None => top = n,
                            Some(pp) => {
                                self.n(pp).nxt = Some(n);
                                self.n(n).prv = Some(pp);
                            }
                        }
                        if let Some(qn) = self.nodes[qi].nxt {
                            self.n(qn).prv = Some(n);
                            self.n(n).nxt = Some(qn);
                        }
                        q = self.nodes[n].nxt;
                        continue;
                    }
                }
                _ => {}
            }
            q = self.nodes[qi].nxt;
        }

        if level > 0 {
            warn_exit(b"too many '('.");
        }
        top
    }

    /*
     * Binding logical not to the next one
     *
     *  P_NOT P_EXP  =>  P_EXP'
     *                     v
     *                   P_NOT
     *                     v
     *                   P_EXP
     * P_EXP is a result of exp_sq() or exp_eq().
     */
    fn exp_not(&mut self, top: usize) -> usize {
        let mut p: Link = Some(top);
        while let Some(pi) = p {
            if self.nodes[pi].typ == PType::Not {
                let q = match self.nodes[pi].nxt {
                    Some(q) => q,
                    None => warn_exit(b"broken not."),
                };
                if self.nodes[q].typ != PType::Exp {
                    warn_exit(b"broken not.");
                }
                let n = self.get_pat(PType::Not);
                let m = self.get_pat(PType::Exp);
                self.n(pi).dwn = Some(n);
                self.n(n).dwn = Some(m);
                self.n(m).dwn = self.nodes[q].dwn;
                self.n(pi).typ = PType::Exp;
                let q = self.nodes[q].nxt;
                if let Some(qi) = q {
                    self.n(qi).prv = Some(pi);
                }
                self.n(pi).nxt = q;
                p = q;
            } else {
                p = self.nodes[pi].nxt;
            }
        }
        top
    }

    /* Binding logical and/or to the previous one and the next one.
     * This is processed backward from the tail so that the result
     * P_EXP can be evaluated naturally.
     *
     * P_EXP1 P_ANDa P_EXP2 P_ANDb P_EXP3
     *
     *              v
     *              v
     *
     * P_EXP5
     *  v
     * P_ANDa P_EXP1 P_EXP4
     *                v
     *               P_ANDb P_EXP2 P_EXP3
     */
    fn exp_lao(&mut self, top: usize, lao: PType) -> usize {
        let mut top = top;
        let mut count = 0;
        let mut p: Link = Some(top);
        let mut last = top;
        while let Some(pi) = p {
            if self.nodes[pi].typ == lao {
                count += 1;
            }
            last = pi;
            p = self.nodes[pi].nxt;
        }
        if count == 0 {
            return top;
        }
        let mut q: Link = Some(last);
        while let Some(qi) = q {
            if self.nodes[qi].typ == lao {
                let (p, r) = match (self.nodes[qi].prv, self.nodes[qi].nxt) {
                    (Some(p), Some(r)) => (p, r),
                    _ => warn_exit(b"broken and/or."),
                };
                if self.nodes[p].typ != PType::Exp || self.nodes[r].typ != PType::Exp {
                    warn_exit(b"broken and/or.");
                }
                let n = self.get_pat(PType::Exp);
                self.n(n).prv = self.nodes[p].prv;
                self.n(n).nxt = self.nodes[r].nxt;
                self.n(n).dwn = Some(p);
                if let Some(pp) = self.nodes[p].prv {
                    self.n(pp).nxt = Some(n);
                }
                if let Some(rn) = self.nodes[r].nxt {
                    self.n(rn).prv = Some(n);
                }
                self.n(p).prv = None;
                self.n(r).nxt = None;
                /* swap p and q */
                self.n(qi).typ = PType::Exp;
                self.n(qi).dwn = self.nodes[p].dwn;
                self.n(p).typ = lao;
                self.n(p).dwn = None;
                top = n;
                q = self.nodes[n].prv;
            } else {
                top = qi;
                q = self.nodes[qi].prv;
            }
        }
        top
    }

    /*
     * Creating an expression tree.
     *
     */
    fn exp_exp(&mut self, top: Link) -> usize {
        let top = match top {
            Some(t) => t,
            None => warn_exit(b"invalid pattern."),
        };
        let top = self.exp_sq(top);
        let top = self.exp_not(top);
        let top = self.exp_lao(top, PType::And);
        self.exp_lao(top, PType::Or)
    }

    /****************************************************************
     *
     * Starter of the parser
     *
     */

    fn parse_pattern(&mut self) -> usize {
        let mut top: Link = None;
        let mut q: Link = None;
        loop {
            let (typ, s) = self.token();
            if typ == PType::Nul {
                break;
            }
            let p = self.get_pat(typ);
            if typ == PType::Str {
                self.n(p).key = s;
            }
            match q {
                None => top = Some(p),
                Some(qi) => {
                    self.n(qi).nxt = Some(p);
                    self.n(p).prv = Some(qi);
                }
            }
            q = Some(p);
        }

        let top = self.exp_eq(top);
        self.exp_exp(top)
    }
}

/****************************************************************
 *
 * External interface
 *
 */

impl Pattern {
    pub fn new(pattern: &[u8]) -> Pattern {
        let mut parser = Parser {
            nodes: Vec::new(),
            /* Plim = strlen(Pvec) */
            pvec: pattern.split(|&c| c == NUL).next().unwrap_or(&[]).to_vec(),
            ppos: 0,
        };
        let top = parser.parse_pattern();
        Pattern {
            nodes: parser.nodes,
            top,
        }
    }

    /* search(key, value, case_sensitive) is search_string() of mewl. */
    pub fn matches<F>(&self, search: &mut F) -> bool
    where
        F: FnMut(&[u8], &[u8], bool) -> bool,
    {
        self.eval_exp(Some(self.top), search)
    }

    /****************************************************************
     *
     * Evaluator of an expression tree
     *
     */

    fn eval_exp<F>(&self, p: Link, search: &mut F) -> bool
    where
        F: FnMut(&[u8], &[u8], bool) -> bool,
    {
        let p = match p {
            Some(p) => &self.nodes[p],
            None => warn_exit(b"broken tree."),
        };
        let nxt2 = |p: &Pat| p.nxt.and_then(|n| self.nodes[n].nxt);
        let key = || p.key.as_deref().unwrap_or(b"");
        let val = || p.val.as_deref().unwrap_or(b"");
        match p.typ {
            PType::Not => !self.eval_exp(p.dwn, search),
            PType::And => {
                if !self.eval_exp(p.nxt, search) {
                    return false;
                }
                self.eval_exp(nxt2(p), search)
            }
            PType::Or => {
                if self.eval_exp(p.nxt, search) {
                    return true;
                }
                self.eval_exp(nxt2(p), search)
            }
            PType::Eq => search(key(), val(), false),
            PType::Ceq => search(key(), val(), true),
            PType::Neq => !search(key(), val(), false),
            PType::Cne => !search(key(), val(), true),
            PType::Exp => self.eval_exp(p.dwn, search),
            _ => warn_exit(b"broken tree."),
        }
    }
}

/*
 * Copyright (C) 2000-2005 Mew developing team.
 * All rights reserved.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions
 * are met:
 *
 * 1. Redistributions of source code must retain the above copyright
 *    notice, this list of conditions and the following disclaimer.
 * 2. Redistributions in binary form must reproduce the above copyright
 *    notice, this list of conditions and the following disclaimer in the
 *    documentation and/or other materials provided with the distribution.
 * 3. Neither the name of the team nor the names of its contributors
 *    may be used to endorse or promote products derived from this software
 *    without specific prior written permission.
 *
 * THIS SOFTWARE IS PROVIDED BY THE TEAM AND CONTRIBUTORS ``AS IS'' AND
 * ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
 * IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR
 * PURPOSE ARE DISCLAIMED.  IN NO EVENT SHALL THE TEAM OR CONTRIBUTORS BE
 * LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
 * CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
 * SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR
 * BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
 * WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE
 * OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN
 * IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
 */

/*
 * pattern.rs ends here
 */
