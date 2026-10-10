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

//! Pure planning logic for bulk autofixup (mirrors `git rebase --autosquash`):
//! matching `fixup!`/`squash!`-prefixed commits to the earlier commit their
//! summary names. No git access — operates on already-loaded `CommitInfo`.

use crate::CommitInfo;
use crate::Oid;
use crate::app::SquashMode;
use bstr::{BStr, BString, ByteSlice};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;

/// One `fixup!`/`squash!` commit matched to the target it will be squashed into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutofixupPair {
    pub source_oid: Oid,
    pub target_oid: Oid,
    pub source_summary: String,
    pub target_summary: String,
    /// Full commit message (summary + body) of the source/target, needed to
    /// build the non-interactive squash message; the confirmation dialog
    /// only shows the summaries.
    pub source_message: String,
    pub target_message: String,
    pub mode: SquashMode,
}

/// One target commit and every `fixup!`/`squash!` commit that will be folded
/// into it, for display grouped by target. Purely a view over `AutofixupPair`:
/// the batch runs the pairs one at a time through its [`BatchPlan`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutofixupGroup {
    pub target_oid: Oid,
    pub target_summary: String,
    pub target_message: String,
    /// Oldest-first, same as `plan_autofixup`'s overall order.
    pub sources: Vec<AutofixupPair>,
}

/// A commit's position in the branch a batch was planned against, oldest
/// first. Unlike its OID, it survives the rewrites of the steps before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlannedPos(pub usize);

/// One squash in a planned batch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedStep {
    pub source: PlannedPos,
    pub target: PlannedPos,
    pub mode: SquashMode,
}

/// A batch planned once against the branch as it stood, the way git plans a
/// rebase todo list.
///
/// Each step folds its source into its target and replays everything above,
/// dropping nothing else, so once some steps have landed the branch is these
/// commits minus those steps' sources, in the same order. That is how a
/// planned position finds its commit again after every OID has changed.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BatchPlan {
    /// The branch's commits when the batch was planned, oldest first.
    pub commits: Vec<Oid>,
    pub steps: Vec<PlannedStep>,
}

impl BatchPlan {
    pub fn new(commits: &[CommitInfo], pairs: &[AutofixupPair]) -> Self {
        let commits: Vec<Oid> = commits
            .iter()
            .filter_map(|c| c.oid.as_oid().cloned())
            .collect();
        let pos = |oid: &Oid| {
            PlannedPos(
                commits
                    .iter()
                    .position(|c| c == oid)
                    .expect("a planned pair names listed commits"),
            )
        };
        let steps = pairs
            .iter()
            .map(|pair| PlannedStep {
                source: pos(&pair.source_oid),
                target: pos(&pair.target_oid),
                mode: pair.mode,
            })
            .collect();
        Self { commits, steps }
    }

    /// Where `pos` sits in the branch once the first `landed` steps are in.
    pub fn current_index(&self, pos: PlannedPos, landed: usize) -> usize {
        let removed_below = self.steps[..landed]
            .iter()
            .filter(|step| step.source.0 < pos.0)
            .count();
        pos.0 - removed_below
    }
}

/// Group `pairs` by target, preserving the order each target first appears
/// in and each group's internal (oldest-first) order.
pub fn group_by_target(pairs: &[AutofixupPair]) -> Vec<AutofixupGroup> {
    let mut groups: Vec<AutofixupGroup> = Vec::new();
    for pair in pairs {
        if let Some(group) = groups.iter_mut().find(|g| g.target_oid == pair.target_oid) {
            group.sources.push(pair.clone());
        } else {
            groups.push(AutofixupGroup {
                target_oid: pair.target_oid.clone(),
                target_summary: pair.target_summary.clone(),
                target_message: pair.target_message.clone(),
                sources: vec![pair.clone()],
            });
        }
    }
    groups
}

/// Final messages the user chose in the confirmation dialog, one per target.
///
/// Keyed by the target's OID when the batch was planned. The OID changes as
/// the batch rewrites the branch, but the batch finds each target again through
/// its [`BatchPlan`], and keys its lookup by the same planned OID.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MessageOverrides(BTreeMap<Oid, BString>);

impl MessageOverrides {
    pub fn for_group(&self, group: &AutofixupGroup) -> Option<&BString> {
        self.for_target(&group.target_oid)
    }

