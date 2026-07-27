//! The changelog modal (click the sidebar version number): a centered,
//! scrollable view of every shipped release's notes, drawn last over a dimmed
//! backdrop. Release text is embedded at build time (see `crate::changelog` /
//! `build.rs`); this module turns it into styled, wrapped lines. `↑↓`/wheel
//! scroll, `esc`/`q`/click-outside close (see `app/input.rs`).

use super::*;
use crate::changelog::{classify, Block as CBlock, CHANGELOG};
use ratatui::widgets::{Borders, Clear};

pub(super) fn draw_changelog(f: &mut RenderTarget, area: Rect, app: &mut App, t: &Theme) {
    dim_backdrop(f, area, t);

    let w = area.width.saturating_sub(6).clamp(50, 92).min(area.width);
    let h = area.height.saturating_sub(2).clamp(12, 44).min(area.height);
    let modal = centered_rect(area, w, h);
    f.render_widget(Clear, modal);
    let block = Block::new()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(t.border_focus).bg(t.surface0))
        .style(Style::new().bg(t.surface0));
    let inner = block.inner(modal);
    f.render_widget(block, modal);

    // ── title + close ──
    let cat = app.catalog;
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!(" {}", cat.changelog),
                Style::new().fg(t.text).bold(),
            ),
            Span::styled(
                concat!("   v", env!("CARGO_PKG_VERSION")),
                Style::new().fg(t.overlay0),
            ),
        ])),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );
    let close = Rect::new(inner.right().saturating_sub(3), inner.y, 3, 1);
    f.render_widget(
        Paragraph::new(Span::styled(" ✕ ", Style::new().fg(t.accent).bold())),
        close,
    );
    app.changelog_close_rect = Some(close);
    hline(f, inner.x, inner.y + 1, inner.width, t);

    // ── "how to update" header, always shown above the notes ──
    // Notify-only: bohay is installed via cargo/brew/etc, so we name the upgrade
    // commands rather than offer a self-update that can't work. When the
    // background check found a newer release, an accent headline leads.
    let mut top = inner.y + 2;
    if let Some(v) = app.update_available.clone() {
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("  ● ", Style::new().fg(t.accent).bold()),
                Span::styled(
                    format!("{} v{v}", cat.update_available),
                    Style::new().fg(t.text).bold(),
                ),
            ])),
            Rect::new(inner.x, top, inner.width, 1),
        );
        top += 1;
    }
    // The upgrade command, always present so a reader always knows how to update.
    f.render_widget(
        Paragraph::new(Span::styled(
            format!("  {}", cat.update_hint),
            Style::new().fg(t.subtext0),
        )),
        Rect::new(inner.x, top, inner.width, 1),
    );
    hline(f, inner.x, top + 1, inner.width, t);
    top += 2; // the hint line + its separator rule

    // ── body ──
    let body = Rect::new(
        inner.x + 1,
        top,
        inner.width.saturating_sub(2),
        inner.bottom().saturating_sub(top + 2),
    );
    let text_w = body.width.saturating_sub(1) as usize; // leave a right gutter
    let lines = build_lines(text_w, t);
    let total = lines.len();
    let visible = body.height as usize;
    let max_scroll = total.saturating_sub(visible) as u16;
    if app.changelog_scroll > max_scroll {
        app.changelog_scroll = max_scroll;
    }
    let start = app.changelog_scroll as usize;
    for (row, line) in lines.into_iter().skip(start).take(visible).enumerate() {
        f.render_widget(
            Paragraph::new(line),
            Rect::new(body.x, body.y + row as u16, body.width, 1),
        );
    }
    // A minimal scroll indicator on the right gutter when the notes overflow.
    if max_scroll > 0 && body.height > 0 {
        let track = body.height as usize;
        let thumb = (track * visible / total.max(1)).clamp(1, track);
        let pos = ((track - thumb) * start) / (max_scroll as usize).max(1);
        let gx = body.right().saturating_sub(1);
        let buf = f.buffer_mut();
        for i in 0..track {
            let on = i >= pos && i < pos + thumb;
            let cell = &mut buf[(gx, body.y + i as u16)];
            cell.set_symbol(" ");
            cell.set_bg(if on { t.overlay1 } else { t.surface1 });
        }
    }

    // ── footer ──
    let footer_y = inner.bottom().saturating_sub(1);
    hline(f, inner.x, footer_y.saturating_sub(1), inner.width, t);
    f.render_widget(
        Paragraph::new(hint_line(
            &[("↑↓ / wheel", cat.act_scroll), ("esc", cat.act_close)],
            t,
        )),
        Rect::new(inner.x, footer_y, inner.width, 1),
    );
}

