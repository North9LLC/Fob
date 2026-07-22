use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

use super::state::{AppState, DashboardTab, Modal, Screen, WizardStep};

// ── Palette ──────────────────────────────────────────────────────────────────
const BLUE: Color = Color::Rgb(79, 127, 255);
const WHITE: Color = Color::Rgb(220, 225, 235);
const MUTED: Color = Color::Rgb(130, 135, 150);
const DIM: Color = Color::Rgb(65, 68, 80);
const RED: Color = Color::Rgb(235, 80, 60);
const GOLD: Color = Color::Rgb(240, 180, 50);
const GREEN: Color = Color::Rgb(60, 200, 100);

fn accent() -> Style {
    Style::default().fg(BLUE)
}
fn muted() -> Style {
    Style::default().fg(MUTED)
}
fn dim() -> Style {
    Style::default().fg(DIM)
}
fn bold_white() -> Style {
    Style::default().fg(WHITE).add_modifier(Modifier::BOLD)
}
fn bold_blue() -> Style {
    Style::default().fg(BLUE).add_modifier(Modifier::BOLD)
}
fn bold_red() -> Style {
    Style::default().fg(RED).add_modifier(Modifier::BOLD)
}

// ── Entry point ───────────────────────────────────────────────────────────────
pub fn render(frame: &mut Frame, state: &AppState) {
    frame.render_widget(
        Block::default().style(Style::default().bg(Color::Rgb(13, 17, 23))),
        frame.area(),
    );

    match &state.screen {
        Screen::Boot => render_boot(frame, state),
        Screen::DevicePicker => render_device_picker(frame, state),
        Screen::Formatting => render_formatting(frame, state),
        Screen::SetupWizard(step) => render_wizard(frame, state, step.clone()),
        Screen::Unlock => render_unlock(frame, state),
        Screen::Dashboard => render_dashboard(frame, state),
        Screen::Done => render_done(frame, state),
        Screen::Error(msg) => render_error(frame, msg),
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────
fn centered_rect(w: u16, h: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(w) / 2;
    let y = area.y + area.height.saturating_sub(h) / 2;
    Rect::new(x, y, w.min(area.width), h.min(area.height))
}

fn hint_span(key: &str) -> Span<'_> {
    Span::styled(
        format!(" {} ", key),
        Style::default()
            .bg(Color::Rgb(30, 35, 45))
            .fg(BLUE)
            .add_modifier(Modifier::BOLD),
    )
}

fn sep_span(text: &str) -> Span<'_> {
    Span::styled(text, muted())
}

// ── Boot ──────────────────────────────────────────────────────────────────────
fn render_boot(frame: &mut Frame, state: &AppState) {
    let area = frame.area();
    let popup = centered_rect(36, 8, area);

    let dots = match (state.tick / 8) % 4 {
        0 => "   ",
        1 => ".  ",
        2 => ".. ",
        _ => "...",
    };

    frame.render_widget(
        Paragraph::new(vec![
            Line::raw(""),
            Line::from(vec![
                Span::styled("  FOB", bold_blue()),
                Span::styled("  ·  Fob", muted()),
            ]),
            Line::raw(""),
            Line::styled("  Encrypted USB Security Vault", muted()),
            Line::raw(""),
            Line::from(vec![
                Span::styled("  Scanning for USB drives", muted()),
                Span::styled(dots, accent()),
            ]),
        ])
        .block(Block::default().borders(Borders::ALL).border_style(dim())),
        popup,
    );
}

// ── Device Picker ─────────────────────────────────────────────────────────────
fn render_device_picker(frame: &mut Frame, state: &AppState) {
    let area = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(4),
            Constraint::Length(1),
        ])
        .split(area);

    // Header
    frame.render_widget(
        Paragraph::new(vec![
            Line::raw(""),
            Line::from(vec![
                Span::styled("  FOB", bold_blue()),
                Span::styled("  —  USB Setup", muted()),
            ]),
            Line::styled(
                "  Select a USB drive to set up as your security vault.",
                muted(),
            ),
        ]),
        chunks[0],
    );

    // Drive list
    let items: Vec<ListItem> = state
        .devices
        .iter()
        .enumerate()
        .map(|(i, dev)| {
            let selected = i == state.selected_device;
            let name_style = if selected { bold_white() } else { muted() };
            let prefix = if selected {
                Span::styled("  ▶ ", accent())
            } else {
                Span::styled("    ", dim())
            };

            let vault_tag = if dev.has_fob_vault {
                Span::styled("  [vault present]", Style::default().fg(GOLD))
            } else {
                Span::styled("  [new drive]", dim())
            };

            ListItem::new(vec![
                Line::from(vec![
                    prefix.clone(),
                    Span::styled(format!("{:<28}", dev.name), name_style),
                    Span::styled(format!("{:>8}", dev.size_display()), muted()),
                    vault_tag,
                ]),
                Line::from(vec![
                    Span::raw("      "),
                    Span::styled(dev.path.display().to_string(), dim()),
                ]),
                Line::raw(""),
            ])
        })
        .collect();

    let is_empty = items.is_empty();
    let mut ls = ListState::default();
    if !is_empty {
        ls.select(Some(state.selected_device));
    }

    frame.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .title(Span::styled("  USB DRIVES ", bold_blue()))
                    .borders(Borders::ALL)
                    .border_style(dim()),
            )
            .highlight_style(Style::default()),
        chunks[1],
        &mut ls,
    );

    if is_empty {
        frame.render_widget(
            Paragraph::new(vec![
                Line::raw(""),
                Line::styled("  No USB drives detected.", muted()),
                Line::styled("  Plug one in, then press r to rescan.", dim()),
            ]),
            Rect::new(
                chunks[1].x + 1,
                chunks[1].y + 1,
                chunks[1].width.saturating_sub(2),
                chunks[1].height.saturating_sub(2),
            ),
        );
    }

    // Hint bar
    let hint = if is_empty {
        Line::from(vec![
            hint_span("r"),
            sep_span(" rescan  "),
            hint_span("q"),
            sep_span(" quit"),
        ])
    } else {
        Line::from(vec![
            hint_span("↑/↓"),
            sep_span(" select  "),
            hint_span("Enter"),
            sep_span(" set up this drive  "),
            hint_span("r"),
            sep_span(" rescan  "),
            hint_span("q"),
            sep_span(" quit"),
        ])
    };
    frame.render_widget(Paragraph::new(hint), chunks[2]);
}