    /// The message for the target whose OID was `planned_oid` when the batch
    /// was planned.
    pub fn for_target(&self, planned_oid: &Oid) -> Option<&BString> {
        self.0.get(planned_oid)
    }

    pub fn set(&mut self, group: &AutofixupGroup, message: BString) {
        self.0.insert(group.target_oid.clone(), message);
    }

    pub fn clear(&mut self, group: &AutofixupGroup) {
        self.0.remove(&group.target_oid);
    }
}

/// A message as the journal stores it: text when it decodes, bytes when not.
struct Message<'a>(&'a BStr);

impl Serialize for Message<'_> {
    fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        crate::domain::message_bytes::serialize(self.0, ser)
    }
}

struct OwnedMessage(BString);

impl<'de> Deserialize<'de> for OwnedMessage {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        crate::domain::message_bytes::deserialize(de).map(OwnedMessage)
    }
}

impl Serialize for MessageOverrides {
    fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        let mut map = ser.serialize_map(Some(self.0.len()))?;
        for (oid, message) in &self.0 {
            map.serialize_entry(oid, &Message(message.as_bstr()))?;
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for MessageOverrides {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        let messages = BTreeMap::<Oid, OwnedMessage>::deserialize(de)?;
        Ok(MessageOverrides(
            messages
                .into_iter()
                .map(|(oid, OwnedMessage(message))| (oid, message))
                .collect(),
        ))
    }
}

const COMMENT_PREFIX: &str = "# ";

/// The message without its trailing newlines, which the template supplies.
fn trim_trailing_newlines(message: &BStr) -> &BStr {
    let end = message
        .iter()
        .rposition(|&b| b != b'\n')
        .map_or(0, |i| i + 1);
    &message[..end]
}

/// Build the text shown in `$EDITOR` when the user edits a target group's
/// final message: `target_message`, live and editable, followed by each
/// source's message commented out — mirroring `git rebase
/// --autosquash`'s own combination template. Left untouched, the commented
/// sources contribute nothing, so a no-op edit is the same as not editing at
/// all (matches `fixup!`'s already-silent default).
pub fn edit_template(target_message: &BStr, sources: &[(SquashMode, BString)]) -> BString {
    let mut text = BString::from(trim_trailing_newlines(target_message));
    text.push(b'\n');
    for (mode, source_message) in sources {
        text.push(b'\n');
        text.extend_from_slice(COMMENT_PREFIX.as_bytes());
        text.extend_from_slice(match mode {
            SquashMode::Fixup => b"The message below is from a fixup! commit being folded in:",
            SquashMode::Squash => b"The message below is from a squash! commit being folded in:",
        });
        text.push(b'\n');
        text.extend_from_slice(COMMENT_PREFIX.as_bytes());
        text.push(b'\n');
        for line in source_message.lines() {
            text.extend_from_slice(COMMENT_PREFIX.as_bytes());
            text.extend_from_slice(line);
            text.push(b'\n');
        }
    }
    text
}

/// Strip `#`-prefixed comment lines and trim surrounding blank lines — mirrors
/// git's own `commit.cleanup=strip` handling of the combination template
/// above, so leaving the commented-out sources untouched discards them.
pub fn strip_comment_lines(text: &BStr) -> BString {
    // Bytes throughout: a commit message is bytes to git, and the editor hands
    // back whatever the user typed. Decoding to `String` first would replace
    // anything that is not UTF-8 with U+FFFD — silently rewriting their text.
    let kept: Vec<&[u8]> = text
        .split(|&b| b == b'\n')
        .map(|line| line.strip_suffix(b"\r").unwrap_or(line))
        .filter(|line| !line.starts_with(b"#"))
        .collect();
    kept.join(&b'\n').trim_ascii().into()
}

/// Match every `fixup!`/`squash!`-prefixed commit in `commits` (oldest first,
/// as returned by `list_commits`) to the earlier commit it names, by the rules
/// `git rebase --autosquash` follows: the oldest commit with exactly that
/// summary, else the one that hash abbreviates, else the oldest whose summary
/// starts with it. Repeated prefixes name the original target, and the first
/// sets the mode. Commits with no target are omitted — the caller leaves them in
/// place. Pairs come oldest fixup first.
pub fn plan_autofixup(commits: &[CommitInfo]) -> Vec<AutofixupPair> {
    let mut pairs = Vec::new();
    // Where each commit folds into, for those that are fixups themselves.
    let mut folds_into: Vec<Option<usize>> = vec![None; commits.len()];
    for (i, commit) in commits.iter().enumerate() {
        let Some((mode, named)) = split_prefixes(&commit.summary_key) else {
            continue;
        };
        let earlier = &commits[..i];
        let Some(named_index) = earlier
            .iter()
            .position(|c| c.summary_key == named)
            .or_else(|| abbreviated(earlier, named))
            .or_else(|| {
                earlier
                    .iter()
                    .position(|c| c.summary_key.starts_with(named))
            })
        else {
            continue;
        };
        // A fixup that names another fixup: that one has folded away by the
        // time this runs, into the original target.
        let target_index = folds_into[named_index].unwrap_or(named_index);
        folds_into[i] = Some(target_index);
        let target = &commits[target_index];

        pairs.push(AutofixupPair {
            source_oid: commit.oid.expect_real_oid(),
            target_oid: target.oid.expect_real_oid(),
            source_summary: commit.summary.clone(),
            target_summary: target.summary.clone(),
            source_message: commit.message.clone(),
            target_message: target.message.clone(),
            mode,
        });
    }
    pairs
}

/// The mode the first `fixup!`/`squash!` prefix sets, and what is left once
/// every one of them is stripped. `None` when there is no prefix, or nothing
/// after it to name a target by.
fn split_prefixes(summary: &[u8]) -> Option<(SquashMode, &[u8])> {
    let (mode, mut named) = strip_prefix(summary)?;
    while let Some((_, rest)) = strip_prefix(named) {
        named = rest;
    }
    (!named.is_empty()).then_some((mode, named))
}

fn strip_prefix(text: &[u8]) -> Option<(SquashMode, &[u8])> {
    [SquashMode::Fixup, SquashMode::Squash]
        .into_iter()
        .find_map(|mode| {
            text.strip_prefix(mode.prefix().as_bytes())
                .map(|rest| (mode, rest))
        })
}

/// The one commit whose hash starts with `named`, when `named` reads as an
/// abbreviated hash: four or more hex digits and nothing else, as git requires.
fn abbreviated(commits: &[CommitInfo], named: &[u8]) -> Option<usize> {
    if named.len() < 4 || !named.iter().all(u8::is_ascii_hexdigit) {
        return None;
    }
    let named = named.to_ascii_lowercase();
    let mut matching = commits.iter().enumerate().filter(|(_, c)| {
        c.oid
            .as_oid()
            .is_some_and(|oid| oid.long().as_bytes().starts_with(&named))
    });
    match (matching.next(), matching.next()) {
        (Some((index, _)), None) => Some(index),
        _ => None,
    }
}

#[cfg(test)]
mod batch_plan_tests {
    use super::*;
    use crate::VirtualOid;

