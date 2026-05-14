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

// Fragmap compatibility tool.
//
// Builds a Fragmap object from the current repository and compares it with
// the output of the original `fragmap` binary on the same commit range.
//
// Usage: cargo run --example fragmap_compat -- <commit-ish>
//
// - <commit-ish> is resolved to a merge-base (the same way `gt --static` does).
// - The original `fragmap` binary must be installed (`pip install fragmap`).
// - Prints "OK" when both tools detect the same commit-cluster relationships
//   (column order may differ between the two tools).
// - Otherwise prints both outputs and a short summary of what differs.

use std::collections::BTreeSet;
use std::process::Command;

use git_tailor::fragmap::{self, FragMap, SquashableScope};
use git_tailor::repo::{Git2Repo, GitRepo};
use git_tailor::static_views;
use git_tailor::{CommitInfo, VirtualOid};

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.get(1).map(|s| s.as_str()) == Some("--sweep") {
        let max_depth: usize = args
            .get(2)
            .and_then(|s| s.parse().ok())
            .unwrap_or(usize::MAX);
        run_sweep(max_depth);
    } else {
        let commit_ish = args
            .get(1)
            .expect("Usage: fragmap_compat <commit-ish> | fragmap_compat --sweep [max-depth]");
        let git_repo = Git2Repo::open(std::env::current_dir().unwrap()).expect("open repo");
        let result = check_commit_ish(&git_repo, commit_ish);
        if result.ok {
            println!("OK");
        } else {
            std::process::exit(1);
        }
    }
}

fn run_sweep(max_depth: usize) {
    let git_repo = Git2Repo::open(std::env::current_dir().unwrap()).expect("open repo");
    let mut passed = 0usize;

    for depth in 1..=max_depth {
        let commit_ish = format!("HEAD~{depth}");
        // Stop when the commit-ish can't be resolved (e.g. past root).
        if git_repo.find_reference_point(&commit_ish).is_err() {
            break;
        }
        let result = check_commit_ish(&git_repo, &commit_ish);
        if result.ok {
            passed += 1;
            println!("HEAD~{depth} ({} commits): OK", result.n_commits);
        } else {
            eprintln!(
                "HEAD~{depth} ({} commits): MISMATCH ({} differ)",
                result.n_commits, result.n_mismatches
            );
            println!("\n{passed} passed, 1 failed");
            std::process::exit(1);
        }
    }

    println!("\n{passed} passed, 0 failed");
}

struct CheckResult {
    ok: bool,
    n_commits: usize,
    n_mismatches: usize,
}

fn check_commit_ish(git_repo: &Git2Repo, commit_ish: &str) -> CheckResult {
    // Build git-tailor Fragmap for the same commit range used by `gt --static`.
    let reference_oid = git_repo
        .find_reference_point(commit_ish)
        .expect("find reference point");
    let head_oid = git_repo.head_oid().expect("get HEAD");
    let commits = git_repo
        .list_commits(&head_oid, &reference_oid)
        .expect("list commits");
    let commits: Vec<CommitInfo> = commits
        .into_iter()
        .filter(|c| c.oid != VirtualOid::Real(reference_oid.clone()))
        .collect();
    let n_commits = commits.len();
    let mut commit_diffs: Vec<_> = commits
        .iter()
        .filter_map(|c| {
            c.oid
                .as_oid()
                .and_then(|oid| git_repo.commit_diff_for_fragmap(oid).ok())
        })
        .collect();
    // Append staged/unstaged working-tree changes exactly as `gt --static` does,
    // so that the comparison is made against the same row set that fragmap shows.
    if let Some(d) = git_repo.staged_diff() {
        commit_diffs.push(d);
    }
    if let Some(d) = git_repo.unstaged_diff() {
        commit_diffs.push(d);
    }

    let fmap = fragmap::build_fragmap(&commit_diffs, true, &mut |_| true)
        .expect("no-op callback never cancels");

    // Run original fragmap binary.
    // Pass `--no-color` so touched cells render as '#' (not ANSI-colored spaces).
    // Pass `-s <reference_oid>` so fragmap shows the same exclusive commit range.
    let fm_raw = run_fragmap(reference_oid.long());
    // Strip any residual ANSI codes defensively.
    let fm_output = strip_ansi(&fm_raw);

    // Parse fragmap output into (sha8, matrix_row) pairs.
    // Lines look like: `{sha8} {title_padded} {matrix_row}` where matrix_row
    // is the last whitespace-delimited token and each cell is one char (# . ^ | v).
    let fm_rows = parse_rows(&fm_output);

    // Compare column profiles: for each cluster column, collect the set of
    // sha8-like identifiers of commits that directly touch it ('#').
    // Column order is irrelevant so we sort both vecs before comparing.
    //
    // Working-tree rows use different sentinel strings across the two tools:
    //   fragmap uses "00000000" (one row for all uncommitted changes)
    //   git-tailor uses "staged" / "unstaged" (separate rows)
    // Both are normalised to the sentinel "<wt>" so they match regardless
    // of whether git-tailor splits staged/unstaged or fragmap merges them.
    let gt_profiles_indexed = gt_column_profiles(&fmap);
    let fm_profiles_indexed = text_column_profiles(&fm_rows);
    let mut gt_profiles = gt_profiles_indexed.clone();
    let mut fm_profiles = fm_profiles_indexed.clone();
    gt_profiles.sort();
    fm_profiles.sort();

    if gt_profiles == fm_profiles {
        return CheckResult {
            ok: true,
            n_commits,
            n_mismatches: 0,
        };
    }

    let gt_output = static_views::fragmap::render(
        &commit_diffs,
        false,
        false,
        false,
        SquashableScope::Commit,
        None,
    );
    println!("=== fragmap output ===");
    print!("{fm_output}");
    println!("\n=== git-tailor output ===");
    print!("{gt_output}");
    println!("\n=== diff ===");
    let n_mismatches = report_diff(
        &gt_profiles,
        &fm_profiles,
        &gt_profiles_indexed,
        &fm_profiles_indexed,
        &commit_diffs,
    );
    CheckResult {
        ok: false,
        n_commits,
        n_mismatches,
    }
}

