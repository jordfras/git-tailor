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

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::{CommitDiff, VirtualOid};

/// One file's identity across the commits of a fragmap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FileId(usize);

impl FileId {
    /// File number `n`, for fabricating spans; real ones come from the
    /// lineages, which number files as they first see them.
    pub fn numbered(n: usize) -> Self {
        FileId(n)
    }
}

/// Which file each change in a list of commit diffs belongs to.
///
/// Each commit sees the files its first parent left — the parent its diff is
/// against — and changes them: a rename carries a file to its new path, a
/// deleted file lies dormant at its path until a later commit restores it,
/// and anything else added is a new file, even at a path a rename left or one
/// deleted in the same commit. A merge brings in its other parents' files: what
/// it seems to add or rename into is the file that parent has there. A path
/// first seen without an addition is a file from before the commits, the same
/// one on every line. No two changes in one commit are then one file.
pub(super) struct FileLineages {
    /// Per commit, per change in that commit's diff.
    of: Vec<Vec<FileId>>,
    /// Per file, the path it was first seen under.
    labels: Vec<PathBuf>,
}

/// The files a commit leaves, by path.
#[derive(Debug, Clone, Default)]
struct Snapshot {
    /// Each path's file, and whether it is there (`true`) or deleted.
    at: HashMap<PathBuf, (FileId, bool)>,
    /// Where each file that is there is.
    live_at: HashMap<FileId, PathBuf>,
}

impl Snapshot {
    fn live(&self, path: &Path) -> Option<FileId> {
        self.at
            .get(path)
            .filter(|&&(_, live)| live)
            .map(|&(id, _)| id)
    }

    /// The file at `path`, there or deleted.
    fn known(&self, path: &Path) -> Option<FileId> {
        self.at.get(path).map(|&(id, _)| id)
    }

    /// The deleted file at `path`, unless it lives on at another path.
    fn restorable(&self, path: &Path) -> Option<FileId> {
        match self.at.get(path) {
            Some(&(id, false)) if !self.live_at.contains_key(&id) => Some(id),
            _ => None,
        }
    }

    fn put(&mut self, path: &Path, id: FileId) {
        self.at.insert(path.to_path_buf(), (id, true));
        self.live_at.insert(id, path.to_path_buf());
    }

    fn bury(&mut self, path: &Path, id: FileId) {
        self.vacate(path);
        self.at.insert(path.to_path_buf(), (id, false));
    }

    fn vacate(&mut self, path: &Path) {
        if let Some((id, true)) = self.at.remove(path)
            && self.live_at.get(&id).is_some_and(|at| at == path)
        {
            self.live_at.remove(&id);
        }
    }
}

impl FileLineages {
    pub(super) fn new(commit_diffs: &[CommitDiff]) -> Self {
        let mut lineages = FileLineages {
            of: Vec::new(),
            labels: Vec::new(),
        };
        let position: HashMap<&VirtualOid, usize> = commit_diffs
            .iter()
            .enumerate()
            .map(|(idx, diff)| (&diff.commit.oid, idx))
            .collect();
        // A commit with no parents at all, as the uncommitted rows, continues
        // from the one listed before it. One whose parents lie outside the
        // commits starts afresh.
        let parents: Vec<(Option<usize>, Vec<usize>)> = commit_diffs
            .iter()
            .enumerate()
            .map(|(idx, diff)| {
                let oids = &diff.commit.parent_oids;
                if oids.is_empty() {
                    return (idx.checked_sub(1), Vec::new());
                }
                let mut listed = oids
                    .iter()
                    .map(|oid| position.get(&VirtualOid::Real(oid.clone())).copied());
                (listed.next().flatten(), listed.flatten().collect())
            })
            .collect();
        let mut children = vec![0usize; commit_diffs.len()];
        for (first, others) in &parents {
            for &parent in first.iter().chain(others) {
                children[parent] += 1;
            }
        }

        let mut snapshots: Vec<Option<Snapshot>> = vec![None; commit_diffs.len()];
        let mut before_range: HashMap<PathBuf, FileId> = HashMap::new();
        let mut of: Vec<Vec<FileId>> = vec![Vec::new(); commit_diffs.len()];
        for idx in parent_first_order(&parents) {
            let diff = &commit_diffs[idx];
            let (first, others) = &parents[idx];
            let mut snapshot = match *first {
                Some(parent) if children[parent] == 1 => snapshots[parent].take(),
                Some(parent) => snapshots[parent].clone(),
                None => None,
            }
            .unwrap_or_default();
            let brought_in: Vec<&Snapshot> = others
                .iter()
                .filter_map(|&parent| snapshots[parent].as_ref())
                .collect();
            of[idx] = lineages.change_files(diff, &mut snapshot, &brought_in, &mut before_range);
            snapshots[idx] = Some(snapshot);
            for &parent in first.iter().chain(others) {
                children[parent] -= 1;
                if children[parent] == 0 {
                    snapshots[parent] = None;
                }
            }
        }
        lineages.of = of;
        lineages
    }

