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

// Fragmap rendering theme trait and built-in theme implementations.

use ratatui::style::{Color, Style};

/// Role of a square (commit-row × cluster-column intersection) relative to the focus commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SquareRole {
    /// The focus commit's own square (always in a focus-cluster column).
    Current,
    /// Another commit's square in a focus-cluster column.
    Related,
    /// Any square in a non-focus-cluster column.
    Unrelated,
}

/// Role of a connector column relative to the focus commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectorRole {
    /// The column belongs to a cluster that the focus commit touches.
    Related,
    /// The column belongs to a cluster the focus commit does not touch.
    Unrelated,
}

/// Whether a cluster column represents a conflict or an easily squashable relationship.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationType {
    Conflict,
    Squashable,
}

/// Role of a commit row's SHA + title text relative to the focus commit.
///
/// The focus commit itself is never passed to `commit_row_style`; its styling
/// is handled separately by the caller before this function is reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitRowRole {
    /// Directly squashable into, or from, the focus commit.
    SquashPartner,
    /// Shares a hunk-group cluster with the focus commit (but not squashable).
    Conflict,
    /// Fully squashable into some other commit (intrinsic property, focus-independent).
    Squashable,
    /// No relationship to the focus commit.
    Unrelated,
}

/// Controls how fragmap squares, connectors, and commit list rows are rendered.
pub trait FragmapTheme {
    fn square_symbol(&self, role: SquareRole, rel: RelationType) -> &str;
    fn square_style(&self, role: SquareRole, rel: RelationType) -> Style;
    fn connector_symbol(&self, role: ConnectorRole, rel: RelationType) -> &str;
    fn connector_style(&self, role: ConnectorRole, rel: RelationType) -> Style;
    fn commit_row_style(&self, role: CommitRowRole) -> Style;
}

/// Uniform heavy-glyph rendering — no focus distinction.
///
/// This is the default theme, matching the original git-tailor rendering:
/// all squares use `█`, all connectors use `│`, colored only by relation type.
pub struct PlainTheme;

impl FragmapTheme for PlainTheme {
    fn square_symbol(&self, _role: SquareRole, _rel: RelationType) -> &str {
        "█"
    }

    fn square_style(&self, _role: SquareRole, rel: RelationType) -> Style {
        match rel {
            RelationType::Squashable => Style::new().fg(Color::DarkGray),
            RelationType::Conflict => Style::new().fg(Color::White),
        }
    }

    fn connector_symbol(&self, _role: ConnectorRole, _rel: RelationType) -> &str {
        "│"
    }

    fn connector_style(&self, _role: ConnectorRole, rel: RelationType) -> Style {
        match rel {
            RelationType::Squashable => Style::new().fg(Color::Yellow),
            RelationType::Conflict => Style::new().fg(Color::Red),
        }
    }

    fn commit_row_style(&self, role: CommitRowRole) -> Style {
        match role {
            CommitRowRole::SquashPartner => Style::new().fg(Color::Yellow),
            CommitRowRole::Conflict => Style::new().fg(Color::Red),
            CommitRowRole::Squashable => Style::new().fg(Color::DarkGray),
            CommitRowRole::Unrelated => Style::default(),
        }
    }
}
