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

//! Tests for `Git2Repo::list_ref_names`, which backs shell completion of the
//! `base` argument (branch/tag candidates).

#[allow(dead_code)]
mod common;

#[test]
fn list_ref_names_includes_branches_and_tags() {
    let test = common::TestRepo::new();
    let c1 = test.commit_file("file.txt", "1", "init");
    test.create_branch("feature/login", c1);
    test.create_tag("v1.2.3", c1);

    let names = test.git_repo().list_ref_names().unwrap();

    assert!(
        names.contains(&"feature/login".to_string()),
        "expected the local branch in {names:?}"
    );
    assert!(
        names.contains(&"v1.2.3".to_string()),
        "expected the tag in {names:?}"
    );
}