// ── Formatting ────────────────────────────────────────────────────────────────
fn render_formatting(frame: &mut Frame, state: &AppState) {
    let area = frame.area();
    let popup = centered_rect(44, 8, area);
    frame.render_widget(Clear, popup);

    let spinner = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let s = spinner[(state.tick as usize / 2) % spinner.len()];

    let dev_name = state
        .devices
        .get(state.selected_device)
        .map(|d| format!("{}  ({})", d.name, d.size_display()))
        .unwrap_or_else(|| "drive".into());

    frame.render_widget(
        Paragraph::new(vec![
            Line::raw(""),
            Line::from(vec![
                Span::styled(format!("  {} ", s), accent()),
                Span::styled(
                    "Formatting drive, please wait...",
                    Style::default().fg(WHITE),
                ),
            ]),
            Line::raw(""),
            Line::from(vec![Span::raw("  "), Span::styled(&dev_name, muted())]),
            Line::raw(""),
            Line::styled("  This may take up to 30 seconds.", dim()),
        ])
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(BLUE)),
        ),
        popup,
    );
}

// ── Setup Wizard ──────────────────────────────────────────────────────────────
fn render_wizard(frame: &mut Frame, state: &AppState, step: WizardStep) {
    let area = frame.area();
    let popup = centered_rect(62, 20, area);
    frame.render_widget(Clear, popup);

    // ExistingVault is its own layout — no numbered steps / progress bar.
    if step == WizardStep::ExistingVault {
        frame.render_widget(
            Block::default()
                .title(Span::styled("  Fob Detected  ", bold_blue()))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(GOLD)),
            popup,
        );
        let content = Rect::new(popup.x + 2, popup.y + 2, popup.width - 4, popup.height - 4);
        render_step_existing_vault(frame, state, content);
        return;
    }

    let (step_n, step_total, title) = match &step {
        WizardStep::ExistingVault => unreachable!(),
        WizardStep::ConfirmWipe => (1, 3, "Erase Drive"),
        WizardStep::Master => (2, 3, "Set Passphrase"),
        WizardStep::Confirm => (3, 3, "Confirm & Create"),
    };

    let filled = (step_n * 16) / step_total;
    let progress = format!("{}{}", "▓".repeat(filled), "░".repeat(16 - filled));

    frame.render_widget(
        Block::default()
            .title(Span::styled(
                format!("  Setup  {}/{}  {}  ", step_n, step_total, title),
                bold_blue(),
            ))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(BLUE)),
        popup,
    );

    let bar_area = Rect::new(popup.x + 2, popup.y + 2, popup.width - 4, 1);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(progress, accent()),
            Span::styled(format!("  step {}/{}", step_n, step_total), muted()),
        ])),
        bar_area,
    );

    let content = Rect::new(popup.x + 2, popup.y + 4, popup.width - 4, popup.height - 6);

    match step {
        WizardStep::ExistingVault => unreachable!(),
        WizardStep::ConfirmWipe => render_step_wipe(frame, state, content),
        WizardStep::Master => render_step_master(frame, state, content),
        WizardStep::Confirm => render_step_confirm(frame, state, content),
    }
}

fn render_step_existing_vault(frame: &mut Frame, state: &AppState, area: Rect) {
    let dev_name = state
        .devices
        .get(state.selected_device)
        .map(|d| format!("{}  ({})", d.name, d.size_display()))
        .unwrap_or_else(|| "Unknown".into());

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Length(1)])
        .split(area);

    frame.render_widget(
        Paragraph::new(vec![
            Line::styled("A Fob vault is already on this drive:", muted()),
            Line::raw(""),
            Line::from(vec![Span::raw("  "), Span::styled(&dev_name, bold_white())]),
            Line::raw(""),
            Line::from(vec![
                Span::styled("  o  ", bold_blue()),
                Span::styled("Open", Style::default().fg(WHITE)),
                Span::styled("  — unlock and browse your vault", muted()),
            ]),
            Line::raw(""),
            Line::from(vec![
                Span::styled("  u  ", bold_blue()),
                Span::styled("Update", Style::default().fg(WHITE)),
                Span::styled(
                    "  — install the latest vault UI, keep your vault data",
                    muted(),
                ),
            ]),
            Line::raw(""),
            Line::from(vec![
                Span::styled("  f  ", bold_red()),
                Span::styled("Fresh setup", Style::default().fg(WHITE)),
                Span::styled("  — erase everything and start over", muted()),
            ]),
        ])
        .wrap(Wrap { trim: true }),
        chunks[0],
    );

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            hint_span("o"),
            sep_span(" open  "),
            hint_span("u"),
            sep_span(" update  "),
            hint_span("f"),
            sep_span(" fresh setup  "),
            hint_span("Esc"),
            sep_span(" back"),
        ])),
        chunks[1],
    );
}

