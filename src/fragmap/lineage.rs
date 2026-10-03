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

//! Which file each change in a fragmap's commits belongs to.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::CommitDiff;

/// One file's identity across the commits of a fragmap.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FileId(usize);

/// Which file each change in a list of commit diffs (oldest first) belongs to.
///
/// A rename carries a file to its new path. A deleted file lies dormant at its
/// path, and a file added there in a later commit is the same file restored.
/// Anything else added at a path — one a rename vacated, or one deleted in the
/// same commit — is a new file. So no two changes in one commit are one file.
pub(super) struct FileLineages {
    /// Per commit, per change in that commit's diff.
    of: Vec<Vec<FileId>>,
    /// Per file, the path it was first seen under.
    labels: Vec<PathBuf>,
}

impl FileLineages {
    pub(super) fn new(commit_diffs: &[CommitDiff]) -> Self {
        let mut lineages = FileLineages {
            of: Vec::with_capacity(commit_diffs.len()),
            labels: Vec::new(),
        };
        let mut live: HashMap<PathBuf, FileId> = HashMap::new();
        let mut dormant: HashMap<PathBuf, FileId> = HashMap::new();
        // Paths a rename left, and the file that left them.
        let mut moved: HashMap<PathBuf, FileId> = HashMap::new();
        for diff in commit_diffs {
            let mut ids: Vec<Option<FileId>> = vec![None; diff.files.len()];
            let mut deleted_now: HashSet<&Path> = HashSet::new();
            // What a commit takes away goes first, so a path it vacates is
            // free for what it adds. A merge's diff repeats what its branch
            // did, so a change already made is recognized, not made again.
            for (change, file) in diff.files.iter().enumerate() {
                if let Some(old) = renamed_from(file) {
                    let new = file.new_path.as_deref().unwrap_or(old);
                    let repeated = moved
                        .get(old)
                        .copied()
                        .filter(|&id| live.get(new) == Some(&id));
                    ids[change] = Some(repeated.unwrap_or_else(|| {
                        let id = live.remove(old).unwrap_or_else(|| lineages.add(old));
                        moved.insert(old.to_path_buf(), id);
                        id
                    }));
                } else if file.status == crate::DeltaStatus::Deleted
                    && let Some(path) = file.old_path.as_deref().or(file.new_path.as_deref())
                {
                    let id = live
                        .remove(path)
                        .or_else(|| dormant.get(path).or(moved.get(path)).copied())
                        .unwrap_or_else(|| lineages.add(path));
                    dormant.insert(path.to_path_buf(), id);
                    deleted_now.insert(path);
                    ids[change] = Some(id);
                }
            }
            for (change, file) in diff.files.iter().enumerate() {
                let Some(path) = file.new_path.as_deref().or(file.old_path.as_deref()) else {
                    continue;
                };
                let id = match ids[change] {
                    Some(_) if file.status == crate::DeltaStatus::Deleted => continue,
                    Some(renamed) => renamed,
                    None if is_addition(file) && deleted_now.contains(path) => lineages.add(path),
                    None if is_addition(file) => match live.get(path) {
                        Some(&repeated) => repeated,
                        None => dormant.remove(path).unwrap_or_else(|| lineages.add(path)),
                    },
                    None => match live.get(path) {
                        Some(&id) => id,
                        // A path a rename or deletion left can still be edited
                        // on a line of history the change has not reached.
                        None => match moved.get(path).or(dormant.get(path)) {
                            Some(&id) => {
                                ids[change] = Some(id);
                                continue;
                            }
                            None => lineages.add(path),
                        },
                    },
                };
                dormant.remove(path);
                live.insert(path.to_path_buf(), id);
                ids[change] = Some(id);
            }
            let ids: Vec<FileId> = ids
                .into_iter()
                .map(|id| id.unwrap_or_else(|| lineages.add(Path::new(""))))
                .collect();
            lineages.of.push(ids);
        }
        lineages
    }

    fn add(&mut self, path: &Path) -> FileId {
        self.labels.push(path.to_path_buf());
        FileId(self.labels.len() - 1)
    }

    /// The file change `change` of commit `commit_idx` belongs to.
    pub(super) fn of(&self, commit_idx: usize, change: usize) -> FileId {
        self.of[commit_idx][change]
    }

    /// The path `file` was first seen under: what its clusters are labeled with.
    pub(super) fn label(&self, file: FileId) -> &Path {
        &self.labels[file.0]
    }

    /// `files` in the order git lists paths, by their labels' bytes.
    ///
    /// `Path` compares component by component, which puts `repo/x.rs` before
    /// `repo.rs` where the bytes put it after. Column order, the numbering of
    /// the split pieces and which hunk the rescue path cuts all come off this
    /// order, so it has to be git's.
    pub(super) fn sorted(&self, files: impl IntoIterator<Item = FileId>) -> Vec<FileId> {
        let mut files: Vec<FileId> = files.into_iter().collect();
        files.sort_by_cached_key(|&file| (crate::domain::path_to_bytes(self.label(file)), file));
        files
    }
}

/// The path `file` was renamed from, if it was. A copy leaves its source in
/// place, so it is an addition, not a rename.
fn renamed_from(file: &crate::FileDiff) -> Option<&Path> {
    match (&file.old_path, &file.new_path) {
        (Some(old), Some(new)) if old != new && file.status != crate::DeltaStatus::Copied => {
            Some(old)
        }
        _ => None,
    }
}

fn is_addition(file: &crate::FileDiff) -> bool {
    matches!(
        file.status,
        crate::DeltaStatus::Added | crate::DeltaStatus::Copied | crate::DeltaStatus::Untracked
    )
}
