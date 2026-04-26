use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

use super::state::{AppState, Screen, WizardStep};

// ── Palette ──────────────────────────────────────────────────────────────────
const BLUE:  Color = Color::Rgb(79, 127, 255);
const WHITE: Color = Color::Rgb(220, 225, 235);
const MUTED: Color = Color::Rgb(130, 135, 150);
const DIM:   Color = Color::Rgb(65, 68, 80);
const RED:   Color = Color::Rgb(235, 80, 60);
const GOLD:  Color = Color::Rgb(240, 180, 50);
const GREEN: Color = Color::Rgb(60, 200, 100);

fn accent()     -> Style { Style::default().fg(BLUE) }
fn muted()      -> Style { Style::default().fg(MUTED) }
fn dim()        -> Style { Style::default().fg(DIM) }
fn bold_white() -> Style { Style::default().fg(WHITE).add_modifier(Modifier::BOLD) }
fn bold_blue()  -> Style { Style::default().fg(BLUE).add_modifier(Modifier::BOLD) }
fn bold_red()   -> Style { Style::default().fg(RED).add_modifier(Modifier::BOLD) }

// ── Entry point ───────────────────────────────────────────────────────────────
pub fn render(frame: &mut Frame, state: &AppState) {
    frame.render_widget(
        Block::default().style(Style::default().bg(Color::Rgb(13, 17, 23))),
        frame.size(),
    );

    match &state.screen {
        Screen::Boot              => render_boot(frame, state),
        Screen::DevicePicker      => render_device_picker(frame, state),
        Screen::Formatting        => render_formatting(frame, state),
        Screen::SetupWizard(step) => render_wizard(frame, state, step.clone()),
        Screen::Done              => render_done(frame),
        Screen::Error(msg)        => render_error(frame, msg),
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
    let area = frame.size();
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
                Span::styled("  SIGIL", bold_blue()),
                Span::styled("  ·  NorthUSB", muted()),
            ]),
            Line::raw(""),
            Line::styled("  Encrypted USB Security Vault", muted()),
            Line::raw(""),
            Line::from(vec![
                Span::styled("  Scanning for USB drives", muted()),
                Span::styled(dots, accent()),
            ]),
        ])
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(dim()),
        ),
        popup,
    );
}

// ── Device Picker ─────────────────────────────────────────────────────────────
fn render_device_picker(frame: &mut Frame, state: &AppState) {
    let area = frame.size();

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
                Span::styled("  SIGIL", bold_blue()),
                Span::styled("  —  USB Setup", muted()),
            ]),
            Line::styled("  Select a USB drive to set up as your security vault.", muted()),
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

            let vault_tag = if dev.has_sigil_vault {
                Span::styled("  [vault exists — will overwrite]", muted())
            } else {
                Span::styled("  [new drive]", dim())
            };

            ListItem::new(vec![
                Line::from(vec![
                    prefix.clone(),
                    Span::styled(format!("{:<28}", &dev.name), name_style),
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

    let mut ls = ListState::default();
    ls.select(Some(state.selected_device));

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

    // Hint bar
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            hint_span("↑/↓"), sep_span(" select  "),
            hint_span("Enter"), sep_span(" set up this drive  "),
            hint_span("q"), sep_span(" quit"),
        ])),
        chunks[2],
    );
}

// ── Formatting ────────────────────────────────────────────────────────────────
fn render_formatting(frame: &mut Frame, state: &AppState) {
    let area = frame.size();
    let popup = centered_rect(44, 8, area);
    frame.render_widget(Clear, popup);

    let spinner = ["⠋","⠙","⠹","⠸","⠼","⠴","⠦","⠧","⠇","⠏"];
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
                Span::styled("Formatting drive, please wait...", Style::default().fg(WHITE)),
            ]),
            Line::raw(""),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(&dev_name, muted()),
            ]),
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
    let area = frame.size();
    let popup = centered_rect(62, 20, area);
    frame.render_widget(Clear, popup);

    let (step_n, step_total, title) = match &step {
        WizardStep::ConfirmWipe => (1, 3, "Erase Drive"),
        WizardStep::Master      => (2, 3, "Set Passphrase"),
        WizardStep::Confirm     => (3, 3, "Confirm & Create"),
    };

    let filled = (step_n * 16) / step_total;
    let progress = format!(
        "{}{}",
        "▓".repeat(filled),
        "░".repeat(16 - filled)
    );

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
        WizardStep::ConfirmWipe => render_step_wipe(frame, state, content),
        WizardStep::Master      => render_step_master(frame, state, content),
        WizardStep::Confirm     => render_step_confirm(frame, state, content),
    }
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
            Line::from(vec![
                Span::raw("  "),
                Span::styled(&dev_name, bold_white()),
            ]),
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
            hint_span("y"), sep_span(" erase and continue  "),
            hint_span("n"), sep_span(" go back"),
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
    let pass_title  = if f0 { " ▶ Passphrase " } else { " Passphrase " };
    frame.render_widget(
        Paragraph::new(Span::styled(
            "●".repeat(state.wizard.main_pass.len()),
            if f0 { Style::default().fg(WHITE) } else { dim() },
        ))
        .block(
            Block::default()
                .title(pass_title)
                .borders(Borders::ALL)
                .border_style(pass_border),
        ),
        chunks[1],
    );

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
            "●".repeat(state.wizard.main_pass_confirm.len()),
            if f1 { Style::default().fg(WHITE) } else { dim() },
        ))
        .block(
            Block::default()
                .title(conf_title)
                .borders(Borders::ALL)
                .border_style(conf_border),
        ),
        chunks[2],
    );

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            hint_span("Tab"), sep_span(" switch field  "),
            hint_span("Enter"), sep_span(" continue  "),
            hint_span("Esc"), sep_span(" back"),
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
            hint_span("Enter"), sep_span(" create vault  "),
            hint_span("Esc"), sep_span(" back"),
        ])),
        chunks[1],
    );
}

// ── Done ──────────────────────────────────────────────────────────────────────
fn render_done(frame: &mut Frame) {
    let area = frame.size();
    let popup = centered_rect(56, 14, area);
    frame.render_widget(Clear, popup);

    frame.render_widget(
        Paragraph::new(vec![
            Line::raw(""),
            Line::from(vec![Span::styled("  Vault created successfully.", bold_blue())]),
            Line::raw(""),
            Line::styled(
                "  Your USB drive is ready to use.",
                Style::default().fg(WHITE),
            ),
            Line::raw(""),
            Line::styled("  Next steps:", muted()),
            Line::styled("    1.  Eject the USB drive safely.", muted()),
            Line::styled("    2.  Plug it in to any computer.", muted()),
            Line::styled("    3.  Open  index.html  in your browser.", muted()),
            Line::raw(""),
            Line::from(vec![
                Span::styled("  Press any key to exit.", dim()),
            ]),
        ])
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(BLUE)),
        ),
        popup,
    );
}

// ── Error ─────────────────────────────────────────────────────────────────────
fn render_error(frame: &mut Frame, msg: &str) {
    let area = frame.size();
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
