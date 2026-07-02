#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 Thomas Johannesson
#
# Build a throwaway git repository used to demo git-tailor. It creates a `main`
# branch with just a README (the point the feature branch is forked from), then
# a `feature/calc` branch that carries a small "calc" interpreter split across
# several commits. The history is arranged so the demo can show off every core
# git-tailor operation:
#
#   * a rich hunk-group matrix        — commits re-touch each other's regions
#   * a clean fixup (yellow)          — #5 into #1, #6 into #2
#   * a DROPME commit with junk       — #7 injects debug tracing
#   * a rebase conflict (red)         — dropping #7 clashes with #8 (same lines)
#   * a splittable commit             — #9 bundles two unrelated concerns
#
# Usage:
#   make-demo-repo.sh [OUTPUT_DIR]
#
# With no argument a fresh temp dir is created and its path printed. If
# OUTPUT_DIR is given it must not exist or be empty (set FORCE=1 to wipe it).
set -euo pipefail

# ---------------------------------------------------------------------------
# Output location
# ---------------------------------------------------------------------------
OUT="${1:-}"
if [[ -z "$OUT" ]]; then
    OUT="$(mktemp -d -t git-tailor-demo-XXXXXX)"
elif [[ -e "$OUT" ]]; then
    if [[ -n "$(ls -A "$OUT" 2>/dev/null)" && "${FORCE:-0}" != "1" ]]; then
        echo "error: '$OUT' exists and is not empty (set FORCE=1 to overwrite)" >&2
        exit 1
    fi
    rm -rf "${OUT:?}"
    mkdir -p "$OUT"
else
    mkdir -p "$OUT"
fi

# ---------------------------------------------------------------------------
# Deterministic identity so recordings are reproducible run to run.
# ---------------------------------------------------------------------------
BASE_TS=1700000000
AUTHOR="Ada Lovelace"
EMAIL="ada@example.com"
BRANCH="feature/calc"
SEQ=0

git -C "$OUT" init -q -b main
git -C "$OUT" config user.name "$AUTHOR"
git -C "$OUT" config user.email "$EMAIL"

# write REL  (file contents come from stdin)
write() {
    local full="$OUT/$1"
    mkdir -p "$(dirname "$full")"
    cat >"$full"
}

# commit "message"  — stages everything and commits with a stepped timestamp.
commit() {
    local ts=$((BASE_TS + SEQ * 60))
    git -C "$OUT" add -A
    GIT_AUTHOR_DATE="$ts +0000" GIT_COMMITTER_DATE="$ts +0000" \
        git -C "$OUT" commit -q -m "$1"
    SEQ=$((SEQ + 1))
}

# ===========================================================================
# main: the fork point — just a README, as if the feature branch was cut here.
# ===========================================================================
write README.md <<'MD'
# calc

`calc` is a tiny arithmetic expression calculator, used as a sample project for
demonstrating [git-tailor](https://github.com/jordfras/git-tailor).

It reads one expression per line from standard input, evaluates it, and prints
the result:

```text
calc> 1 + 2 * 3
7
calc> (1 + 2) * 3
9
```

## Grammar

```text
expression := term (('+' | '-') term)*
term       := factor (('*' | '/') factor)*
factor     := NUMBER | '(' expression ')'
```

## Status

Early work in progress. This branch collects the initial implementation before
it is merged into `main`.
MD
commit "docs: add project README"

git -C "$OUT" checkout -q -b "$BRANCH"

# ===========================================================================
# 1. feat: add token lexer
# ===========================================================================
write src/lexer.rs <<'RS'
//! Lexer: turns source text into a stream of tokens.

/// A single lexical token produced from the input source.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Number(f64),
    Plus,
    Minus,
    Star,
    Slash,
}

/// Convert a source string into a vector of tokens.
///
/// Whitespace is skipped. Any character that is not part of a recognised token
/// is reported as an error together with its byte offset.
pub fn tokenize(src: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut chars = src.char_indices().peekable();

    while let Some(&(i, c)) = chars.peek() {
        match c {
            ' ' | '\t' | '\n' | '\r' => {
                chars.next();
            }
            '+' => {
                tokens.push(Token::Plus);
                chars.next();
            }
            '-' => {
                tokens.push(Token::Minus);
                chars.next();
            }
            '*' => {
                tokens.push(Token::Star);
                chars.next();
            }
            '/' => {
                tokens.push(Token::Slash);
                chars.next();
            }
            '0'..='9' | '.' => {
                let mut text = String::new();
                while let Some(&(_, d)) = chars.peek() {
                    if d.is_ascii_digit() || d == '.' {
                        text.push(d);
                        chars.next();
                    } else {
                        break;
                    }
                }
                let value: f64 = text
                    .parse()
                    .map_err(|_| format!("invalid number '{text}' at byte {i}"))?;
                tokens.push(Token::Number(value));
            }
            other => return Err(format!("unexpected character '{other}' at byte {i}")),
        }
    }

    Ok(tokens)
}
RS
commit "feat: add token lexer"

