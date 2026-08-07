//! Release notes embedded at build time (see `build.rs`) plus the pure text
//! helpers the changelog modal uses. Kept free of rendering types — the UI layer
//! (`ui/changelog.rs`) turns this into styled, wrapped lines.

// Generated: `pub static CHANGELOG: &[(&str, &str, &str)] = &[(version, date, body), …]`
// newest release first.
include!(concat!(env!("OUT_DIR"), "/changelog_gen.rs"));

/// One run of inline text from a release note, with the URL it points at if it
/// came from a markdown link.
///
/// Segments rather than a flat `String` because the modal makes commit and PR
/// references **clickable** — the URL has to survive parsing to reach the
/// renderer, which turns it into a hit-testable rect.
#[derive(Debug, Clone, PartialEq)]
pub struct Seg {
    pub text: String,
    pub url: Option<String>,
}

impl Seg {
    pub fn plain(text: impl Into<String>) -> Self {
        Seg {
            text: text.into(),
            url: None,
        }
    }
}

/// Parse the inline markdown a release note uses into display segments:
/// `**bold**` and `` `code` `` markers are dropped, and `[text](url)` becomes a
/// segment carrying its URL. Everything else passes through unchanged.
pub fn inline(s: &str) -> Vec<Seg> {
    let chars: Vec<char> = s.chars().collect();
    let mut out: Vec<Seg> = Vec::new();
    let mut buf = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        // `**` bold markers → dropped.
        if c == '*' && chars.get(i + 1) == Some(&'*') {
            i += 2;
            continue;
        }
        // Inline code backticks → dropped (keep the code text).
        if c == '`' {
            i += 1;
            continue;
        }
        // `[text](url)` → a linked segment. The label is parsed too, so markup
        // *inside* it (the `` `hash` `` commit-ref shape) is stripped as well.
        if c == '[' {
            if let Some(close) = find(&chars, i + 1, ']') {
                if chars.get(close + 1) == Some(&'(') {
                    if let Some(paren) = find(&chars, close + 2, ')') {
                        let label: String = chars[i + 1..close].iter().collect();
                        let url: String = chars[close + 2..paren].iter().collect();
                        if !buf.is_empty() {
                            out.push(Seg::plain(std::mem::take(&mut buf)));
                        }
                        let text = strip_inline(&label);
                        // A link with no label, or one pointing nowhere, is text.
                        if text.is_empty() || url.trim().is_empty() {
                            buf.push_str(&text);
                        } else {
                            out.push(Seg {
                                text,
                                url: Some(url.trim().to_string()),
                            });
                        }
                        i = paren + 1;
                        continue;
                    }
                }
            }
        }
        buf.push(c);
        i += 1;
    }
    if !buf.is_empty() {
        out.push(Seg::plain(buf));
    }
    out
}

/// The plain text of some inline markdown, links flattened to their label. Used
/// where styling is irrelevant (link labels, tests).
pub fn strip_inline(s: &str) -> String {
    inline(s).into_iter().map(|seg| seg.text).collect()
}

fn find(chars: &[char], from: usize, target: char) -> Option<usize> {
    (from..chars.len()).find(|&j| chars[j] == target)
}

/// A parsed body line, classified for the renderer.
pub enum Block {
    /// A section heading (`##`/`###`…), with the `#`s stripped.
    Heading(String),
    /// A bullet item; `depth` is the indent level (0 = top).
    Bullet { depth: usize, segs: Vec<Seg> },
    /// A normal paragraph line.
    Para(Vec<Seg>),
    /// A blank spacer.
    Blank,
}