    fn commit(oid: &str, summary: &str) -> CommitInfo {
        CommitInfo {
            oid: VirtualOid::Real(Oid::new(oid.repeat(40))),
            summary: summary.to_string(),
            summary_key: summary.into(),
            author: None,
            date: None,
            parent_oids: vec![],
            message: summary.to_string(),
            author_email: None,
            author_date: None,
            committer: None,
            committer_email: None,
            commit_date: None,
        }
    }

    #[test]
    fn a_fixup_of_a_fixup_lands_on_the_target_the_first_one_folded_into() {
        let commits = vec![
            commit("a", "Add parser"),
            commit("b", "fixup! Add parser"),
            commit("c", "fixup! fixup! Add parser"),
        ];
        let plan = BatchPlan::new(&commits, &plan_autofixup(&commits));
        assert!(plan.steps.iter().all(|step| step.target == PlannedPos(0)));
    }

    #[test]
    fn a_position_moves_down_past_each_source_removed_below_it() {
        let commits = vec![
            commit("a", "Add parser"),
            commit("b", "fixup! Add parser"),
            commit("c", "Add lexer"),
            commit("d", "fixup! Add lexer"),
        ];
        let plan = BatchPlan::new(&commits, &plan_autofixup(&commits));
        assert_eq!(plan.current_index(PlannedPos(2), 0), 2);
        assert_eq!(plan.current_index(PlannedPos(2), 1), 1);
        assert_eq!(plan.current_index(PlannedPos(0), 1), 0);
    }
}

#[cfg(test)]
mod message_overrides_tests {
    use super::*;

