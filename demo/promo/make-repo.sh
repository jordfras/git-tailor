#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 Thomas Johannesson
#
# Build the throwaway repository the promo video is filmed on.
#
# Separate from the GIF's fixture (demo/gif/make-repo.sh) on purpose. That
# fixture engineers a rebase conflict, and its tape navigates by row offsets, so
# every change here would force the GIF to be recalibrated and republished. The
# two stories also want different things: the GIF wants a conflict, the promo
# wants a clean run.
#
# The branch tells one story:
#
#   * `feature/calc` — what you actually have. One commit bundles two fixes that
#     belong to two *different* earlier commits, and a WIP commit carries debug
#     output that should never be merged.
#   * `tidy`         — what you want: the same code, six commits, each doing one
#     thing. This is what the video opens on, and what the cleanup arrives at.
#
# The cleanup the video performs is: split the bundled fix (`p`), fixup each
# half into its home (`f`), drop the WIP commit (`d`). Nothing conflicts —
# every commit after the WIP one touches a different file than it does.
#
# Files are deliberately small. Diffs have to be legible at 26pt on a 1080p
# screen, which a realistic parser would not be.
#
# Usage:
#   demo/promo/make-repo.sh [OUTPUT_DIR]
set -euo pipefail

OUT="${1:-}"
if [[ -z "$OUT" ]]; then
    OUT="$(mktemp -d -t git-tailor-promo-XXXXXX)"
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

BASE_TS=1700000000
AUTHOR="Ada Lovelace"
EMAIL="ada@example.com"
SEQ=0

git -C "$OUT" init -q -b main
git -C "$OUT" config user.name "$AUTHOR"
git -C "$OUT" config user.email "$EMAIL"

write() {
    local full="$OUT/$1"
    mkdir -p "$(dirname "$full")"
    cat >"$full"
}

# Stepped timestamps so the log is stable run to run.
commit() {
    local ts=$((BASE_TS + SEQ * 3600))
    SEQ=$((SEQ + 1))
    git -C "$OUT" add -A
    GIT_AUTHOR_DATE="$ts +0000" GIT_COMMITTER_DATE="$ts +0000" \
        git -C "$OUT" commit -q -m "$1"
}

# ---------------------------------------------------------------------------
# File states, each defined once and shared by both branches — that is what
# keeps `tidy` honest: it is the same code, only grouped differently.
# ---------------------------------------------------------------------------

lexer_v1() {
    cat <<'RS'
//! Turns source text into a stream of tokens.

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Number(f64),
    Plus,
    Minus,
    Star,
    Slash,
}

pub fn tokenize(src: &str) -> Vec<Token> {
    let mut out = Vec::new();
    for word in src.split_whitespace() {
        out.push(match word {
            "+" => Token::Plus,
            "-" => Token::Minus,
            "*" => Token::Star,
            "/" => Token::Slash,
            n => Token::Number(n.parse().unwrap_or(0.0)),
        });
    }
    out
}
RS
}

lexer_parens() {
    cat <<'RS'
//! Turns source text into a stream of tokens.

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

pub fn tokenize(src: &str) -> Vec<Token> {
    let mut out = Vec::new();
    for word in src.split_whitespace() {
        out.push(match word {
            "+" => Token::Plus,
            "-" => Token::Minus,
            "*" => Token::Star,
            "/" => Token::Slash,
            "(" => Token::LParen,
            ")" => Token::RParen,
            n => Token::Number(n.parse().unwrap_or(0.0)),
        });
    }
    out
}
RS
}

# The WIP commit: debug output left in the tokenizer. Nothing after this commit
# touches lexer.rs, which is what makes dropping it a clean operation.
lexer_debug() {
    cat <<'RS'
//! Turns source text into a stream of tokens.

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

pub fn tokenize(src: &str) -> Vec<Token> {
    let mut out = Vec::new();
    for word in src.split_whitespace() {
        out.push(match word {
            "+" => Token::Plus,
            "-" => Token::Minus,
            "*" => Token::Star,
            "/" => Token::Slash,
            "(" => Token::LParen,
            ")" => Token::RParen,
            n => Token::Number(n.parse().unwrap_or(0.0)),
        });
    }
    eprintln!("DEBUG tokens: {out:?}");
    out
}
RS
}