fn run_fragmap(since_oid: &str) -> String {
    let result = Command::new("fragmap")
        .args(["--no-color", "-s", since_oid])
        .output();
    match result {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("fragmap not found – install with: pip install fragmap");
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("failed to run fragmap: {e}");
            std::process::exit(1);
        }
        Ok(out) if !out.status.success() => {
            eprintln!(
                "fragmap exited with {}: {}",
                out.status,
                String::from_utf8_lossy(&out.stderr)
            );
            std::process::exit(1);
        }
        Ok(out) => String::from_utf8_lossy(&out.stdout).into_owned(),
    }
}

/// Strip ANSI CSI escape sequences (e.g. color codes) from a string.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            chars.next(); // consume '['
            while let Some(&nc) = chars.peek() {
                chars.next();
                if nc.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Parse a fragmap plain-text output into (sha8, matrix_row) pairs.
///
/// fragmap uses `\r` (carriage return) to overwrite its progress lines on
/// stdout, so the raw output contains both `\r` and `\n` as line separators.
/// We split on both.
///
/// The line format is `{sha8} {title}{matrix}`.  When the title fills its
/// fixed-width column exactly (truncated) there is no space before the matrix.
/// We therefore scan from the right to find the `[#.^|v]+` matrix suffix
/// rather than using `split_whitespace`.
///
/// fragmap uses "00000000" for staged+unstaged changes; this is normalised to
/// the sentinel `"<wt>"` so it compares equal to git-tailor's synthetic rows.
fn parse_rows(output: &str) -> Vec<(String, String)> {
    let candidates: Vec<(String, String)> = output
        .split(['\n', '\r'])
        .filter(|l| l.len() >= 9)
        .filter_map(|l| {
            let raw_sha8 = &l[..8];
            if !raw_sha8.chars().all(|c| c.is_ascii_hexdigit()) {
                return None;
            }
            let sha8 = normalize_sha(raw_sha8);
            // Scan from the right, skipping trailing whitespace, to extract
            // the matrix suffix.
            let trimmed = l.trim_end();
            let suffix: String = trimmed
                .chars()
                .rev()
                .take_while(|&c| "#.^|v".contains(c))
                .collect::<String>()
                .chars()
                .rev()
                .collect();
            if suffix.is_empty() {
                None
            } else {
                Some((sha8, suffix))
            }
        })
        .collect();

    if candidates.is_empty() {
        return Vec::new();
    }

    // If a commit title happens to end with chars from [#.^|v], the suffix
    // scan includes those chars and the row appears longer than the true
    // matrix width.  The minimum across all rows is the true matrix width;
    // we clamp every row to that length (taking chars from the right).
    let expected_len = candidates.iter().map(|(_, m)| m.len()).min().unwrap_or(0);
    candidates
        .into_iter()
        .map(|(sha, m)| {
            let matrix: String = if m.len() > expected_len {
                m.chars().skip(m.len() - expected_len).collect()
            } else {
                m
            };
            (sha, matrix)
        })
        .collect()
}

type Profile = BTreeSet<String>;

/// Normalise a sha-like identifier to a uniform sentinel when it refers to
/// working-tree changes (staged or unstaged) rather than a real commit.
///
/// - fragmap uses "00000000" for all uncommitted changes.
/// - git-tailor uses "staged" and "unstaged" as separate synthetic oids.
///
/// All three collapse to "<wt>" so they compare equal.
fn normalize_sha(s: &str) -> String {
    if s == "00000000" || s.starts_with("staged") || s.starts_with("unstage") {
        "<wt>".to_string()
    } else {
        s[..8.min(s.len())].to_string()
    }
}

/// Build one cluster profile per column directly from the FragMap matrix.
/// A profile is the set of sha8 prefixes of commits that directly touch that column.
fn gt_column_profiles(fmap: &FragMap) -> Vec<Profile> {
    (0..fmap.clusters.len())
        .map(|ci| {
            fmap.clusters[ci]
                .commit_oids
                .iter()
                .map(|oid| normalize_sha(oid.long()))
                .collect()
        })
        .collect()
}

/// Build one cluster profile per column from parsed (sha8, matrix_row) pairs.
/// A sha8 is in a profile when its cell character at that column position is '#'.
fn text_column_profiles(rows: &[(String, String)]) -> Vec<Profile> {
    let n_cols = rows.first().map(|(_, m)| m.len()).unwrap_or(0);
    (0..n_cols)
        .map(|ci| {
            rows.iter()
                .filter(|(_, m)| m.chars().nth(ci) == Some('#'))
                .map(|(sha, _)| sha.clone())
                .collect()
        })
        .collect()
}

fn report_diff(
    gt_profiles: &[Profile],
    fm_profiles: &[Profile],
    gt_indexed: &[Profile],
    fm_indexed: &[Profile],
    commit_diffs: &[git_tailor::CommitDiff],
) -> usize {
    // Build sha8 -> short summary lookup from our own commit diffs.
    let summaries: std::collections::HashMap<String, &str> = commit_diffs
        .iter()
        .map(|d| {
            (
                normalize_sha(d.commit.oid.long()),
                d.commit.summary.as_str(),
            )
        })
        .collect();

    let label = |sha: &str| -> String {
        let summary = summaries.get(sha).copied().unwrap_or("");
        let short: String = summary.chars().take(50).collect();
        if short.is_empty() {
            sha.to_string()
        } else {
            format!("{sha}  {short}")
        }
    };

    // Return all 1-based column numbers (left-to-right) where `profile` appears.
    let cols_in = |indexed: &[Profile], profile: &Profile| -> Vec<usize> {
        indexed
            .iter()
            .enumerate()
            .filter(|(_, p)| *p == profile)
            .map(|(i, _)| i + 1)
            .collect()
    };

    // Partition into exactly-matched and unmatched profiles.
    let mut gt_remaining: Vec<&Profile> = gt_profiles.iter().collect();
    let mut fm_unmatched: Vec<&Profile> = Vec::new();
    for p in fm_profiles {
        if let Some(pos) = gt_remaining.iter().position(|x| *x == p) {
            gt_remaining.remove(pos);
        } else {
            fm_unmatched.push(p);
        }
    }

    let n_mismatches = fm_unmatched.len().max(gt_remaining.len());
    println!(
        "{} mismatched cluster{}:",
        n_mismatches,
        if n_mismatches == 1 { "" } else { "s" }
    );

    // Greedily pair each fm-only profile with the gt-only profile that shares
    // the most commits (largest intersection).
    let mut gt_unpaired: Vec<Option<&Profile>> = gt_remaining.iter().map(|p| Some(*p)).collect();
    let mut pairs: Vec<(&Profile, Option<&Profile>)> = Vec::new();

    for fm_p in &fm_unmatched {
        let best = gt_unpaired
            .iter()
            .enumerate()
            .filter_map(|(i, opt)| opt.map(|gt_p| (i, gt_p.intersection(fm_p).count())))
            .max_by_key(|&(_, score)| score);
        if let Some((idx, _)) = best {
            pairs.push((fm_p, gt_unpaired[idx].take()));
        } else {
            pairs.push((fm_p, None));
        }
    }

    let print_profile = |side: &str, profile: &Profile, cols: &[usize]| {
        let col_str = cols
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        println!("  {side} col {col_str}:");
        for sha in profile {
            println!("    {}", label(sha));
        }
    };

    let mut n = 0;
    for (fm_p, maybe_gt) in &pairs {
        n += 1;
        let fm_cols = cols_in(fm_indexed, fm_p);
        println!();
        if let Some(gt_p) = maybe_gt {
            let gt_cols = cols_in(gt_indexed, gt_p);
            let shared: Profile = fm_p.intersection(gt_p).cloned().collect();
            let fm_only: Profile = fm_p.difference(gt_p).cloned().collect();
            let gt_only: Profile = gt_p.difference(fm_p).cloned().collect();
            println!(
                "  Mismatch {n}: fragmap col {} vs git-tailor col {}",
                fm_cols
                    .iter()
                    .map(|c| c.to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
                gt_cols
                    .iter()
                    .map(|c| c.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            for sha in &shared {
                println!("    both            {}", label(sha));
            }
            for sha in &fm_only {
                println!("    fragmap only    {}", label(sha));
            }
            for sha in &gt_only {
                println!("    git-tailor only {}", label(sha));
            }
        } else {
            print_profile(&format!("Column only in fragmap ({n}),"), fm_p, &fm_cols);
        }
    }

    for (i, gt_p) in gt_unpaired.into_iter().flatten().enumerate() {
        let gt_cols = cols_in(gt_indexed, gt_p);
        println!();
        print_profile(
            &format!("Column only in git-tailor ({}),", i + 1),
            gt_p,
            &gt_cols,
        );
    }

    n_mismatches
}