fn render_step_wipe(frame: &mut Frame, state: &AppState, area: Rect) {
    let dev_name = state
        .devices
        .get(state.selected_device)
        .map(|d| format!("{}  ({})", d.name, d.size_display()))
        .unwrap_or_else(|| "Unknown".into());

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(6), Constraint::Length(1)])
        .split(area);

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled("WARNING  ", bold_red()),
                Span::styled("All data on this drive will be erased:", muted()),
            ]),
            Line::raw(""),
            Line::from(vec![Span::raw("  "), Span::styled(&dev_name, bold_white())]),
            Line::raw(""),
            Line::styled(
                "The drive will be formatted and a fresh encrypted vault",
                muted(),
            ),
            Line::styled("will be created. This cannot be undone.", muted()),
            Line::raw(""),
        ])
        .wrap(Wrap { trim: true }),
        chunks[0],
    );

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            hint_span("y"),
            sep_span(" erase and continue  "),
            hint_span("n"),
            sep_span(" go back"),
        ])),
        chunks[1],
    );
}

fn render_step_master(frame: &mut Frame, state: &AppState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    frame.render_widget(
        Paragraph::new(vec![
            Line::styled("Choose a strong passphrase to protect your vault.", muted()),
            Line::styled("Use 5+ random words for best security.", dim()),
        ]),
        chunks[0],
    );

    let f0 = state.wizard.field == 0;
    let f1 = state.wizard.field == 1;

    let mismatch = state.wizard.mismatch_flash > 0;

    let pass_border = if f0 { Style::default().fg(BLUE) } else { dim() };
    let pass_title = if f0 {
        " ▶ Passphrase "
    } else {
        " Passphrase "
    };
    frame.render_widget(
        Paragraph::new(Span::styled(
            "●".repeat(state.wizard.main_pass.chars().count()),
            if f0 {
                Style::default().fg(WHITE)
            } else {
                dim()
            },
        ))
        .block(
            Block::default()
                .title(pass_title)
                .borders(Borders::ALL)
                .border_style(pass_border),
        ),
        chunks[1],
    );
    if f0 {
        let inner_width = chunks[1].width.saturating_sub(2);
        let x_off = (state.wizard.cursor as u16).min(inner_width.saturating_sub(1));
        frame.set_cursor_position((chunks[1].x + 1 + x_off, chunks[1].y + 1));
    }

    let conf_border = if mismatch {
        Style::default().fg(RED)
    } else if f1 {
        Style::default().fg(BLUE)
    } else {
        dim()
    };
    let conf_title = if mismatch {
        " ✗ Passphrases don't match "
    } else if f1 {
        " ▶ Confirm passphrase "
    } else {
        " Confirm passphrase "
    };
    frame.render_widget(
        Paragraph::new(Span::styled(
            "●".repeat(state.wizard.main_pass_confirm.chars().count()),
            if f1 {
                Style::default().fg(WHITE)
            } else {
                dim()
            },
        ))
        .block(
            Block::default()
                .title(conf_title)
                .borders(Borders::ALL)
                .border_style(conf_border),
        ),
        chunks[2],
    );
    if f1 {
        let inner_width = chunks[2].width.saturating_sub(2);
        let x_off = (state.wizard.cursor as u16).min(inner_width.saturating_sub(1));
        frame.set_cursor_position((chunks[2].x + 1 + x_off, chunks[2].y + 1));
    }

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            hint_span("Tab"),
            sep_span(" switch field  "),
            hint_span("Enter"),
            sep_span(" continue  "),
            hint_span("Esc"),
            sep_span(" back"),
        ])),
        chunks[4],
    );
}

fn render_step_confirm(frame: &mut Frame, state: &AppState, area: Rect) {
    let dev_name = state
        .devices
        .get(state.selected_device)
        .map(|d| format!("{}  ({})", d.name, d.size_display()))
        .unwrap_or_else(|| "Unknown".into());

    let pass_len = state.wizard.main_pass.len();
    let (strength_label, strength_color) = if pass_len >= 20 {
        ("Strong", GREEN)
    } else if pass_len >= 12 {
        ("Good", GOLD)
    } else {
        ("Short — consider using a longer passphrase", RED)
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(6), Constraint::Length(1)])
        .split(area);

    frame.render_widget(
        Paragraph::new(vec![
            Line::styled("Ready to create your vault:", muted()),
            Line::raw(""),
            Line::from(vec![
                Span::styled("  Drive       ", muted()),
                Span::styled(&dev_name, Style::default().fg(WHITE)),
            ]),
            Line::from(vec![
                Span::styled("  Passphrase  ", muted()),
                Span::styled(strength_label, Style::default().fg(strength_color)),
            ]),
            Line::raw(""),
            Line::styled(
                "After setup, open index.html from the USB drive in your browser.",
                dim(),
            ),
        ]),
        chunks[0],
    );

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            hint_span("Enter"),
            sep_span(" create vault  "),
            hint_span("Esc"),
            sep_span(" back"),
        ])),
        chunks[1],
    );
}

// ── Done ──────────────────────────────────────────────────────────────────────
fn render_done(frame: &mut Frame, state: &AppState) {
    let area = frame.area();
    let popup = centered_rect(56, 14, area);
    frame.render_widget(Clear, popup);

    let (headline, sub) = if state.update_mode {
        ("  Vault UI updated.", "  Your vault data is untouched.")
    } else {
        (
            "  Vault created successfully.",
            "  Your USB drive is ready to use.",
        )
    };

    frame.render_widget(
        Paragraph::new(vec![
            Line::raw(""),
            Line::from(vec![Span::styled(headline, bold_blue())]),
            Line::raw(""),
            Line::styled(sub, Style::default().fg(WHITE)),
            Line::raw(""),
            Line::styled("  Next steps:", muted()),
            Line::styled("    1.  Eject the USB drive safely.", muted()),
            Line::styled("    2.  Plug it in to any computer.", muted()),
            Line::styled("    3.  Open  index.html  in your browser.", muted()),
            Line::raw(""),
            Line::from(vec![Span::styled("  Press any key to exit.", dim())]),
        ])
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(BLUE)),
        ),
        popup,
    );
}

