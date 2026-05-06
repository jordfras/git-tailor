# TASKS Checklist

Guidelines:
- Each task line: `- [ ] T### P? category - Title (Flags: ...)`
- Priorities: P0 (urgent) → P3 (low).
- Categories: bug | feat | fix | idea | human.
- Flags (optional): CLARIFICATION, HUMAN INPUT, HUMAN TASK, DUPLICATE.
- Mark completion by [ ] → [X]. Keep changes atomic (one commit per task).
- Mark won't-do tasks by [ ] → [-] and add `WONT DO` to Flags.
- Completed tasks are archived in TASKS-COMPLETED.md.


## UNCATEGORIZED

## Interactivity — Commit List & Operations
- [ ] T190 P2 feat - Support dropping the root commit: currently `drop_commit`
  bails with "Cannot drop a merge or root commit" when `commit.parent_count()`
  `== 0`; update `drop_op.rs` to handle the root case separately — collect all
  descendants, make the first descendant an orphan root commit (using its
  existing tree and metadata, reusing the `plan_move_root_to_later` pattern from
  `move_op.rs`), then cherry-pick the rest of the chain on top; split the
  parent-count guard into two branches: `parent_count > 1` bails with "Cannot
  drop a merge commit", `parent_count == 0` takes the root path, `parent_count
  == 1` is the existing fast path; also update `validate_single_parent_op` (or
  introduce a separate `validate_non_merge_op`) if the refactored helper from
  T178 makes the split guard awkward; add a test in
  `tests/drop_commit/root_commit.rs` that verifies the root commit is dropped
  and the history is correctly rewritten.

## Interactivity — Commit Detail View
- [ ] T138 P3 feat - Add syntax highlighting to diff code in commit detail view:
  use `syntect` (already a transitive dependency) to highlight the code portions
  of diff hunks based on the file extension / language; convert syntect's
  `(Style, &str)` token pairs to ratatui `Span`s with mapped foreground colors;
  diff-specific styling (green/red for added/removed lines, hunk headers) should
  remain and take precedence — syntax colors apply to the code content within
  those lines; add a `syntect::parsing::SyntaxSet` and
  `syntect::highlighting::ThemeSet` to the application state (loaded once at
  startup) so highlighting is performed per-hunk on demand without re-loading
  assets; consider caching highlighted output per commit to avoid
  re-highlighting on every render
- [ ] T143 P3 feat - Add half-page scrolling to the commit detail view: bind
  `Ctrl-D` / `Ctrl-U` (vim convention) and `Ctrl-PageDown` / `Ctrl-PageUp` to
  scroll approximately half the visible content area at a time; the scroll
  amount should be derived from the current panel height so it stays
  proportional regardless of terminal size
- [ ] T144 P3 feat - Add jump-to-top/bottom keybindings in the commit detail
  view: bind `Home` to scroll to the very first line and `End` to scroll to the
  very last line of the diff content
- [ ] T145 P3 feat - Add horizontal scroll-to-edge keybindings in the commit
  detail view: bind `Ctrl-A` / `Ctrl-E` (emacs convention) and `Ctrl-Home` /
  `Ctrl-End` to scroll the diff content fully left (column 0) or fully right
  (rightmost position) respectively
- [ ] T166 P3 feat - Increase and decrease diff context lines in commit detail
  view with `+` and `-`: pressing `+` should increase the number of context
  lines shown around each hunk (default 3, matching git's default), and `-`
  should decrease it (minimum 0); store the context line count in `AppState` and
  pass it through to `commit_diff` (or re-render the cached diff with the new
  context); changing the value should trigger a re-fetch or re-render of the
  diff so the change is immediately visible; show the current context line count
  in the footer or status line so the user knows the active value
- [ ] T165 P3 feat - Navigate between files in commit detail view by pressing
  `f`: pressing `f` should jump the scroll position to the start of the next
  file's diff block in the commit detail view; pressing `F` (shift) should jump
  to the previous file; the file boundary can be detected from the rendered line
  list (each `FileDiff` entry starts with a file header line); wrap around when
  reaching the end/beginning of the file list so the navigation is cyclic
- [ ] T146 P3 feat - Make the help overlay context-sensitive: pressing `?` (or
  `h`) in the commit detail view should show only the keybindings relevant to
  that view (scrolling, search, navigation back), while pressing it in the
  commit list shows only commit-list bindings; the current single monolithic
  help window is becoming too long as new keybindings are added; implement by
  passing the current `AppMode` to the help renderer and selecting the
  appropriate subset of bindings to display

## CLI — Shell Completion
- [ ] T140 P3 feat - Add shell completion for CLI options: use `clap_complete`
  to generate static completion scripts (bash, zsh, fish) for all flags and
  value_enum variants (e.g. `--squashable-scope`). NOTE: zero-setup completions
  require distribution via a package manager (apt, brew, etc.) that can deposit
  the script in the right system directory at install time; users installing via
  `cargo install` will still need a manual one-time setup step.
- [ ] T141 P3 feat - Add branch/tag completion for the BASE argument: extend the
  completion mechanism from T140 so that the positional `base` argument offers
  branch and tag candidates by querying `git2` for local branches,
  remote-tracking refs, and tags; degrade gracefully if the current directory is
  not inside a git repository. Same distribution requirement as T140.

## CLI Output & Compatibility

## Build & CI
- [ ] T118 P2 feat - Set up GitHub Releases with pre-built binaries: create
  `.github/workflows/release.yml` that triggers on version tags (`v*`), builds
  the `gt` binary for `x86_64-unknown-linux-musl` (fully static, covers WSL2 and
  all Linux distros), `x86_64-pc-windows-msvc` (Windows native), and optionally
  `aarch64-unknown-linux-gnu` and `aarch64-apple-darwin`; use
  `taiki-e/upload-rust-binary-action` to strip, archive, and attach binaries to
  the GitHub Release automatically; the musl target should produce a zero
  shared-library binary (add `RUSTFLAGS=-C target-feature=+crt-static` if
  needed) so no system libs beyond the kernel are required

## Refactoring — TUI Architecture
- [ ] T116 P3 feat - Review codebase for refactoring opportunities: audit
  existing code for duplication, overly complex functions, inconsistent
  patterns, and areas where abstractions could simplify implementation; identify
  specific refactoring targets like extracting common dialog patterns,
  consolidating similar error handling, reducing parameter passing, and
  improving module boundaries; create follow-up tasks for the most impactful
  improvements
- [X] T169 P1 feat - Extract shared list-selector key handling for squash_select
  / move_select / split_select: the three modal pickers in
  `src/views/{squash_select,move_select,split_select}.rs` each implement
  near-identical `handle_key(KeyCommand, &mut AppState) -> AppAction` bodies
  (MoveUp/MoveDown/PageUp/PageDown with index clamping, Confirm, Quit,
  ShowHelp). Extract a `handle_list_navigation(action, cursor: &mut usize, len:
  usize, page_size: usize) -> ListNav` helper (or trait) in a new
  `views/list_nav.rs` (or inside `views/dialog.rs`) that returns `Moved`,
  `Confirmed`, `Cancelled`, `Help`, or `Unhandled`; each picker then becomes a
  small wrapper that maps `Confirmed` to its mode-specific `AppAction`. Should
  remove ~100 LOC of near-duplication and make adding new pickers trivial.
- [X] T170 P1 feat - Reuse `build_conflict_state` across drop / move /
  conflict-continuation paths: `src/repo/git2_impl/squash_op.rs` already defines
  a `build_conflict_state(...)` helper, but `src/repo/git2_impl/drop_op.rs:65`,
  `src/repo/git2_impl/move_op.rs:82`, `src/repo/git2_impl/conflict.rs:41` and
  `src/repo/git2_impl/conflict.rs:87` each construct
  `RebaseOutcome::Conflict(Box::new(ConflictState { ... }))` inline with
  duplicated field-population logic. Promote `build_conflict_state` to
  `src/repo/git2_impl.rs` (or a new `repo/git2_impl/conflict_builder.rs`),
  generalise its parameters to cover all four call sites, and replace the inline
  constructions. Centralises conflict-state assembly so future fields (e.g.
  operation label for the conflict dialog header) only need to be added once.
- [X] T171 P1 feat - Consolidate `render_squash_footer` and `render_move_footer`
  into a single `render_action_footer`: `src/views/commit_list.rs:726` and
  `src/views/commit_list.rs:769` are ~85% identical — both truncate the
  source-commit summary to the available width, build a `Line` with key-hint
  spans (Enter / Esc), and apply the same dim/footer styling; only the action
  label and the instruction text differ. Replace both with a single
  `render_action_footer(frame, app, area, label: &str, source_oid, instructions:
  &[(&str, &str)])` helper and call it from both call sites (lines 684 and 693).
  Reduces ~40 LOC and ensures squash/move footers stay visually consistent.
- [X] T172 P1 feat - Split `dispatch_action` in main.rs into per-AppAction
  helper functions: `src/main.rs:248` defines `dispatch_action` as a ~290-line
  `match` over `AppAction` where each arm contains 20–40 lines of side-effect
  logic (PrepareSplit, ExecuteSplit, PrepareReword, PrepareSquash, PrepareMove,
  …). Extract each non-trivial arm into a private
  `fn handle_<action>(...) -> Result<LoopAction>` helper so `dispatch_action`
  becomes a thin dispatcher (~80 LOC) where each branch is one function call.
  Use the existing `LoopAction` / `get_head_oid_or_continue!` infrastructure; do
  not change behaviour. Greatly improves navigability of the event loop and
  makes individual actions easier to reason about and test.
- [X] T173 P2 feat - Split `app.rs` into `app/state.rs` + `app/keymap.rs`:
  `src/app.rs` (876 lines) currently mixes three concerns — the `AppState`
  struct and its many helper methods (move_*, scroll_*, page_*, set_message, …),
  the `AppMode` state-machine enum and its transitions, and the `KeyCommand`
  enum together with `AppMode::parse_key` / `read_event`. Convert `app.rs` to a
  module declaration that owns `AppMode`, `AppAction`, and `SplitStrategy`, move
  `AppState` and its inherent impls to `src/app/state.rs`, and move
  `KeyCommand`, `parse_key`, and `read_event` to `src/app/keymap.rs`. Re-export
  so external callers (`main.rs`, `views/*`) need no import changes. No
  behaviour change.
- [X] T174 P2 fix - Replace hand-rolled scrollbars in commit_detail and dialog
  with ratatui's built-in `Scrollbar` widget: `src/views/commit_detail.rs`
  contains two custom `Paragraph`-based implementations — `render_scrollbar`
  (vertical, ~45 LOC) and `render_h_scrollbar` (horizontal, ~40 LOC) — that
  manually build `"█"` / `"│"` / `"─"` character strings; `src/views/dialog.rs`
  has a third, `render_dialog_scrollbar` (~35 LOC), with the same approach.
  `commit_list.rs` and `hunk_groups.rs` already use
  `ratatui::widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState}`
  correctly. Replace the three custom implementations with the same ratatui
  widget (using `VerticalLeft`, `VerticalRight`, or `HorizontalBottom` as
  appropriate); the two-pass layout geometry in `commit_detail.rs` that
  determines scrollbar area sizes must be kept — only the rendering step
  changes. Removes ~120 LOC of duplicated thumb-sizing arithmetic and aligns all
  scrollbars on a single rendering path.
- [X] T175 P2 feat - Extract cherry-pick helpers from `repo/git2_impl.rs` into
  `repo/git2_impl/cherry_pick.rs`: `src/repo/git2_impl.rs` (517 lines) currently
  houses the trait impl plus `cherry_pick_chain` (line 433),
  `rebase_descendants` (line 333), `collect_descendants` (line 402), and the
  internal `CherryPickResult` type (line 510). These are the shared rebase
  primitives consumed by drop / move / squash / split ops and form a cohesive
  sub-module of their own. Move them (plus any required helpers) to a new
  `src/repo/git2_impl/cherry_pick.rs`, expose them through `pub(super)` items,
  and re-export from `git2_impl.rs`. Brings `git2_impl.rs` closer to its
  trait-impl role and improves the mental model around rebase orchestration.
- [x] T176 P2 feat - Introduce a `Dialog` builder to reduce dialog boilerplate:
  Added `Dialog` struct to `src/views/dialog.rs` with a fluent builder API:
  `blank()`, `title()`, `section()`, `styled_line()`, `plain()`, `wrapped()`,
  `wrapped_indent()`, `wrapped_styled()`, `wrapped_styled_bold()`,
  `key_binding()`, `instructions()`, `push_line()`, and `render()`. `title()`
  adds surrounding blank lines implicitly; `section()` adds only a leading
  blank; `render()` pads the border title with spaces automatically. Refactored
  `drop.rs`, `conflict.rs`, `help.rs`, and `split_select.rs` to use it, removing
  ~55 net lines of repetitive span/style construction.
- [X] T177 P3 feat - Move domain types from `lib.rs` into a `domain/` submodule
  tree: `src/lib.rs` currently mixes the public domain types (`CommitInfo`,
  `FileDiff`, `Hunk`, `DiffLine`, `CommitDiff`, `DeltaStatus`, `DiffLineKind`,
  `Oid`, `VirtualOid`) with the module declarations and re-exports. Split into
  `src/domain/commit.rs` (commit + oid types) and `src/domain/diff.rs`
  (diff/hunk/line types), then re-export from `lib.rs` so external imports
  remain unchanged. Keeps `lib.rs` focused on crate-level wiring.
- [-] T178 P3 feat - Extract `validate_operation_preconditions` for drop / move
  ops: `src/repo/git2_impl/drop_op.rs` and `src/repo/git2_impl/move_op.rs` both
  open with the same prelude — call `check_no_dirty_state`, parse the commit and
  head OIDs, look up the commit objects, validate parent count (single-parent
  only). Extract a `fn validate_single_parent_op(repo, commit_oid, head_oid) ->
  Result<(Commit, Commit)>` helper in `git2_impl.rs`. Saves ~10 LOC and removes
  a class of copy-paste hazards. (Flags: WONT DO)
- [-] T179 P3 feat - Extract list-view scroll/selection helpers shared by
  commit_list and commit_detail: `src/views/commit_list.rs` (832 lines)
  and `src/views/commit_detail.rs` (822 lines) both implement scroll-
  bound clamping, page-size-derived navigation, and selection /
  scroll-offset coupling. Introduce a small `views/list_view.rs` with
  `compute_scroll_bounds(content_height, visible_height) ->
  (max_scroll, clamped_offset)` and a `ListRenderContext { selection_idx,
  scroll_offset, visible_height }` helper used by both; sets the
  pattern for any future scrolling list view. (Flags: WONT DO)
- [X] T180 P2 feat - Extract `compute_page_size` helper in app.rs: the idiom
  `visible_height.saturating_sub(1).max(1)` (keep at least one line of overlap
  when paging) is repeated at `src/app.rs:485`, `:494`, `:501`, `:507`, `:737`,
  `:743` (the dialog variant uses `dialog_visible_height` but the same
  arithmetic). Extract a small free function
  `fn page_size(visible_height: usize) -> usize` (with a doc comment explaining
  the one-line overlap rule) and call it from all six sites; remove the inline
  `// Keep at least one line overlap` comments now that the name documents the
  intent.
- [ ] T181 P2 feat - Extract scroll-offset clamping helper: the pattern
  `.min(max_scroll)` for keeping a scroll offset within bounds appears in
  `src/app.rs` and several places in `src/views/commit_detail.rs` (around lines
  179, 295, 431, 432) and dialog scroll handling. Add a
  `fn clamp_scroll(offset: usize, max: usize) -> usize` helper (or
  `AppState::clamp_*_scroll` methods that wrap the field accesses) and use at
  all clamping sites. Reduces the chance of forgetting the clamp on a new code
  path.
- [ ] T182 P2 feat - Add `VirtualOid::expect_real_oid()` (or `real_oid_cloned`)
  to eliminate `.as_oid().unwrap().clone()` chains: the pattern
  `commit.oid.as_oid().unwrap().clone()` appears at
  `src/views/commit_list.rs:100`, `:112`, `src/views/squash_select.rs:92`,
  `:93`, and `src/views/move_select.rs:108`. Add a method on `VirtualOid` such
  as `pub fn expect_real_oid(&self, ctx: &str) -> Oid` that clones the inner
  `Oid` or panics with a clear message if the variant is synthetic. Replace all
  five call sites; provides a single, well-named audit point if
  synthetic-vs-real handling ever needs revisiting.
- [ ] T183 P3 feat - Replace `forward: bool` parameter with a `SearchDirection`
  enum: `advance_search_match(app: &mut AppState, forward:
  bool)` at `src/views/commit_detail.rs:153` is called with raw `true` /
  `false`, losing meaning at the call site. Define
  `enum SearchDirection { Next, Prev }` (in `app.rs` or
  `views/commit_detail.rs`) and use it instead so call sites read
  `advance_search_match(app, SearchDirection::Next)`. Trivial change but
  improves grep-ability and readability.
- [ ] T184 P3 feat - Extract `next_match_index` pure helper for search cycling:
  the wrap-around modulo arithmetic in `advance_search_match` at
  `src/views/commit_detail.rs:157-166` mixes cursor cycling logic with
  `AppState` mutation. Extract `fn next_match_index(current: Option<usize>, len:
  usize, dir: SearchDirection) -> usize` (combine with T183) as a pure function
  so the cycling logic can be unit tested independently from the AppState
  plumbing.
- [ ] T185 P3 feat - Extract `diff_path_with_prefix` helper for diff file
  headers: `src/views/commit_detail.rs:569-575` repeats
  `path.map(|s| format!("X/{}", s)).unwrap_or_else(|| "/dev/null".to_string())`
  for both `a/` and `b/` prefixes when rendering the `--- a/foo.rs` /
  `+++ b/foo.rs` diff header lines. Extract `fn diff_path_with_prefix(path:
  Option<&str>, prefix: &str) -> String` and call it twice; also defines a
  single place to change the `/dev/null` sentinel if needed.
- [ ] T186 P3 feat - Introduce `DIALOG_BORDER_HEIGHT` / `DIALOG_BORDER_WIDTH`
  constants in `views/dialog.rs`: hardcoded `saturating_sub(2)` / `+ 2`
  arithmetic representing the top+bottom (or left+right) border occupies dialog
  inner-area calculations at `src/views/dialog.rs:45`, `:46`, `:80`. Define
  `const DIALOG_BORDER_HEIGHT: u16 = 2;` (and width if applicable) at module top
  and replace the magic `2`s. Also a good template for future per-view layout
  constants.
- [ ] T187 P3 feat - Replace `"staged"` / `"unstaged"` string literals in
  `VirtualOid` with named constants: the labels appear at `src/lib.rs:88` and
  `:98` (and in doc comments at lines 72-74) for `VirtualOid::Staged` /
  `VirtualOid::Unstaged` rendering. Define
  `const STAGED_LABEL: &str = "staged";` and `const UNSTAGED_LABEL: &str =
  "unstaged";` at the top of the relevant impl block (or near the `VirtualOid`
  definition) and reference them from both arms, ensuring the two methods cannot
  drift out of sync.
- [ ] T188 P3 feat - Introduce `ORIGIN_HEAD_REF` constant in
  `repo/git2_impl/reads.rs`: the magic string `"refs/remotes/origin/HEAD"` is
  hardcoded inside `find_reference("refs/remotes/origin/HEAD")` at
  `src/repo/git2_impl/reads.rs:209`, while related doc comments in
  `src/repo.rs:362-369` and `src/cli.rs:32-33` reference the same ref shape. Add
  `const ORIGIN_HEAD_REF: &str = "refs/remotes/origin/HEAD";` at the top of
  `reads.rs` and use it; if the value ever needs to change (e.g. for a
  non-`origin` remote default), there is one place to update.
- [ ] T189 P3 feat - Switch `AppState` to `#[derive(Default)]`: the hand-written
  `impl Default for AppState` at `src/app.rs:375-410` enumerates ~25 fields,
  almost all of which already have natural zero/empty defaults. The only
  obstacle is `reference_oid: Oid::from("")` — add `impl Default for Oid`
  (returning the empty-string variant with the existing semantics) so `AppState`
  can be derived. Reduces ~30 lines of mechanical boilerplate and means new
  fields with `Default` types no longer require touching the constructor.

## Notes
