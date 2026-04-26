use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap,
    },
    Frame,
};

use super::state::{AppState, Screen, VaultView, WizardStep};

// ── Palette ─────────────────────────────────────────────────────────────────
const GREEN:   Color = Color::Rgb(0, 255, 128);
const DIM:     Color = Color::Rgb(90, 90, 90);
const MUTED:   Color = Color::Rgb(150, 150, 150);
const GOLD:    Color = Color::Rgb(255, 204, 0);
const RED:     Color = Color::Rgb(255, 60, 60);
const BLUE:    Color = Color::Rgb(80, 160, 255);
const BG:      Color = Color::Reset;

fn accent() -> Style { Style::default().fg(GREEN) }
fn dim()    -> Style { Style::default().fg(DIM) }
fn muted()  -> Style { Style::default().fg(MUTED) }
fn bold()   -> Style { Style::default().add_modifier(Modifier::BOLD) }
fn bold_green() -> Style { Style::default().fg(GREEN).add_modifier(Modifier::BOLD) }
fn bold_red()   -> Style { Style::default().fg(RED).add_modifier(Modifier::BOLD) }
fn bold_gold()  -> Style { Style::default().fg(GOLD).add_modifier(Modifier::BOLD) }

// ── ASCII art logo lines ─────────────────────────────────────────────────────
const LOGO: &[&str] = &[
    "  ╔═══════════════════════════════════╗",
    "  ║                                   ║",
    "  ║   ▄▄▄▄▄  ▀█▀  ▄▀▀▀▄  ▀█▀  █      ║",
    "  ║   █        █  █        █   █      ║",
    "  ║   ▀▀▀▀▄    █  █  ▀▀▀  █   █      ║",
    "  ║       █    █  █    █  █   █      ║",
    "  ║   ▀▀▀▀▀  ▄▄█▄▄ ▀▀▀▀  ▄█▄  █████  ║",
    "  ║                                   ║",
    "  ║      Encrypted USB Security Key   ║",
    "  ║      NorthUSB  ·  v0.1.0          ║",
    "  ║                                   ║",
    "  ╚═══════════════════════════════════╝",
];

const LOGO_COMPACT: &[&str] = &[
    "  ╔════════════════════════╗",
    "  ║  ░░  S I G I L  ░░    ║",
    "  ║  NorthUSB · v0.1.0    ║",
    "  ╚════════════════════════╝",
];

