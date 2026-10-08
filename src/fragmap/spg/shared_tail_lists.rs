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

//! Lists that share their tails, for paths built backwards a node at a time.

use std::cmp::Ordering;

/// A list in a [`SharedTailLists`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct ListId(u32);

impl ListId {
    pub(super) const EMPTY: ListId = ListId(u32::MAX);
}

/// Many lists stored in one vector, where prepending shares the old list as
/// the tail, so extending a path by one node does not copy the rest of it.
pub(super) struct SharedTailLists<T> {
    cells: Vec<(T, ListId)>,
}

impl<T: Copy + Ord> SharedTailLists<T> {
    pub(super) fn new() -> Self {
        SharedTailLists { cells: Vec::new() }
    }

    pub(super) fn prepend(&mut self, head: T, tail: ListId) -> ListId {
        self.cells.push((head, tail));
        ListId((self.cells.len() - 1) as u32)
    }

    pub(super) fn iter(&self, mut list: ListId) -> impl Iterator<Item = T> + '_ {
        std::iter::from_fn(move || {
            let (head, tail) = *self.cells.get(list.0 as usize)?;
            list = tail;
            Some(head)
        })
    }

    pub(super) fn cmp(&self, mut a: ListId, mut b: ListId) -> Ordering {
        while a != b {
            match (a, b) {
                (ListId::EMPTY, _) => return Ordering::Less,
                (_, ListId::EMPTY) => return Ordering::Greater,
                _ => {}
            }
            let (head_a, tail_a) = self.cells[a.0 as usize];
            let (head_b, tail_b) = self.cells[b.0 as usize];
            match head_a.cmp(&head_b) {
                Ordering::Equal => (a, b) = (tail_a, tail_b),
                unequal => return unequal,
            }
        }
        Ordering::Equal
    }
}
