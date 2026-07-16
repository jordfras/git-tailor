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

use ratatui::{Frame, style::Color};

use crate::app::{AppMode, AppState};
use crate::views::{
    commit_list,
    dialog::{Dialog, DialogKind, TextRole},
};

fn render_background(app: &mut AppState, frame: &mut Frame) {
    if !app.commits.is_empty() {
        commit_list::render(app, frame);
    }
}

pub fn render(app: &mut AppState, frame: &mut Frame) {
    let AppMode::Loading {
        title,
        message,
        progress,
        skippable,
    } = app.mode
    else {
        return;
    };

    render_background(app, frame);

    let text = match progress {
        Some((n, t)) => format!(" {message} {n}/{t}"),
        None => format!(" {message}"),
    };
    let mut dialog = Dialog::new(DialogKind::Info, app.colors)
        .blank()
        .styled_line(text, TextRole::Normal);
    let hint = if skippable {
        Some(("s", Color::Cyan, "to skip"))
    } else if progress.is_some() {
        Some(("Ctrl-c", Color::Cyan, "to quit"))
    } else {
        None
    };
    if let Some((key, color, label)) = hint {
        dialog = dialog.blank().instructions(&[(key, color, label)]);
    }
    dialog.blank().render(frame, title, 60, 0);
}