# ===========================================================================
# 2. feat: add recursive-descent parser
# ===========================================================================
write src/parser.rs <<'RS'
//! Parser: builds an expression tree from a token stream.

use crate::lexer::Token;

/// An arithmetic expression tree.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Number(f64),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
}

/// A recursive-descent parser over a slice of tokens.
pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Parser { tokens, pos: 0 }
    }

    /// Parse a full expression and ensure all input was consumed.
    pub fn parse(&mut self) -> Result<Expr, String> {
        let expr = self.expression()?;
        if self.pos != self.tokens.len() {
            return Err(format!("unexpected trailing tokens at position {}", self.pos));
        }
        Ok(expr)
    }

    /// expression := term (('+' | '-') term)*
    fn expression(&mut self) -> Result<Expr, String> {
        let mut node = self.term()?;
        while let Some(tok) = self.peek() {
            match tok {
                Token::Plus => {
                    self.pos += 1;
                    node = Expr::Add(Box::new(node), Box::new(self.term()?));
                }
                Token::Minus => {
                    self.pos += 1;
                    node = Expr::Sub(Box::new(node), Box::new(self.term()?));
                }
                _ => break,
            }
        }
        Ok(node)
    }

    /// term := factor (('*' | '/') factor)*
    fn term(&mut self) -> Result<Expr, String> {
        let mut node = self.factor()?;
        while let Some(tok) = self.peek() {
            match tok {
                Token::Star => {
                    self.pos += 1;
                    node = Expr::Mul(Box::new(node), Box::new(self.factor()?));
                }
                Token::Slash => {
                    self.pos += 1;
                    node = Expr::Div(Box::new(node), Box::new(self.factor()?));
                }
                _ => break,
            }
        }
        Ok(node)
    }

    /// factor := NUMBER
    fn factor(&mut self) -> Result<Expr, String> {
        match self.peek() {
            Some(Token::Number(n)) => {
                let n = *n;
                self.pos += 1;
                Ok(Expr::Number(n))
            }
            other => Err(format!("expected a number, found {other:?}")),
        }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }
}
RS
commit "feat: add recursive-descent parser"

# ===========================================================================
# 3. feat: add tree-walking evaluator
# ===========================================================================
write src/eval.rs <<'RS'
//! Evaluator: walks an expression tree and computes its value.

use crate::parser::Expr;

/// Recursively evaluate an expression tree to a single number.
pub fn eval(expr: &Expr) -> f64 {
    match expr {
        Expr::Number(n) => *n,
        Expr::Add(a, b) => eval(a) + eval(b),
        Expr::Sub(a, b) => eval(a) - eval(b),
        Expr::Mul(a, b) => eval(a) * eval(b),
        Expr::Div(a, b) => eval(a) / eval(b),
    }
}
RS
commit "feat: add tree-walking evaluator"

# ===========================================================================
# 4. feat: wire up the REPL loop
# ===========================================================================
write Cargo.toml <<'TOML'
[package]
name = "calc"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "calc"
path = "src/main.rs"
TOML
write src/main.rs <<'RS'
//! A tiny command-line calculator REPL.

mod eval;
mod lexer;
mod parser;

use std::io::{self, Write};

use crate::eval::eval;
use crate::lexer::tokenize;
use crate::parser::Parser;

fn main() {
    let stdin = io::stdin();
    let mut line = String::new();

    loop {
        print!("calc> ");
        io::stdout().flush().expect("failed to flush stdout");

        line.clear();
        let bytes = stdin.read_line(&mut line).expect("failed to read line");
        if bytes == 0 {
            break; // EOF
        }

        let source = line.trim();
        if source.is_empty() {
            continue;
        }

        match run(source) {
            Ok(value) => println!("{value}"),
            Err(err) => eprintln!("error: {err}"),
        }
    }
}

/// Tokenize, parse and evaluate a single line of input.
fn run(source: &str) -> Result<f64, String> {
    let tokens = tokenize(source)?;
    let mut parser = Parser::new(tokens);
    let expr = parser.parse()?;
    Ok(eval(&expr))
}
RS
commit "feat: wire up the REPL loop"

