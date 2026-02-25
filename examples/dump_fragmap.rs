// Quick utility to dump a fragmap matrix for comparison with the original fragmap tool.
// Usage: cargo run --example dump_fragmap -- <commit-ish>

use git_tailor::{fragmap, repo, CommitInfo};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let commit_ish = args
        .get(1)
        .expect("Usage: dump_fragmap <commit-ish> [--spg-debug]");
    let spg_debug = args.iter().any(|a| a == "--spg-debug");

    let git_repo = repo::try_open_repo(std::env::current_dir().unwrap()).expect("open repo");
    let reference_oid =
        repo::find_reference_point(&git_repo, commit_ish).expect("find reference point");

    let head_ref = git_repo.head().unwrap().target().unwrap();
    let commits = repo::list_commits(&git_repo, &head_ref.to_string(), &reference_oid).unwrap();

    let commits: Vec<CommitInfo> = commits
        .into_iter()
        .filter(|c| c.oid != reference_oid)
        .collect();

    // Get diffs (zero-context for fragmap analysis)
    let commit_diffs: Vec<_> = commits
        .iter()
        .filter_map(|commit| repo::commit_diff_for_fragmap(&git_repo, &commit.oid).ok())
        .collect();

    // Dump spans per commit
    if !spg_debug {
        eprintln!("=== SPANS ===");
        for diff in &commit_diffs {
            let spans = fragmap::extract_spans(diff);
            if !spans.is_empty() {
                eprintln!(
                    "{} {}:",
                    &diff.commit.oid[..8],
                    &diff.commit.summary[..diff.commit.summary.len().min(40)]
                );
                for s in &spans {
                    eprintln!("  {} [{}-{}]", s.path, s.start_line, s.end_line);
                }
            }
        }
    }

    if spg_debug {
        fragmap::dump_per_file_spg_stats(&commit_diffs);
        return;
    }

    let fm = fragmap::build_fragmap(&commit_diffs, true);

    // Dump clusters
    eprintln!("\n=== CLUSTERS ({}) ===", fm.clusters.len());
    for (i, cluster) in fm.clusters.iter().enumerate() {
        eprintln!(
            "  cluster {}: commits={:?}",
            i,
            cluster
                .commit_oids
                .iter()
                .map(|o| &o[..8])
                .collect::<Vec<_>>()
        );
        for s in &cluster.spans {
            eprintln!("    {} [{}-{}]", s.path, s.start_line, s.end_line);
        }
    }

    // Print matrix
    eprintln!("\n=== MATRIX ===");
    let n_clusters = fm.clusters.len();
    for (ci, oid) in fm.commits.iter().enumerate() {
        let row: String = (0..n_clusters)
            .map(|cl| match fm.matrix[ci][cl] {
                fragmap::TouchKind::None => '.',
                fragmap::TouchKind::Added => '#',
                fragmap::TouchKind::Modified => '#',
                fragmap::TouchKind::Deleted => '#',
            })
            .collect();

        let summary = &commit_diffs[ci].commit.summary;
        let short_summary = &summary[..summary.len().min(30)];
        println!("{} {:30} {}", &oid[..8], short_summary, row);
    }
}
