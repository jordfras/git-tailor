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

#[path = "squash_commit/auto_stage.rs"]
mod auto_stage;
#[path = "squash_commit/conflict.rs"]
mod conflict;
#[path = "squash_commit/dirty_state.rs"]
mod dirty_state;
#[path = "squash_commit/happy_path.rs"]
mod happy_path;
#[path = "squash_commit/root_commit.rs"]
mod root_commit;
