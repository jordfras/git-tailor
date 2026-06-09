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

#[allow(dead_code)]
mod common;

#[path = "split_commit/dirty_state.rs"]
mod dirty_state;
#[path = "split_commit/multi_path.rs"]
mod multi_path;
#[path = "split_commit/out_file.rs"]
mod out_file;
#[path = "split_commit/per_file.rs"]
mod per_file;
#[path = "split_commit/per_hunk.rs"]
mod per_hunk;
#[path = "split_commit/per_hunk_group.rs"]
mod per_hunk_group;
#[path = "split_commit/root_commit.rs"]
mod root_commit;
