//! Mission Control render (docs/54): a per-workspace table of the node's agents —
//! status, tokens, context and estimated cost, plus a header aggregate. One line
//! per agent (cursor + scroll like the orch board). Data is precomputed into
//! `MissionRowView`s by `App::build_mission_rows`, so drawing borrows no `App`.

use super::*;
use crate::i18n::Catalog;
use crate::mission::MissionRowView;

/// Format a token count compactly: `945`, `12.3k`, `1.2M`.
fn fmt_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

/// A left-padded, ellipsized fixed-width column.
fn col(s: &str, w: usize) -> String {
    format!("{:<w$}", truncate(s, w), w = w)
}

fn fill_bg(f: &mut RenderTarget, rect: Rect, color: Color) {
    let buf = f.buffer_mut();
    for y in rect.y..rect.bottom() {
        for x in rect.x..rect.right() {
            buf[(x, y)].set_bg(color);
        }
    }
}

fn hline(f: &mut RenderTarget, x: u16, y: u16, w: u16, t: &Theme) {
    let buf = f.buffer_mut();
    for cx in x..x + w {
        buf[(cx, y)]
            .set_symbol("─")
            .set_style(Style::new().fg(t.surface1).bg(t.mantle));
    }
}

/// One agent row: a state dot + label (coloured), then the agent, where it lives,
/// tokens, context and cost — each column added only while it still fits, so a
/// narrow tab keeps the essentials and a wide one shows everything.
fn row_line(r: &MissionRowView, width: usize, t: &Theme) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut used = 0usize;
    // A live agent shows its coloured state dot + label; a resumable on-disk
    // session shows a dim "○ resume" cue instead (MC-4), like the sidebar.
    let (dot, label, color) = if r.resumable {
        ("○".to_string(), " resume  ".to_string(), t.overlay1)
    } else {
        (
            r.state.dot().to_string(),
            format!(" {}  ", r.state.label()),
            r.state.color(t),
        )
    };
    used += display_width(&dot) + display_width(&label);
    spans.push(Span::styled(dot, Style::new().fg(color)));
    spans.push(Span::styled(label, Style::new().fg(color)));

    // The agent name always shows (truncated if need be).
    let name_w = 14usize.min(width.saturating_sub(used));
    spans.push(Span::styled(
        col(&r.agent, name_w),
        Style::new().fg(t.subtext1),
    ));
    used += name_w;

    // Column values. `—` when unknown; empty strings render as blank cells.
    let u = r.usage.as_ref();
    let ctx_frac = u.and_then(|x| x.context);
    let tokens = u
        .map(|x| format!("{} tok", fmt_tokens(x.total_tokens())))
        .unwrap_or_else(|| "—".into());
    let cost = u
        .and_then(|x| x.cost)
        .map(|c| format!("${c:.2}"))
        .unwrap_or_else(|| "—".into());
    let ctx = ctx_frac
        .map(|c| format!("{}%", (c * 100.0).round() as u32))
        .unwrap_or_else(|| "—".into());
    // Compaction headroom: how much context is left before the auto-compact line
    // (docs/54) — the "when will it compact" cue.
    let comp = ctx_frac
        .map(|c| {
            format!(
                "→{}%",
                ((crate::mission::COMPACT_AT - c) * 100.0).max(0.0).round() as u32
            )
        })
        .unwrap_or_default();
    let model = u.map(|x| short_model(&x.model)).unwrap_or_default();
    // Context + headroom turn coral near the auto-compact line, so "about to
    // compact and lose history" reads at a glance (docs/54 MC-3).
    let near = ctx_frac.is_some_and(|c| c >= crate::mission::COMPACT_AT);
    let warn = if near { t.coral } else { t.subtext0 };
    let cols: [(String, usize, Color); 6] = [
        (ctx, 6, warn),
        (comp, 7, warn),
        (tokens, 10, t.subtext0),
        (cost, 8, t.green),
        (model, 8, t.mint),
        (r.location.clone(), 12, t.overlay1),
    ];
    for (text, w, color) in cols {
        if used + w < width {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(col(&text, w), Style::new().fg(color)));
            used += w + 1;
        }
    }
    // For a blocked agent, show *what* it's waiting on at the end of the row, so
    // you can answer it without opening the pane (docs/54).
    if let Some(hint) = &r.blocked_hint {
        let room = width.saturating_sub(used);
        if room > 6 {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                truncate(hint, room - 1),
                Style::new().fg(t.coral),
            ));
        }
    }
    Line::from(spans)
}