// ── Unlock ────────────────────────────────────────────────────────────────────
fn render_unlock(frame: &mut Frame, state: &AppState) {
    let area = frame.area();
    let popup = centered_rect(56, 11, area);
    frame.render_widget(Clear, popup);

    frame.render_widget(
        Block::default()
            .title(Span::styled("  Unlock Vault  ", bold_blue()))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(BLUE)),
        popup,
    );

    let content = Rect::new(popup.x + 2, popup.y + 2, popup.width - 4, popup.height - 4);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(content);

    frame.render_widget(
        Paragraph::new(Line::styled(
            "Enter your passphrase — main, decoy, or duress.",
            muted(),
        )),
        chunks[0],
    );

    frame.render_widget(
        Paragraph::new(Span::styled(
            "●".repeat(state.unlock.passphrase.chars().count()),
            Style::default().fg(WHITE),
        ))
        .block(
            Block::default()
                .title(" Passphrase ")
                .borders(Borders::ALL)
                .border_style(if state.unlock.error.is_some() {
                    Style::default().fg(RED)
                } else {
                    Style::default().fg(BLUE)
                }),
        ),
        chunks[1],
    );
    {
        let inner_width = chunks[1].width.saturating_sub(2);
        let x_off = (state.unlock.cursor as u16).min(inner_width.saturating_sub(1));
        frame.set_cursor_position((chunks[1].x + 1 + x_off, chunks[1].y + 1));
    }

    if let Some(err) = &state.unlock.error {
        frame.render_widget(
            Paragraph::new(Line::styled(err.as_str(), Style::default().fg(RED))),
            chunks[2],
        );
    }

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            hint_span("Enter"),
            sep_span(" unlock  "),
            hint_span("Esc"),
            sep_span(" back"),
        ])),
        chunks[4],
    );
}

// ── Dashboard ─────────────────────────────────────────────────────────────────
fn mask(s: &str, reveal: bool) -> String {
    if reveal {
        s.to_string()
    } else {
        "•".repeat(s.chars().count().clamp(4, 32))
    }
}

fn render_dashboard(frame: &mut Frame, state: &AppState) {
    let Some(dash) = state.dashboard.as_ref() else {
        return;
    };
    let area = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(6),
            Constraint::Length(1),
        ])
        .split(area);

    // Tab bar.
    let tabs: Vec<Span> = DashboardTab::ALL
        .iter()
        .flat_map(|t| {
            let active = *t == dash.tab;
            let label = format!(" {} ", t.label());
            vec![
                Span::styled(
                    label,
                    if active {
                        Style::default()
                            .fg(WHITE)
                            .bg(Color::Rgb(30, 60, 140))
                            .add_modifier(Modifier::BOLD)
                    } else {
                        muted()
                    },
                ),
                Span::raw(" "),
            ]
        })
        .collect();

    let header_line = if dash.tab == DashboardTab::Ssh {
        let mut spans = tabs;
        match &dash.ssh_agent {
            Some(agent) => {
                spans.push(Span::styled("   Agent: ", muted()));
                spans.push(Span::styled(
                    agent.socket_path().display().to_string(),
                    Style::default().fg(GREEN),
                ));
            }
            None => spans.push(Span::styled("   Agent: not running", dim())),
        }
        Line::from(spans)
    } else {
        Line::from(tabs)
    };
    frame.render_widget(Paragraph::new(header_line), chunks[0]);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
        .split(chunks[1]);

    render_dashboard_list(frame, dash, body[0]);
    render_dashboard_detail(frame, dash, body[1]);

    // Footer hint / status bar.
    let footer = if let Some(status) = &dash.status {
        Line::styled(format!("  {status}"), Style::default().fg(GREEN))
    } else {
        Line::from(vec![
            hint_span("↑/↓"),
            sep_span(" select  "),
            hint_span("Tab"),
            sep_span(" category  "),
            hint_span("a"),
            sep_span(" add  "),
            hint_span("e"),
            sep_span(" edit  "),
            hint_span("d"),
            sep_span(" delete  "),
            hint_span("r"),
            sep_span(" reveal  "),
            hint_span("c"),
            sep_span(" copy  "),
            hint_span("q"),
            sep_span(" lock"),
        ])
    };
    frame.render_widget(Paragraph::new(footer), chunks[2]);

    if !matches!(dash.modal, Modal::None) {
        render_dashboard_modal(frame, dash, area);
    }
}

fn render_dashboard_list(frame: &mut Frame, dash: &super::state::DashboardState, area: Rect) {
    let items: Vec<ListItem> = match dash.tab {
        DashboardTab::Passwords => dash
            .blob
            .passwords
            .iter()
            .map(|e| ListItem::new(format!("{}  ({})", e.name, e.username)))
            .collect(),
        DashboardTab::Totp => dash
            .blob
            .totps
            .iter()
            .map(|e| ListItem::new(format!("{}  ({})", e.issuer, e.account)))
            .collect(),
        DashboardTab::Ssh => dash
            .blob
            .ssh_keys
            .iter()
            .map(|e| ListItem::new(format!("{}  [{:?}]", e.name, e.algorithm)))
            .collect(),
        DashboardTab::Notes => dash
            .blob
            .notes
            .iter()
            .map(|e| ListItem::new(e.title.clone()))
            .collect(),
    };

    let empty = items.is_empty();
    let mut ls = ListState::default();
    if !empty {
        ls.select(Some(dash.selected));
    }

    frame.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .title(Span::styled(
                        format!("  {}  ", dash.tab.label()),
                        bold_blue(),
                    ))
                    .borders(Borders::ALL)
                    .border_style(dim()),
            )
            .highlight_style(Style::default().fg(WHITE).bg(Color::Rgb(30, 40, 60))),
        area,
        &mut ls,
    );

    if empty {
        frame.render_widget(
            Paragraph::new(Line::styled(
                "  Nothing here yet — press a to add one.",
                dim(),
            )),
            Rect::new(area.x + 1, area.y + 1, area.width.saturating_sub(2), 1),
        );
    }
}