parser_v1() {
    cat <<'RS'
//! Builds an expression tree from a token stream.

use crate::lexer::Token;

#[derive(Debug)]
pub enum Expr {
    Number(f64),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
}

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn factor(&mut self) -> Expr {
        match self.tokens.get(self.pos) {
            Some(Token::Number(n)) => {
                self.pos += 1;
                Expr::Number(*n)
            }
            _ => panic!("expected a number"),
        }
    }
}
RS
}

parser_parens() {
    cat <<'RS'
//! Builds an expression tree from a token stream.

use crate::lexer::Token;

#[derive(Debug)]
pub enum Expr {
    Number(f64),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
}

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn factor(&mut self) -> Expr {
        match self.tokens.get(self.pos) {
            Some(Token::Number(n)) => {
                self.pos += 1;
                Expr::Number(*n)
            }
            Some(Token::LParen) => {
                self.pos += 1;
                let inner = self.expression();
                self.pos += 1;
                inner
            }
            _ => panic!("expected a number"),
        }
    }
}
RS
}

eval_v1() {
    cat <<'RS'
//! Walks an expression tree and computes its value.

use crate::parser::Expr;

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
}

eval_guard() {
    cat <<'RS'
//! Walks an expression tree and computes its value.

use crate::parser::Expr;

pub fn eval(expr: &Expr) -> f64 {
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
}

main_v1() {
    cat <<'RS'
mod eval;
mod lexer;
mod parser;

use std::io::{self, BufRead};

fn main() {
    for line in io::stdin().lock().lines() {
        let source = line.expect("read stdin");
        let tokens = lexer::tokenize(&source);
        let tree = parser::parse(&tokens);
        println!("{}", eval::eval(&tree));
    }
}
RS
}

main_friendly() {
    cat <<'RS'
mod eval;
mod lexer;
mod parser;

use std::io::{self, BufRead};

fn main() {
    for line in io::stdin().lock().lines() {
        let source = line.expect("read stdin");
        let tokens = lexer::tokenize(&source);
        match parser::parse(&tokens) {
            Ok(tree) => println!("{}", eval::eval(&tree)),
            Err(err) => eprintln!("calc: cannot evaluate '{source}': {err}"),
        }
    }
}
RS
}

cargo_toml() {
    cat <<'TOML'
[package]
name = "calc"
version = "0.1.0"
edition = "2021"
TOML
}

readme() {
    cat <<'MD'
# calc

A very small expression calculator.
MD
}

# ---------------------------------------------------------------------------
# main — the fork point
# ---------------------------------------------------------------------------
readme | write README.md
commit "docs: add project README"

# ---------------------------------------------------------------------------
# tidy — what the video opens on, and what the cleanup arrives at
# ---------------------------------------------------------------------------
git -C "$OUT" checkout -q -b tidy

lexer_parens | write src/lexer.rs
commit "feat: add token lexer"

parser_parens | write src/parser.rs
commit "feat: add expression parser"

eval_v1 | write src/eval.rs
commit "feat: add evaluator"

main_v1 | write src/main.rs
cargo_toml | write Cargo.toml
commit "feat: wire up the CLI"

eval_guard | write src/eval.rs
commit "fix: guard against divide by zero"

main_friendly | write src/main.rs
commit "feat: friendlier error messages"

# ---------------------------------------------------------------------------
# feature/calc — what you actually have
# ---------------------------------------------------------------------------
SEQ=0
git -C "$OUT" checkout -q main
git -C "$OUT" checkout -q -b feature/calc

lexer_v1 | write src/lexer.rs
commit "feat: add token lexer"

parser_v1 | write src/parser.rs
commit "feat: add expression parser"

eval_v1 | write src/eval.rs
commit "feat: add evaluator"

main_v1 | write src/main.rs
cargo_toml | write Cargo.toml
commit "feat: wire up the CLI"

# The bundled commit the video splits. Its two halves belong to two different
# earlier commits — the lexer half to "add token lexer", the parser half to
# "add expression parser" — which is what makes split-then-fixup the natural
# fix, and what the hunk group matrix shows at a glance.
lexer_parens | write src/lexer.rs
parser_parens | write src/parser.rs
commit "fix: handle parentheses"

lexer_debug | write src/lexer.rs
commit "WIP: leftover debug output"

eval_guard | write src/eval.rs
commit "fix: guard against divide by zero"

main_friendly | write src/main.rs
commit "feat: friendlier error messages"

git -C "$OUT" checkout -q feature/calc
echo "$OUT"
