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

use std::fmt;

/// A real git object identifier (40-hex-char SHA).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Oid(String);

impl Oid {
    /// Construct an `Oid` from any owned `String`.
    pub fn new(s: String) -> Self {
        Oid(s)
    }

    /// First 8 characters of the SHA, suitable for display.
    pub fn short(&self) -> &str {
        if self.0.len() >= 8 {
            &self.0[..8]
        } else {
            &self.0
        }
    }

    /// Full hex SHA string.
    pub fn long(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Oid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for Oid {
    fn from(s: String) -> Self {
        Oid(s)
    }
}

impl From<&str> for Oid {
    fn from(s: &str) -> Self {
        Oid(s.to_owned())
    }
}

/// An OID that may refer to a real git commit or a synthetic working-tree row.
///
/// The TUI shows "staged" and "unstaged" pseudo-commits alongside real commits.
/// `VirtualOid` unifies the two cases so callers never need to compare raw
/// strings against `"staged"` / `"unstaged"`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum VirtualOid {
    Real(Oid),
    Staged,
    Unstaged,
}

impl VirtualOid {
    /// Full string representation: hex SHA for real commits, fixed label for
    /// synthetic rows.
    pub fn long(&self) -> &str {
        match self {
            VirtualOid::Real(oid) => oid.long(),
            VirtualOid::Staged => "staged",
            VirtualOid::Unstaged => "unstaged",
        }
    }

    /// Short display string: 8-char SHA for real commits, fixed label for
    /// synthetic rows.
    pub fn short(&self) -> &str {
        match self {
            VirtualOid::Real(oid) => oid.short(),
            VirtualOid::Staged => "staged",
            VirtualOid::Unstaged => "unstaged",
        }
    }

    /// Returns `true` for the `Staged` and `Unstaged` variants.
    pub fn is_synthetic(&self) -> bool {
        matches!(self, VirtualOid::Staged | VirtualOid::Unstaged)
    }

    /// Returns the inner `Oid` if this is a real commit, or `None` for synthetic rows.
    pub fn as_oid(&self) -> Option<&Oid> {
        if let VirtualOid::Real(oid) = self {
            Some(oid)
        } else {
            None
        }
    }

    /// Returns a clone of the inner `Oid`, panicking if this is a synthetic row.
    ///
    /// Use at call sites that have already verified the commit is real (e.g. guarded by
    /// `is_synthetic()` checks upstream).
    pub fn expect_real_oid(&self) -> Oid {
        match self {
            VirtualOid::Real(oid) => oid.clone(),
            _ => panic!("unexpected synthetic commit"),
        }
    }
}

impl From<Oid> for VirtualOid {
    fn from(oid: Oid) -> Self {
        VirtualOid::Real(oid)
    }
}

impl fmt::Display for VirtualOid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.long())
    }
}

/// Represents commit metadata extracted from git repository.
///
/// This is a pure data structure containing commit information
/// without any git2 object dependencies.
#[derive(Debug, Clone)]
pub struct CommitInfo {
    pub oid: VirtualOid,
    pub summary: String,
    /// Author name. `None` for synthetic pseudo-commits (staged/unstaged changes).
    pub author: Option<String>,
    /// Raw commit timestamp (seconds since epoch). `None` for synthetic pseudo-commits.
    pub date: Option<String>,
    pub parent_oids: Vec<Oid>,
    /// Full commit message including body (all lines).
    pub message: String,
    /// Author email address. `None` for synthetic pseudo-commits.
    pub author_email: Option<String>,
    /// Author date with timezone. `None` for synthetic pseudo-commits.
    pub author_date: Option<time::OffsetDateTime>,
    /// Committer name. `None` for synthetic pseudo-commits.
    pub committer: Option<String>,
    /// Committer email address. `None` for synthetic pseudo-commits.
    pub committer_email: Option<String>,
    /// Commit date with timezone. `None` for synthetic pseudo-commits.
    pub commit_date: Option<time::OffsetDateTime>,
}
