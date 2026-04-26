use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{
        Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap,
    },
    Frame,
};

use super::state::{AppState, Screen, VaultView, WizardStep};

const LOGO: &[&str] = &[
    "  ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓  ",
    "  ▓                          ▓  ",
    "  ▓        S I G I L         ▓  ",
    "  ▓                          ▓  ",
    "  ▓     NORTHUSB VAULT       ▓  ",
    "  ▓     v0.1.0               ▓  ",
    "  ▓                          ▓  ",
    "  ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓  ",
];

pub fn render(frame: &mut Frame, state: &AppState) {
    match &state.screen {
        Screen::Boot => render_boot(frame, state),
        Screen::DevicePicker => render_device_picker(frame, state),
        Screen::SetupWizard(step) => render_wizard(frame, state, step.clone()),
        Screen::Unlock => render_unlock(frame, state),
        Screen::Vault(view) => render_vault(frame, state, view.clone()),
        Screen::Locked => render_locked(frame, state),
        Screen::Error(msg) => render_error(frame, msg),
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn render_boot(frame: &mut Frame, state: &AppState) {
    let area = frame.size();
    let block = Block::default().style(Style::default().bg(Color::Reset));
    frame.render_widget(block, area);

    let logo_area = centered_rect(40, 60, area);
    let tick = state.boot_tick as usize;

    let lines: Vec<Line> = LOGO
        .iter()
        .enumerate()
        .map(|(i, line)| {
            if i * 2 < tick {
                Line::styled(*line, Style::default().fg(Color::Green))
            } else {
                Line::raw("")
            }
        })
        .collect();

    let para = Paragraph::new(lines).alignment(Alignment::Center);
    frame.render_widget(para, logo_area);
}

fn render_device_picker(frame: &mut Frame, state: &AppState) {
    let area = frame.size();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(10),  // logo
            Constraint::Min(8),      // device list
            Constraint::Length(1),   // bottom bar
        ])
        .split(area);

    // Logo.
    let logo_lines: Vec<Line> = LOGO
        .iter()
        .map(|l| Line::styled(*l, Style::default().fg(Color::Green)))
        .collect();
    frame.render_widget(
        Paragraph::new(logo_lines).alignment(Alignment::Center),
        chunks[0],
    );

    // Device list.
    let items: Vec<ListItem> = state
        .devices
        .iter()
        .enumerate()
        .map(|(i, dev)| {
            let status = if dev.has_sigil_vault {
                Span::styled("✓ Sigil vault present", Style::default().fg(Color::Green))
            } else {
                Span::styled("  No Sigil vault", Style::default().fg(Color::DarkGray))
            };

            let line1 = Line::from(vec![
                Span::styled(
                    format!("[{}]  {:<30}", i + 1, &dev.name),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!("{:>8}", dev.size_display())),
                Span::raw(format!("    {}", dev.path.display())),
            ]);
            let line2 = Line::from(vec![
                Span::raw("       "),
                Span::styled(
                    dev.serial.as_deref().unwrap_or("(no serial)"),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::raw("  "),
                status,
            ]);

            ListItem::new(vec![line1, line2, Line::raw("")])
        })
        .collect();

    let mut list_state = ListState::default();
    list_state.select(Some(state.selected_device));

    let list = List::new(items)
        .block(
            Block::default()
                .title(" DETECTED USB DEVICES ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Green)),
        )
        .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
        .highlight_symbol("▶ ");

    frame.render_stateful_widget(list, chunks[1], &mut list_state);

    // Bottom bar.
    let hint = Paragraph::new("  ↑/↓ or j/k select   Enter confirm   q quit")
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(hint, chunks[2]);
}

fn render_unlock(frame: &mut Frame, state: &AppState) {
    let area = frame.size();
    let popup = centered_rect(50, 40, area);

    frame.render_widget(Clear, popup);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(popup);

    // Logo (compact).
    let logo = Paragraph::new(
        LOGO[0..5]
            .iter()
            .map(|l| Line::styled(*l, Style::default().fg(Color::Green)))
            .collect::<Vec<_>>(),
    )
    .alignment(Alignment::Center);
    frame.render_widget(logo, chunks[0]);

    // Passphrase field.
    let masked: String = "•".repeat(state.passphrase_input.len());
    let pass_field = Paragraph::new(masked)
        .block(
            Block::default()
                .title(" PASSPHRASE ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Green)),
        )
        .style(Style::default());
    frame.render_widget(pass_field, chunks[1]);

    // Hint.
    let hint = Paragraph::new("  Enter to unlock   Esc to go back")
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(hint, chunks[2]);
}

fn render_wizard(frame: &mut Frame, state: &AppState, step: WizardStep) {
    let area = frame.size();
    let popup = centered_rect(60, 70, area);
    frame.render_widget(Clear, popup);

    let outer = Block::default()
        .title(" VAULT SETUP WIZARD ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green));
    frame.render_widget(outer, popup);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(popup);

    match step {
        WizardStep::Master => {
            let title = Paragraph::new(Line::styled(
                "Step 1 of 3 — Master Passphrase",
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
            ));
            frame.render_widget(title, inner[0]);

            let f0 = state.wizard.field == 0;
            let masked_a: String = "•".repeat(state.wizard.main_pass.len());
            let pass_a = Paragraph::new(masked_a).block(
                Block::default()
                    .title(if f0 { " ▶ PASSPHRASE " } else { " PASSPHRASE " })
                    .borders(Borders::ALL)
                    .border_style(if f0 {
                        Style::default().fg(Color::Green)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    }),
            );
            frame.render_widget(pass_a, inner[1]);

            let f1 = state.wizard.field == 1;
            let masked_b: String = "•".repeat(state.wizard.main_pass_confirm.len());
            let pass_b = Paragraph::new(masked_b).block(
                Block::default()
                    .title(if f1 { " ▶ CONFIRM " } else { " CONFIRM " })
                    .borders(Borders::ALL)
                    .border_style(if f1 {
                        Style::default().fg(Color::Green)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    }),
            );
            frame.render_widget(pass_b, inner[2]);

            let hint = Paragraph::new("  Tab switch field   Enter confirm   Esc back")
                .style(Style::default().fg(Color::DarkGray));
            frame.render_widget(hint, inner[3]);
        }
        WizardStep::Decoy => {
            let title = Paragraph::new(Line::styled(
                "Step 2 of 3 — Decoy Vault",
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
            ));
            frame.render_widget(title, inner[0]);

            let body = Paragraph::new(
                "A decoy vault opens with a different passphrase, \
                 showing realistic-looking fake credentials. \
                 An adversary who forces you to unlock sees only the decoy.\n\n\
                 Enable decoy vault? [Y/n]",
            )
            .wrap(Wrap { trim: true })
            .style(Style::default());
            frame.render_widget(body, inner[1]);
        }
        WizardStep::Duress => {
            let title = Paragraph::new(Line::styled(
                "Step 3 of 3 — Duress Wipe",
                Style::default()
                    .fg(Color::Red)
                    .add_modifier(Modifier::BOLD),
            ));
            frame.render_widget(title, inner[0]);

            let body = Paragraph::new(
                "⚠  IRREVERSIBLE: A duress passphrase triggers \
                 instant cryptographic wipe of the vault. All data is lost.\n\n\
                 Enable duress wipe? [y/N]",
            )
            .wrap(Wrap { trim: true })
            .style(Style::default().fg(Color::Yellow));
            frame.render_widget(body, inner[1]);
        }
        WizardStep::Confirm => {
            let title = Paragraph::new(Line::styled(
                "Ready to create vault",
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
            ));
            frame.render_widget(title, inner[0]);

            let decoy_line = if state.wizard.decoy_enabled {
                "  ✓ Decoy vault: enabled"
            } else {
                "  ✗ Decoy vault: disabled"
            };
            let duress_line = if state.wizard.duress_enabled {
                "  ✓ Duress wipe: ENABLED (irreversible)"
            } else {
                "  ✗ Duress wipe: disabled"
            };

            let body = Paragraph::new(format!(
                "Vault will be written to selected device.\n\n{}\n{}\n\n\
                 Press Enter to create, Esc to go back.",
                decoy_line, duress_line
            ))
            .wrap(Wrap { trim: true });
            frame.render_widget(body, inner[1]);
        }
    }
}

fn render_vault(frame: &mut Frame, state: &AppState, view: VaultView) {
    let area = frame.size();

    let vault = match &state.vault {
        Some(v) => v,
        None => {
            render_error(frame, "Vault unexpectedly None in Vault screen");
            return;
        }
    };

    // Top status bar.
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    let slot_label = match vault.slot {
        sigil_core::vault::SlotKind::Main => "MAIN",
        sigil_core::vault::SlotKind::Decoy => "DECOY",
        _ => "?",
    };
    let entry_count = vault.blob.entry_count();
    let now = chrono_utc_hms();

    let status_bar = Paragraph::new(Line::from(vec![
        Span::styled(
            format!(" STATUS: UNLOCKED [{}]", slot_label),
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(
            "   ENTRIES: {}   FP: {}   {}",
            entry_count, vault.fingerprint, now
        )),
    ]))
    .style(Style::default().bg(Color::Reset));
    frame.render_widget(status_bar, chunks[0]);

    // Body: sidebar + content.
    let body_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(20), Constraint::Min(0)])
        .split(chunks[1]);

    render_sidebar(frame, state, body_chunks[0]);
    render_content(frame, state, &view, body_chunks[1]);

    // Bottom hint bar.
    let hint = Paragraph::new(
        "  q quit   l lock   Tab focus   J/K category   j/k select   ? help",
    )
    .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(hint, chunks[2]);
}

fn render_sidebar(frame: &mut Frame, state: &AppState, area: Rect) {
    let vault = match &state.vault {
        Some(v) => v,
        None => return,
    };

    let categories = [
        (VaultView::Passwords, vault.blob.passwords.len()),
        (VaultView::Totp, vault.blob.totps.len()),
        (VaultView::SshKeys, vault.blob.ssh_keys.len()),
        (VaultView::Files, vault.blob.files.len()),
        (VaultView::Notes, vault.blob.notes.len()),
        (VaultView::Settings, 0usize),
    ];

    let items: Vec<ListItem> = categories
        .iter()
        .map(|(cat, count)| {
            let active = &state.vault_view == cat;
            let label = if *count > 0 {
                format!("{:<10} {:>3}", cat.label(), count)
            } else {
                format!("{}", cat.label())
            };
            let style = if active {
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(vec![
                Span::styled(if active { "▶ " } else { "  " }, style),
                Span::styled(label, style),
            ]))
        })
        .collect();

    let system_items = vec![
        ListItem::new(Line::from(vec![
            Span::raw("  ─ SYSTEM ─"),
        ])),
        ListItem::new(Line::from(vec![
            Span::raw("  AGENT  "),
            Span::styled("○", Style::default().fg(Color::DarkGray)),
        ])),
        ListItem::new(Line::from(vec![Span::raw("  EXPORT")])),
        ListItem::new(Line::from(vec![
            Span::styled("  LOCK", Style::default().fg(Color::Yellow)),
        ])),
    ];

    let all_items: Vec<ListItem> = items
        .into_iter()
        .chain(std::iter::once(ListItem::new("")))
        .chain(system_items)
        .collect();

    let list = List::new(all_items).block(
        Block::default()
            .borders(Borders::RIGHT)
            .border_style(Style::default().fg(Color::DarkGray)),
    );

    frame.render_widget(list, area);
}

fn render_content(frame: &mut Frame, state: &AppState, view: &VaultView, area: Rect) {
    let vault = match &state.vault {
        Some(v) => v,
        None => return,
    };

    let title = format!(" {} ", view.label());
    let block = Block::default()
        .title(title.as_str())
        .borders(Borders::NONE)
        .border_style(Style::default().fg(Color::Green));

    match view {
        VaultView::Passwords => render_passwords(frame, state, &vault.blob.passwords, area),
        VaultView::Totp => render_totps(frame, state, &vault.blob.totps, area),
        VaultView::SshKeys => render_ssh_keys(frame, state, &vault.blob.ssh_keys, area),
        VaultView::Files => render_files(frame, state, &vault.blob.files, area),
        VaultView::Notes => render_notes(frame, state, &vault.blob.notes, area),
        VaultView::Settings => render_settings(frame, area),
    }
}

fn render_passwords(
    frame: &mut Frame,
    state: &AppState,
    passwords: &[sigil_core::types::PasswordEntry],
    area: Rect,
) {
    if passwords.is_empty() {
        let empty = Paragraph::new("\n  No passwords stored.\n  Press 'a' to add one.")
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(empty, area);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);

    // Entry list.
    let items: Vec<ListItem> = passwords
        .iter()
        .map(|p| {
            ListItem::new(Line::from(vec![
                Span::styled(format!("{:<30}", &p.name), Style::default()),
                Span::styled(
                    format!("  {}", &p.username),
                    Style::default().fg(Color::DarkGray),
                ),
            ]))
        })
        .collect();

    let mut list_state = ListState::default();
    let cursor = state.content_cursor.min(passwords.len().saturating_sub(1));
    list_state.select(Some(cursor));

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
        .highlight_symbol("▶ ");

    frame.render_stateful_widget(list, chunks[0], &mut list_state);

    // Detail pane.
    if let Some(entry) = passwords.get(cursor) {
        render_password_detail(frame, entry, chunks[1]);
    }
}

fn render_password_detail(
    frame: &mut Frame,
    entry: &sigil_core::types::PasswordEntry,
    area: Rect,
) {
    let lines = vec![
        Line::from(vec![
            Span::styled(&entry.name, Style::default().add_modifier(Modifier::BOLD)),
        ]),
        Line::from("─".repeat(entry.name.len())),
        Line::raw(""),
        Line::from(vec![
            Span::styled("Username:  ", Style::default().fg(Color::DarkGray)),
            Span::raw(&entry.username),
            Span::raw("                  "),
            Span::styled("[c]", Style::default().fg(Color::Green)),
        ]),
        Line::from(vec![
            Span::styled("Password:  ", Style::default().fg(Color::DarkGray)),
            Span::raw("••••••••••••••••"),
            Span::raw("  "),
            Span::styled("[r]", Style::default().fg(Color::Green)),
            Span::raw(" "),
            Span::styled("[c]", Style::default().fg(Color::Green)),
        ]),
        Line::from(vec![
            Span::styled("URL:       ", Style::default().fg(Color::DarkGray)),
            Span::raw(entry.url.as_deref().unwrap_or("—")),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled(
                "  [c]opy  [r]eveal  [e]dit  [d]elete",
                Style::default().fg(Color::DarkGray),
            ),
        ]),
    ];

    let para = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    frame.render_widget(para, area);
}

fn render_totps(
    frame: &mut Frame,
    state: &AppState,
    totps: &[sigil_core::types::TotpEntry],
    area: Rect,
) {
    if totps.is_empty() {
        let empty = Paragraph::new("\n  No TOTP entries stored.\n  Press 'a' to add one.")
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(empty, area);
        return;
    }

    let items: Vec<ListItem> = totps
        .iter()
        .map(|t| {
            let code = sigil_core::totp::generate_now(t)
                .unwrap_or_else(|_| "------".into());
            let remaining = sigil_core::totp::seconds_remaining(t.period);
            let bar = super::widgets::totp_bar(remaining, t.period, 10);

            ListItem::new(Line::from(vec![
                Span::styled(format!("{:<25}", &t.issuer), Style::default()),
                Span::styled(
                    format!("{:<10}", &t.account),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    format!("  {}  ", code),
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("[{}] {}s", bar, remaining),
                    if remaining <= 5 {
                        Style::default().fg(Color::Red)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    },
                ),
            ]))
        })
        .collect();

    let mut list_state = ListState::default();
    list_state.select(Some(
        state.content_cursor.min(totps.len().saturating_sub(1)),
    ));

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .highlight_style(Style::default().bg(Color::DarkGray))
        .highlight_symbol("▶ ");

    frame.render_stateful_widget(list, area, &mut list_state);
}

fn render_ssh_keys(
    frame: &mut Frame,
    state: &AppState,
    keys: &[sigil_core::types::SshKeyEntry],
    area: Rect,
) {
    if keys.is_empty() {
        let empty = Paragraph::new("\n  No SSH keys stored.\n  Press 'a' to add or import one.")
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(empty, area);
        return;
    }

    let items: Vec<ListItem> = keys
        .iter()
        .map(|k| {
            ListItem::new(Line::from(vec![
                Span::styled(format!("{:<25}", &k.name), Style::default()),
                Span::styled(
                    format!("{:?}", k.algorithm),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    format!("  {}", &k.fingerprint),
                    Style::default().fg(Color::DarkGray),
                ),
            ]))
        })
        .collect();

    let mut list_state = ListState::default();
    list_state.select(Some(
        state.content_cursor.min(keys.len().saturating_sub(1)),
    ));

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .highlight_style(Style::default().bg(Color::DarkGray))
        .highlight_symbol("▶ ");

    frame.render_stateful_widget(list, area, &mut list_state);
}

fn render_files(
    frame: &mut Frame,
    state: &AppState,
    files: &[sigil_core::types::FileEntry],
    area: Rect,
) {
    if files.is_empty() {
        let empty = Paragraph::new("\n  No files stored.\n  Press 'a' to encrypt a file.")
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(empty, area);
        return;
    }

    let items: Vec<ListItem> = files
        .iter()
        .map(|f| {
            let size_display = if f.size >= 1_048_576 {
                format!("{:.1} MB", f.size as f64 / 1_048_576.0)
            } else {
                format!("{:.0} KB", f.size as f64 / 1024.0)
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{:<30}", &f.name), Style::default()),
                Span::styled(
                    format!("{:>8}", size_display),
                    Style::default().fg(Color::DarkGray),
                ),
            ]))
        })
        .collect();

    let mut list_state = ListState::default();
    list_state.select(Some(
        state.content_cursor.min(files.len().saturating_sub(1)),
    ));

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .highlight_style(Style::default().bg(Color::DarkGray))
        .highlight_symbol("▶ ");

    frame.render_stateful_widget(list, area, &mut list_state);
}

