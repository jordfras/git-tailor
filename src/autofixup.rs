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
use serde::de::{MapAccess, SeqAccess, Visitor};
use serde::ser::SerializeSeq;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;
use std::fmt;

/// One `fixup!`/`squash!` commit matched to the target it will be squashed into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutofixupPair {
    pub source_oid: Oid,
    pub target_oid: Oid,
    pub source_summary: String,
    pub target_summary: String,
    /// The target's summary as git compares it (see
    /// [`CommitInfo::summary_key`]): what identifies the target, since two
    /// summaries can render alike.
    pub target_summary_key: BString,
    /// Full commit message (summary + body) of the source/target, needed to
    /// build the non-interactive squash message; the confirmation dialog
    /// only shows the summaries.
    pub source_message: String,
    pub target_message: String,
    pub mode: SquashMode,
}

/// One target commit and every `fixup!`/`squash!` commit that will be folded
/// into it, for display grouped by target. Purely a view over `AutofixupPair`
/// — execution still proceeds pair by pair (see `plan_autofixup`'s docs on
/// why re-matching by summary, not by this grouping, drives the batch).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutofixupGroup {
    pub target_oid: Oid,
    pub target_summary: String,
    pub target_summary_key: BString,
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
        let mut steps: Vec<PlannedStep> = Vec::with_capacity(pairs.len());
        for pair in pairs {
            // A fixup of a fixup: by the time it runs, the commit it names has
            // been folded away, into the target it was aimed at.
            let mut target = pos(&pair.target_oid);
            while let Some(earlier) = steps.iter().find(|step| step.source == target) {
                target = earlier.target;
            }
            steps.push(PlannedStep {
                source: pos(&pair.source_oid),
                target,
                mode: pair.mode,
            });
        }
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
        if let Some(group) = groups
            .iter_mut()
            .find(|g| g.target_summary_key == pair.target_summary_key)
        {
            group.sources.push(pair.clone());
        } else {
            groups.push(AutofixupGroup {
                target_oid: pair.target_oid.clone(),
                target_summary: pair.target_summary.clone(),
                target_summary_key: pair.target_summary_key.clone(),
                target_message: pair.target_message.clone(),
                sources: vec![pair.clone()],
            });
        }
    }
    groups
}

/// Final messages the user chose in the confirmation dialog, one per target.
///
/// Keyed by the target's summary, which survives the batch's cascading rebases
/// where its OID does not. Only groups and pairs can look one up, so the dialog
/// that sets a message and the batch that applies it cannot key it differently.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MessageOverrides(BTreeMap<BString, BString>);

impl MessageOverrides {
    pub fn for_group(&self, group: &AutofixupGroup) -> Option<&BString> {
        self.0.get(&group.target_summary_key)
    }

    pub fn for_pair(&self, pair: &AutofixupPair) -> Option<&BString> {
        self.0.get(&pair.target_summary_key)
    }

    pub fn for_summary_key(&self, summary_key: &BStr) -> Option<&BString> {
        self.0.get(summary_key)
    }

    pub fn set(&mut self, group: &AutofixupGroup, message: BString) {
        self.0.insert(group.target_summary_key.clone(), message);
    }

    pub fn clear(&mut self, group: &AutofixupGroup) {
        self.0.remove(&group.target_summary_key);
    }
}

/// One override as the journal stores it. A list of these rather than a map,
/// because a JSON object needs keys that are text and a summary may not be.
#[derive(Serialize, Deserialize)]
struct OverrideEntry {
    #[serde(with = "crate::domain::message_bytes")]
    summary: BString,
    #[serde(with = "crate::domain::message_bytes")]
    message: BString,
}

impl Serialize for MessageOverrides {
    fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        let mut seq = ser.serialize_seq(Some(self.0.len()))?;
        for (summary, message) in &self.0 {
            seq.serialize_element(&OverrideEntry {
                summary: summary.clone(),
                message: message.clone(),
            })?;
        }
        seq.end()
    }
}

impl<'de> Deserialize<'de> for MessageOverrides {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        struct Message(BString);

