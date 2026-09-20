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

//! The synthetic repository behind the README screenshot.
//!
//! Kept as a fixture in its own right rather than replaced by one of the demo
//! repositories under `demo/`, because the two are built for opposite ends: the
//! history here deliberately entangles commits so the matrix shows *both*
//! connector colors, while the promo fixture is engineered so nothing
//! conflicts (see `demo/promo/make-repo.sh`). Filming the README image on that
//! one would quietly drop the red connectors it exists to demonstrate.
//!
//! Lives beside `image_render` in `examples/common/` so the driver stays about
//! shots rather than about commit contents.

use std::path::Path;

use anyhow::{Context, Result};
use git2::{Repository, Signature, Time};

/// One commit's worth of changes: a message and the full new contents of each
/// touched file.
struct Change {
    message: &'static str,
    files: &'static [(&'static str, &'static str)],
}

/// Build a small interpreter-project history on a `work` branch forked from
/// `main`. Several commits deliberately re-touch the same regions of the same
/// files (the `fixup!` commits) so the hunk-group matrix shows squash partners
/// and conflicts.
pub fn build(path: &Path) -> Result<()> {
    let mut opts = git2::RepositoryInitOptions::new();
    opts.initial_head("main");
    let repo = Repository::init_opts(path, &opts)?;

    // The merge-base on `main`: the scaffold the branch was forked from.
    commit(&repo, 0, &base_change())?;
    let base_oid = repo.head()?.target().context("no HEAD after base commit")?;
    repo.branch("work", &repo.find_commit(base_oid)?, false)?;
    repo.set_head("refs/heads/work")?;
    repo.checkout_head(None)?;

    for (i, change) in branch_changes().iter().enumerate() {
        commit(&repo, (i + 1) as i64, change)?;
    }
    Ok(())
}

/// Create one commit applying `change`, with a deterministic signature/time so
/// the generated image is reproducible. `seq` spaces commit times apart.
fn commit(repo: &Repository, seq: i64, change: &Change) -> Result<()> {
    let workdir = repo.workdir().context("bare repo has no workdir")?;
    let mut index = repo.index()?;
    for (rel, contents) in change.files {
        let full = workdir.join(rel);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&full, contents)?;
        index.add_path(Path::new(rel))?;
    }
    index.write()?;
    let tree = repo.find_tree(index.write_tree()?)?;

    // Deterministic identity and timestamps (one minute apart per commit).
    let when = Time::new(1_700_000_000 + seq * 60, 0);
    let sig = Signature::new("Ada Lovelace", "ada@example.com", &when)?;

    let parents: Vec<git2::Commit> = match repo.head().ok().and_then(|h| h.target()) {
        Some(oid) => vec![repo.find_commit(oid)?],
        None => vec![],
    };
    let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
    repo.commit(
        Some("HEAD"),
        &sig,
        &sig,
        change.message,
        &tree,
        &parent_refs,
    )?;
    Ok(())
}

// The file contents below are illustrative scaffolding for a toy expression
// interpreter. What matters for the screenshot is *where* successive commits
// edit each file: re-touching the same line range as an earlier commit makes
// the two share a hunk-group column, and the color of the connector between
// them shows whether they can be cleanly squashed.
//
// The history is arranged to show both connector colors:
//   * yellow — a clean fixup (only re-touches one earlier commit's region), and
//   * red    — an entangled commit that touches regions belonging to *two*
//              different earlier commits, so it can't be folded into a single
//              one (`squash_target` is None). The two cross-cutting commits
//              ("thread source spans…" and "report errors with line numbers")
//              each produce a pair of red connectors.