/// A short model tag for the model column (`opus`, `sonnet`, `gpt-4o`, …), else a
/// truncated id.
fn short_model(m: &str) -> String {
    let l = m.to_lowercase();
    for k in ["opus", "sonnet", "haiku", "gpt-5", "gpt-4o", "o3", "o1"] {
        if l.contains(k) {
            return k.to_string();
        }
    }
    if m.is_empty() {
        String::new()
    } else {
        truncate(m, 8)
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render(
    f: &mut RenderTarget,
    area: Rect,
    rows: &[MissionRowView],
    scroll: usize,
    cursor: usize,
    burn: Option<f64>,
    budget: Option<f64>,
    compact: bool,
    cat: &Catalog,
    t: &Theme,
) -> usize {
    if area.height < 4 || area.width < 16 {
        return 0;
    }
    // Header: title + a live aggregate (agents, working, blocked), then the
    // workspace's total cost + fleet burn rate, with an over-budget warning.
    let working = rows.iter().filter(|r| r.state == State::Working).count();
    let blocked = rows.iter().filter(|r| r.state == State::Blocked).count();
    let total_cost: f64 = rows.iter().filter_map(|r| r.usage.as_ref()?.cost).sum();
    let over_budget = budget.is_some_and(|b| total_cost > b);
    let mut header = vec![
        Span::styled(
            format!(" {} ", cat.mc_title),
            Style::new().fg(t.accent).bold(),
        ),
        Span::styled(
            format!(
                "{} {} · {} {} · {} {}",
                rows.len(),
                cat.mc_agents,
                working,
                cat.mc_working,
                blocked,
                cat.mc_blocked,
            ),
            Style::new().fg(t.subtext0),
        ),
    ];
    if total_cost > 0.0 {
        let mut cost = format!(" · ${total_cost:.2} {}", cat.mc_total);
        if let Some(b) = budget {
            cost.push_str(&format!(" / ${b:.2}"));
        }
        let color = if over_budget { t.coral } else { t.green };
        header.push(Span::styled(cost, Style::new().fg(color)));
    }
    if let Some(rate) = burn.filter(|r| *r >= 0.005) {
        header.push(Span::styled(
            format!(" · ${rate:.2}/hr"),
            Style::new().fg(t.subtext0),
        ));
    }
    f.render_widget(
        Paragraph::new(Line::from(header)),
        Rect::new(area.x, area.y, area.width, 1),
    );
    hline(f, area.x, area.y + 1, area.width, t);

    let footer_h: u16 = if compact { 0 } else { 2 };
    if !compact {
        let footer_y = area.bottom().saturating_sub(1);
        hline(f, area.x, footer_y.saturating_sub(1), area.width, t);
        f.render_widget(
            Paragraph::new(super::hint_line(
                &[
                    ("⏎", cat.mc_go),
                    ("a", cat.mc_answer),
                    ("i", cat.mc_stop),
                    ("x", cat.act_close),
                    ("o", cat.board_details),
                ],
                t,
            )),
            Rect::new(area.x, footer_y, area.width, 1),
        );
    }

    let body = Rect::new(
        area.x + 1,
        area.y + 2,
        area.width.saturating_sub(2),
        area.height.saturating_sub(2 + footer_h),
    );
    if body.height == 0 {
        return 0;
    }
    if rows.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("  {}", cat.mc_empty),
                Style::new().fg(t.overlay0),
            ))),
            body,
        );
        return 0;
    }

    let vis = body.height as usize;
    let cursor = cursor.min(rows.len().saturating_sub(1));
    let mut scroll = scroll;
    if cursor < scroll {
        scroll = cursor;
    } else if cursor >= scroll + vis {
        scroll = cursor + 1 - vis;
    }
    scroll = scroll.min(rows.len().saturating_sub(vis));
    for (row, i) in (scroll..rows.len().min(scroll + vis)).enumerate() {
        let rect = Rect::new(body.x, body.y + row as u16, body.width, 1);
        if i == cursor {
            fill_bg(f, rect, t.surface1);
        }
        f.render_widget(
            Paragraph::new(row_line(&rows[i], body.width as usize, t)),
            rect,
        );
    }
    scroll
}