        impl<'de> Deserialize<'de> for Message {
            fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
                crate::domain::message_bytes::deserialize(de).map(Message)
            }
        }

        struct Overrides;

        impl<'de> Visitor<'de> for Overrides {
            type Value = MessageOverrides;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("commit messages keyed by summary")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut out = BTreeMap::new();
                while let Some(entry) = seq.next_element::<OverrideEntry>()? {
                    out.insert(entry.summary, entry.message);
                }
                Ok(MessageOverrides(out))
            }

            /// The shape a journal from 3.0.0 or 3.1.0 holds: an object keyed
            /// by the summary as text.
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut out = BTreeMap::new();
                while let Some((summary, Message(message))) = map.next_entry::<String, Message>()? {
                    out.insert(BString::from(summary), message);
                }
                Ok(MessageOverrides(out))
            }
        }

        de.deserialize_any(Overrides)
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

/// Match every `fixup!`/`squash!`-prefixed commit in `commits` (oldest-first,
/// as returned by `list_commits`) to the nearest earlier commit whose summary
/// its prefix names. Commits with no resolvable target are omitted — they are
/// left in place by the caller. Pairs are returned oldest-fixup-first, so
/// applying them in order naturally stacks multiple fixups aimed at the same
/// target (each squash keeps the target's summary, so later matches still
/// resolve correctly against the rewritten commit).
pub fn plan_autofixup(commits: &[CommitInfo]) -> Vec<AutofixupPair> {
    let mut pairs = Vec::new();
    for (i, commit) in commits.iter().enumerate() {
        let Some((mode, target_bytes)) = [SquashMode::Fixup, SquashMode::Squash]
            .into_iter()
            .find_map(|mode| {
                commit
                    .summary_key
                    .strip_prefix(mode.prefix().as_bytes())
                    .map(|bytes| (mode, bytes))
            })
        else {
            continue;
        };

        // Nearest preceding commit wins, in case of duplicate summaries.
        let Some(target) = commits[..i]
            .iter()
            .rev()
            .find(|c| c.summary_key == target_bytes)
        else {
            continue;
        };

        pairs.push(AutofixupPair {
            source_oid: commit.oid.expect_real_oid(),
            target_oid: target.oid.expect_real_oid(),
            source_summary: commit.summary.clone(),
            target_summary: target.summary.clone(),
            target_summary_key: target.summary_key.clone(),
            source_message: commit.message.clone(),
            target_message: target.message.clone(),
            mode,
        });
    }
    pairs
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

    fn overrides(entries: &[(&[u8], &[u8])]) -> MessageOverrides {
        MessageOverrides(
            entries
                .iter()
                .map(|(summary, message)| (BString::from(*summary), BString::from(*message)))
                .collect(),
        )
    }

    #[test]
    fn utf8_entries_round_trip_as_plain_strings() {
        let wrapper = overrides(&[(b"Add parser", b"Edited\n")]);
        let json = serde_json::to_string(&wrapper).unwrap();
        assert_eq!(json, r#"[{"summary":"Add parser","message":"Edited\n"}]"#);
        let back: MessageOverrides = serde_json::from_str(&json).unwrap();
        assert_eq!(back, wrapper);
    }

    #[test]
    fn a_summary_and_message_that_do_not_decode_round_trip() {
        let wrapper = overrides(&[(b"Fix f\xf6r", b"Fix f\xf6r \xe5\xe4\xf6\n")]);
        let json = serde_json::to_string(&wrapper).unwrap();
        let back: MessageOverrides = serde_json::from_str(&json).unwrap();
        assert_eq!(back, wrapper);
    }

    #[test]
    fn a_journal_keyed_by_summary_text_still_loads() {
        let json = r#"{"Add parser":"Edited\n"}"#;
        let loaded: MessageOverrides = serde_json::from_str(json).unwrap();
        assert_eq!(loaded, overrides(&[(b"Add parser", b"Edited\n")]));
    }

    #[test]
    fn a_journal_that_wrote_every_message_as_a_byte_array_still_loads() {
        // The shape the derived `Vec<u8>` serialization produced, which a
        // journal parked by an earlier build still holds.
        let json = r#"{"Add parser":[69,100,105,116,101,100,10]}"#;
        let loaded: MessageOverrides = serde_json::from_str(json).unwrap();
        assert_eq!(loaded, overrides(&[(b"Add parser", b"Edited\n")]));
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