/// Classify one raw markdown line into a [`Block`], with inline markdown already
/// parsed. Used by the modal to style + wrap each line.
///
/// Headings flatten to plain text: they are styling, never links.
pub fn classify(raw: &str) -> Block {
    if raw.trim().is_empty() {
        return Block::Blank;
    }
    let trimmed = raw.trim_start();
    if let Some(rest) = trimmed.strip_prefix('#') {
        let heading = rest.trim_start_matches('#').trim();
        return Block::Heading(strip_inline(heading));
    }
    // Bullets: `- ` or `* `, nesting by leading-space count (2 spaces per level).
    let indent = raw.len() - trimmed.len();
    if let Some(rest) = trimmed
        .strip_prefix("- ")
        .or_else(|| trimmed.strip_prefix("* "))
    {
        return Block::Bullet {
            depth: indent / 2,
            segs: inline(rest.trim()),
        };
    }
    Block::Para(inline(trimmed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_bold_code_and_links() {
        assert_eq!(strip_inline("**Fork** a session"), "Fork a session");
        assert_eq!(strip_inline("press `Ctrl+f` now"), "press Ctrl+f now");
        assert_eq!(
            strip_inline("see [#19](https://x/pull/19) for more"),
            "see #19 for more"
        );
        // Markup inside a link's text is stripped too (the commit-ref shape).
        assert_eq!(
            strip_inline("([`59a2bd5`](https://x/c/59a2bd5))"),
            "(59a2bd5)"
        );
        // A stray bracket that is not a link is left intact.
        assert_eq!(strip_inline("array[0] value"), "array[0] value");
    }

    /// The URL survives parsing, which is what makes a commit ref clickable.
    #[test]
    fn keeps_link_urls_as_segments() {
        assert_eq!(
            inline("see [#19](https://x/pull/19) for more"),
            vec![
                Seg::plain("see "),
                Seg {
                    text: "#19".into(),
                    url: Some("https://x/pull/19".into())
                },
                Seg::plain(" for more"),
            ]
        );
        // The commit-ref shape: backticks inside the label are stripped, the URL kept.
        assert_eq!(
            inline("([`59a2bd5`](https://x/c/59a2bd5))"),
            vec![
                Seg::plain("("),
                Seg {
                    text: "59a2bd5".into(),
                    url: Some("https://x/c/59a2bd5".into())
                },
                Seg::plain(")"),
            ]
        );
        // A link with no label or no target degrades to plain text, never a
        // zero-width thing you cannot see but can click.
        assert_eq!(inline("[](https://x)"), vec![]);
        assert_eq!(inline("[label]()"), vec![Seg::plain("label")]);
        // Not a link at all.
        assert_eq!(inline("array[0] value"), vec![Seg::plain("array[0] value")]);
    }

    #[test]
    fn classifies_lines() {
        assert!(matches!(classify(""), Block::Blank));
        assert!(matches!(classify("### ✨ Added"), Block::Heading(h) if h == "✨ Added"));
        assert!(
            matches!(classify("- **A** thing"), Block::Bullet { depth: 0, segs } if segs == vec![Seg::plain("A thing")])
        );
        assert!(matches!(
            classify("  - nested"),
            Block::Bullet { depth: 1, .. }
        ));
        assert!(
            matches!(classify("Just prose."), Block::Para(p) if p == vec![Seg::plain("Just prose.")])
        );
    }

    #[test]
    fn changelog_is_embedded_and_ordered() {
        assert!(!CHANGELOG.is_empty(), "release notes are embedded");
        // Every entry has a version; bodies are non-empty.
        for (v, _d, body) in CHANGELOG {
            assert!(!v.is_empty(), "entry has a version");
            assert!(!body.is_empty(), "entry has body text");
        }
        // Newest first: the first entry's version is >= the last.
        let ver = |s: &str| -> (u32, u32, u32) {
            let s = s.trim_start_matches('v');
            let mut it = s.split('.').map(|p| p.parse::<u32>().unwrap_or(0));
            (
                it.next().unwrap_or(0),
                it.next().unwrap_or(0),
                it.next().unwrap_or(0),
            )
        };
        if CHANGELOG.len() > 1 {
            assert!(
                ver(CHANGELOG[0].0) >= ver(CHANGELOG[CHANGELOG.len() - 1].0),
                "entries are newest-first"
            );
        }
    }

    /// The embedded notes are stripped of credits for the in-app modal (the
    /// changelog itself, no author/contributor section or bare compare-link
    /// footer). `build.rs::clean_body` does this; the raw files + website keep it.
    #[test]
    fn embedded_notes_drop_the_contributor_section_and_footer() {
        for (_v, _d, body) in CHANGELOG {
            assert!(
                !body.contains("### Contributors") && !body.contains("## Contributors"),
                "the Contributors heading is stripped"
            );
            assert!(
                !body.contains("Full Changelog**") && !body.contains("/compare/"),
                "the trailing Full Changelog compare link is stripped"
            );
        }
    }
}