// ── Entry point ──────────────────────────────────────────────────────────────
pub fn render(frame: &mut Frame, state: &AppState) {
    // Dark background
    frame.render_widget(
        Block::default().style(Style::default().bg(Color::Black)),
        frame.size(),
    );

    match &state.screen {
        Screen::Boot              => render_boot(frame, state),
        Screen::DevicePicker      => render_device_picker(frame, state),
        Screen::SetupWizard(step) => render_wizard(frame, state, step.clone()),
        Screen::Unlock            => render_unlock(frame, state),
        Screen::Vault(view)       => render_vault(frame, state, view.clone()),
        Screen::Locked            => render_locked(frame, state),
        Screen::Error(msg)        => render_error(frame, msg),
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────
fn centered_rect(w: u16, h: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(w) / 2;
    let y = area.y + area.height.saturating_sub(h) / 2;
    Rect::new(x, y, w.min(area.width), h.min(area.height))
}

fn centered_pct(pct_w: u16, pct_h: u16, area: Rect) -> Rect {
    let w = area.width * pct_w / 100;
    let h = area.height * pct_h / 100;
    centered_rect(w, h, area)
}

fn utc_clock() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let s = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{:02}:{:02}:{:02}Z", (s / 3600) % 24, (s / 60) % 60, s % 60)
}

// ── Boot ─────────────────────────────────────────────────────────────────────
fn render_boot(frame: &mut Frame, state: &AppState) {
    let area = frame.size();
    let tick = state.boot_tick as usize;

    let logo_h = LOGO.len() as u16 + 4;
    let logo_w = 45u16;
    let logo_area = centered_rect(logo_w, logo_h, area);

    let lines: Vec<Line> = LOGO
        .iter()
        .enumerate()
        .map(|(i, line)| {
            // Staggered reveal: each line appears 2 ticks after the previous.
            if i * 2 < tick {
                Line::styled(*line, bold_green())
            } else {
                Line::raw("")
            }
        })
        .collect();

    let mut all_lines = lines;
    if tick >= LOGO.len() * 2 {
        all_lines.push(Line::raw(""));
        all_lines.push(Line::from(vec![
            Span::styled("  Scanning for USB devices", muted()),
            Span::styled(
                match (tick / 4) % 4 {
                    0 => "   ",
                    1 => ".  ",
                    2 => ".. ",
                    _ => "...",
                },
                accent(),
            ),
        ]));
    }

    frame.render_widget(
        Paragraph::new(all_lines).alignment(Alignment::Left),
        logo_area,
    );
}

// ── Device picker ────────────────────────────────────────────────────────────
fn render_device_picker(frame: &mut Frame, state: &AppState) {
    let area = frame.size();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),   // compact logo
            Constraint::Min(6),      // device list
            Constraint::Length(1),   // hint bar
        ])
        .split(area);

    // Compact logo
    let logo_lines: Vec<Line> = LOGO_COMPACT
        .iter()
        .map(|l| Line::styled(*l, bold_green()))
        .collect();
    frame.render_widget(
        Paragraph::new(logo_lines).alignment(Alignment::Center),
        chunks[0],
    );

    // Device cards
    let items: Vec<ListItem> = state
        .devices
        .iter()
        .enumerate()
        .map(|(i, dev)| {
            let is_selected = i == state.selected_device;

            let (status_icon, status_text, status_style) = if dev.has_sigil_vault {
                ("✓", "Sigil vault detected", Style::default().fg(GREEN))
            } else {
                ("◯", "No vault — press Enter to initialize", muted())
            };

            let border_style = if is_selected {
                Style::default().fg(GREEN)
            } else {
                Style::default().fg(DIM)
            };

            let prefix = if is_selected { "▶ " } else { "  " };

            let line1 = Line::from(vec![
                Span::styled(prefix, accent()),
                Span::styled(
                    format!("[{}]  ", i + 1),
                    if is_selected { bold_green() } else { dim() },
                ),
                Span::styled(
                    format!("{:<28}", &dev.name),
                    if is_selected { bold() } else { Style::default() },
                ),
                Span::styled(
                    format!("{:>10}", dev.size_display()),
                    muted(),
                ),
                Span::styled(
                    format!("   {}", dev.path.display()),
                    dim(),
                ),
            ]);
            let serial_str = dev.serial.as_deref().unwrap_or("—");
            let line2 = Line::from(vec![
                Span::raw("     "),
                Span::styled(
                    format!("Serial: {:<26}", serial_str),
                    dim(),
                ),
                Span::styled(
                    format!("{} {}", status_icon, status_text),
                    status_style,
                ),
            ]);

            ListItem::new(vec![line1, line2, Line::raw("")])
        })
        .collect();

    let devices_block = Block::default()
        .title(Span::styled(
            "  USB DEVICES ",
            Style::default().fg(GREEN).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(DIM));

    let list = List::new(items)
        .block(devices_block)
        .highlight_style(Style::default());

    let mut ls = ListState::default();
    ls.select(Some(state.selected_device));
    frame.render_stateful_widget(list, chunks[1], &mut ls);

    // Hint bar
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            hint_span("↑/↓"), sep_span(" navigate  "),
            hint_span("Enter"), sep_span(" select  "),
            hint_span("q"), sep_span(" quit"),
        ])),
        chunks[2],
    );
}

// ── Unlock ───────────────────────────────────────────────────────────────────
fn render_unlock(frame: &mut Frame, state: &AppState) {
    let area = frame.size();
    let popup = centered_rect(52, 18, area);
    frame.render_widget(Clear, popup);

    let flashing = state.unlock_flash_ticks > 0;
    let border_style = if flashing {
        Style::default().fg(RED)
    } else {
        Style::default().fg(GREEN)
    };

    let title = if state.unlock_attempts == 0 {
        "  UNLOCK VAULT  ".to_string()
    } else {
        format!("  UNLOCK VAULT  [attempt {}]  ", state.unlock_attempts + 1)
    };

    let outer = Block::default()
        .title(Span::styled(&title, border_style.add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_style(border_style);
    frame.render_widget(outer, popup);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(4),  // logo
            Constraint::Length(1),  // spacer
            Constraint::Length(3),  // passphrase field
            Constraint::Length(1),  // spacer
            Constraint::Length(1),  // hint
            Constraint::Min(0),
        ])
        .split(popup);

    // Mini logo
    let logo_lines: Vec<Line> = LOGO_COMPACT
        .iter()
        .map(|l| Line::styled(*l, Style::default().fg(GREEN)))
        .collect();
    frame.render_widget(
        Paragraph::new(logo_lines).alignment(Alignment::Center),
        inner[0],
    );

    // Passphrase field
    let masked = "●".repeat(state.passphrase_input.len());
    let field_border = if flashing {
        Style::default().fg(RED)
    } else {
        Style::default().fg(GREEN)
    };
    let pass_field = Paragraph::new(masked)
        .block(
            Block::default()
                .title(Span::styled(" PASSPHRASE ", field_border.add_modifier(Modifier::BOLD)))
                .borders(Borders::ALL)
                .border_style(field_border),
        );
    frame.render_widget(pass_field, inner[2]);

    // Wrong passphrase message
    if flashing {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("  ✗ Wrong passphrase", bold_red()),
            ]))
            .alignment(Alignment::Center),
            inner[4],
        );
    } else {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                hint_span("Enter"), sep_span(" unlock   "),
                hint_span("Esc"), sep_span(" back"),
            ]))
            .alignment(Alignment::Center),
            inner[4],
        );
    }
}

