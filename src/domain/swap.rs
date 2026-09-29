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

//! Swap groups: the changes in one diff that a split has to keep together.

use std::collections::BTreeMap;
use std::ops::Bound;
use std::path::Path;

use super::diff::{DeltaStatus, FileDiff};

/// The changes of one diff that collide: one takes a path away and another
/// puts back that path, a path above it or a path below it — a directory
/// replaced by a file, a file by a directory, a binary file by a symlink. No
/// tree holds both sides of such a pair, so a split piece has either every
/// change of a group or none of them.
#[derive(Debug, Default)]
pub struct SwapGroups {
    group_of: Vec<Option<usize>>,
    groups: Vec<Vec<usize>>,
}

impl SwapGroups {
    /// Group the changes of a diff, given in diff order as their status and
    /// old and new paths.
    pub fn new<'p>(
        changes: impl IntoIterator<Item = (DeltaStatus, Option<&'p Path>, Option<&'p Path>)>,
    ) -> Self {
        let sides: Vec<(Option<&Path>, Option<&Path>)> = changes
            .into_iter()
            .map(|(status, old, new)| match status {
                DeltaStatus::Added | DeltaStatus::Copied => (None, new),
                DeltaStatus::Deleted => (old, None),
                DeltaStatus::Renamed => (old, new),
                _ => (None, None),
            })
            .collect();
        let put_back: BTreeMap<&Path, usize> = sides
            .iter()
            .enumerate()
            .filter_map(|(change, &(_, added))| Some((added?, change)))
            .collect();

        let mut parent: Vec<usize> = (0..sides.len()).collect();
        for (change, &(taken, _)) in sides.iter().enumerate() {
            let Some(taken) = taken else { continue };
            let at_or_above = taken
                .ancestors()
                .filter_map(|path| put_back.get(path).copied());
            // Paths order by component, so everything under `taken` directly
            // follows it.
            let below = put_back
                .range::<Path, _>((Bound::Excluded(taken), Bound::Unbounded))
                .take_while(|(path, _)| path.starts_with(taken))
                .map(|(_, &other)| other);
            for other in at_or_above.chain(below).collect::<Vec<_>>() {
                let (a, b) = (root(&mut parent, change), root(&mut parent, other));
                parent[a.max(b)] = a.min(b);
            }
        }

        let roots: Vec<usize> = (0..sides.len()).map(|c| root(&mut parent, c)).collect();
        let mut size = vec![0usize; sides.len()];
        for &change_root in &roots {
            size[change_root] += 1;
        }
        let mut group_of = vec![None; sides.len()];
        let mut groups: Vec<Vec<usize>> = Vec::new();
        let mut group_by_root: BTreeMap<usize, usize> = BTreeMap::new();
        for (change, &change_root) in roots.iter().enumerate() {
            if size[change_root] < 2 {
                continue;
            }
            let group = *group_by_root.entry(change_root).or_insert_with(|| {
                groups.push(Vec::new());
                groups.len() - 1
            });
            groups[group].push(change);
            group_of[change] = Some(group);
        }
        Self { group_of, groups }
    }

    /// Group the changes of `files`.
    pub fn of_files(files: &[FileDiff]) -> Self {
        Self::new(files.iter().map(|file| {
            (
                file.status,
                file.old_path.as_deref(),
                file.new_path.as_deref(),
            )
        }))
    }

    /// The group change `change` belongs to, if it collides with any other.
    pub fn group_of(&self, change: usize) -> Option<usize> {
        self.group_of.get(change).copied().flatten()
    }

    /// Every group's changes in diff order, the groups ordered by their first.
    pub fn groups(&self) -> &[Vec<usize>] {
        &self.groups
    }

    /// The changes of the group `change` belongs to, or `change` alone.
    pub fn with_group(&self, change: usize) -> Vec<usize> {
        match self.group_of(change) {
            Some(group) => self.groups[group].clone(),
            None => vec![change],
        }
    }

    /// The changes in diff order, each group gathered where its first change
    /// sits: the units a split can place without tearing a swap apart.
    pub fn units(&self) -> Vec<Vec<usize>> {
        (0..self.group_of.len())
            .filter(|&change| {
                self.group_of(change)
                    .is_none_or(|group| self.groups[group][0] == change)
            })
            .map(|change| self.with_group(change))
            .collect()
    }
}

fn root(parent: &mut [usize], mut change: usize) -> usize {
    while parent[change] != change {
        parent[change] = parent[parent[change]];
        change = parent[change];
    }
    change
}

#[cfg(test)]
mod tests {
    use super::*;
    use DeltaStatus::{Added, Deleted, Modified, Renamed};

    fn groups(changes: &[(DeltaStatus, &str, &str)]) -> Vec<Vec<usize>> {
        SwapGroups::new(
            changes
                .iter()
                .map(|&(status, old, new)| (status, Some(Path::new(old)), Some(Path::new(new)))),
        )
        .groups()
        .to_vec()
    }

    #[test]
    fn a_file_replacing_a_directory_groups_with_its_files() {
        assert_eq!(
            groups(&[
                (Modified, "0.txt", "0.txt"),
                (Added, "a", "a"),
                (Deleted, "a/x", "a/x"),
                (Deleted, "a/y", "a/y"),
            ]),
            [vec![1, 2, 3]]
        );
    }

    #[test]
    fn a_directory_replacing_a_file_groups_with_it() {
        assert_eq!(
            groups(&[
                (Deleted, "a", "a"),
                (Added, "a/x", "a/x"),
                (Added, "a/y", "a/y")
            ]),
            [vec![0, 1, 2]]
        );
    }

    #[test]
    fn a_path_deleted_and_added_back_groups() {
        assert_eq!(
            groups(&[(Deleted, "link", "link"), (Added, "link", "link")]),
            [vec![0, 1]]
        );
    }

    #[test]
    fn a_rename_groups_with_what_it_collides_with_on_either_side() {
        assert_eq!(
            groups(&[
                (Renamed, "z", "d"),
                (Deleted, "d/y", "d/y"),
                (Added, "z", "z")
            ]),
            [vec![0, 1, 2]]
        );
    }

    #[test]
    fn a_path_merely_sharing_a_prefix_does_not_collide() {
        assert!(groups(&[(Added, "a", "a"), (Deleted, "a.txt", "a.txt")]).is_empty());
        assert!(groups(&[(Deleted, "a", "a"), (Added, "ab/x", "ab/x")]).is_empty());
    }

    #[test]
    fn units_keep_each_group_where_its_first_change_sits() {
        let swaps = SwapGroups::new(
            [
                (Modified, "0.txt"),
                (Added, "a"),
                (Deleted, "a/x"),
                (Modified, "b.txt"),
            ]
            .map(|(status, path)| (status, Some(Path::new(path)), Some(Path::new(path)))),
        );
        assert_eq!(swaps.units(), [vec![0], vec![1, 2], vec![3]]);
    }
}
