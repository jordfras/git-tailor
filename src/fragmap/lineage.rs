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

use super::position::{ChangePos, CommitPos};
use crate::{CommitDiff, Oid};

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
/// Each commit starts from which file was at each path after its first parent
/// — the parent its diff is against — and changes that: a rename carries a file
/// to its new path, a deleted file is remembered at its path so a later commit
/// can restore it, and anything else added is a new file, even at a path a
/// rename emptied or one deleted in the same commit. What a merge seems to add
/// or rename into is the file its merged-in parent has at that path. A path
/// first seen without an addition is a file from before the commits, the same
/// one on every line. No two changes in one commit are then one file.
pub(super) struct FileLineages {
    /// The file of each change: indexed by the commit's position, then by the
    /// change's position in that commit's diff. One row per commit, as long as
    /// its diff.
    of: Vec<Vec<FileId>>,
    /// The path each file was first seen under, indexed by its `FileId`.
    labels: Vec<PathBuf>,
}

/// Which file is at each path after a commit: what its children start from.
#[derive(Debug, Clone, Default)]
struct PathFiles {
    /// Each path's file, and whether it is present (`true`) or was deleted
    /// there and is remembered for a restore.
    files: HashMap<PathBuf, (FileId, bool)>,
    /// The path each present file is at: tells a deleted file that lives on
    /// under another path from one a later addition can restore, and a file a
    /// merged-in line moved from one it left in place.
    present_at: HashMap<FileId, PathBuf>,
}

impl PathFiles {
    /// The file present at `path`.
    fn present(&self, path: &Path) -> Option<FileId> {
        self.files
            .get(path)
            .filter(|&&(_, present)| present)
            .map(|&(id, _)| id)
    }

    /// The file at `path`, present or deleted.
    fn file_at(&self, path: &Path) -> Option<FileId> {
        self.files.get(path).map(|&(id, _)| id)
    }

    /// The file deleted at `path`, unless it is present at another path.
    fn deleted_file_at(&self, path: &Path) -> Option<FileId> {
        match self.files.get(path) {
            Some(&(id, false)) if !self.present_at.contains_key(&id) => Some(id),
            _ => None,
        }
    }

    /// Put file `id` at `path`, replacing whatever was there.
    fn place(&mut self, path: &Path, id: FileId) {
        self.remove(path);
        self.files.insert(path.to_path_buf(), (id, true));
        self.present_at.insert(id, path.to_path_buf());
    }

    /// Mark file `id` deleted at `path` but keep it there, so a later commit
    /// adding a file at `path` restores this one instead of starting anew.
    fn mark_deleted(&mut self, path: &Path, id: FileId) {
        self.remove(path);
        self.files.insert(path.to_path_buf(), (id, false));
    }

    /// Forget whatever is at `path`: the old path of a rename, which keeps
    /// nothing to restore, or a path about to get another file.
    fn remove(&mut self, path: &Path) {
        if let Some((id, true)) = self.files.remove(path)
            && self.present_at.get(&id).is_some_and(|at| at == path)
        {
            self.present_at.remove(&id);
        }
    }
}

/// The merged-in parents' path-files of a merge, for what it brings in.
struct MergedIn<'a>(Vec<&'a PathFiles>);

impl MergedIn<'_> {
    /// The file a merged-in parent has present at `path`.
    fn file_at(&self, path: &Path) -> Option<FileId> {
        self.0.iter().find_map(|parent| parent.present(path))
    }

    /// The file a merged-in parent has at `path`, if that parent moved file
    /// `id` from there to another path.
    fn replacing(&self, id: FileId, path: &Path) -> Option<FileId> {
        self.0.iter().find_map(|parent| {
            let elsewhere = parent.present_at.get(&id).is_some_and(|at| at != path);
            elsewhere.then(|| parent.present(path)).flatten()
        })
    }
}

/// Each commit's path-files, kept until the last child has started from them.
struct PathFilesStore {
    path_files: Vec<Option<PathFiles>>,
    /// How many children have yet to start from each commit's path-files.
    children: Vec<usize>,
}

impl PathFilesStore {
    fn new(children_of: &[Vec<CommitPos>]) -> Self {
        PathFilesStore {
            path_files: vec![None; children_of.len()],
            children: children_of.iter().map(Vec::len).collect(),
        }
    }

