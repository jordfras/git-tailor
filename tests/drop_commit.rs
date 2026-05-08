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

#[path = "drop_commit/auto_stage.rs"]
mod auto_stage;
#[path = "drop_commit/conflict.rs"]
mod conflict;
#[path = "drop_commit/continue_abort.rs"]
mod continue_abort;
#[path = "drop_commit/dirty_state.rs"]
mod dirty_state;
#[path = "drop_commit/error_cases.rs"]
mod error_cases;
#[path = "drop_commit/happy_path.rs"]
mod happy_path;
#[path = "drop_commit/root_commit.rs"]
mod root_commit;