/// Flatten every embedded release into styled, wrapped display lines (newest
/// first): a version header + rule, then the note body with headings, bullets,
/// and paragraphs. `width` is the usable text width.
fn build_lines(width: usize, t: &Theme) -> Vec<Line<'static>> {
    let mut out: Vec<Line<'static>> = Vec::new();
    for (i, (version, date, body)) in CHANGELOG.iter().enumerate() {
        if i > 0 {
            out.push(Line::default());
        }
        let head = if date.is_empty() {
            version.to_string()
        } else {
            format!("{version}   ·   {date}")
        };
        out.push(Line::from(Span::styled(
            head,
            Style::new().fg(t.accent).bold(),
        )));
        out.push(Line::from(Span::styled(
            "─".repeat(width),
            Style::new().fg(t.surface1),
        )));

        for raw in body.lines() {
            match classify(raw) {
                CBlock::Blank => out.push(Line::default()),
                CBlock::Heading(text) => {
                    out.push(Line::default());
                    for chunk in wrap(&text, width) {
                        out.push(Line::from(Span::styled(
                            chunk,
                            Style::new().fg(t.subtext1).bold(),
                        )));
                    }
                }
                CBlock::Bullet { depth, text } => {
                    let indent = depth.saturating_mul(2);
                    let avail = width.saturating_sub(indent + 2).max(1);
                    let pad: String = " ".repeat(indent);
                    for (k, chunk) in wrap(&text, avail).into_iter().enumerate() {
                        if k == 0 {
                            out.push(Line::from(vec![
                                Span::raw(pad.clone()),
                                Span::styled("• ", Style::new().fg(t.accent)),
                                Span::styled(chunk, Style::new().fg(t.subtext0)),
                            ]));
                        } else {
                            out.push(Line::from(vec![
                                Span::raw(format!("{pad}  ")),
                                Span::styled(chunk, Style::new().fg(t.subtext0)),
                            ]));
                        }
                    }
                }
                CBlock::Para(text) => {
                    for chunk in wrap(&text, width) {
                        out.push(Line::from(Span::styled(chunk, Style::new().fg(t.subtext0))));
                    }
                }
            }
        }
    }
    out
}

/// Greedy word-wrap to `width` display columns. A word longer than `width` is
/// left on its own (over-long) line rather than split mid-word — fine for the
/// prose and short tokens in release notes.
fn wrap(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }
    let mut lines = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        if cur.is_empty() {
            cur.push_str(word);
        } else if display_width(&cur) + 1 + display_width(word) <= width {
            cur.push(' ');
            cur.push_str(word);
        } else {
            lines.push(std::mem::take(&mut cur));
            cur.push_str(word);
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

// ── local render helpers (each modal module keeps its own, as elsewhere) ──

fn centered_rect(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
}

fn dim_backdrop(f: &mut RenderTarget, area: Rect, t: &Theme) {
    let buf = f.buffer_mut();
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            let cell = &mut buf[(x, y)];
            cell.set_fg(t.overlay0);
            cell.set_bg(t.crust);
        }
    }
}

fn hline(f: &mut RenderTarget, x: u16, y: u16, w: u16, t: &Theme) {
    let buf = f.buffer_mut();
    for i in 0..w {
        buf[(x + i, y)]
            .set_symbol("─")
            .set_style(Style::new().fg(t.surface1).bg(t.surface0));
    }
}