    /// What a commit with first parent `first` starts from.
    fn start_from(&mut self, first: Option<CommitPos>) -> PathFiles {
        match first {
            Some(parent) if self.children[parent.0] == 1 => self.path_files[parent.0].take(),
            Some(parent) => self.path_files[parent.0].clone(),
            None => None,
        }
        .unwrap_or_default()
    }

    fn merged_in(&self, others: &[CommitPos]) -> MergedIn<'_> {
        MergedIn(
            others
                .iter()
                .filter_map(|parent| self.path_files[parent.0].as_ref())
                .collect(),
        )
    }

    /// Keep `commit`'s path-files, and drop its parents' once no child still
    /// needs them.
    fn keep(&mut self, commit: CommitPos, files: PathFiles, parents: &Parents) {
        self.path_files[commit.0] = Some(files);
        for parent in parents.all() {
            self.children[parent.0] -= 1;
            if self.children[parent.0] == 0 {
                self.path_files[parent.0] = None;
            }
        }
    }
}

impl FileLineages {
    pub(super) fn new(commit_diffs: &[CommitDiff]) -> Self {
        let mut lineages = FileLineages {
            of: vec![Vec::new(); commit_diffs.len()],
            labels: Vec::new(),
        };
        let parents = parent_positions(commit_diffs);
        let children_of = children_of(&parents);
        let mut store = PathFilesStore::new(&children_of);
        let mut before_range: HashMap<PathBuf, FileId> = HashMap::new();
        for commit in parent_first_order(&parents, &children_of) {
            let parents = &parents[commit.0];
            let mut files = store.start_from(parents.first);
            let merged_in = store.merged_in(&parents.others);
            lineages.of[commit.0] = lineages.change_files(
                &commit_diffs[commit.0],
                &mut files,
                &merged_in,
                &mut before_range,
            );
            store.keep(commit, files, parents);
        }
        lineages
    }

    /// The file each change of `diff` belongs to, updating `files` from the
    /// first parent's path-files to the commit's own.
    fn change_files(
        &mut self,
        diff: &CommitDiff,
        files: &mut PathFiles,
        merged_in: &MergedIn,
        before_range: &mut HashMap<PathBuf, FileId>,
    ) -> Vec<FileId> {
        // What a commit takes away goes first, so a path it empties is free
        // for what it adds.
        let (removed, deleted_now) = self.remove_changes(diff, files, merged_in, before_range);
        diff.files
            .iter()
            .zip(removed)
            .map(|(file, removed)| match removed {
                Some(deleted) if file.status == crate::DeltaStatus::Deleted => deleted,
                _ => {
                    let path = file
                        .new_path
                        .as_deref()
                        .or(file.old_path.as_deref())
                        .expect("libgit2 sets a path on every change");
                    let id = removed.unwrap_or_else(|| {
                        self.placed_file(file, path, files, merged_in, &deleted_now, before_range)
                    });
                    files.place(path, id);
                    id
                }
            })
            .collect()
    }