fn base_change() -> Change {
    Change {
        message: "chore: initial project scaffold",
        files: &[(
            "Cargo.toml",
            "[package]\nname = \"calc\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )],
    }
}

// Successive states of each file, named by the commit that writes them.
const LEXER_V1: &str = "pub enum Token {\n    Number(f64),\n    Plus,\n    Minus,\n    Star,\n    Slash,\n}\n\npub fn tokenize(src: &str) -> Vec<Token> {\n    src.chars().filter_map(classify).collect()\n}\n";
const LEXER_V2: &str = "pub enum Token {\n    Number(f64),\n    Plus,\n    Minus,\n    Star,\n    Slash,\n    LParen,\n    RParen,\n}\n\npub fn tokenize(src: &str) -> Vec<Token> {\n    src.chars().filter_map(classify).collect()\n}\n";
const LEXER_V3: &str = "pub enum Token {\n    Number(f64, Span),\n    Plus,\n    Minus,\n    Star,\n    Slash,\n    LParen,\n    RParen,\n}\n\npub fn tokenize(src: &str) -> Vec<Token> {\n    src.chars().filter_map(classify).collect()\n}\n";

const PARSER_V1: &str = "use crate::lexer::Token;\n\npub struct Parser {\n    tokens: Vec<Token>,\n    pos: usize,\n}\n\nimpl Parser {\n    pub fn parse(&mut self) -> Expr {\n        self.expression()\n    }\n\n    fn expression(&mut self) -> Expr {\n        self.term()\n    }\n}\n";
const PARSER_V2: &str = "use crate::lexer::Token;\n\npub struct Parser {\n    tokens: Vec<Token>,\n    pos: usize,\n}\n\nimpl Parser {\n    pub fn parse(&mut self) -> Expr {\n        self.expression()\n    }\n\n    fn expression(&mut self) -> Expr {\n        if self.eat(Token::Minus) {\n            return Expr::Neg(Box::new(self.term()));\n        }\n        self.term()\n    }\n}\n";
const PARSER_V3: &str = "use crate::lexer::Token;\n\npub struct Parser {\n    tokens: Vec<Token>,\n    pos: usize,\n}\n\nimpl Parser {\n    pub fn parse(&mut self) -> Expr {\n        self.expression()\n    }\n\n    fn expression(&mut self) -> Expr {\n        if self.eat(Token::Minus) {\n            return Expr::Neg(self.span(), Box::new(self.term()));\n        }\n        self.term()\n    }\n}\n";
const PARSER_V4: &str = "use crate::lexer::Token;\n\npub struct Parser {\n    tokens: Vec<Token>,\n    pos: usize,\n}\n\nimpl Parser {\n    pub fn parse(&mut self) -> Expr {\n        self.expression()\n    }\n\n    fn expression(&mut self) -> Expr {\n        if self.eat(Token::Minus) {\n            return Expr::Neg(self.span(), Box::new(self.unary()));\n        }\n        self.term()\n    }\n}\n";

const EVAL_V1: &str = "use crate::parser::Expr;\n\npub fn eval(expr: &Expr) -> f64 {\n    match expr {\n        Expr::Number(n) => *n,\n        Expr::Add(a, b) => eval(a) + eval(b),\n        Expr::Sub(a, b) => eval(a) - eval(b),\n        Expr::Mul(a, b) => eval(a) * eval(b),\n    }\n}\n";
const EVAL_V2: &str = "use crate::parser::Expr;\n\npub fn eval(expr: &Expr) -> Result<f64, Error> {\n    match expr {\n        Expr::Number(n) => Ok(*n),\n        Expr::Add(a, b) => Ok(eval(a)? + eval(b)?),\n        Expr::Sub(a, b) => Ok(eval(a)? - eval(b)?),\n        Expr::Mul(a, b) => Ok(eval(a)? * eval(b)?),\n    }\n}\n";

const MAIN_V1: &str = "mod eval;\nmod lexer;\nmod parser;\n\nfn main() {\n    for line in std::io::stdin().lines() {\n        let line = line.unwrap();\n        println!(\"= {}\", run(&line));\n    }\n}\n";
const MAIN_V2: &str = "mod eval;\nmod lexer;\nmod parser;\n\nfn main() {\n    for (n, line) in std::io::stdin().lines().enumerate() {\n        let line = line.unwrap();\n        match run(&line) {\n            Ok(value) => println!(\"= {}\", value),\n            Err(e) => eprintln!(\"line {}: {}\", n + 1, e),\n        }\n    }\n}\n";

const README: &str = "# calc\n\nA tiny expression interpreter.\n\n## Grammar\n\n```\nexpr := term ((+|-) term)*\nterm := factor ((*|/) factor)*\n```\n";

fn branch_changes() -> &'static [Change] {
    &[
        Change {
            message: "feat: add token kinds",
            files: &[("src/lexer.rs", LEXER_V1)],
        },
        Change {
            message: "feat: add expression parser",
            files: &[("src/parser.rs", PARSER_V1)],
        },
        Change {
            message: "feat: add tree-walking evaluator",
            files: &[("src/eval.rs", EVAL_V1)],
        },
        Change {
            message: "feat: wire up the REPL entry point",
            files: &[("src/main.rs", MAIN_V1)],
        },
        Change {
            // Re-touches only the lexer enum — cleanly squashes into "add token
            // kinds": yellow connector.
            message: "fixup! add token kinds",
            files: &[("src/lexer.rs", LEXER_V2)],
        },
        Change {
            message: "fix: handle unary minus in the parser",
            files: &[("src/parser.rs", PARSER_V2)],
        },
        Change {
            // Touches BOTH the lexer enum (owned by "add token kinds") and the
            // parser (owned by "handle unary minus") — entangled across two
            // commits, so neither connector is a clean squash: red.
            message: "refactor: track source spans",
            files: &[("src/lexer.rs", LEXER_V3), ("src/parser.rs", PARSER_V3)],
        },
        Change {
            // Touches BOTH the evaluator (owned by "add evaluator") and main
            // (owned by "wire up the REPL") — again red on both connectors.
            message: "feat: report errors with line numbers",
            files: &[("src/eval.rs", EVAL_V2), ("src/main.rs", MAIN_V2)],
        },
        Change {
            // Re-touches only the parser expression — a clean fixup: yellow.
            message: "fixup! add expression parser",
            files: &[("src/parser.rs", PARSER_V4)],
        },
        Change {
            message: "docs: document the grammar",
            files: &[("README.md", README)],
        },
    ]
}
