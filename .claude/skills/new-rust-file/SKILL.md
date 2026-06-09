---
description: Create a new Rust source file with the project license header and module wiring. Use when adding a new .rs file to src/.
argument-hint: <src/path/to/file.rs>
disable-model-invocation: true
allowed-tools: Read Edit Write Bash(grep *)
---

Create a new Rust source file at the path given in $ARGUMENTS, following project conventions.

**1. Validate the path** — must be a `.rs` path under `src/` or `tests/`. For `src/`, reject paths ending in `mod.rs` (project uses Rust 2018+ module style). `mod.rs` is allowed under `tests/` for shared test helpers (e.g. `tests/common/mod.rs`). If no argument is given, ask for the path.

**2. Write the file** with this exact Apache-2.0 header, then a blank line, then leave the rest empty (just the header + trailing newline) unless the user described what the module should contain:

```
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
```

**3. Wire up the module declaration** — find the parent `.rs` file and add `pub mod <name>;` (or `mod <name>;`) in the right place:
- `src/views/foo.rs` → add to `src/views.rs`
- `src/repo/bar.rs` → add to `src/repo.rs`
- `src/baz.rs` → add to `src/lib.rs` (or `src/main.rs` for binary-only)
- `tests/common/foo.rs` → add to `tests/common/mod.rs`
- Top-level files directly under `tests/` (e.g. `tests/foo.rs`) are integration test entry points and need no module declaration.

Insert alphabetically among existing `mod` declarations.

**4. Report** — show the new file path and the line added to the parent module (if any).