    /// Take away what `diff` renames from and deletes, and return the file of
    /// each such change, and the paths it deletes.
    fn remove_changes<'d>(
        &mut self,
        diff: &'d CommitDiff,
        files: &mut PathFiles,
        merged_in: &MergedIn,
        before_range: &mut HashMap<PathBuf, FileId>,
    ) -> (Vec<Option<FileId>>, HashSet<&'d Path>) {
        let mut ids: Vec<Option<FileId>> = vec![None; diff.files.len()];
        let mut deleted_now: HashSet<&Path> = HashSet::new();
        for (change, file) in diff.files.iter().enumerate() {
            if let Some(old) = renamed_from(file) {
                let new = file.new_path.as_deref().unwrap_or(old);
                let id = merged_in
                    .file_at(new)
                    .or_else(|| files.file_at(old))
                    .unwrap_or_else(|| self.before_range(before_range, old));
                files.remove(old);
                ids[change] = Some(id);
            } else if file.status == crate::DeltaStatus::Deleted
                && let Some(path) = file.old_path.as_deref().or(file.new_path.as_deref())
            {
                let id = files
                    .file_at(path)
                    .unwrap_or_else(|| self.before_range(before_range, path));
                files.mark_deleted(path, id);
                deleted_now.insert(path);
                ids[change] = Some(id);
            }
        }
        (ids, deleted_now)
    }

    /// The file a change that neither renames nor deletes puts at `path`.
    fn placed_file(
        &mut self,
        file: &crate::FileDiff,
        path: &Path,
        files: &PathFiles,
        merged_in: &MergedIn,
        deleted_now: &HashSet<&Path>,
        before_range: &mut HashMap<PathBuf, FileId>,
    ) -> FileId {
        if is_addition(file) {
            return merged_in.file_at(path).unwrap_or_else(|| {
                let restored = (!deleted_now.contains(path))
                    .then(|| files.deleted_file_at(path))
                    .flatten();
                restored.unwrap_or_else(|| self.add(path))
            });
        }
        match files.file_at(path) {
            // A merged-in line may have moved this path's file away and put
            // another one here.
            Some(id) => merged_in.replacing(id, path).unwrap_or(id),
            None => merged_in
                .file_at(path)
                .unwrap_or_else(|| self.before_range(before_range, path)),
        }
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

    /// The file change `change` of commit `commit` belongs to.
    pub(super) fn of(&self, commit: CommitPos, change: ChangePos) -> FileId {
        self.of[commit.0][change.0]
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

/// A commit's parents among the commits: the first, which its diff is
/// against, and the others a merge brings in.
struct Parents {
    first: Option<CommitPos>,
    others: Vec<CommitPos>,
}

impl Parents {
    fn all(&self) -> impl Iterator<Item = CommitPos> + '_ {
        self.first.iter().chain(&self.others).copied()
    }
}

/// Each commit's parents, by position. The uncommitted rows have no parents
/// and continue from the one listed before them, as does every commit of a
/// list made without parents. A root, or a commit whose parents lie outside
/// the commits, starts afresh.
fn parent_positions(commit_diffs: &[CommitDiff]) -> Vec<Parents> {
    let position: HashMap<&Oid, CommitPos> = commit_diffs
        .iter()
        .enumerate()
        .filter_map(|(idx, diff)| Some((diff.commit.oid.as_oid()?, CommitPos(idx))))
        .collect();
    let without_parents = commit_diffs
        .iter()
        .all(|diff| diff.commit.parent_oids.is_empty());
    commit_diffs
        .iter()
        .enumerate()
        .map(|(idx, diff)| {
            let oids = &diff.commit.parent_oids;
            if oids.is_empty() {
                let continues = diff.commit.oid.is_synthetic() || without_parents;
                return Parents {
                    first: idx.checked_sub(1).filter(|_| continues).map(CommitPos),
                    others: Vec::new(),
                };
            }
            let mut listed = oids.iter().map(|oid| position.get(oid).copied());
            Parents {
                first: listed.next().flatten(),
                others: listed.flatten().collect(),
            }
        })
        .collect()
}

/// Each commit's children among the commits, by position.
fn children_of(parents: &[Parents]) -> Vec<Vec<CommitPos>> {
    let mut children: Vec<Vec<CommitPos>> = vec![Vec::new(); parents.len()];
    for (idx, parents) in parents.iter().enumerate() {
        for parent in parents.all() {
            children[parent.0].push(CommitPos(idx));
        }
    }
    children
}

/// The commits in an order that puts every parent before its children:
/// commit times need not grow from parent to child, so a date-ordered list
/// can hold a commit before its parent. Ties keep the list's order.
fn parent_first_order(parents: &[Parents], children_of: &[Vec<CommitPos>]) -> Vec<CommitPos> {
    let mut waiting: Vec<usize> = parents
        .iter()
        .map(|parents| parents.all().count())
        .collect();
    let mut ready: BinaryHeap<Reverse<CommitPos>> = (0..parents.len())
        .filter(|&idx| waiting[idx] == 0)
        .map(|idx| Reverse(CommitPos(idx)))
        .collect();
    let mut order = Vec::with_capacity(parents.len());
    while let Some(Reverse(commit)) = ready.pop() {
        order.push(commit);
        for &child in &children_of[commit.0] {
            waiting[child.0] -= 1;
            if waiting[child.0] == 0 {
                ready.push(Reverse(child));
            }
        }
    }
    debug_assert_eq!(order.len(), parents.len(), "parent edges form a cycle");
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