// ── Setup Wizard ─────────────────────────────────────────────────────────────
fn render_wizard(frame: &mut Frame, state: &AppState, step: WizardStep) {
    let area = frame.size();
    let popup = centered_rect(64, 22, area);
    frame.render_widget(Clear, popup);

    let (step_n, step_total, title) = match &step {
        WizardStep::Master         => (1, 5, "Master Passphrase"),
        WizardStep::Decoy          => (2, 5, "Decoy Vault"),
        WizardStep::DecoyPassphrase => (2, 5, "Decoy Passphrase"),
        WizardStep::Duress         => (3, 5, "Duress Wipe"),
        WizardStep::DuressPassphrase => (3, 5, "Duress Passphrase"),
        WizardStep::Confirm        => (4, 5, "Confirm & Create"),
    };

    let progress_bar = {
        let filled = (step_n as usize * 12) / step_total;
        let empty = 12 - filled;
        format!("{}{}",
            "█".repeat(filled),
            "░".repeat(empty))
    };

    let outer = Block::default()
        .title(Span::styled(
            format!("  VAULT SETUP  Step {}/{}: {}  ", step_n, step_total, title),
            bold_green(),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(GREEN));
    frame.render_widget(outer, popup);

    // Progress bar
    let bar_area = Rect::new(popup.x + 2, popup.y + 2, popup.width - 4, 1);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(&progress_bar, Style::default().fg(GREEN)),
            Span::styled(
                format!(" {}/{}", step_n, step_total),
                muted(),
            ),
        ])),
        bar_area,
    );

    let content_area = Rect::new(popup.x + 2, popup.y + 4, popup.width - 4, popup.height - 6);

    match step {
        WizardStep::Master => render_wizard_master(frame, state, content_area),
        WizardStep::Decoy  => render_wizard_decoy(frame, state, content_area),
        WizardStep::DecoyPassphrase => render_wizard_decoy_pass(frame, state, content_area),
        WizardStep::Duress => render_wizard_duress(frame, state, content_area),
        WizardStep::DuressPassphrase => render_wizard_duress_pass(frame, state, content_area),
        WizardStep::Confirm => render_wizard_confirm(frame, state, content_area),
    }
}

fn render_wizard_master(frame: &mut Frame, state: &AppState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),  // description
            Constraint::Length(3),  // passphrase
            Constraint::Length(3),  // confirm
            Constraint::Length(1),  // spacer
            Constraint::Length(1),  // hint
        ])
        .split(area);

    frame.render_widget(
        Paragraph::new("Choose a strong passphrase. This is the only key to your vault.\nUse a passphrase with 5+ words for best security.")
            .style(muted())
            .wrap(Wrap { trim: true }),
        chunks[0],
    );

    let f0 = state.wizard.field == 0;
    let f1 = state.wizard.field == 1;

    let mismatch = state.wizard.mismatch_flash > 0
        && !state.wizard.main_pass.is_empty()
        && !state.wizard.main_pass_confirm.is_empty()
        && state.wizard.main_pass != state.wizard.main_pass_confirm;

    frame.render_widget(
        Paragraph::new("●".repeat(state.wizard.main_pass.len()))
            .block(
                Block::default()
                    .title(if f0 { " ▶ PASSPHRASE " } else { " PASSPHRASE " })
                    .borders(Borders::ALL)
                    .border_style(if f0 { Style::default().fg(GREEN) } else { Style::default().fg(DIM) }),
            ),
        chunks[1],
    );

    let confirm_style = if mismatch { Style::default().fg(RED) } else if f1 { Style::default().fg(GREEN) } else { Style::default().fg(DIM) };
    let confirm_title = if mismatch { " ✗ MISMATCH " } else if f1 { " ▶ CONFIRM " } else { " CONFIRM " };
    frame.render_widget(
        Paragraph::new("●".repeat(state.wizard.main_pass_confirm.len()))
            .block(
                Block::default()
                    .title(confirm_title)
                    .borders(Borders::ALL)
                    .border_style(confirm_style),
            ),
        chunks[2],
    );

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            hint_span("Tab"), sep_span(" next field   "),
            hint_span("Enter"), sep_span(" continue   "),
            hint_span("Esc"), sep_span(" back"),
        ])),
        chunks[4],
    );
}

fn render_wizard_decoy(frame: &mut Frame, state: &AppState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);

    frame.render_widget(
        Paragraph::new(vec![
            Line::styled("What is a decoy vault?", bold()),
            Line::raw(""),
            Line::styled("A second passphrase that opens a convincing fake vault.", muted()),
            Line::styled("If forced to unlock, hand over this passphrase — your real", muted()),
            Line::styled("data remains hidden and inaccessible.", muted()),
        ])
        .wrap(Wrap { trim: true }),
        chunks[0],
    );

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            hint_span("y"), sep_span(" enable decoy   "),
            hint_span("n"), sep_span(" skip   "),
            hint_span("Esc"), sep_span(" back"),
        ])),
        chunks[2],
    );
}

