//! Terminal output shared by every command: one table renderer, one style set.

use std::io::IsTerminal;
use std::sync::OnceLock;

/// ANSI styling, suppressed when stdout is redirected or `NO_COLOR` is set.
pub fn colored() -> bool {
    static COLORED: OnceLock<bool> = OnceLock::new();
    *COLORED
        .get_or_init(|| std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal())
}

macro_rules! style {
    ($name:ident, $code:literal) => {
        pub fn $name(text: &str) -> String {
            if colored() {
                format!("\x1b[{}m{text}\x1b[0m", $code)
            } else {
                text.to_string()
            }
        }
    };
}

/// Paint with a colour from [`crate::brand::palette`], so the CLI and the TUI
/// cannot end up with two different ideas of the same role.
fn painted(color: ratatui::style::Color, text: &str) -> String {
    let ratatui::style::Color::Rgb(r, g, b) = color else {
        return text.to_string();
    };
    if colored() {
        format!("\x1b[38;2;{r};{g};{b}m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

pub fn accent(text: &str) -> String {
    painted(crate::brand::palette::ACCENT, text)
}

style!(bold, "1");
style!(dim, "2");
style!(green, "32");
style!(yellow, "33");
style!(red, "31");
style!(cyan, "36");

/// Marker for a yes/no column that reads the same without color.
pub fn tick(on: bool) -> String {
    if on {
        green("yes")
    } else {
        dim("no")
    }
}

/// A left-aligned text table that sizes its own columns.
///
/// Widths are measured in characters rather than bytes so non-ASCII provider
/// names do not skew the alignment.
#[derive(Default)]
pub struct Table {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}

impl Table {
    pub fn new<I, S>(headers: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self { headers: headers.into_iter().map(Into::into).collect(), rows: Vec::new() }
    }

    pub fn row<I, S>(&mut self, cells: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.rows.push(cells.into_iter().map(Into::into).collect());
        self
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// A table of aligned rows with no header, for label/value listings.
    pub fn plain() -> Self {
        Self::default()
    }

    pub fn render(&self) -> String {
        let columns = self.headers.len().max(self.rows.iter().map(Vec::len).max().unwrap_or(0));
        let mut widths = vec![0usize; columns];
        for cells in std::iter::once(&self.headers).chain(&self.rows) {
            for (index, cell) in cells.iter().take(columns).enumerate() {
                widths[index] = widths[index].max(visible_width(cell));
            }
        }

        let mut out = String::new();
        if !self.headers.is_empty() {
            push_row(&mut out, &self.headers, &widths, bold);
        }
        for row in &self.rows {
            push_row(&mut out, row, &widths, |cell| cell.to_string());
        }
        out
    }
}

fn push_row(out: &mut String, cells: &[String], widths: &[usize], style: impl Fn(&str) -> String) {
    let last = cells.len().saturating_sub(1);
    for (index, cell) in cells.iter().enumerate() {
        out.push_str(&style(cell));
        if index != last {
            let pad = widths[index].saturating_sub(visible_width(cell)) + 2;
            out.push_str(&" ".repeat(pad));
        }
    }
    out.push('\n');
}

/// Character count excluding ANSI escape sequences, which occupy no columns.
fn visible_width(text: &str) -> usize {
    let mut width = 0;
    let mut in_escape = false;
    for ch in text.chars() {
        match ch {
            '\x1b' => in_escape = true,
            'm' if in_escape => in_escape = false,
            _ if in_escape => {}
            _ => width += 1,
        }
    }
    width
}

/// Token counts read better as 200K than as 200000.
///
/// The CLI and the TUI both list model limits, so the rounding lives here and
/// the two cannot end up disagreeing about how big a context window is.
pub fn tokens(count: u64) -> String {
    match count {
        n if n >= 1_000_000 && n % 1_000_000 == 0 => format!("{}M", n / 1_000_000),
        n if n >= 1_000_000 => format!("{:.1}M", n as f64 / 1_000_000.0),
        n if n >= 1_000 => format!("{}K", n / 1_000),
        n => n.to_string(),
    }
}

/// Shorten to `max` characters, marking the cut so a truncated URL is never
/// mistaken for a real one.
pub fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let kept: String = text.chars().take(max.saturating_sub(1)).collect();
    format!("{kept}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_second_column_starts_at_one_offset_on_every_row() {
        let mut table = Table::new(["id", "url"]);
        table.row(["a", "short"]).row(["longer-id", "x"]);

        let rendered = table.render();
        // The widest first-column cell is "longer-id", so every row's second
        // cell must begin at its width plus the two-space gutter.
        let expected = "longer-id".len() + 2;
        for line in rendered.lines() {
            let gap = line.find("  ").expect("no column gutter");
            let second = line[gap..].trim_start().to_string();
            assert_eq!(line.len() - second.len(), expected, "misaligned row: {line:?}");
        }
        assert_eq!(rendered.lines().count(), 3);
    }

    #[test]
    fn width_ignores_ansi_escapes() {
        assert_eq!(visible_width("\x1b[32myes\x1b[0m"), 3);
        assert_eq!(visible_width("plain"), 5);
    }

    #[test]
    fn token_counts_round_to_something_readable() {
        assert_eq!(tokens(1_000_000), "1M");
        assert_eq!(tokens(1_100_000), "1.1M");
        assert_eq!(tokens(2_000_000), "2M");
        assert_eq!(tokens(128_000), "128K");
        assert_eq!(tokens(8_192), "8K");
        assert_eq!(tokens(512), "512");
    }

    #[test]
    fn truncation_marks_the_cut() {
        assert_eq!(truncate("https://byesu.com/v1", 10), "https://b…");
        assert_eq!(truncate("short", 10), "short");
    }

    #[test]
    fn empty_table_still_renders_its_header() {
        let table = Table::new(["a", "b"]);
        assert!(table.is_empty());
        assert_eq!(table.render().lines().count(), 1);
    }
}
