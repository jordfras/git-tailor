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
use crate::views::{commit_list, dialog::Dialog};

fn render_background(app: &mut AppState, frame: &mut Frame) {
    if !app.commits.is_empty() {
        commit_list::render(app, frame);
    }
}

pub fn render(app: &mut AppState, frame: &mut Frame) {
    let AppMode::Loading {
        title,
        message,
        count,
    } = app.mode
    else {
        return;
    };

    render_background(app, frame);

    let text = match count {
        Some(n) => format!(" {message} {n}"),
        None => format!(" {message}"),
    };
    let mut dialog = Dialog::new().blank().styled_line(text, Color::White);
    if count.is_some() {
        dialog = dialog
            .blank()
            .instructions(&[("Ctrl-C", Color::Cyan, " to quit")]);
    }
    dialog.blank().render(frame, title, Color::Cyan, 60, 0);
}

pub fn render_matrix_confirm(app: &mut AppState, frame: &mut Frame, n: usize) {
    render_background(app, frame);
    Dialog::new()
        .blank()
        .styled_line(format!(" This branch has {n} commits."), Color::White)
        .plain(" Compute hunk group matrix?")
        .blank()
        .instructions(&[("y", Color::Cyan, " Yes   "), ("n", Color::Cyan, " No")])
        .blank()
        .render(frame, "Hunk Group Matrix", Color::Cyan, 60, 0);
}