fn render_dashboard_detail(frame: &mut Frame, dash: &super::state::DashboardState, area: Rect) {
    let block = Block::default()
        .title(Span::styled("  Detail  ", bold_blue()))
        .borders(Borders::ALL)
        .border_style(dim());
    frame.render_widget(block, area);
    let inner = Rect::new(
        area.x + 2,
        area.y + 1,
        area.width.saturating_sub(4),
        area.height.saturating_sub(2),
    );

    let lines: Vec<Line> = match dash.tab {
        DashboardTab::Passwords => match dash.blob.passwords.get(dash.selected) {
            Some(e) => vec![
                Line::from(vec![
                    Span::styled("Name      ", muted()),
                    Span::styled(&e.name, bold_white()),
                ]),
                Line::from(vec![
                    Span::styled("Username  ", muted()),
                    Span::styled(&e.username, Style::default().fg(WHITE)),
                ]),
                Line::from(vec![
                    Span::styled("Password  ", muted()),
                    Span::styled(
                        mask(e.password.expose(), dash.reveal),
                        Style::default().fg(WHITE),
                    ),
                ]),
            ],
            None => vec![],
        },
        DashboardTab::Totp => match dash.blob.totps.get(dash.selected) {
            Some(e) => {
                let mut lines = vec![
                    Line::from(vec![
                        Span::styled("Issuer   ", muted()),
                        Span::styled(&e.issuer, bold_white()),
                    ]),
                    Line::from(vec![
                        Span::styled("Account  ", muted()),
                        Span::styled(&e.account, Style::default().fg(WHITE)),
                    ]),
                ];
                if dash.reveal {
                    match fob_core::totp::generate_now(e) {
                        Ok(code) => {
                            let remaining =
                                fob_core::totp::seconds_remaining(e.period).unwrap_or(0);
                            lines.push(Line::from(vec![
                                Span::styled("Code     ", muted()),
                                Span::styled(
                                    code,
                                    Style::default().fg(GREEN).add_modifier(Modifier::BOLD),
                                ),
                                Span::styled(format!("   ({remaining}s)"), dim()),
                            ]));
                        }
                        Err(e) => lines.push(Line::styled(
                            format!("Code error: {e}"),
                            Style::default().fg(RED),
                        )),
                    }
                } else {
                    lines.push(Line::styled("Code      press r to reveal", dim()));
                }
                lines
            }
            None => vec![],
        },
        DashboardTab::Ssh => match dash.blob.ssh_keys.get(dash.selected) {
            Some(e) => vec![
                Line::from(vec![
                    Span::styled("Name         ", muted()),
                    Span::styled(&e.name, bold_white()),
                ]),
                Line::from(vec![
                    Span::styled("Algorithm    ", muted()),
                    Span::styled(format!("{:?}", e.algorithm), Style::default().fg(WHITE)),
                ]),
                Line::from(vec![
                    Span::styled("Fingerprint  ", muted()),
                    Span::styled(&e.fingerprint, Style::default().fg(WHITE)),
                ]),
                Line::from(vec![
                    Span::styled("Public key   ", muted()),
                    Span::styled(&e.public_key, dim()),
                ]),
                Line::from(vec![
                    Span::styled("Private key  ", muted()),
                    Span::styled(
                        mask(e.private_key.expose(), dash.reveal),
                        Style::default().fg(WHITE),
                    ),
                ]),
            ],
            None => vec![],
        },
        DashboardTab::Notes => match dash.blob.notes.get(dash.selected) {
            Some(e) => vec![
                Line::from(vec![
                    Span::styled("Title  ", muted()),
                    Span::styled(&e.title, bold_white()),
                ]),
                Line::raw(""),
                Line::styled(
                    mask(e.body.expose(), dash.reveal),
                    Style::default().fg(WHITE),
                ),
            ],
            None => vec![],
        },
    };

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}

fn render_dashboard_modal(frame: &mut Frame, dash: &super::state::DashboardState, area: Rect) {
    match &dash.modal {
        Modal::None => {}
        Modal::ConfirmDelete => {
            let popup = centered_rect(48, 7, area);
            frame.render_widget(Clear, popup);
            frame.render_widget(
                Paragraph::new(vec![
                    Line::raw(""),
                    Line::styled(
                        "  Delete this entry? This cannot be undone.",
                        Style::default().fg(WHITE),
                    ),
                    Line::raw(""),
                    Line::from(vec![
                        hint_span("y"),
                        sep_span(" delete  "),
                        hint_span("n"),
                        sep_span(" cancel"),
                    ]),
                ])
                .block(
                    Block::default()
                        .title(Span::styled("  Confirm Delete  ", bold_red()))
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(RED)),
                ),
                popup,
            );
        }
        Modal::AddPassword(form) => {
            // height 13, not 11: 3 fields (Length(3) each = 9) + the Min(0)
            // gap + the Length(1) hint footer need at least 10 content rows,
            // which needs at least 12 popup rows once the 2 border rows are
            // added — 11 was clipping the footer (including the F2 hint)
            // entirely off the bottom, caught by a TestBackend render test.
            // Width 72, not 56: this is the one modal with the extra "F2
            // generate" hint, and the footer line (base hints + F2 hint)
            // doesn't fit the narrower width the other modals use — it was
            // being silently truncated off the right edge.
            let popup = centered_rect(72, 13, area);
            frame.render_widget(Clear, popup);
            render_form(
                frame,
                popup,
                if form.editing.is_some() {
                    "Edit Password"
                } else {
                    "Add Password"
                },
                form.cursor,
                &[
                    ("Name", &form.name, form.field == 0, false),
                    ("Username", &form.username, form.field == 1, false),
                    ("Password", &form.password, form.field == 2, true),
                ],
                Some(("F2", "generate")),
            );
        }
        Modal::AddNote(form) => {
            let popup = centered_rect(56, 11, area);
            frame.render_widget(Clear, popup);
            render_form(
                frame,
                popup,
                if form.editing.is_some() {
                    "Edit Note"
                } else {
                    "Add Note"
                },
                form.cursor,
                &[
                    ("Title", &form.title, form.field == 0, false),
                    ("Body", &form.body, form.field == 1, false),
                ],
                None,
            );
        }
        Modal::AddTotp(form) => {
            // Same fix as AddPassword above — 3 fields need popup height 13,
            // not 11, or the hint footer is clipped off entirely.
            let popup = centered_rect(56, 13, area);
            frame.render_widget(Clear, popup);
            render_form(
                frame,
                popup,
                if form.editing.is_some() {
                    "Edit TOTP (base32 secret)"
                } else {
                    "Add TOTP (base32 secret)"
                },
                form.cursor,
                &[
                    ("Issuer", &form.issuer, form.field == 0, false),
                    ("Account", &form.account, form.field == 1, false),
                    ("Secret", &form.secret, form.field == 2, true),
                ],
                None,
            );
        }
        Modal::AddSsh(form) => {
            let popup = centered_rect(64, 13, area);
            frame.render_widget(Clear, popup);
            render_form(
                frame,
                popup,
                if form.editing.is_some() {
                    "Edit SSH Key"
                } else {
                    "Import SSH Key"
                },
                form.cursor,
                &[
                    ("Name", &form.name, form.field == 0, false),
                    ("Public key", &form.public_key, form.field == 1, false),
                    ("Private key", &form.private_key, form.field == 2, true),
                ],
                None,
            );
        }
    }
}