fn render_wizard_decoy_pass(frame: &mut Frame, state: &AppState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);

    frame.render_widget(
        Paragraph::new(Line::styled("Choose a passphrase for the decoy vault:", muted())),
        chunks[0],
    );
    frame.render_widget(
        Paragraph::new("●".repeat(state.wizard.decoy_pass.len()))
            .block(
                Block::default()
                    .title(" DECOY PASSPHRASE ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(GOLD)),
            ),
        chunks[1],
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            hint_span("Enter"), sep_span(" continue   "),
            hint_span("Esc"), sep_span(" back"),
        ])),
        chunks[3],
    );
}

fn render_wizard_duress(frame: &mut Frame, state: &AppState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);

    frame.render_widget(
        Paragraph::new(vec![
            Line::styled("⚠  DURESS WIPE  ⚠", bold_red()),
            Line::raw(""),
            Line::styled("A third passphrase that silently and permanently destroys", muted()),
            Line::styled("all vault data. The wipe is cryptographically instant and", muted()),
            Line::styled("irreversible. It looks identical to a wrong passphrase.", muted()),
            Line::raw(""),
            Line::styled("Only enable this if you understand and accept the risk.", Style::default().fg(RED)),
        ]),
        chunks[0],
    );

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            hint_span("y"), sep_span(" enable duress wipe   "),
            hint_span("n"), sep_span(" skip   "),
            hint_span("Esc"), sep_span(" back"),
        ])),
        chunks[2],
    );
}

fn render_wizard_duress_pass(frame: &mut Frame, state: &AppState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);

    frame.render_widget(
        Paragraph::new(Line::styled("⚠ Choose the duress wipe passphrase:", bold_red())),
        chunks[0],
    );
    frame.render_widget(
        Paragraph::new("●".repeat(state.wizard.duress_pass.len()))
            .block(
                Block::default()
                    .title(" DURESS PASSPHRASE ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(RED)),
            ),
        chunks[1],
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            hint_span("Enter"), sep_span(" continue   "),
            hint_span("Esc"), sep_span(" back"),
        ])),
        chunks[3],
    );
}

fn render_wizard_confirm(frame: &mut Frame, state: &AppState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);

    let decoy_line = if state.wizard.decoy_enabled {
        Line::from(vec![
            Span::styled("  ✓  ", accent()),
            Span::styled("Decoy vault:   ", muted()),
            Span::styled("enabled", accent()),
        ])
    } else {
        Line::from(vec![
            Span::styled("  ◯  ", dim()),
            Span::styled("Decoy vault:   ", muted()),
            Span::styled("disabled", dim()),
        ])
    };

    let duress_line = if state.wizard.duress_enabled {
        Line::from(vec![
            Span::styled("  ⚠  ", Style::default().fg(RED)),
            Span::styled("Duress wipe:   ", muted()),
            Span::styled("ENABLED — irreversible", Style::default().fg(RED)),
        ])
    } else {
        Line::from(vec![
            Span::styled("  ◯  ", dim()),
            Span::styled("Duress wipe:   ", muted()),
            Span::styled("disabled", dim()),
        ])
    };

    frame.render_widget(
        Paragraph::new(vec![
            Line::styled("Ready to create vault:", bold()),
            Line::raw(""),
            decoy_line,
            duress_line,
            Line::raw(""),
            Line::styled("This will write the vault to the selected USB drive.", muted()),
        ]),
        chunks[0],
    );

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            hint_span("Enter"), sep_span(" create vault   "),
            hint_span("Esc"), sep_span(" back"),
        ])),
        chunks[2],
    );
}

