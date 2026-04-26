use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

/// Render a passphrase input field with masked characters.
pub fn passphrase_field<'a>(label: &'a str, value: &'a str, focused: bool) -> Paragraph<'a> {
    let masked: String = "•".repeat(value.len());
    let border_style = if focused {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    Paragraph::new(masked).block(
        Block::default()
            .title(label)
            .borders(Borders::ALL)
            .border_style(border_style),
    )
}

/// Render a plain text input field.
pub fn text_field<'a>(label: &'a str, value: &'a str, focused: bool) -> Paragraph<'a> {
    let border_style = if focused {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    Paragraph::new(value.to_string()).block(
        Block::default()
            .title(label)
            .borders(Borders::ALL)
            .border_style(border_style),
    )
}

/// Render a thick banner box around a title.
pub fn banner_block<'a>(title: &'a str) -> Block<'a> {
    Block::default()
        .title(Span::styled(
            title,
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green))
}

/// Status indicator: ● (green) or ○ (dim) for a boolean state.
pub fn status_dot(active: bool) -> Span<'static> {
    if active {
        Span::styled("●", Style::default().fg(Color::Green))
    } else {
        Span::styled("○", Style::default().fg(Color::DarkGray))
    }
}

/// Render a TOTP countdown bar.
/// `remaining` is seconds left in the current period; `period` is the full period.
pub fn totp_bar(remaining: u32, period: u32, width: u16) -> String {
    let filled = (remaining as f32 / period as f32 * width as f32) as usize;
    let empty = width as usize - filled;
    let bar_color = if remaining <= 5 { "!" } else { "█" };
    format!("{}{}", bar_color.repeat(filled), "░".repeat(empty))
}

/// A section separator line.
pub fn separator(width: u16) -> String {
    "─".repeat(width as usize)
}