    fn overrides(entries: &[(&str, &[u8])]) -> MessageOverrides {
        MessageOverrides(
            entries
                .iter()
                .map(|(oid, message)| (Oid::from(*oid), BString::from(*message)))
                .collect(),
        )
    }

    #[test]
    fn a_utf8_message_is_written_as_plain_text() {
        let wrapper = overrides(&[("abc123", b"Edited\n")]);
        let json = serde_json::to_string(&wrapper).unwrap();
        assert_eq!(json, r#"{"abc123":"Edited\n"}"#);
        let back: MessageOverrides = serde_json::from_str(&json).unwrap();
        assert_eq!(back, wrapper);
    }

    #[test]
    fn a_message_that_does_not_decode_round_trips() {
        let wrapper = overrides(&[("abc123", b"Fix f\xf6r \xe5\xe4\xf6\n")]);
        let json = serde_json::to_string(&wrapper).unwrap();
        let back: MessageOverrides = serde_json::from_str(&json).unwrap();
        assert_eq!(back, wrapper);
    }

    #[test]
    fn a_message_written_as_a_byte_array_still_loads() {
        let json = r#"{"abc123":[69,100,105,116,101,100,10]}"#;
        let loaded: MessageOverrides = serde_json::from_str(json).unwrap();
        assert_eq!(loaded, overrides(&[("abc123", b"Edited\n")]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VirtualOid;
    use bstr::ByteSlice;

    fn commit(oid: &str, summary: &str) -> CommitInfo {
        CommitInfo {
            oid: VirtualOid::Real(Oid::new(oid.repeat(40))),
            summary: summary.to_string(),
            summary_key: summary.into(),
            author: None,
            date: None,
            parent_oids: vec![],
            message: summary.to_string(),
            author_email: None,
            author_date: None,
            committer: None,
            committer_email: None,
            commit_date: None,
        }
    }

    /// A commit whose summary is `raw`, rendered the way the list draws it.
    fn commit_with_raw_summary(oid: &str, raw: &[u8]) -> CommitInfo {
        CommitInfo {
            summary: String::from_utf8_lossy(raw).into_owned(),
            summary_key: BString::from(raw),
            ..commit(oid, "")
        }
    }

    // Latin-1 "Fix för" and "Fix fär": both render as "Fix f\u{fffd}r".
    const FOR: &[u8] = b"Fix f\xf6r";
    const FAR: &[u8] = b"Fix f\xe4r";

    fn fixup_of(oid: &str, target: &[u8]) -> CommitInfo {
        commit_with_raw_summary(oid, &[b"fixup! ", target].concat())
    }

    #[test]
    fn a_fixup_matches_its_targets_bytes_not_their_rendering() {
        let commits = vec![
            commit_with_raw_summary("a", FOR),
            commit_with_raw_summary("b", FAR),
            fixup_of("c", FOR),
        ];
        let pairs = plan_autofixup(&commits);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].target_oid, Oid::new("a".repeat(40)));
    }

    #[test]
    fn targets_that_render_alike_keep_their_own_groups_and_messages() {
        let commits = vec![
            commit_with_raw_summary("a", FOR),
            commit_with_raw_summary("b", FAR),
            fixup_of("c", FOR),
            fixup_of("d", FAR),
        ];
        let groups = group_by_target(&plan_autofixup(&commits));
        assert_eq!(groups.len(), 2);

        let mut overrides = MessageOverrides::default();
        overrides.set(&groups[0], BString::from("Edited\n"));
        assert_eq!(overrides.for_group(&groups[1]), None);
    }

    #[test]
    fn matches_a_fixup_to_its_target() {
        let commits = vec![commit("a", "Add parser"), commit("b", "fixup! Add parser")];
        let pairs = plan_autofixup(&commits);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].source_summary, "fixup! Add parser");
        assert_eq!(pairs[0].target_summary, "Add parser");
        assert_eq!(pairs[0].mode, SquashMode::Fixup);
    }