// ── Vault ─────────────────────────────────────────────────────────────────────
fn render_vault(frame: &mut Frame, state: &AppState, view: VaultView) {
    let area = frame.size();
    let vault = match &state.vault {
        Some(v) => v,
        None    => { render_error(frame, "Internal error: vault state missing"); return; }
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // status bar
            Constraint::Length(1),  // separator
            Constraint::Min(0),     // body
            Constraint::Length(1),  // hint bar
        ])
        .split(area);

    // ── Status bar ────────────────────────────────────────────────────────────
    let slot_label = match vault.slot {
        sigil_core::vault::SlotKind::Main  => "MAIN",
        sigil_core::vault::SlotKind::Decoy => "DECOY",
        _                                   => "?",
    };
    let total = vault.blob.entry_count();

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  SIGIL ", bold_green()),
            Span::styled("│ ", dim()),
            Span::styled(format!("UNLOCKED [{}]", slot_label), accent()),
            Span::styled(format!("  {} entries", total), muted()),
            Span::styled(
                format!("  FP: {}", vault.fingerprint),
                dim(),
            ),
            Span::raw("  "),
            Span::styled(utc_clock(), dim()),
        ])),
        chunks[0],
    );

    // Separator
    frame.render_widget(
        Paragraph::new(Line::styled(
            "─".repeat(area.width as usize),
            dim(),
        )),
        chunks[1],
    );

    // ── Body: sidebar + content ───────────────────────────────────────────────
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(22), Constraint::Min(0)])
        .split(chunks[2]);

    render_sidebar(frame, state, body[0]);
    render_content(frame, state, &view, body[1]);

    // ── Hint bar ─────────────────────────────────────────────────────────────
    let hints = match &view {
        VaultView::Totp => Line::from(vec![
            hint_span("j/k"), sep_span(" navigate  "),
            hint_span("c"), sep_span(" copy code  "),
            hint_span("J/K"), sep_span(" category  "),
            hint_span("l"), sep_span(" lock  "),
            hint_span("q"), sep_span(" quit"),
        ]),
        VaultView::Passwords => Line::from(vec![
            hint_span("j/k"), sep_span(" navigate  "),
            hint_span("c"), sep_span(" copy pw  "),
            hint_span("r"), sep_span(" reveal  "),
            hint_span("a"), sep_span(" add  "),
            hint_span("d"), sep_span(" delete  "),
            hint_span("J/K"), sep_span(" category  "),
            hint_span("l"), sep_span(" lock"),
        ]),
        _ => Line::from(vec![
            hint_span("j/k"), sep_span(" navigate  "),
            hint_span("a"), sep_span(" add  "),
            hint_span("J/K"), sep_span(" category  "),
            hint_span("l"), sep_span(" lock  "),
            hint_span("q"), sep_span(" quit"),
        ]),
    };
    frame.render_widget(Paragraph::new(hints), chunks[3]);
}

fn render_sidebar(frame: &mut Frame, state: &AppState, area: Rect) {
    let vault = match &state.vault { Some(v) => v, None => return };

    let mut items = Vec::new();

    for cat in VaultView::all() {
        let count = state.entry_count_for_view(cat);
        let active = &state.vault_view == cat;
        let style = if active { bold_green() } else { muted() };

        let label = if count > 0 {
            format!("{:<12} {:>3}", cat.label(), count)
        } else {
            cat.label().to_string()
        };

        items.push(ListItem::new(Line::from(vec![
            Span::styled(if active { " ▶ " } else { "   " }, accent()),
            Span::styled(label, style),
        ])));
    }

    // System section
    items.push(ListItem::new(
        Line::styled("   ─────────────────", dim()),
    ));

    let agent_dot = Span::styled("○", dim()); // TODO: real agent status
    items.push(ListItem::new(Line::from(vec![
        Span::styled("   AGENT  ", muted()),
        agent_dot,
    ])));
    items.push(ListItem::new(
        Line::styled("   EXPORT  [e]", muted()),
    ));
    items.push(ListItem::new(
        Line::from(vec![Span::styled("   LOCK    [l]", Style::default().fg(GOLD))]),
    ));

    frame.render_widget(
        List::new(items).block(
            Block::default()
                .borders(Borders::RIGHT)
                .border_style(dim()),
        ),
        area,
    );
}

fn render_content(frame: &mut Frame, state: &AppState, view: &VaultView, area: Rect) {
    let vault = match &state.vault { Some(v) => v, None => return };

    match view {
        VaultView::Passwords => render_passwords(frame, state, area),
        VaultView::Totp      => render_totp(frame, state, area),
        VaultView::SshKeys   => render_ssh(frame, state, area),
        VaultView::Files     => render_files_view(frame, state, area),
        VaultView::Notes     => render_notes(frame, state, area),
        VaultView::Settings  => render_settings(frame, area),
    }
}