fn render_notes(
    frame: &mut Frame,
    state: &AppState,
    notes: &[sigil_core::types::NoteEntry],
    area: Rect,
) {
    if notes.is_empty() {
        let empty = Paragraph::new("\n  No notes stored.\n  Press 'a' to add one.")
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(empty, area);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(area);

    let cursor = state.content_cursor.min(notes.len().saturating_sub(1));

    let items: Vec<ListItem> = notes
        .iter()
        .map(|n| ListItem::new(Line::raw(&n.title)))
        .collect();

    let mut list_state = ListState::default();
    list_state.select(Some(cursor));

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .highlight_style(Style::default().bg(Color::DarkGray))
        .highlight_symbol("▶ ");

    frame.render_stateful_widget(list, chunks[0], &mut list_state);

    if let Some(note) = notes.get(cursor) {
        let body = Paragraph::new(note.body.expose().to_string())
            .block(
                Block::default()
                    .title(format!(" {} ", &note.title))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)),
            )
            .wrap(Wrap { trim: false });
        frame.render_widget(body, chunks[1]);
    }
}

fn render_settings(frame: &mut Frame, area: Rect) {
    let para = Paragraph::new("\n  Settings coming soon.")
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(para, area);
}

fn render_locked(frame: &mut Frame, state: &AppState) {
    let area = frame.size();
    let popup = centered_rect(40, 30, area);

    let lines = vec![
        Line::raw(""),
        Line::styled(
            "  ████  LOCKED  ████",
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ),
        Line::raw(""),
        Line::raw("  All keys cleared from memory."),
        Line::raw(""),
        Line::styled(
            "  Press Enter to unlock, q to quit.",
            Style::default().fg(Color::DarkGray),
        ),
    ];

    let para = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow)),
    );

    frame.render_widget(Clear, popup);
    frame.render_widget(para, popup);
}

fn render_error(frame: &mut Frame, msg: &str) {
    let area = frame.size();
    let popup = centered_rect(60, 40, area);

    let para = Paragraph::new(format!("\n  ERROR:\n\n  {}\n\n  Press q to quit.", msg))
        .block(
            Block::default()
                .title(" ERROR ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Red)),
        )
        .wrap(Wrap { trim: true })
        .style(Style::default().fg(Color::Red));

    frame.render_widget(Clear, popup);
    frame.render_widget(para, popup);
}

fn chrono_utc_hms() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let h = (secs / 3600) % 24;
    let m = (secs / 60) % 60;
    let s = secs % 60;
    format!("{:02}:{:02}:{:02}Z", h, m, s)
}
