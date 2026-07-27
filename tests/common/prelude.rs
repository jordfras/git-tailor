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

// Re-export macros defined at the integration-test crate root (assert_rebase_complete!,
// expect_rebase_conflict!, assert_history!, assert_file_contents!, etc.) so that
// sub-modules only need `use crate::common::prelude::*;` to pull everything in.
pub use crate::*;
pub use git_tailor::app::SquashMode;
pub use git_tailor::repo::{Git2Repo, GitRepo, RebaseOutcome, RepoRead, RepoWrite};
pub use git_tailor::{Oid, VirtualOid};