// ── Passwords ─────────────────────────────────────────────────────────────────
fn render_passwords(frame: &mut Frame, state: &AppState, area: Rect) {
    let vault = match &state.vault { Some(v) => v, None => return };
    let passwords = &vault.blob.passwords;

    let title_bar = Rect::new(area.x, area.y, area.width, 1);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  PASSWORDS", bold()),
            Span::styled(
                format!("  {} entries", passwords.len()),
                muted(),
            ),
            Span::styled("                          ", Style::default()),
            Span::styled("[a] add new", dim()),
        ])),
        title_bar,
    );

    if passwords.is_empty() {
        let empty_area = Rect::new(area.x, area.y + 2, area.width, area.height - 2);
        frame.render_widget(
            Paragraph::new(vec![
                Line::raw(""),
                Line::styled("  No passwords stored yet.", muted()),
                Line::raw(""),
                Line::from(vec![
                    Span::styled("  Press ", muted()),
                    Span::styled("a", bold_green()),
                    Span::styled(" to add your first password.", muted()),
                ]),
            ]),
            empty_area,
        );
        return;
    }

    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(Rect::new(area.x, area.y + 1, area.width, area.height.saturating_sub(1)));

    // Entry list
    let cursor = state.content_cursor.min(passwords.len().saturating_sub(1));
    let items: Vec<ListItem> = passwords.iter().map(|p| {
        ListItem::new(Line::from(vec![
            Span::styled(format!("  {:<28}", &p.name), bold()),
            Span::styled(
                format!("  {}", &p.username),
                muted(),
            ),
        ]))
    }).collect();

    let mut ls = ListState::default();
    ls.select(Some(cursor));
    frame.render_stateful_widget(
        List::new(items)
            .block(Block::default().borders(Borders::ALL).border_style(dim()))
            .highlight_style(Style::default().bg(Color::Rgb(20, 40, 20)).fg(GREEN))
            .highlight_symbol("▶ "),
        split[0],
        &mut ls,
    );

    // Detail pane
    if let Some(entry) = passwords.get(cursor) {
        let pass_display = if state.show_password {
            entry.password.expose().to_string()
        } else {
            "●".repeat(entry.password.expose().len().min(20))
        };

        let reveal_hint = if state.show_password { "[r] hide" } else { "[r] reveal" };

        let lines = vec![
            Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(&entry.name, bold_green().add_modifier(Modifier::BOLD)),
            ]),
            Line::styled("  " .to_string() + &"─".repeat(entry.name.len()), dim()),
            Line::raw(""),
            field_line("  Username ", &entry.username, "[c] copy"),
            Line::raw(""),
            Line::from(vec![
                Span::styled("  Password ", muted()),
                Span::styled(&pass_display, if state.show_password {
                    Style::default().fg(GOLD)
                } else {
                    dim()
                }),
                Span::styled(
                    format!("   {} [c] copy", reveal_hint),
                    dim(),
                ),
            ]),
            Line::raw(""),
            field_line("  URL      ", entry.url.as_deref().unwrap_or("—"), "[o] open"),
            Line::raw(""),
            Line::from(vec![
                Span::styled("  Added    ", muted()),
                Span::styled(fmt_ts(entry.created), dim()),
                Span::styled("   Modified ", muted()),
                Span::styled(fmt_ts(entry.modified), dim()),
            ]),
        ];

        frame.render_widget(
            Paragraph::new(lines)
                .block(Block::default().borders(Borders::ALL).border_style(dim())),
            split[1],
        );
    }
}

fn field_line<'a>(label: &'a str, value: &'a str, action: &'a str) -> Line<'a> {
    Line::from(vec![
        Span::styled(label, muted()),
        Span::styled(value, Style::default()),
        Span::styled(format!("   {}", action), dim()),
    ])
}

// ── TOTP ──────────────────────────────────────────────────────────────────────
fn render_totp(frame: &mut Frame, state: &AppState, area: Rect) {
    let vault = match &state.vault { Some(v) => v, None => return };
    let totps = &vault.blob.totps;

    let title_bar = Rect::new(area.x, area.y, area.width, 1);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  TOTP AUTHENTICATOR", bold()),
            Span::styled(
                format!("  {} accounts", totps.len()),
                muted(),
            ),
        ])),
        title_bar,
    );

    if totps.is_empty() {
        let empty_area = Rect::new(area.x, area.y + 2, area.width, area.height - 2);
        frame.render_widget(
            Paragraph::new(vec![
                Line::raw(""),
                Line::styled("  No TOTP accounts configured.", muted()),
                Line::raw(""),
                Line::from(vec![
                    Span::styled("  Press ", muted()),
                    Span::styled("a", bold_green()),
                    Span::styled(" to add a TOTP account (scan QR or paste secret).", muted()),
                ]),
            ]),
            empty_area,
        );
        return;
    }

    let list_area = Rect::new(area.x, area.y + 1, area.width, area.height.saturating_sub(1));
    let cursor = state.content_cursor.min(totps.len().saturating_sub(1));

    let items: Vec<ListItem> = totps.iter().enumerate().map(|(i, t)| {
        let code = sigil_core::totp::generate_now(t)
            .unwrap_or_else(|_| "------".into());
        let remaining = sigil_core::totp::seconds_remaining(t.period);
        let bar_width = 12usize;
        let filled = (remaining as usize * bar_width) / t.period as usize;
        let empty = bar_width - filled;

        let (bar_color, urgency) = if remaining <= 5 {
            (RED, true)
        } else if remaining <= 10 {
            (GOLD, false)
        } else {
            (GREEN, false)
        };

        let bar = format!("{}{}",
            "█".repeat(filled),
            "░".repeat(empty),
        );

        // Format code with space in middle for readability: "418 291"
        let code_fmt = if code.len() == 6 {
            format!("{} {}", &code[..3], &code[3..])
        } else {
            code.clone()
        };

        ListItem::new(Line::from(vec![
            Span::styled(format!("  {:<22}", &t.issuer), if i == cursor { bold() } else { Style::default() }),
            Span::styled(format!("{:<14}", &t.account), muted()),
            Span::styled(
                format!("  {}  ", code_fmt),
                Style::default()
                    .fg(if urgency { RED } else { GREEN })
                    .add_modifier(if urgency { Modifier::BOLD | Modifier::RAPID_BLINK } else { Modifier::BOLD }),
            ),
            Span::styled(bar, Style::default().fg(bar_color)),
            Span::styled(
                format!("  {:>2}s", remaining),
                if urgency { Style::default().fg(RED) } else { muted() },
            ),
        ]))
    }).collect();

    let mut ls = ListState::default();
    ls.select(Some(cursor));
    frame.render_stateful_widget(
        List::new(items)
            .block(Block::default().borders(Borders::ALL).border_style(dim()))
            .highlight_style(Style::default().bg(Color::Rgb(0, 25, 10)))
            .highlight_symbol("▶ "),
        list_area,
        &mut ls,
    );
}

