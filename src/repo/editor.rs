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

/// Resolve the editor command to use for editing commit messages.
///
/// Walks git's canonical editor lookup chain:
/// 1. `GIT_EDITOR` environment variable
/// 2. `core.editor` git config setting
/// 3. `VISUAL` environment variable
/// 4. `EDITOR` environment variable
/// 5. Fallback: `"vi"`
fn resolve_editor(repo: &git2::Repository) -> String {
    if let Ok(e) = std::env::var("GIT_EDITOR") {
        return e.trim().to_string();
    }

    if let Some(e) = repo
        .config()
        .ok()
        .and_then(|cfg| cfg.get_string("core.editor").ok())
    {
        return e.trim().to_string();
    }

    for var in ["VISUAL", "EDITOR"] {
        if let Ok(e) = std::env::var(var) {
            return e.trim().to_string();
        }
    }

    "vi".to_string()
}