/// Shared renderer for the add-entry modals: a title bar, one bordered field
/// per row (masked if `secret`), and a fixed hint footer. `cursor` is the
/// char position within whichever field is currently active.
fn render_form(
    frame: &mut Frame,
    popup: Rect,
    title: &str,
    cursor: usize,
    fields: &[(&str, &str, bool, bool)],
    extra_hint: Option<(&str, &str)>,
) {
    frame.render_widget(
        Block::default()
            .title(Span::styled(format!("  {title}  "), bold_blue()))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(BLUE)),
        popup,
    );

    let mut constraints: Vec<Constraint> = fields.iter().map(|_| Constraint::Length(3)).collect();
    constraints.push(Constraint::Min(0));
    constraints.push(Constraint::Length(1));

    let content = Rect::new(popup.x + 2, popup.y + 1, popup.width - 4, popup.height - 2);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(content);

    for (i, (label, value, active, secret)) in fields.iter().enumerate() {
        let display = if *secret {
            "●".repeat(value.chars().count())
        } else {
            (*value).to_string()
        };
        frame.render_widget(
            Paragraph::new(Span::styled(
                display,
                if *active {
                    Style::default().fg(WHITE)
                } else {
                    dim()
                },
            ))
            .block(
                Block::default()
                    .title(format!(" {label} "))
                    .borders(Borders::ALL)
                    .border_style(if *active {
                        Style::default().fg(BLUE)
                    } else {
                        dim()
                    }),
            ),
            chunks[i],
        );
        if *active {
            let inner_width = chunks[i].width.saturating_sub(2);
            let x_off = (cursor as u16).min(inner_width.saturating_sub(1));
            frame.set_cursor_position((chunks[i].x + 1 + x_off, chunks[i].y + 1));
        }
    }

    let extra_desc = extra_hint.map(|(_, desc)| format!(" {desc}"));
    let mut hints = vec![
        hint_span("Tab"),
        sep_span(" next field  "),
        hint_span("Enter"),
        sep_span(" next / save  "),
        hint_span("Esc"),
        sep_span(" cancel"),
    ];
    if let Some((key, _)) = extra_hint {
        hints.push(sep_span("  "));
        hints.push(hint_span(key));
        hints.push(sep_span(extra_desc.as_deref().unwrap()));
    }
    frame.render_widget(Paragraph::new(Line::from(hints)), chunks[fields.len() + 1]);
}