// ── SSH Keys ──────────────────────────────────────────────────────────────────
fn render_ssh(frame: &mut Frame, state: &AppState, area: Rect) {
    let vault = match &state.vault { Some(v) => v, None => return };
    let keys = &vault.blob.ssh_keys;

    let title_bar = Rect::new(area.x, area.y, area.width, 1);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  SSH KEYS", bold()),
            Span::styled(format!("  {} keys", keys.len()), muted()),
        ])),
        title_bar,
    );

    if keys.is_empty() {
        let empty_area = Rect::new(area.x, area.y + 2, area.width, area.height - 2);
        frame.render_widget(
            Paragraph::new(vec![
                Line::raw(""),
                Line::styled("  No SSH keys stored.", muted()),
                Line::raw(""),
                Line::from(vec![
                    Span::styled("  Press ", muted()),
                    Span::styled("a", bold_green()),
                    Span::styled(" to generate or import an SSH key.", muted()),
                ]),
            ]),
            empty_area,
        );
        return;
    }

    let cursor = state.content_cursor.min(keys.len().saturating_sub(1));
    let items: Vec<ListItem> = keys.iter().map(|k| {
        ListItem::new(Line::from(vec![
            Span::styled(format!("  {:<24}", &k.name), bold()),
            Span::styled(format!("{:<10}", format!("{:?}", k.algorithm)), muted()),
            Span::styled(&k.fingerprint, dim()),
        ]))
    }).collect();

    let mut ls = ListState::default();
    ls.select(Some(cursor));
    frame.render_stateful_widget(
        List::new(items)
            .block(Block::default().borders(Borders::ALL).border_style(dim()))
            .highlight_style(Style::default().bg(Color::Rgb(20, 40, 20)).fg(GREEN))
            .highlight_symbol("▶ "),
        Rect::new(area.x, area.y + 1, area.width, area.height.saturating_sub(1)),
        &mut ls,
    );
}

// ── Files ─────────────────────────────────────────────────────────────────────
fn render_files_view(frame: &mut Frame, state: &AppState, area: Rect) {
    let vault = match &state.vault { Some(v) => v, None => return };
    let files = &vault.blob.files;

    let title_bar = Rect::new(area.x, area.y, area.width, 1);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  ENCRYPTED FILES", bold()),
            Span::styled(format!("  {} files", files.len()), muted()),
        ])),
        title_bar,
    );

    if files.is_empty() {
        let empty_area = Rect::new(area.x, area.y + 2, area.width, area.height - 2);
        frame.render_widget(
            Paragraph::new(vec![
                Line::raw(""),
                Line::styled("  No encrypted files stored.", muted()),
                Line::from(vec![
                    Span::styled("  Press ", muted()),
                    Span::styled("a", bold_green()),
                    Span::styled(" to encrypt a file into the vault.", muted()),
                ]),
            ]),
            empty_area,
        );
        return;
    }

    let cursor = state.content_cursor.min(files.len().saturating_sub(1));
    let items: Vec<ListItem> = files.iter().map(|f| {
        let size = if f.size >= 1_048_576 {
            format!("{:.1} MB", f.size as f64 / 1_048_576.0)
        } else {
            format!("{:.0} KB", f.size as f64 / 1024.0)
        };
        ListItem::new(Line::from(vec![
            Span::styled(format!("  {:<32}", &f.name), bold()),
            Span::styled(format!("{:>8}", size), muted()),
            Span::styled(
                format!("  {}", &f.mime.as_deref().unwrap_or("unknown")),
                dim(),
            ),
        ]))
    }).collect();

    let mut ls = ListState::default();
    ls.select(Some(cursor));
    frame.render_stateful_widget(
        List::new(items)
            .block(Block::default().borders(Borders::ALL).border_style(dim()))
            .highlight_style(Style::default().bg(Color::Rgb(20, 40, 20)).fg(GREEN))
            .highlight_symbol("▶ "),
        Rect::new(area.x, area.y + 1, area.width, area.height.saturating_sub(1)),
        &mut ls,
    );
}

