//! Interactive Skill Browser TUI using ratatui.
//!
//! A rich terminal UI for browsing, searching, and previewing skills interactively.
//! Provides two-pane layout with search, filtering, and keyboard navigation.

use std::io::{self, IsTerminal, Stdout};
use std::time::Duration;

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::error::{MsError, Result};
use crate::storage::sqlite::{Database, SkillRecord};

/// Focus state for TUI panels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusPanel {
    List,
    Detail,
}

impl FocusPanel {
    const fn toggle(self) -> Self {
        match self {
            Self::List => Self::Detail,
            Self::Detail => Self::List,
        }
    }
}

/// Filter configuration for skill list.
#[derive(Debug, Default, Clone)]
pub struct Filters {
    /// Filter by layer (base, org, project, user)
    pub layer: Option<String>,
    /// Filter by tags
    pub tags: Vec<String>,
    /// Minimum quality score filter
    pub min_quality: Option<f64>,
}

impl Filters {
    /// Parse filter syntax from search query.
    /// Supports: `layer:base`, `tag:rust`, `quality:>0.8`
    fn parse_special_filters(query: &str) -> (String, Self) {
        let mut filters = Self::default();
        let mut text_parts = Vec::new();

        for part in query.split_whitespace() {
            if let Some(layer) = part.strip_prefix("layer:") {
                filters.layer = Some(layer.to_string());
            } else if let Some(tag) = part.strip_prefix("tag:") {
                filters.tags.push(tag.to_string());
            } else if let Some(quality) = part.strip_prefix("quality:>") {
                if let Ok(q) = quality.parse::<f64>() {
                    filters.min_quality = Some(q);
                }
            } else {
                text_parts.push(part);
            }
        }

        (text_parts.join(" "), filters)
    }
}

/// Action to take after handling input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Exit the TUI without selecting a skill
    Quit,
    /// Load and output the selected skill
    Load(String),
    /// Continue running the TUI
    Continue,
}

/// Summary data for displaying a skill in the list.
#[derive(Debug, Clone)]
pub struct SkillSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub layer: String,
    pub quality_score: f64,
    pub tags: Vec<String>,
    pub body: String,
}

impl From<&SkillRecord> for SkillSummary {
    fn from(r: &SkillRecord) -> Self {
        // Parse tags from metadata_json
        let tags = serde_json::from_str::<serde_json::Value>(&r.metadata_json)
            .ok()
            .and_then(|meta| meta.get("tags")?.as_array().cloned())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        Self {
            id: r.id.clone(),
            name: r.name.clone(),
            description: r.description.clone(),
            layer: normalize_layer(&r.source_layer),
            quality_score: r.quality_score,
            tags,
            body: r.body.clone(),
        }
    }
}

/// TUI application state.
pub struct BrowseTui {
    /// All loaded skills
    skills: Vec<SkillSummary>,
    /// Indices into skills after filtering
    filtered: Vec<usize>,
    /// List selection state
    list_state: ListState,
    /// Current search query
    search_query: String,
    /// Whether search box is focused
    search_focused: bool,
    /// Active filters
    filters: Filters,
    /// Currently focused panel
    focus: FocusPanel,
    /// Detail pane scroll offset
    detail_scroll: u16,
    /// Whether to show help overlay
    show_help: bool,
    /// Status message to display
    status_message: Option<String>,
    /// Whether app should exit
    should_quit: bool,
}

impl BrowseTui {
    /// Create a new Browse TUI with skills from the database.
    pub fn new(db: &Database) -> Result<Self> {
        let records = db.list_skills(1000, 0)?;
        let skills: Vec<SkillSummary> = records.iter().map(SkillSummary::from).collect();
        let filtered: Vec<usize> = (0..skills.len()).collect();

        let mut list_state = ListState::default();
        if !filtered.is_empty() {
            list_state.select(Some(0));
        }

        Ok(Self {
            skills,
            filtered,
            list_state,
            search_query: String::new(),
            search_focused: false,
            filters: Filters::default(),
            focus: FocusPanel::List,
            detail_scroll: 0,
            show_help: false,
            status_message: None,
            should_quit: false,
        })
    }