# ===========================================================================
# 5. feat: support parentheses in the lexer
#    Re-touches ONLY the lexer's token enum + match (owned by commit #1), so it
#    folds cleanly into "add token lexer" — a yellow squash partner.
# ===========================================================================
write src/lexer.rs <<'RS'
//! Lexer: turns source text into a stream of tokens.

/// A single lexical token produced from the input source.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Number(f64),
    Plus,
    Minus,
    Star,
    Slash,
    LParen,
    RParen,
}

/// Convert a source string into a vector of tokens.
///
/// Whitespace is skipped. Any character that is not part of a recognised token
/// is reported as an error together with its byte offset.
pub fn tokenize(src: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut chars = src.char_indices().peekable();

    while let Some(&(i, c)) = chars.peek() {
        match c {
            ' ' | '\t' | '\n' | '\r' => {
                chars.next();
            }
            '+' => {
                tokens.push(Token::Plus);
                chars.next();
            }
            '-' => {
                tokens.push(Token::Minus);
                chars.next();
            }
            '*' => {
                tokens.push(Token::Star);
                chars.next();
            }
            '/' => {
                tokens.push(Token::Slash);
                chars.next();
            }
            '(' => {
                tokens.push(Token::LParen);
                chars.next();
            }
            ')' => {
                tokens.push(Token::RParen);
                chars.next();
            }
            '0'..='9' | '.' => {
                let mut text = String::new();
                while let Some(&(_, d)) = chars.peek() {
                    if d.is_ascii_digit() || d == '.' {
                        text.push(d);
                        chars.next();
                    } else {
                        break;
                    }
                }
                let value: f64 = text
                    .parse()
                    .map_err(|_| format!("invalid number '{text}' at byte {i}"))?;
                tokens.push(Token::Number(value));
            }
            other => return Err(format!("unexpected character '{other}' at byte {i}")),
        }
    }

    Ok(tokens)
}
RS
commit "feat: support parentheses in the lexer"

# ===========================================================================
# 6. fix: parse parenthesised expressions
#    Re-touches ONLY the parser's factor() (owned by commit #2) — a clean,
#    yellow squash partner for "add recursive-descent parser".
# ===========================================================================
write src/parser.rs <<'RS'
//! Parser: builds an expression tree from a token stream.

use crate::lexer::Token;

/// An arithmetic expression tree.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Number(f64),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
}

/// A recursive-descent parser over a slice of tokens.
pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Parser { tokens, pos: 0 }
    }

    /// Parse a full expression and ensure all input was consumed.
    pub fn parse(&mut self) -> Result<Expr, String> {
        let expr = self.expression()?;
        if self.pos != self.tokens.len() {
            return Err(format!("unexpected trailing tokens at position {}", self.pos));
        }
        Ok(expr)
    }

    /// expression := term (('+' | '-') term)*
    fn expression(&mut self) -> Result<Expr, String> {
        let mut node = self.term()?;
        while let Some(tok) = self.peek() {
            match tok {
                Token::Plus => {
                    self.pos += 1;
                    node = Expr::Add(Box::new(node), Box::new(self.term()?));
                }
                Token::Minus => {
                    self.pos += 1;
                    node = Expr::Sub(Box::new(node), Box::new(self.term()?));
                }
                _ => break,
            }
        }
        Ok(node)
    }

    /// term := factor (('*' | '/') factor)*
    fn term(&mut self) -> Result<Expr, String> {
        let mut node = self.factor()?;
        while let Some(tok) = self.peek() {
            match tok {
                Token::Star => {
                    self.pos += 1;
                    node = Expr::Mul(Box::new(node), Box::new(self.factor()?));
                }
                Token::Slash => {
                    self.pos += 1;
                    node = Expr::Div(Box::new(node), Box::new(self.factor()?));
                }
                _ => break,
            }
        }
        Ok(node)
    }

    /// factor := NUMBER | '(' expression ')'
    fn factor(&mut self) -> Result<Expr, String> {
        match self.peek() {
            Some(Token::Number(n)) => {
                let n = *n;
                self.pos += 1;
                Ok(Expr::Number(n))
            }
            Some(Token::LParen) => {
                self.pos += 1;
                let inner = self.expression()?;
                match self.peek() {
                    Some(Token::RParen) => {
                        self.pos += 1;
                        Ok(inner)
                    }
                    other => Err(format!("expected ')', found {other:?}")),
                }
            }
            other => Err(format!("expected a number, found {other:?}")),
        }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }
}
RS
commit "fix: parse parenthesised expressions"