/// The row-detail overlay (MC-5): a small modal with the selected agent's full
/// breakdown — model, tokens, context and estimated cost. Read-only; any of
/// esc/o/q/⏎ closes it. Drawn last, over a dimmed backdrop like the other modals.
pub(super) fn draw_detail(
    f: &mut RenderTarget,
    area: Rect,
    r: &MissionRowView,
    cat: &Catalog,
    t: &Theme,
) {
    use ratatui::widgets::{Block, Borders, Clear};
    super::help::dim_backdrop(f, area, t);
    let w = area.width.saturating_sub(6).clamp(40, 64).min(area.width);
    let modal = super::help::centered_rect(area, w, 16);
    f.render_widget(Clear, modal);
    let block = Block::new()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(t.border_focus).bg(t.surface0))
        .style(Style::new().bg(t.surface0));
    let inner = block.inner(modal);
    f.render_widget(block, modal);

    let kv = |k: &str, v: String| -> Line<'static> {
        Line::from(vec![
            Span::styled(format!(" {k:<9}"), Style::new().fg(t.subtext0)),
            Span::styled(v, Style::new().fg(t.text)),
        ])
    };
    let mut lines: Vec<Line> = vec![
        Line::from(Span::styled(
            format!(" {} — {}", cat.mc_title, r.agent),
            Style::new().fg(t.text).bold(),
        )),
        Line::from(""),
    ];
    let status = if r.resumable {
        "resumable".to_string()
    } else {
        r.state.label().to_string()
    };
    lines.push(kv("status", status));
    lines.push(kv("where", r.location.clone()));
    match &r.usage {
        Some(u) => {
            if !u.model.is_empty() {
                lines.push(kv("model", u.model.clone()));
            }
            lines.push(kv("input", format!("{} tok", fmt_tokens(u.tokens_in))));
            lines.push(kv("output", format!("{} tok", fmt_tokens(u.tokens_out))));
            lines.push(kv("cache", format!("{} tok", fmt_tokens(u.cache))));
            if let Some(c) = u.context {
                let headroom = ((crate::mission::COMPACT_AT - c) * 100.0).max(0.0).round() as u32;
                lines.push(kv(
                    "context",
                    format!(
                        "{}% used · {}% until compact",
                        (c * 100.0).round() as u32,
                        headroom
                    ),
                ));
            }
            if let Some(cost) = u.cost {
                lines.push(kv("cost", format!("${cost:.2} (estimate)")));
            }
        }
        None => lines.push(Line::from(Span::styled(
            "  no usage data for this session",
            Style::new().fg(t.overlay0),
        ))),
    }
    // What it's blocked on, if anything.
    if let Some(hint) = &r.blocked_hint {
        lines.push(Line::from(""));
        lines.push(kv(
            "waiting",
            truncate(hint, inner.width.saturating_sub(11) as usize),
        ));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!("  esc · {}", cat.act_close),
        Style::new().fg(t.overlay0),
    )));
    f.render_widget(Paragraph::new(lines), inner);
}

/// The inline "answer the agent" input (docs/54): a one-line prompt to type a
/// reply that is sent to the selected blocked agent's pane. `⏎` sends, `esc`
/// cancels. Drawn last, over a dimmed backdrop.
pub(super) fn draw_answer(f: &mut RenderTarget, area: Rect, text: &str, cat: &Catalog, t: &Theme) {
    use ratatui::widgets::{Block, Borders, Clear};
    super::help::dim_backdrop(f, area, t);
    let w = area.width.saturating_sub(6).clamp(40, 72).min(area.width);
    let modal = super::help::centered_rect(area, w, 5);
    f.render_widget(Clear, modal);
    let block = Block::new()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(t.border_focus).bg(t.surface0))
        .style(Style::new().bg(t.surface0));
    let inner = block.inner(modal);
    f.render_widget(block, modal);
    let shown = truncate(text, inner.width.saturating_sub(4) as usize);
    f.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                format!(" {}", cat.mc_answer),
                Style::new().fg(t.text).bold(),
            )),
            Line::from(vec![
                Span::styled(" > ", Style::new().fg(t.overlay1)),
                Span::styled(format!("{shown}▏"), Style::new().fg(t.text)),
            ]),
            Line::from(Span::styled(
                format!("  ⏎ · esc {}", cat.act_cancel),
                Style::new().fg(t.overlay0),
            )),
        ]),
        inner,
    );
}