    #[test]
    fn matches_a_squash_to_its_target() {
        let commits = vec![commit("a", "Add parser"), commit("b", "squash! Add parser")];
        let pairs = plan_autofixup(&commits);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].mode, SquashMode::Squash);
    }

    #[test]
    fn a_fixup_with_no_matching_target_is_skipped() {
        let commits = vec![commit("a", "Add parser"), commit("b", "fixup! Nope")];
        assert_eq!(plan_autofixup(&commits), vec![]);
    }

    #[test]
    fn multiple_fixups_for_the_same_target_stack_in_branch_order() {
        let commits = vec![
            commit("a", "Add parser"),
            commit("b", "fixup! Add parser"),
            commit("c", "Unrelated"),
            commit("d", "fixup! Add parser"),
        ];
        let pairs = plan_autofixup(&commits);
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0].source_summary, "fixup! Add parser");
        assert_eq!(pairs[1].source_summary, "fixup! Add parser");
        // Both target the same original commit, oldest fixup first.
        assert_eq!(pairs[0].target_oid, pairs[1].target_oid);
    }

    #[test]
    fn mixed_fixup_and_squash_prefixes() {
        let commits = vec![
            commit("a", "Add parser"),
            commit("b", "Add lexer"),
            commit("c", "fixup! Add parser"),
            commit("d", "squash! Add lexer"),
        ];
        let pairs = plan_autofixup(&commits);
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0].mode, SquashMode::Fixup);
        assert_eq!(pairs[1].mode, SquashMode::Squash);
    }

    #[test]
    fn targets_with_the_same_summary_keep_their_own_groups_and_messages() {
        let commits = vec![
            commit("a", "Tweak"),
            commit("b", "Tweak"),
            commit("c", "fixup! Tweak"),
            commit("d", "fixup! bbbbbbb"),
        ];
        let groups = group_by_target(&plan_autofixup(&commits));
        assert_eq!(groups.len(), 2);

        let mut overrides = MessageOverrides::default();
        overrides.set(&groups[1], BString::from("Edited\n"));
        assert_eq!(overrides.for_group(&groups[0]), None);
    }

    /// The fixup it names is folded away first, into the original target, so
    /// that is the target the dialog shows and the batch uses.
    #[test]
    fn a_fixup_naming_another_fixup_targets_the_original() {
        let commits = vec![
            commit("a", "Add parser"),
            commit("b", "fixup! Add parser"),
            commit("c", "fixup! bbbbbbb"),
        ];
        let pairs = plan_autofixup(&commits);
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[1].target_oid, commits[0].oid.expect_real_oid());
        assert_eq!(group_by_target(&pairs).len(), 1);
    }

    #[test]
    fn the_oldest_match_wins_on_duplicate_summaries() {
        let commits = vec![
            commit("a", "Tweak"),
            commit("b", "Tweak"),
            commit("c", "fixup! Tweak"),
        ];
        let pairs = plan_autofixup(&commits);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].target_oid, commits[0].oid.expect_real_oid());
    }

    #[test]
    fn a_fixup_can_name_its_target_by_hash() {
        let commits = vec![
            commit("a", "Tweak"),
            commit("b", "Tweak"),
            commit("c", "fixup! bbbbbbb"),
        ];
        let pairs = plan_autofixup(&commits);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].target_oid, commits[1].oid.expect_real_oid());
    }

    #[test]
    fn a_fixup_can_name_its_target_by_the_start_of_its_summary() {
        let commits = vec![commit("a", "Add parser"), commit("b", "fixup! Add pa")];
        let pairs = plan_autofixup(&commits);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].target_oid, commits[0].oid.expect_real_oid());
    }

    #[test]
    fn a_whole_summary_wins_over_an_earlier_one_it_only_starts() {
        let commits = vec![
            commit("a", "Other"),
            commit("b", "Ot"),
            commit("c", "fixup! Ot"),
        ];
        let pairs = plan_autofixup(&commits);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].target_oid, commits[1].oid.expect_real_oid());
    }

    #[test]
    fn repeated_prefixes_name_the_original_target() {
        let commits = vec![
            commit("a", "Add parser"),
            commit("b", "squash! fixup! Add parser"),
        ];
        let pairs = plan_autofixup(&commits);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].target_oid, commits[0].oid.expect_real_oid());
        assert_eq!(pairs[0].mode, SquashMode::Squash);
    }

    #[test]
    fn group_by_target_stacks_multiple_sources_under_one_group() {
        let commits = vec![
            commit("a", "Add parser"),
            commit("b", "fixup! Add parser"),
            commit("c", "Add lexer"),
            commit("d", "fixup! Add parser"),
            commit("e", "squash! Add lexer"),
        ];
        let pairs = plan_autofixup(&commits);
        let groups = group_by_target(&pairs);

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].target_summary, "Add parser");
        assert_eq!(groups[0].sources.len(), 2);
        assert_eq!(groups[1].target_summary, "Add lexer");
        assert_eq!(groups[1].sources.len(), 1);
    }

    /// The template for `group`, built the way `dispatch::autofixup::edit_seed`
    /// builds it.
    fn template_for(group: &AutofixupGroup) -> BString {
        let sources: Vec<(SquashMode, BString)> = group
            .sources
            .iter()
            .map(|pair| (pair.mode, BString::from(pair.source_message.clone())))
            .collect();
        edit_template(group.target_message.as_str().into(), &sources)
    }

    /// A commit message ends in a newline, which is a terminator and not an
    /// empty last line. Treating it as one puts a bare "# " under every folded
    /// source in the editor.
    #[test]
    fn edit_template_does_not_comment_a_line_past_the_end_of_a_message() {
        let target = BString::from("Add parser\n");
        let sources = vec![(SquashMode::Fixup, BString::from("fixup! Add parser\n"))];

        let template = edit_template(target.as_bstr(), &sources);

        assert_eq!(
            template,
            concat!(
                "Add parser\n",
                "\n",
                "# The message below is from a fixup! commit being folded in:\n",
                "# \n",
                "# fixup! Add parser\n",
            )
        );
    }

    #[test]
    fn edit_template_comments_out_every_source() {
        let commits = vec![commit("a", "Add parser"), commit("b", "fixup! Add parser")];
        let pairs = plan_autofixup(&commits);
        let group = &group_by_target(&pairs)[0];

        let template = template_for(group);
        assert!(template.starts_with(b"Add parser\n"));
        for line in template
            .split(|&b| b == b'\n')
            .skip(1)
            .filter(|l| !l.is_empty())
        {
            assert!(
                line.starts_with(b"#"),
                "expected every non-blank line after the target message to be commented: {:?}",
                line.as_bstr()
            );
        }
    }

    #[test]
    fn strip_comment_lines_on_an_untouched_template_yields_just_the_target_message() {
        let commits = vec![
            commit("a", "Add parser"),
            commit("b", "fixup! Add parser"),
            commit("c", "fixup! Add parser"),
        ];
        let pairs = plan_autofixup(&commits);
        let group = &group_by_target(&pairs)[0];

        let template = template_for(group);
        assert_eq!(strip_comment_lines(template.as_bstr()), b"Add parser");
    }

    #[test]
    fn strip_comment_lines_on_an_all_commented_template_yields_empty() {
        // If the user deletes the live target line too (or comments it out),
        // the result is empty — main.rs relies on exactly this to know the
        // edit should clear any existing override rather than store a blank
        // message.
        let text = "# Add parser\n# fixup! Add parser";
        assert_eq!(strip_comment_lines(text.into()), b"");
    }

    #[test]
    fn strip_comment_lines_keeps_uncommented_additions() {
        let text = "Add parser\n\n# comment\nExtra detail the user typed\n# more comment";
        assert_eq!(
            strip_comment_lines(text.into()),
            b"Add parser\n\nExtra detail the user typed"
        );
    }

    /// Bytes in, bytes out. Routing this through `String` would replace
    /// anything that is not UTF-8 with U+FFFD, silently rewriting a message the
    /// user typed — which is what git stores verbatim.
    #[test]
    fn strip_comment_lines_keeps_bytes_that_are_not_utf8() {
        // Latin-1 "Fix för åäö handling": valid git, invalid UTF-8.
        let text: &[u8] = b"Fix f\xf6r \xe5\xe4\xf6 handling\n# a comment\n";
        assert_eq!(
            strip_comment_lines(text.as_bstr()),
            b"Fix f\xf6r \xe5\xe4\xf6 handling".to_vec()
        );
    }

    #[test]
    fn strip_comment_lines_preserves_internal_blank_lines_in_a_multi_paragraph_message() {
        let text = "Summary\n\nBody paragraph one.\n\nBody paragraph two.";
        assert_eq!(strip_comment_lines(text.into()), text.as_bytes());
    }
}
