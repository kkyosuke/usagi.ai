use crate::presentation::cli::hop::app::{HopApp, InputModal, SelectModal};
use crate::presentation::tui::utils;
use crate::presentation::ui::modal::SelectionModal;
use anyhow::Result;
use console::{measure_text_width, strip_ansi_codes, style};

pub fn render(app: &HopApp) -> Result<()> {
    let term = &app.term;
    let (height, width) = term.size();
    let left_width = 30;
    let right_width = (width as usize)
        .saturating_sub(left_width)
        .saturating_sub(3); // 3 for separators

    // ヘッダー表示
    term.move_cursor_to(0, 0)?;
    term.clear_screen()?;

    term.write_line(&format!(
        "{}",
        style("----- USAGI TERMINAL -----").magenta().bold()
    ))?;
    term.write_line(&format!(
        "MODE: {}",
        style(app.mode().label()).bold().cyan()
    ))?;

    // 左右分割描画
    for i in 0..(height as usize - 6) {
        let left_content = if i == 0 {
            style("workspace").bold().to_string()
        } else {
            let wt_idx = (i - 1) / 2;
            if wt_idx < app.state.worktrees.len() {
                let wt = &app.state.worktrees[wt_idx];
                let is_second_line = (i - 1) % 2 == 1;

                if !is_second_line {
                    let mark_char = "●";
                    let mark_width = measure_text_width(mark_char);
                    let cursor = if wt_idx == app.selected_index && !app.is_command_mode {
                        ">"
                    } else {
                        " "
                    };
                    let is_selected = wt_idx == app.selected_index;
                    let mark = if is_selected {
                        style(mark_char).green().to_string()
                    } else {
                        " ".repeat(mark_width)
                    };

                    let status_icon = match wt.status {
                        crate::domain::project::SessionStatus::Todo => {
                            style(wt.status.icon()).dim().to_string()
                        }
                        crate::domain::project::SessionStatus::Running => {
                            style(wt.status.icon()).green().bold().to_string()
                        }
                        crate::domain::project::SessionStatus::Done => {
                            style(wt.status.icon()).blue().bold().to_string()
                        }
                    };
                    let upstream_icon = if wt.has_upstream {
                        style(" 󰄵").cyan().to_string()
                    } else {
                        "".to_string()
                    };

                    let exists = app.project_path.join(&wt.directory).exists();
                    let branch_text = if exists {
                        wt.branch.clone()
                    } else {
                        format!("{} (--deleted)", wt.branch)
                    };

                    if wt_idx == app.selected_index {
                        format!(
                            "{} {}  {}{}  {}",
                            cursor,
                            mark,
                            style(&branch_text).cyan().bold(),
                            upstream_icon,
                            status_icon
                        )
                    } else {
                        format!(
                            "{} {}  {}{}  {}",
                            cursor,
                            mark,
                            if exists {
                                branch_text
                            } else {
                                style(&branch_text).dim().to_string()
                            },
                            upstream_icon,
                            status_icon
                        )
                    }
                } else {
                    format!(
                        "   {}",
                        style(utils::format_modified_at(&wt.modified_at)).dim()
                    )
                }
            } else {
                "".to_string()
            }
        };

        // 左側の幅を調整
        let left_padding = left_width.saturating_sub(measure_text_width(&left_content));
        let left_display = format!("{}{}", left_content, " ".repeat(left_padding));

        // 右側の表示内容 (履歴)
        let right_content = if i == 0 {
            if let Some(cmd) = &app.active_interaction {
                format!("{}", style(cmd.interaction_label()).bold().cyan())
            } else {
                let label = if app.is_terminal_view {
                    style("TERMINAL").bold().cyan().to_string()
                } else {
                    style("Welcome to usagi terminal!").bold().to_string()
                };
                let wt = &app.state.worktrees[app.selected_index];
                format!("{} (Dir: {})", label, style(&wt.directory).dim())
            }
        } else {
            let history_idx = i.saturating_sub(1);
            if history_idx < app.history.terminal_lines.len() {
                app.history.terminal_lines[history_idx].clone()
            } else {
                "".to_string()
            }
        };

        let right_padding =
            right_width.saturating_sub(measure_text_width(&strip_ansi_codes(&right_content)));
        let right_display = format!("{}{}", right_content, " ".repeat(right_padding));

        term.write_line(&format!("{} | {}", left_display, right_display))?;
    }

    // 入力欄
    term.move_cursor_to(0, height as usize - 4)?;
    term.write_line(&format!("{:-<width$}", "", width = width as usize))?;
    let command_padding = left_width.saturating_sub(measure_text_width("COMMAND"));
    let prompt_prefix = if let Some(cmd) = &app.active_interaction {
        cmd.prompt_sign()
    } else {
        "|"
    };
    let command_prompt = format!(
        "COMMAND{:padding$} {} {}",
        "",
        prompt_prefix,
        app.current_input,
        padding = command_padding
    );
    let command_display = format!("{:width$}", command_prompt, width = width as usize);
    term.write_line(&command_display)?;

    // 下部ヘルプ
    term.move_cursor_to(0, (height as usize).saturating_sub(1))?;
    let help_text = if app.is_command_mode {
        style("Enter: execute, Escape: cancel, Tab: select suggestion, Type to command...")
            .dim()
            .to_string()
    } else {
        style("Use Up/Down to select, Enter to type command, 'q' to quit.")
            .dim()
            .to_string()
    };
    // 最終行でのスクロールを避けるため write_str を使用
    let help_display = format!("{:width$}", help_text, width = width as usize);
    term.write_str(&help_display)?;

    // コマンドモードのポップアップ表示
    if app.is_command_mode && app.active_interaction.is_none() && app.select_modal.is_none() {
        render_command_popup(app, height as usize, width as usize, left_width)?;
    }

    if let Some(modal) = &app.select_modal {
        render_generic_modal(app, modal, height as usize, width as usize, left_width)?;
    }

    if let Some(modal) = &app.input_modal {
        render_input_modal(app, modal, height as usize, width as usize, left_width)?;
    }

    if app.is_command_mode && app.input_modal.is_none() {
        let input_prefix: String = app.current_input.chars().take(app.cursor_pos).collect();
        let prompt_width = if let Some(cmd) = &app.active_interaction {
            cmd.prompt_sign().len()
        } else {
            "|".len()
        };
        let cursor_x = left_width + 2 + prompt_width + measure_text_width(&input_prefix);
        term.move_cursor_to(cursor_x, height as usize - 3)?;
        term.show_cursor()?;
    } else if app.input_modal.is_none() {
        term.hide_cursor()?;
    }

    let _ = term.flush();
    Ok(())
}