// ── Notes ─────────────────────────────────────────────────────────────────────
fn render_notes(frame: &mut Frame, state: &AppState, area: Rect) {
    let vault = match &state.vault { Some(v) => v, None => return };
    let notes = &vault.blob.notes;

    let title_bar = Rect::new(area.x, area.y, area.width, 1);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  SECURE NOTES", bold()),
            Span::styled(format!("  {} notes", notes.len()), muted()),
        ])),
        title_bar,
    );

    if notes.is_empty() {
        let empty_area = Rect::new(area.x, area.y + 2, area.width, area.height - 2);
        frame.render_widget(
            Paragraph::new(vec![
                Line::raw(""),
                Line::styled("  No secure notes stored.", muted()),
                Line::from(vec![
                    Span::styled("  Press ", muted()),
                    Span::styled("a", bold_green()),
                    Span::styled(" to create a note.", muted()),
                ]),
            ]),
            empty_area,
        );
        return;
    }

    let body_area = Rect::new(area.x, area.y + 1, area.width, area.height.saturating_sub(1));
    let cursor = state.content_cursor.min(notes.len().saturating_sub(1));

    let split = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(body_area);

    let items: Vec<ListItem> = notes.iter().map(|n|
        ListItem::new(Line::styled(format!("  {}", &n.title), Style::default()))
    ).collect();

    let mut ls = ListState::default();
    ls.select(Some(cursor));
    frame.render_stateful_widget(
        List::new(items)
            .block(Block::default().borders(Borders::ALL).border_style(dim()))
            .highlight_style(Style::default().bg(Color::Rgb(20, 40, 20)).fg(GREEN))
            .highlight_symbol("▶ "),
        split[0],
        &mut ls,
    );

    if let Some(note) = notes.get(cursor) {
        frame.render_widget(
            Paragraph::new(note.body.expose().to_string())
                .block(
                    Block::default()
                        .title(format!(" {} ", &note.title))
                        .borders(Borders::ALL)
                        .border_style(dim()),
                )
                .wrap(Wrap { trim: false }),
            split[1],
        );
    }
}

// ── Settings ──────────────────────────────────────────────────────────────────
fn render_settings(frame: &mut Frame, area: Rect) {
    frame.render_widget(
        Paragraph::new(vec![
            Line::styled("  SETTINGS", bold()),
            Line::raw(""),
            Line::from(vec![
                Span::styled("  Auto-lock timeout  ", muted()),
                Span::styled("15 minutes", Style::default()),
            ]),
            Line::from(vec![
                Span::styled("  Clipboard clear    ", muted()),
                Span::styled("30 seconds", Style::default()),
            ]),
            Line::from(vec![
                Span::styled("  Agent socket       ", muted()),
                Span::styled("$XDG_RUNTIME_DIR/sigil-agent.sock", dim()),
            ]),
            Line::raw(""),
            Line::styled("  Full settings editing coming in v0.2.", dim()),
        ]),
        area,
    );
}

// ── Locked screen ─────────────────────────────────────────────────────────────
fn render_locked(frame: &mut Frame, _state: &AppState) {
    let area = frame.size();
    let popup = centered_rect(46, 12, area);
    frame.render_widget(Clear, popup);

    let lines = vec![
        Line::raw(""),
        Line::styled("  ████████  VAULT LOCKED  ████████", bold_gold()),
        Line::raw(""),
        Line::styled("  All key material cleared from memory.", muted()),
        Line::styled("  Agent stopped. Clipboard cleared.", muted()),
        Line::raw(""),
        Line::from(vec![
            Span::styled("  Press ", muted()),
            Span::styled("Enter", bold_green()),
            Span::styled(" to unlock,  ", muted()),
            Span::styled("q", bold_green()),
            Span::styled(" to quit.", muted()),
        ]),
    ];

    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(GOLD)),
            ),
        popup,
    );
}

// ── Error screen ──────────────────────────────────────────────────────────────
fn render_error(frame: &mut Frame, msg: &str) {
    let area = frame.size();
    let popup = centered_rect(60, 12, area);
    frame.render_widget(Clear, popup);

    frame.render_widget(
        Paragraph::new(format!("\n  {}\n\n  Press q to quit.", msg))
            .block(
                Block::default()
                    .title(Span::styled("  ERROR  ", bold_red()))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(RED)),
            )
            .wrap(Wrap { trim: true })
            .style(Style::default().fg(RED)),
        popup,
    );
}

// ── Shared hint widget helpers ────────────────────────────────────────────────
fn hint_span(key: &str) -> Span<'_> {
    Span::styled(
        format!(" {} ", key),
        Style::default()
            .bg(Color::Rgb(30, 30, 30))
            .fg(GREEN)
            .add_modifier(Modifier::BOLD),
    )
}

fn sep_span(text: &str) -> Span<'_> {
    Span::styled(text, muted())
}

// ── Timestamp formatting ──────────────────────────────────────────────────────
fn fmt_ts(unix: u64) -> String {
    if unix == 0 { return "—".into(); }
    let s = unix;
    let y = 1970 + s / 31_557_600;
    let rem = s % 31_557_600;
    let m = rem / 2_629_800 + 1;
    let d = (rem % 2_629_800) / 86400 + 1;
    format!("{}-{:02}-{:02}", y, m, d)
}
