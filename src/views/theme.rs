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

use clap::ValueEnum;
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

/// Relationship of a square (commit-row × cluster-column intersection) to earlier
/// commits in the same cluster column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SquareRelation {
    /// Can be squashed with an earlier commit in this cluster.
    Squashable,
    /// Cannot be squashed — intervening commits exist in this cluster.
    Conflict,
    /// Topmost square in the column: no earlier commit touches this cluster.
    Origin,
}

/// Relationship of a connector between two squares in a cluster column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectorRelation {
    /// The lower square is squashable with the earlier one.
    Squashable,
    /// The lower square conflicts with the earlier one.
    Conflict,
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
    fn square_symbol(&self, role: SquareRole, rel: SquareRelation) -> &str;
    fn square_style(&self, role: SquareRole, rel: SquareRelation) -> Style;
    fn connector_symbol(&self, role: ConnectorRole, rel: ConnectorRelation) -> &str;
    fn connector_style(&self, role: ConnectorRole, rel: ConnectorRelation) -> Style;
    fn commit_row_style(&self, role: CommitRowRole) -> Style;
}

/// Uniform heavy-glyph rendering — no focus distinction.
///
/// This is the default theme, matching the original git-tailor rendering:
/// all squares use `█`, all connectors use `│`, colored only by relation type.
pub struct PlainTheme;

impl FragmapTheme for PlainTheme {
    fn square_symbol(&self, _role: SquareRole, _rel: SquareRelation) -> &str {
        "█"
    }

    fn square_style(&self, _role: SquareRole, rel: SquareRelation) -> Style {
        match rel {
            SquareRelation::Squashable => Style::new().fg(Color::DarkGray),
            SquareRelation::Conflict => Style::new().fg(Color::White),
            SquareRelation::Origin => Style::new().fg(Color::White),
        }
    }

    fn connector_symbol(&self, _role: ConnectorRole, _rel: ConnectorRelation) -> &str {
        "│"
    }

    fn connector_style(&self, _role: ConnectorRole, rel: ConnectorRelation) -> Style {
        match rel {
            ConnectorRelation::Squashable => Style::new().fg(Color::Yellow),
            ConnectorRelation::Conflict => Style::new().fg(Color::Red),
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

/// Focus highlighting theme.
///
/// Dims unrelated columns (those the focus commit does not touch) to emphasize
/// the selected commit's clusters. Related connectors use heavy `┃`, unrelated
/// use light `│`. Squashable relations are green, conflicts are red/white.
pub struct HighlightTheme;

impl FragmapTheme for HighlightTheme {
    fn square_symbol(&self, _role: SquareRole, _rel: SquareRelation) -> &str {
        "█"
    }

    fn square_style(&self, role: SquareRole, rel: SquareRelation) -> Style {
        let style = match rel {
            SquareRelation::Squashable => Style::new().fg(Color::Green),
            SquareRelation::Conflict => Style::new().fg(if role == SquareRole::Current {
                Color::LightRed
            } else {
                Color::White
            }),
            SquareRelation::Origin => Style::new().fg(Color::White),
        };
        match role {
            SquareRole::Current => style,
            SquareRole::Related => style,
            SquareRole::Unrelated => style.add_modifier(ratatui::style::Modifier::DIM),
        }
    }

    fn connector_symbol(&self, role: ConnectorRole, _rel: ConnectorRelation) -> &str {
        match role {
            ConnectorRole::Related => "┃",
            ConnectorRole::Unrelated => "│",
        }
    }

    fn connector_style(&self, role: ConnectorRole, rel: ConnectorRelation) -> Style {
        let style = match rel {
            ConnectorRelation::Squashable => Style::new().fg(Color::Green),
            ConnectorRelation::Conflict => Style::new().fg(Color::Red),
        };
        match role {
            ConnectorRole::Related => style.add_modifier(ratatui::style::Modifier::BOLD),
            ConnectorRole::Unrelated => style.add_modifier(ratatui::style::Modifier::DIM),
        }
    }

    fn commit_row_style(&self, role: CommitRowRole) -> Style {
        match role {
            CommitRowRole::SquashPartner => Style::new().fg(Color::Green),
            CommitRowRole::Conflict => Style::new().fg(Color::Red),
            CommitRowRole::Squashable => Style::new()
                .fg(Color::Green)
                .add_modifier(ratatui::style::Modifier::DIM),
            CommitRowRole::Unrelated => Style::default(),
        }
    }
}

/// Traditional fragmap appearance matching the `--static` colored output.
///
/// Touched squares are rendered as a space with a white background; connectors
/// use yellow (squashable) or red (conflict) backgrounds — identical to the
/// ANSI color output produced by `git-tailor --static`.
pub struct ClassicTheme;

impl FragmapTheme for ClassicTheme {
    fn square_symbol(&self, _role: SquareRole, _rel: SquareRelation) -> &str {
        " "
    }

    fn square_style(&self, _role: SquareRole, _rel: SquareRelation) -> Style {
        Style::new().bg(Color::White)
    }

    fn connector_symbol(&self, _role: ConnectorRole, _rel: ConnectorRelation) -> &str {
        " "
    }

    fn connector_style(&self, _role: ConnectorRole, rel: ConnectorRelation) -> Style {
        match rel {
            ConnectorRelation::Squashable => Style::new().bg(Color::Yellow),
            ConnectorRelation::Conflict => Style::new().bg(Color::Red),
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

/// Selects which `FragmapTheme` implementation is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum Theme {
    /// No focus highlighting (default).
    #[default]
    Plain,
    /// Highlight clusters related to the selected commit.
    Highlight,
    /// Background-color style, matching the `--static` output.
    Classic,
}

impl Theme {
    pub fn as_theme(&self) -> &dyn FragmapTheme {
        match self {
            Theme::Plain => &PlainTheme,
            Theme::Highlight => &HighlightTheme,
            Theme::Classic => &ClassicTheme,
        }
    }
}