    /// The file each change of `diff` belongs to, updating `snapshot` from
    /// what the first parent left to what the commit leaves.
    fn change_files(
        &mut self,
        diff: &CommitDiff,
        snapshot: &mut Snapshot,
        brought_in: &[&Snapshot],
        before_range: &mut HashMap<PathBuf, FileId>,
    ) -> Vec<FileId> {
        let brought = |path: &Path| brought_in.iter().find_map(|parent| parent.live(path));
        let mut ids: Vec<Option<FileId>> = vec![None; diff.files.len()];
        let mut deleted_now: HashSet<&Path> = HashSet::new();
        // What a commit takes away goes first, so a path it vacates is free
        // for what it adds.
        for (change, file) in diff.files.iter().enumerate() {
            if let Some(old) = renamed_from(file) {
                let new = file.new_path.as_deref().unwrap_or(old);
                let id = brought(new)
                    .or_else(|| snapshot.known(old))
                    .unwrap_or_else(|| self.before_range(before_range, old));
                snapshot.vacate(old);
                ids[change] = Some(id);
            } else if file.status == crate::DeltaStatus::Deleted
                && let Some(path) = file.old_path.as_deref().or(file.new_path.as_deref())
            {
                let id = snapshot
                    .known(path)
                    .unwrap_or_else(|| self.before_range(before_range, path));
                snapshot.bury(path, id);
                deleted_now.insert(path);
                ids[change] = Some(id);
            }
        }
        diff.files
            .iter()
            .zip(ids)
            .map(|(file, id)| {
                let path = file
                    .new_path
                    .as_deref()
                    .or(file.old_path.as_deref())
                    .expect("libgit2 sets a path on every change");
                let id = match id {
                    Some(deleted) if file.status == crate::DeltaStatus::Deleted => {
                        return deleted;
                    }
                    Some(renamed) => renamed,
                    None if is_addition(file) => brought(path).unwrap_or_else(|| {
                        let restored = (!deleted_now.contains(path))
                            .then(|| snapshot.restorable(path))
                            .flatten();
                        restored.unwrap_or_else(|| self.add(path))
                    }),
                    None => snapshot
                        .known(path)
                        .unwrap_or_else(|| self.before_range(before_range, path)),
                };
                snapshot.put(path, id);
                id
            })
            .collect()
    }

    /// The file at `path` from before the commits: one per path, on every line.
    fn before_range(&mut self, before_range: &mut HashMap<PathBuf, FileId>, path: &Path) -> FileId {
        match before_range.get(path) {
            Some(&id) => id,
            None => {
                let id = self.add(path);
                before_range.insert(path.to_path_buf(), id);
                id
            }
        }
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

/// The commits in an order that puts every parent before its children:
/// commit times need not grow from parent to child, so a date-ordered list
/// can hold a commit before its parent. Ties keep the list's order, and a
/// commit no order can place — a root listed after its own descendant, which
/// the uncommitted rows' fallback then makes its child — comes last.
fn parent_first_order(parents: &[(Option<usize>, Vec<usize>)]) -> Vec<usize> {
    let mut waiting: Vec<usize> = vec![0; parents.len()];
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); parents.len()];
    for (idx, (first, others)) in parents.iter().enumerate() {
        for &parent in first.iter().chain(others) {
            waiting[idx] += 1;
            children[parent].push(idx);
        }
    }
    let mut ready: BinaryHeap<Reverse<usize>> = (0..parents.len())
        .filter(|&idx| waiting[idx] == 0)
        .map(Reverse)
        .collect();
    let mut order = Vec::with_capacity(parents.len());
    while let Some(Reverse(idx)) = ready.pop() {
        order.push(idx);
        for &child in &children[idx] {
            waiting[child] -= 1;
            if waiting[child] == 0 {
                ready.push(Reverse(child));
            }
        }
    }
    let placed: HashSet<usize> = order.iter().copied().collect();
    order.extend((0..parents.len()).filter(|idx| !placed.contains(idx)));
    order
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