// ── Error ─────────────────────────────────────────────────────────────────────
fn render_error(frame: &mut Frame, msg: &str) {
    let area = frame.area();
    let popup = centered_rect(60, 10, area);
    frame.render_widget(Clear, popup);

    frame.render_widget(
        Paragraph::new(format!("\n  {msg}\n\n  Press any key to exit."))
            .block(
                Block::default()
                    .title(Span::styled("  Error  ", bold_red()))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(RED)),
            )
            .wrap(Wrap { trim: true })
            .style(muted()),
        popup,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::state::{DashboardState, PasswordForm};
    use fob_core::vault::{unlock_vault, VaultFile, VaultInitParams};
    use ratatui::{backend::TestBackend, Terminal};

    /// Render `state` into an in-memory buffer and flatten it to plain text,
    /// so tests can assert on what a user would actually see rendered —
    /// catching layout/wiring bugs that a pure state-mutation test can't
    /// (e.g. a hint that's computed but never reaches the screen).
    fn rendered_text(state: &AppState) -> String {
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, state)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        let mut out = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                out.push_str(buffer[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    fn dashboard_state_with_modal(modal: Modal) -> AppState {
        dashboard_state(DashboardTab::Passwords, modal, false)
    }

    /// Build a `Screen::Dashboard` state on the given tab, optionally with
    /// one populated entry of the matching kind pushed into every tab's
    /// blob (so populated-state tests can flip tabs on the same fixture).
    fn dashboard_state(tab: DashboardTab, modal: Modal, populate: bool) -> AppState {
        let bytes = fob_core::vault::init_vault(VaultInitParams {
            main_passphrase: b"test-pass".to_vec(),
            decoy_passphrase: None,
            duress_passphrase: None,
            vault_size: 256 * 1024,
            decoy_blob: None,
            kdf_iterations: 1000,
        })
        .unwrap();
        let (slot, mut blob) = unlock_vault(&bytes, b"test-pass").unwrap();
        let vault_file = VaultFile::from_bytes(bytes).unwrap();

        if populate {
            blob.passwords.push(fob_core::types::PasswordEntry::new(
                "GitHub", "alice", "hunter2",
            ));
            blob.totps.push(fob_core::types::TotpEntry::new(
                "Example",
                "alice@example.com",
                b"12345678901234567890".to_vec(),
            ));
            blob.ssh_keys.push(
                fob_core::types::SshKeyEntry::new(
                    "laptop",
                    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIEJm7X5tIxbUkIb6VLD91P65Cr0iqKyTKTDd0cYpQHtv test@example",
                    "-----BEGIN OPENSSH PRIVATE KEY-----\nfake\n-----END OPENSSH PRIVATE KEY-----",
                )
                .unwrap(),
            );
            blob.notes
                .push(fob_core::types::NoteEntry::new("Recovery", "1234-5678"));
        }

        let dash = DashboardState {
            vault_path: std::path::PathBuf::from("/tmp/does-not-matter/vault.fob"),
            vault_file,
            slot,
            passphrase: "test-pass".to_string(),
            blob,
            tab,
            selected: 0,
            modal,
            reveal: false,
            status: None,
            ssh_agent: None,
            clipboard: None,
        };

        let mut state = AppState::new(Vec::new());
        state.screen = Screen::Dashboard;
        state.dashboard = Some(dash);
        state
    }

    fn test_device(name: &str) -> crate::device::UsbDevice {
        crate::device::UsbDevice {
            name: name.to_string(),
            size_bytes: 32 * 1024 * 1024 * 1024,
            path: std::path::PathBuf::from("/mnt/test"),
            disk_node: "disk4".to_string(),
            serial: None,
            has_fob_vault: false,
        }
    }

    #[test]
    fn password_modal_shows_the_f2_generate_hint_when_password_field_is_active() {
        let form = PasswordForm {
            name: String::new(),
            username: String::new(),
            password: String::new(),
            field: 2, // Password field active
            cursor: 0,
            editing: None,
        };
        let state = dashboard_state_with_modal(Modal::AddPassword(form));
        let text = rendered_text(&state);
        assert!(
            text.contains("F2") && text.contains("generate"),
            "expected the F2 generate hint to render in the password modal:\n{text}"
        );
    }

    #[test]
    fn other_modals_do_not_show_the_f2_hint() {
        let form = crate::tui::state::NoteForm {
            title: String::new(),
            body: String::new(),
            field: 0,
            cursor: 0,
            editing: None,
        };
        let state = dashboard_state_with_modal(Modal::AddNote(form));
        let text = rendered_text(&state);
        assert!(
            !text.contains("F2"),
            "the note modal has no password field — it must not show the generate hint:\n{text}"
        );
    }

    // ── Setup wizard ────────────────────────────────────────────────────

    #[test]
    fn wizard_existing_vault_step_shows_open_update_and_fresh_options() {
        let mut state = AppState::new(vec![test_device("Kingston DataTraveler")]);
        state.screen = Screen::SetupWizard(WizardStep::ExistingVault);
        let text = rendered_text(&state);
        assert!(text.contains("Fob Detected"), "{text}");
        assert!(text.contains("Open") && text.contains("Update") && text.contains("Fresh setup"));
        assert!(text.contains("Kingston DataTraveler"), "{text}");
    }

    #[test]
    fn wizard_confirm_wipe_step_shows_warning_and_device_name() {
        let mut state = AppState::new(vec![test_device("SanDisk Ultra")]);
        state.screen = Screen::SetupWizard(WizardStep::ConfirmWipe);
        let text = rendered_text(&state);
        assert!(text.contains("WARNING"), "{text}");
        assert!(text.contains("SanDisk Ultra"), "{text}");
        assert!(text.contains("erase and continue"));
    }

    #[test]
    fn wizard_master_step_shows_masked_passphrase_dots() {
        let mut state = AppState::new(Vec::new());
        state.screen = Screen::SetupWizard(WizardStep::Master);
        state.wizard.main_pass = "hunter2hunter2".to_string();
        state.wizard.field = 0;
        state.wizard.cursor = state.wizard.main_pass.chars().count();
        let text = rendered_text(&state);
        assert!(
            text.contains("●●●●●●●●●●●●●●"),
            "expected 14 masked dots for the passphrase field:\n{text}"
        );
        assert!(
            !text.contains("hunter2"),
            "passphrase must never render in cleartext:\n{text}"
        );
    }

    #[test]
    fn wizard_master_step_mismatch_flash_shows_error_style() {
        let mut state = AppState::new(Vec::new());
        state.screen = Screen::SetupWizard(WizardStep::Master);
        state.wizard.main_pass = "correct-horse".to_string();
        state.wizard.main_pass_confirm = "incorrect-horse".to_string();
        state.wizard.mismatch_flash = 15;
        let text = rendered_text(&state);
        assert!(
            text.contains("match"),
            "expected the mismatch message to render when mismatch_flash is active:\n{text}"
        );
    }

    #[test]
    fn wizard_confirm_step_flags_a_short_passphrase_as_weak() {
        let mut state = AppState::new(vec![test_device("Drive")]);
        state.screen = Screen::SetupWizard(WizardStep::Confirm);
        state.wizard.main_pass = "short".to_string();
        let text = rendered_text(&state);
        assert!(text.contains("Short"), "{text}");
    }

    #[test]
    fn wizard_confirm_step_flags_a_long_passphrase_as_strong() {
        let mut state = AppState::new(vec![test_device("Drive")]);
        state.screen = Screen::SetupWizard(WizardStep::Confirm);
        state.wizard.main_pass = "a".repeat(20);
        let text = rendered_text(&state);
        assert!(text.contains("Strong"), "{text}");
    }

    // ── Unlock ──────────────────────────────────────────────────────────

    #[test]
    fn unlock_error_shown_state_renders_the_error_message() {
        let mut state = AppState::new(Vec::new());
        state.screen = Screen::Unlock;
        state.unlock.passphrase = "wrong-pass".to_string();
        state.unlock.cursor = state.unlock.passphrase.chars().count();
        state.unlock.error = Some("Incorrect passphrase".to_string());
        let text = rendered_text(&state);
        assert!(text.contains("Incorrect passphrase"), "{text}");
    }

    #[test]
    fn unlock_no_error_state_renders_no_error_text() {
        let mut state = AppState::new(Vec::new());
        state.screen = Screen::Unlock;
        let text = rendered_text(&state);
        assert!(!text.contains("Incorrect"));
    }

    // ── Dashboard tabs: empty and populated ──────────────────────────────

    #[test]
    fn every_dashboard_tab_shows_the_empty_hint_when_no_entries() {
        for tab in DashboardTab::ALL {
            let state = dashboard_state(tab, Modal::None, false);
            let text = rendered_text(&state);
            assert!(
                text.contains("Nothing here yet"),
                "tab {:?} should show the empty-state hint:\n{text}",
                tab
            );
        }
    }

    #[test]
    fn passwords_tab_populated_shows_entry_and_masked_password() {
        let state = dashboard_state(DashboardTab::Passwords, Modal::None, true);
        let text = rendered_text(&state);
        assert!(text.contains("GitHub"), "{text}");
        assert!(text.contains("alice"), "{text}");
        assert!(
            !text.contains("hunter2"),
            "password must be masked by default:\n{text}"
        );
    }

    #[test]
    fn totp_tab_populated_shows_issuer_and_account_but_hides_code_until_reveal() {
        let state = dashboard_state(DashboardTab::Totp, Modal::None, true);
        let text = rendered_text(&state);
        assert!(text.contains("Example"), "{text}");
        assert!(text.contains("alice@example.com"), "{text}");
        assert!(text.contains("press r to reveal"), "{text}");
    }

    #[test]
    fn ssh_tab_populated_shows_name_and_public_key_but_masks_private_key() {
        let state = dashboard_state(DashboardTab::Ssh, Modal::None, true);
        let text = rendered_text(&state);
        assert!(text.contains("laptop"), "{text}");
        assert!(text.contains("ssh-ed25519"), "{text}");
        assert!(
            !text.contains("BEGIN OPENSSH PRIVATE KEY"),
            "private key must be masked by default:\n{text}"
        );
    }

    #[test]
    fn notes_tab_populated_shows_title_but_masks_body() {
        let state = dashboard_state(DashboardTab::Notes, Modal::None, true);
        let text = rendered_text(&state);
        assert!(text.contains("Recovery"), "{text}");
        assert!(
            !text.contains("1234-5678"),
            "note body must be masked by default:\n{text}"
        );
    }

    // ── Delete confirmation ───────────────────────────────────────────────

    #[test]
    fn delete_confirmation_modal_shows_warning_and_keys() {
        let state = dashboard_state(DashboardTab::Passwords, Modal::ConfirmDelete, true);
        let text = rendered_text(&state);
        assert!(text.contains("Delete this entry"), "{text}");
        assert!(text.contains("delete") && text.contains("cancel"), "{text}");
    }

    // ── Edit-mode modals ──────────────────────────────────────────────────

    #[test]
    fn edit_mode_password_modal_shows_edit_title_not_add() {
        let form = PasswordForm {
            name: "GitHub".to_string(),
            username: "alice".to_string(),
            password: "hunter2".to_string(),
            field: 0,
            cursor: 0,
            editing: Some(0),
        };
        let state = dashboard_state_with_modal(Modal::AddPassword(form));
        let text = rendered_text(&state);
        assert!(text.contains("Edit Password"), "{text}");
        assert!(!text.contains("Add Password"), "{text}");
    }

    #[test]
    fn edit_mode_note_modal_shows_edit_title_not_add() {
        let form = crate::tui::state::NoteForm {
            title: "Recovery".to_string(),
            body: "1234-5678".to_string(),
            field: 0,
            cursor: 0,
            editing: Some(0),
        };
        let state = dashboard_state_with_modal(Modal::AddNote(form));
        let text = rendered_text(&state);
        assert!(text.contains("Edit Note"), "{text}");
        assert!(!text.contains("Add Note"), "{text}");
    }

    #[test]
    fn edit_mode_totp_modal_shows_edit_title_not_add() {
        let form = crate::tui::state::TotpForm {
            issuer: "Example".to_string(),
            account: "alice@example.com".to_string(),
            secret: "JBSWY3DPEHPK3PXP".to_string(),
            field: 0,
            cursor: 0,
            editing: Some(0),
        };
        let state = dashboard_state_with_modal(Modal::AddTotp(form));
        let text = rendered_text(&state);
        assert!(text.contains("Edit TOTP"), "{text}");
        assert!(!text.contains("Add TOTP"), "{text}");
    }

    #[test]
    fn edit_mode_ssh_modal_shows_edit_title_not_import() {
        let form = crate::tui::state::SshForm {
            name: "laptop".to_string(),
            public_key: "ssh-ed25519 AAAA...".to_string(),
            private_key: "-----BEGIN OPENSSH PRIVATE KEY-----".to_string(),
            field: 0,
            cursor: 0,
            editing: Some(0),
        };
        let state = dashboard_state_with_modal(Modal::AddSsh(form));
        let text = rendered_text(&state);
        assert!(text.contains("Edit SSH Key"), "{text}");
        assert!(!text.contains("Import SSH Key"), "{text}");
    }
}