# ===========================================================================
# 7. DROPME: hack in debug tracing
#    Debug junk that should never be merged. It rewrites the Div arm of eval()
#    — the SAME lines commit #8 rewrites — so dropping this commit forces a
#    rebase conflict that the demo then resolves.
# ===========================================================================
write src/eval.rs <<'RS'
//! Evaluator: walks an expression tree and computes its value.

use crate::parser::Expr;

/// Recursively evaluate an expression tree to a single number.
pub fn eval(expr: &Expr) -> f64 {
    eprintln!("DEBUG eval: {expr:?}");
    match expr {
        Expr::Number(n) => *n,
        Expr::Add(a, b) => eval(a) + eval(b),
        Expr::Sub(a, b) => eval(a) - eval(b),
        Expr::Mul(a, b) => eval(a) * eval(b),
        Expr::Div(a, b) => {
            eprintln!("DEBUG div: {a:?} / {b:?}");
            eval(a) / eval(b)
        }
    }
}
RS
commit "DROPME: hack in debug tracing"

# ===========================================================================
# 8. fix: guard against division by zero
#    A legitimate fix that rewrites the Div arm — clashing with the debug hack
#    in #7. Because both edit the same lines, dropping #7 conflicts here.
# ===========================================================================
write src/eval.rs <<'RS'
//! Evaluator: walks an expression tree and computes its value.

use crate::parser::Expr;

/// Recursively evaluate an expression tree to a single number.
pub fn eval(expr: &Expr) -> f64 {
    eprintln!("DEBUG eval: {expr:?}");
    match expr {
        Expr::Number(n) => *n,
        Expr::Add(a, b) => eval(a) + eval(b),
        Expr::Sub(a, b) => eval(a) - eval(b),
        Expr::Mul(a, b) => eval(a) * eval(b),
        Expr::Div(a, b) => {
            let divisor = eval(b);
            if divisor == 0.0 {
                return f64::NAN;
            }
            eval(a) / divisor
        }
    }
}
RS
commit "fix: guard against division by zero"

# ===========================================================================
# 9. feat: unary negation, plus friendlier errors
#    Bundles TWO unrelated concerns — a new Neg arm in eval() and a reworded
#    REPL error message — across two files. A natural candidate for splitting.
# ===========================================================================
write src/eval.rs <<'RS'
//! Evaluator: walks an expression tree and computes its value.

use crate::parser::Expr;

/// Recursively evaluate an expression tree to a single number.
pub fn eval(expr: &Expr) -> f64 {
    eprintln!("DEBUG eval: {expr:?}");
    match expr {
        Expr::Number(n) => *n,
        Expr::Neg(a) => -eval(a),
        Expr::Add(a, b) => eval(a) + eval(b),
        Expr::Sub(a, b) => eval(a) - eval(b),
        Expr::Mul(a, b) => eval(a) * eval(b),
        Expr::Div(a, b) => {
            let divisor = eval(b);
            if divisor == 0.0 {
                return f64::NAN;
            }
            eval(a) / divisor
        }
    }
}
RS
write src/main.rs <<'RS'
//! A tiny command-line calculator REPL.

mod eval;
mod lexer;
mod parser;

use std::io::{self, Write};

use crate::eval::eval;
use crate::lexer::tokenize;
use crate::parser::Parser;

fn main() {
    let stdin = io::stdin();
    let mut line = String::new();

    loop {
        print!("calc> ");
        io::stdout().flush().expect("failed to flush stdout");

        line.clear();
        let bytes = stdin.read_line(&mut line).expect("failed to read line");
        if bytes == 0 {
            break; // EOF
        }

        let source = line.trim();
        if source.is_empty() {
            continue;
        }

        match run(source) {
            Ok(value) => println!("{value}"),
            Err(err) => eprintln!("calc: cannot evaluate '{source}': {err}"),
        }
    }
}

/// Tokenize, parse and evaluate a single line of input.
fn run(source: &str) -> Result<f64, String> {
    let tokens = tokenize(source)?;
    let mut parser = Parser::new(tokens);
    let expr = parser.parse()?;
    Ok(eval(&expr))
}
RS
commit "feat: unary negation, plus friendlier errors"

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
echo "Demo repo ready at: $OUT"
echo
git -C "$OUT" log --oneline --graph --decorate --all
echo
echo "Browse it with:  (cd '$OUT' && gt)"