fn render_command_popup(
    app: &HopApp,
    height: usize,
    width: usize,
    left_width: usize,
) -> Result<()> {
    let term = &app.term;
    let (usage_text, suggestions) =
        crate::presentation::cli::hop::completion::compute_suggestions(app);

    let mut offset = 5;
    let popup_x = left_width + 3; // | の右側
    let popup_width = width.saturating_sub(popup_x);

    if let Some(usage) = &usage_text {
        let lines: Vec<&str> = usage.lines().collect();
        for (i, line) in lines.iter().rev().enumerate() {
            let y = height.saturating_sub(offset + i);
            term.move_cursor_to(popup_x, y)?;
            let display_line = format!("{:<width$}", line, width = popup_width);
            term.write_str(&style(display_line).black().on_white().to_string())?;
        }
        offset += lines.len();
    }

    if !suggestions.is_empty() && usage_text.is_none() {
        let max_suggestions = 10;
        let display_count = suggestions.len().min(max_suggestions);

        for (idx, (name, desc)) in suggestions.iter().take(display_count).enumerate() {
            let y = height.saturating_sub(offset + (display_count - 1 - idx));
            term.move_cursor_to(popup_x, y)?;
            let content = format!(
                "{:<10} | {:<width$}",
                name,
                desc,
                width = popup_width.saturating_sub(13)
            );
            if app.suggestion_index == Some(idx) {
                term.write_str(&style(content).black().on_cyan().to_string())?;
            } else {
                term.write_str(&style(content).black().on_white().to_string())?;
            }
        }
    }

    Ok(())
}

fn render_generic_modal(
    app: &HopApp,
    modal: &SelectModal,
    height: usize,
    width: usize,
    left_width: usize,
) -> Result<()> {
    let modal_ui = SelectionModal::new(&modal.title, &modal.items, modal.selected_index);
    modal_ui.render(&app.term, height, width, left_width)
}

fn render_input_modal(
    app: &HopApp,
    modal: &InputModal,
    height: usize,
    width: usize,
    left_width: usize,
) -> Result<()> {
    crate::presentation::ui::modal::InputModal::new(
        &modal.title,
        &modal.value,
        modal.value.chars().count(),
    )
    .render(&app.term, height, width, left_width)
}