    /// Create a BrowseTui with pre-loaded skills (for testing).
    #[cfg(test)]
    pub fn with_test_skills(skills: Vec<SkillSummary>) -> Self {
        let filtered: Vec<usize> = (0..skills.len()).collect();
        let mut list_state = ListState::default();
        if !filtered.is_empty() {
            list_state.select(Some(0));
        }

        Self {
            skills,
            filtered,
            list_state,
            search_query: String::new(),
            search_focused: false,
            filters: Filters::default(),
            focus: FocusPanel::List,
            detail_scroll: 0,
            show_help: false,
            status_message: None,
            should_quit: false,
        }
    }

    /// Run the TUI main loop.
    pub fn run(mut self, terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<Option<String>> {
        loop {
            terminal.draw(|f| self.draw(f))?;

            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    match self.handle_key(key.code, key.modifiers) {
                        Action::Quit => return Ok(None),
                        Action::Load(skill_id) => return Ok(Some(skill_id)),
                        Action::Continue => {}
                    }
                }
            }
        }
    }

    fn draw(&mut self, f: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),  // Title bar
                Constraint::Length(3),  // Search bar
                Constraint::Min(10),    // Main content
                Constraint::Length(1),  // Help bar
            ])
            .split(f.area());

        self.draw_title_bar(f, chunks[0]);
        self.draw_search_bar(f, chunks[1]);
        self.draw_main_content(f, chunks[2]);
        self.draw_help_bar(f, chunks[3]);

        // Draw help overlay if active
        if self.show_help {
            self.draw_help_overlay(f);
        }
    }

    fn draw_title_bar(&self, f: &mut Frame, area: Rect) {
        let status = self
            .status_message
            .as_ref()
            .map(|m| format!(" | {m}"))
            .unwrap_or_default();

        let title = Line::from(vec![
            Span::styled("ms browse", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(format!(
                " | {} skills ({} shown){}",
                self.skills.len(),
                self.filtered.len(),
                status
            )),
        ]);

        let paragraph = Paragraph::new(title).style(Style::default().fg(Color::Cyan));
        f.render_widget(paragraph, area);
    }

    fn draw_search_bar(&self, f: &mut Frame, area: Rect) {
        let border_style = if self.search_focused {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };

        let search_text = if self.search_focused {
            format!("{}_", self.search_query)
        } else if self.search_query.is_empty() {
            "Type / to search...".to_string()
        } else {
            self.search_query.clone()
        };

        let paragraph = Paragraph::new(search_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(border_style)
                    .title(" Search "),
            )
            .style(if self.search_query.is_empty() && !self.search_focused {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default()
            });

        f.render_widget(paragraph, area);
    }

    fn draw_main_content(&mut self, f: &mut Frame, area: Rect) {
        // Split into two columns: list (40%) + detail (60%)
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(area);

        self.draw_list_panel(f, columns[0]);
        self.draw_detail_panel(f, columns[1]);
    }

    fn draw_list_panel(&mut self, f: &mut Frame, area: Rect) {
        let is_focused = self.focus == FocusPanel::List && !self.search_focused;
        let border_style = if is_focused {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };

        let items: Vec<ListItem> = self
            .filtered
            .iter()
            .filter_map(|&idx| self.skills.get(idx))
            .map(|s| {
                let layer_color = match s.layer.as_str() {
                    "base" => Color::Blue,
                    "org" => Color::Green,
                    "project" => Color::Yellow,
                    "user" => Color::Magenta,
                    _ => Color::White,
                };

                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("[{}] ", truncate(&s.layer, 4)),
                        Style::default().fg(layer_color),
                    ),
                    Span::raw(truncate(&s.name, 30)),
                ]))
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(border_style)
                    .title(if is_focused {
                        " Skills [*] "
                    } else {
                        " Skills "
                    }),
            )
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
            .highlight_symbol("> ");

        f.render_stateful_widget(list, area, &mut self.list_state);
    }

    fn draw_detail_panel(&mut self, f: &mut Frame, area: Rect) {
        let is_focused = self.focus == FocusPanel::Detail && !self.search_focused;
        let border_style = if is_focused {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };

        let content = self.get_selected_detail();

        let paragraph = Paragraph::new(content)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(border_style)
                    .title(if is_focused {
                        " Details [*] "
                    } else {
                        " Details "
                    }),
            )
            .wrap(Wrap { trim: false })
            .scroll((self.detail_scroll, 0));

        f.render_widget(paragraph, area);
    }

    fn draw_help_bar(&self, f: &mut Frame, area: Rect) {
        let help_text = if self.search_focused {
            "Enter: apply  Esc: cancel  Backspace: delete"
        } else {
            "j/k: navigate  /: search  Enter/l: load  Tab: switch pane  f: favorite  ?: help  q: quit"
        };

        let paragraph = Paragraph::new(help_text).style(Style::default().fg(Color::DarkGray));
        f.render_widget(paragraph, area);
    }

    fn draw_help_overlay(&self, f: &mut Frame) {
        let area = f.area();

        // Center the help dialog
        let help_width = 60.min(area.width.saturating_sub(4));
        let help_height = 20.min(area.height.saturating_sub(4));
        let x = (area.width - help_width) / 2;
        let y = (area.height - help_height) / 2;
        let help_area = Rect::new(x, y, help_width, help_height);

        // Clear the area behind the popup
        f.render_widget(Clear, help_area);

        let help_text = vec![
            Line::from(Span::styled("Keyboard Shortcuts", Style::default().add_modifier(Modifier::BOLD))),
            Line::from(""),
            Line::from("Navigation:"),
            Line::from("  j / Down     Move down in list"),
            Line::from("  k / Up       Move up in list"),
            Line::from("  G            Jump to last item"),
            Line::from("  g            Jump to first item"),
            Line::from("  Tab          Switch focus between panels"),
            Line::from("  PgUp/PgDn    Scroll detail pane"),
            Line::from(""),
            Line::from("Actions:"),
            Line::from("  /            Focus search box"),
            Line::from("  Enter / l    Load selected skill"),
            Line::from("  f            Toggle favorite (not implemented)"),
            Line::from("  h            Toggle hidden (not implemented)"),
            Line::from(""),
            Line::from("Search Filters:"),
            Line::from("  layer:base   Filter by layer"),
            Line::from("  tag:rust     Filter by tag"),
            Line::from("  quality:>0.8 Filter by quality score"),
            Line::from(""),
            Line::from("Press ? or Esc to close this help"),
        ];

        let paragraph = Paragraph::new(help_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan))
                    .title(" Help "),
            )
            .wrap(Wrap { trim: false });

        f.render_widget(paragraph, help_area);
    }

    fn get_selected_detail(&self) -> Text<'static> {
        let Some(selected_idx) = self.list_state.selected() else {
            return Text::from("No skill selected");
        };

        let Some(&skill_idx) = self.filtered.get(selected_idx) else {
            return Text::from("No skill selected");
        };

        let Some(skill) = self.skills.get(skill_idx) else {
            return Text::from("No skill selected");
        };

        let mut lines: Vec<Line<'static>> = vec![
            Line::from(Span::styled(
                skill.name.clone(),
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Line::from(format!("ID: {}", skill.id)),
            Line::from(""),
            Line::from(Span::styled(
                "Layer: ".to_string(),
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Line::from(format!("  {}", skill.layer)),
            Line::from(""),
            Line::from(Span::styled(
                format!("Quality: {:.0}%", skill.quality_score * 100.0),
                Style::default().fg(quality_color(skill.quality_score)),
            )),
            Line::from(""),
        ];

        if !skill.tags.is_empty() {
            lines.push(Line::from(Span::styled(
                "Tags: ".to_string(),
                Style::default().add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(format!("  {}", skill.tags.join(", "))));
            lines.push(Line::from(""));
        }

        if !skill.description.is_empty() {
            lines.push(Line::from(Span::styled(
                "Description:".to_string(),
                Style::default().add_modifier(Modifier::BOLD),
            )));
            for desc_line in skill.description.lines() {
                lines.push(Line::from(format!("  {}", desc_line)));
            }
            lines.push(Line::from(""));
        }

        // Add a preview of the body content
        lines.push(Line::from(Span::styled(
            "Content Preview:".to_string(),
            Style::default().add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from("─".repeat(40)));

        // Show first ~30 lines of body
        for (i, body_line) in skill.body.lines().enumerate() {
            if i > 30 {
                lines.push(Line::from("... (truncated)"));
                break;
            }
            lines.push(Line::from(body_line.to_string()));
        }

        Text::from(lines)
    }

    fn handle_key(&mut self, key: KeyCode, modifiers: KeyModifiers) -> Action {
        // Handle help overlay
        if self.show_help {
            match key {
                KeyCode::Char('?') | KeyCode::Esc | KeyCode::Enter => {
                    self.show_help = false;
                }
                _ => {}
            }
            return Action::Continue;
        }

        // Handle search mode
        if self.search_focused {
            return self.handle_search_key(key);
        }

        // Global shortcuts
        match key {
            KeyCode::Char('q') => return Action::Quit,
            KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => return Action::Quit,
            KeyCode::Char('/') => {
                self.search_focused = true;
                return Action::Continue;
            }
            KeyCode::Char('?') => {
                self.show_help = true;
                return Action::Continue;
            }
            KeyCode::Tab => {
                self.focus = self.focus.toggle();
                return Action::Continue;
            }
            KeyCode::Esc => {
                // Clear search/filters
                if !self.search_query.is_empty() {
                    self.search_query.clear();
                    self.filters = Filters::default();
                    self.apply_filters();
                    self.status_message = Some("Filters cleared".to_string());
                }
                return Action::Continue;
            }
            _ => {}
        }

        // Panel-specific handling
        match self.focus {
            FocusPanel::List => self.handle_list_key(key),
            FocusPanel::Detail => self.handle_detail_key(key),
        }
    }

    fn handle_search_key(&mut self, key: KeyCode) -> Action {
        match key {
            KeyCode::Enter => {
                self.search_focused = false;
                self.apply_filters();
                self.status_message = if self.search_query.is_empty() {
                    Some("Search cleared".to_string())
                } else {
                    Some(format!("Searching: {}", self.search_query))
                };
            }
            KeyCode::Esc => {
                self.search_focused = false;
                // Don't apply changes on Esc
            }
            KeyCode::Char(c) => {
                self.search_query.push(c);
            }
            KeyCode::Backspace => {
                self.search_query.pop();
            }
            _ => {}
        }
        Action::Continue
    }

    fn handle_list_key(&mut self, key: KeyCode) -> Action {
        match key {
            KeyCode::Down | KeyCode::Char('j') => {
                self.select_next();
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.select_prev();
            }
            KeyCode::Char('G') => {
                // Jump to last
                if !self.filtered.is_empty() {
                    self.list_state.select(Some(self.filtered.len() - 1));
                    self.detail_scroll = 0;
                }
            }
            KeyCode::Char('g') => {
                // Jump to first
                if !self.filtered.is_empty() {
                    self.list_state.select(Some(0));
                    self.detail_scroll = 0;
                }
            }
            KeyCode::Enter | KeyCode::Char('l') => {
                return self.load_selected();
            }
            KeyCode::Char('f') => {
                self.status_message = Some("Favorite toggle not yet implemented".to_string());
            }
            KeyCode::Char('h') => {
                self.status_message = Some("Hide toggle not yet implemented".to_string());
            }
            _ => {}
        }
        Action::Continue
    }

    fn handle_detail_key(&mut self, key: KeyCode) -> Action {
        match key {
            KeyCode::Down | KeyCode::Char('j') | KeyCode::PageDown => {
                self.detail_scroll = self.detail_scroll.saturating_add(3);
            }
            KeyCode::Up | KeyCode::Char('k') | KeyCode::PageUp => {
                self.detail_scroll = self.detail_scroll.saturating_sub(3);
            }
            KeyCode::Char('G') => {
                self.detail_scroll = u16::MAX / 2; // Scroll to near-end
            }
            KeyCode::Char('g') => {
                self.detail_scroll = 0;
            }
            KeyCode::Enter | KeyCode::Char('l') => {
                return self.load_selected();
            }
            _ => {}
        }
        Action::Continue
    }

    fn select_next(&mut self) {
        if self.filtered.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => {
                if i >= self.filtered.len() - 1 {
                    0 // Wrap to beginning
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
        self.detail_scroll = 0;
    }

    fn select_prev(&mut self) {
        if self.filtered.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.filtered.len() - 1 // Wrap to end
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
        self.detail_scroll = 0;
    }

    fn load_selected(&self) -> Action {
        if let Some(selected_idx) = self.list_state.selected() {
            if let Some(&skill_idx) = self.filtered.get(selected_idx) {
                if let Some(skill) = self.skills.get(skill_idx) {
                    return Action::Load(skill.id.clone());
                }
            }
        }
        Action::Continue
    }

    fn apply_filters(&mut self) {
        // Parse special filters from search query
        let (text_query, parsed_filters) = Filters::parse_special_filters(&self.search_query);

        // Merge parsed filters with existing
        self.filters = parsed_filters;

        let text_query_lower = text_query.to_lowercase();

        self.filtered = self
            .skills
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                // Text search (case-insensitive)
                if !text_query_lower.is_empty() {
                    let matches_name = s.name.to_lowercase().contains(&text_query_lower);
                    let matches_desc = s.description.to_lowercase().contains(&text_query_lower);
                    let matches_id = s.id.to_lowercase().contains(&text_query_lower);
                    if !matches_name && !matches_desc && !matches_id {
                        return false;
                    }
                }

                // Layer filter
                if let Some(ref layer) = self.filters.layer {
                    if s.layer.to_lowercase() != layer.to_lowercase() {
                        return false;
                    }
                }

                // Tag filter
                if !self.filters.tags.is_empty() {
                    let skill_tags_lower: Vec<String> =
                        s.tags.iter().map(|t| t.to_lowercase()).collect();
                    let has_matching_tag = self
                        .filters
                        .tags
                        .iter()
                        .any(|t| skill_tags_lower.contains(&t.to_lowercase()));
                    if !has_matching_tag {
                        return false;
                    }
                }

                // Quality filter
                if let Some(min_quality) = self.filters.min_quality {
                    if s.quality_score < min_quality {
                        return false;
                    }
                }

                true
            })
            .map(|(i, _)| i)
            .collect();

        // Reset selection to first item if available
        if self.filtered.is_empty() {
            self.list_state.select(None);
        } else {
            self.list_state.select(Some(0));
        }
        self.detail_scroll = 0;
    }

    /// Get selected item index for testing.
    #[cfg(test)]
    pub fn selected(&self) -> Option<usize> {
        self.list_state.selected()
    }

    /// Get filtered count for testing.
    #[cfg(test)]
    pub fn filtered_count(&self) -> usize {
        self.filtered.len()
    }

    /// Set search query for testing.
    #[cfg(test)]
    pub fn set_search_query(&mut self, query: &str) {
        self.search_query = query.to_string();
        self.apply_filters();
    }
}

/// RAII Guard to ensure terminal state is restored even on panic.
struct TerminalGuard;

impl TerminalGuard {
    fn new() -> Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            LeaveAlternateScreen,
            DisableMouseCapture
        );
    }
}

/// Run the browse TUI.
pub fn run_browse_tui(db: &Database) -> Result<Option<String>> {
    // Check if stdout is a terminal
    if !io::stdout().is_terminal() {
        return Err(MsError::ValidationFailed(
            "browse command requires an interactive terminal".to_string(),
        ));
    }

    let _guard = TerminalGuard::new()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;

    let app = BrowseTui::new(db)?;
    app.run(&mut terminal)
}

fn normalize_layer(input: &str) -> String {
    match input.to_lowercase().as_str() {
        "system" => "base",
        "global" => "org",
        "local" => "user",
        other => other,
    }
    .to_string()
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() > max_len {
        format!("{}...", s.chars().take(max_len - 3).collect::<String>())
    } else {
        s.to_string()
    }
}

fn quality_color(quality: f64) -> Color {
    if quality >= 0.8 {
        Color::Green
    } else if quality >= 0.5 {
        Color::Yellow
    } else {
        Color::Red
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_skill(id: &str, name: &str, layer: &str, quality: f64, tags: Vec<&str>) -> SkillSummary {
        SkillSummary {
            id: id.to_string(),
            name: name.to_string(),
            description: format!("Description for {}", name),
            layer: layer.to_string(),
            quality_score: quality,
            tags: tags.into_iter().map(String::from).collect(),
            body: format!("# {}\n\nBody content for {}.", name, name),
        }
    }

    #[test]
    fn test_filter_by_search_query() {
        let skills = vec![
            make_test_skill("rust-errors", "Rust Error Handling", "base", 0.9, vec!["rust", "error"]),
            make_test_skill("python-errors", "Python Error Handling", "base", 0.85, vec!["python", "error"]),
            make_test_skill("rust-async", "Rust Async Patterns", "base", 0.8, vec!["rust", "async"]),
        ];

        let mut app = BrowseTui::with_test_skills(skills);

        app.set_search_query("rust");

        assert_eq!(app.filtered_count(), 2);
    }

    #[test]
    fn test_filter_by_layer() {
        let skills = vec![
            make_test_skill("s1", "Skill 1", "base", 0.9, vec![]),
            make_test_skill("s2", "Skill 2", "project", 0.85, vec![]),
            make_test_skill("s3", "Skill 3", "base", 0.8, vec![]),
        ];

        let mut app = BrowseTui::with_test_skills(skills);

        app.set_search_query("layer:base");

        assert_eq!(app.filtered_count(), 2);
    }

    #[test]
    fn test_filter_by_tag() {
        let skills = vec![
            make_test_skill("s1", "Skill 1", "base", 0.9, vec!["rust"]),
            make_test_skill("s2", "Skill 2", "base", 0.85, vec!["python"]),
            make_test_skill("s3", "Skill 3", "base", 0.8, vec!["rust", "async"]),
        ];

        let mut app = BrowseTui::with_test_skills(skills);

        app.set_search_query("tag:rust");

        assert_eq!(app.filtered_count(), 2);
    }

    #[test]
    fn test_filter_by_quality() {
        let skills = vec![
            make_test_skill("s1", "Skill 1", "base", 0.9, vec![]),
            make_test_skill("s2", "Skill 2", "base", 0.5, vec![]),
            make_test_skill("s3", "Skill 3", "base", 0.85, vec![]),
        ];

        let mut app = BrowseTui::with_test_skills(skills);

        app.set_search_query("quality:>0.8");

        assert_eq!(app.filtered_count(), 2);
    }

    #[test]
    fn test_combined_filters() {
        let skills = vec![
            make_test_skill("s1", "Rust Errors", "base", 0.9, vec!["rust"]),
            make_test_skill("s2", "Python Errors", "base", 0.85, vec!["python"]),
            make_test_skill("s3", "Rust Async", "project", 0.8, vec!["rust"]),
        ];

        let mut app = BrowseTui::with_test_skills(skills);

        // Search for "rust" + layer:base
        app.set_search_query("rust layer:base");

        assert_eq!(app.filtered_count(), 1);
    }

    #[test]
    fn test_navigation_wraps() {
        let skills = vec![
            make_test_skill("s1", "Skill 1", "base", 0.9, vec![]),
            make_test_skill("s2", "Skill 2", "base", 0.85, vec![]),
            make_test_skill("s3", "Skill 3", "base", 0.8, vec![]),
        ];

        let mut app = BrowseTui::with_test_skills(skills);

        // Initially at 0
        assert_eq!(app.selected(), Some(0));

        // Go up at top wraps to bottom
        app.select_prev();
        assert_eq!(app.selected(), Some(2));

        // Go down at bottom wraps to top
        app.select_next();
        assert_eq!(app.selected(), Some(0));
    }

    #[test]
    fn test_focus_toggle() {
        assert_eq!(FocusPanel::List.toggle(), FocusPanel::Detail);
        assert_eq!(FocusPanel::Detail.toggle(), FocusPanel::List);
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 8), "hello...");
    }

    #[test]
    fn test_quality_color() {
        assert_eq!(quality_color(0.9), Color::Green);
        assert_eq!(quality_color(0.6), Color::Yellow);
        assert_eq!(quality_color(0.3), Color::Red);
    }

    #[test]
    fn test_normalize_layer() {
        assert_eq!(normalize_layer("system"), "base");
        assert_eq!(normalize_layer("global"), "org");
        assert_eq!(normalize_layer("local"), "user");
        assert_eq!(normalize_layer("project"), "project");
    }
}
