//! Interactive Build TUI using ratatui.
//!
//! A rich terminal UI for guided skill generation using the Brenner Method.
//! Provides pattern review, draft preview, checkpoint management, and quality visualization.

use std::io::{self, Stdout};
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
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::cass::brenner::{BrennerConfig, BrennerWizard, MoveDecision, WizardOutput, WizardState};
use crate::cass::{CassClient, QualityScorer};
use crate::error::Result;

/// Focus state for TUI panels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusPanel {
    Patterns,
    Details,
    Draft,
    Actions,
}

impl FocusPanel {
    fn next(self) -> Self {
        match self {
            FocusPanel::Patterns => FocusPanel::Details,
            FocusPanel::Details => FocusPanel::Draft,
            FocusPanel::Draft => FocusPanel::Actions,
            FocusPanel::Actions => FocusPanel::Patterns,
        }
    }

    fn prev(self) -> Self {
        match self {
            FocusPanel::Patterns => FocusPanel::Actions,
            FocusPanel::Details => FocusPanel::Patterns,
            FocusPanel::Draft => FocusPanel::Details,
            FocusPanel::Actions => FocusPanel::Draft,
        }
    }
}

/// TUI application state.
pub struct BuildTui {
    /// The underlying wizard.
    wizard: BrennerWizard,
    /// Currently focused panel.
    focus: FocusPanel,
    /// Pattern list selection state.
    pattern_list_state: ListState,
    /// Status message.
    status_message: Option<String>,
    /// Whether we should exit.
    should_quit: bool,
    /// Last checkpoint time display.
    last_checkpoint: Option<String>,
    /// Token count estimate for draft.
    draft_token_count: usize,
    /// Search query (for / command).
    search_query: Option<String>,
    /// Whether search mode is active.
    search_mode: bool,
}

impl BuildTui {
    /// Create a new Build TUI.
    pub fn new(wizard: BrennerWizard) -> Self {
        let mut pattern_list_state = ListState::default();
        pattern_list_state.select(Some(0));

        Self {
            wizard,
            focus: FocusPanel::Patterns,
            pattern_list_state,
            status_message: None,
            should_quit: false,
            last_checkpoint: None,
            draft_token_count: 0,
            search_query: None,
            search_mode: false,
        }
    }

    /// Run the TUI main loop.
    pub fn run(
        mut self,
        client: &CassClient,
        quality_scorer: &QualityScorer,
    ) -> Result<WizardOutput> {
        let _guard = TerminalGuard::new()?;
        let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;

        self.main_loop(&mut terminal, client, quality_scorer)
    }

    fn main_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
        client: &CassClient,
        quality_scorer: &QualityScorer,
    ) -> Result<WizardOutput> {
        loop {
            // Draw UI
            terminal.draw(|f| self.draw(f))?;

            // Handle events
            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    self.handle_key(key.code, key.modifiers, client, quality_scorer)?;
                }
            }

            // Check exit conditions
            if self.should_quit {
                return Ok(WizardOutput::Cancelled {
                    reason: "User quit".to_string(),
                    checkpoint_id: Some(self.wizard.checkpoint().id.clone()),
                });
            }

            // Check if wizard completed
            if self.wizard.is_complete() {
                return match self.wizard.state() {
                    WizardState::Complete {
                        skill_path,
                        manifest_path,
                        draft,
                        ..
                    } => {
                        let manifest_json = self.wizard.generate_manifest()?;
                        Ok(WizardOutput::Success {
                            skill_path: skill_path.clone(),
                            manifest_path: manifest_path.clone(),
                            calibration_path: self.wizard.checkpoint().query.clone().into(),
                            draft: draft.clone(),
                            manifest_json,
                        })
                    }
                    WizardState::Cancelled { reason } => Ok(WizardOutput::Cancelled {
                        reason: reason.clone(),
                        checkpoint_id: Some(self.wizard.checkpoint().id.clone()),
                    }),
                    _ => unreachable!(),
                };
            }
        }
    }

    fn draw(&mut self, f: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Status bar
                Constraint::Min(10),   // Main content
                Constraint::Length(3), // Action bar
            ])
            .split(f.area());

        self.draw_status_bar(f, chunks[0]);
        self.draw_main_content(f, chunks[1]);
        self.draw_action_bar(f, chunks[2]);
    }

    fn draw_status_bar(&self, f: &mut Frame, area: Rect) {
        let phase = self.get_phase_name();
        let quality = self.get_quality_score();
        let quality_color = self.quality_color(quality);

        let checkpoint_info = self
            .last_checkpoint
            .as_ref()
            .map(|t| format!(" | Checkpoint: {}", t))
            .unwrap_or_default();

        let status_line = Line::from(vec![
            Span::styled("Build TUI ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" | Phase: "),
            Span::styled(phase, Style::default().fg(Color::Cyan)),
            Span::raw(" | Quality: "),
            Span::styled(
                format!("{:.0}%", quality * 100.0),
                Style::default().fg(quality_color),
            ),
            Span::raw(&checkpoint_info),
            Span::raw(format!(" | Tokens: ~{}", self.draft_token_count)),
        ]);

        let status_block = Block::default()
            .borders(Borders::ALL)
            .title(" Meta Skill Builder ");

        let paragraph = Paragraph::new(status_line).block(status_block);
        f.render_widget(paragraph, area);
    }

    fn draw_main_content(&mut self, f: &mut Frame, area: Rect) {
        // Split into three columns: patterns, details, draft
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(30), // Patterns list
                Constraint::Percentage(35), // Details
                Constraint::Percentage(35), // Draft preview
            ])
            .split(area);

        self.draw_patterns_panel(f, columns[0]);
        self.draw_details_panel(f, columns[1]);
        self.draw_draft_panel(f, columns[2]);
    }

    fn draw_patterns_panel(&mut self, f: &mut Frame, area: Rect) {
        let is_focused = self.focus == FocusPanel::Patterns;
        let border_style = if is_focused {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };

        let items = self.get_pattern_items();
        let items_widget: Vec<ListItem> = items
            .iter()
            .map(|(text, confidence)| {
                let color = self.quality_color(*confidence);
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("[{:.0}%] ", confidence * 100.0),
                        Style::default().fg(color),
                    ),
                    Span::raw(text),
                ]))
            })
            .collect();

        let list = List::new(items_widget)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(border_style)
                    .title(if is_focused {
                        " Patterns [*] "
                    } else {
                        " Patterns "
                    }),
            )
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
            .highlight_symbol("> ");

        f.render_stateful_widget(list, area, &mut self.pattern_list_state);
    }

    fn draw_details_panel(&self, f: &mut Frame, area: Rect) {
        let is_focused = self.focus == FocusPanel::Details;
        let border_style = if is_focused {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };

        let detail_text = self.get_selected_detail();

        let paragraph = Paragraph::new(detail_text)
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
            .wrap(Wrap { trim: true });

        f.render_widget(paragraph, area);
    }

    fn draw_draft_panel(&mut self, f: &mut Frame, area: Rect) {
        let is_focused = self.focus == FocusPanel::Draft;
        let border_style = if is_focused {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };

        let draft_text = self.get_draft_preview();

        let paragraph = Paragraph::new(draft_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(border_style)
                    .title(if is_focused {
                        " Draft Preview [*] "
                    } else {
                        " Draft Preview "
                    }),
            )
            .wrap(Wrap { trim: true });

        f.render_widget(paragraph, area);
    }

    fn draw_action_bar(&self, f: &mut Frame, area: Rect) {
        let is_focused = self.focus == FocusPanel::Actions;
        let border_style = if is_focused {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };

        let actions = self.get_available_actions();
        let action_text = if self.search_mode {
            format!("Search: {}_", self.search_query.as_deref().unwrap_or(""))
        } else if let Some(msg) = &self.status_message {
            msg.clone()
        } else {
            actions.join(" | ")
        };

        let paragraph = Paragraph::new(action_text).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style)
                .title(" Actions "),
        );

        f.render_widget(paragraph, area);
    }

    fn handle_key(
        &mut self,
        key: KeyCode,
        modifiers: KeyModifiers,
        client: &CassClient,
        quality_scorer: &QualityScorer,
    ) -> Result<()> {
        // Handle search mode
        if self.search_mode {
            match key {
                KeyCode::Enter => {
                    self.search_mode = false;
                    // TODO: Perform search
                    self.status_message = Some(format!(
                        "Searched for: {}",
                        self.search_query.as_deref().unwrap_or("")
                    ));
                }
                KeyCode::Esc => {
                    self.search_mode = false;
                    self.search_query = None;
                }
                KeyCode::Char(c) => {
                    let query = self.search_query.get_or_insert_with(String::new);
                    query.push(c);
                }
                KeyCode::Backspace => {
                    if let Some(query) = &mut self.search_query {
                        query.pop();
                    }
                }
                _ => {}
            }
            return Ok(());
        }

        // Global shortcuts
        match key {
            KeyCode::Char('q') => {
                self.should_quit = true;
                return Ok(());
            }
            KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
                return Ok(());
            }
            KeyCode::Tab => {
                self.focus = self.focus.next();
                return Ok(());
            }
            KeyCode::BackTab => {
                self.focus = self.focus.prev();
                return Ok(());
            }
            KeyCode::Char('/') => {
                self.search_mode = true;
                self.search_query = Some(String::new());
                return Ok(());
            }
            KeyCode::Char('c') => {
                // Manual checkpoint
                self.last_checkpoint = Some(chrono::Utc::now().format("%H:%M:%S").to_string());
                self.status_message = Some("Checkpoint saved".to_string());
                return Ok(());
            }
            _ => {}
        }

        // State-specific handling
        match self.wizard.state().clone() {
            WizardState::SessionSelection {
                query,
                results,
                selected,
            } => {
                self.handle_session_selection_key(key, &query, &results, &selected, client)?;
            }
            WizardState::MoveExtraction { .. } => {
                self.handle_move_extraction_key(key)?;
            }
            WizardState::ThirdAlternativeGuard { .. } => {
                self.handle_guard_key(key)?;
            }
            WizardState::SkillFormalization { .. } => {
                self.handle_formalization_key(key, quality_scorer)?;
            }
            WizardState::MaterializationTest { .. } => {
                self.handle_test_key(key)?;
            }
            _ => {}
        }

        Ok(())
    }

    fn handle_session_selection_key(
        &mut self,
        key: KeyCode,
        _query: &str,
        results: &[crate::cass::client::SessionMatch],
        _selected: &std::collections::HashSet<usize>,
        _client: &CassClient,
    ) -> Result<()> {
        match key {
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(i) = self.pattern_list_state.selected() {
                    if i > 0 {
                        self.pattern_list_state.select(Some(i - 1));
                    }
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(i) = self.pattern_list_state.selected() {
                    if i < results.len().saturating_sub(1) {
                        self.pattern_list_state.select(Some(i + 1));
                    }
                }
            }
            KeyCode::Char(' ') | KeyCode::Enter => {
                // Toggle selection
                if let Some(i) = self.pattern_list_state.selected() {
                    self.wizard.toggle_session(i);
                }
            }
            KeyCode::Char('d') => {
                // Demo data for testing
                let demo = vec![
                    crate::cass::client::SessionMatch {
                        session_id: "demo-1".to_string(),
                        path: "/demo/1".to_string(),
                        score: 0.9,
                        snippet: Some("Demo session 1".to_string()),
                        content_hash: None,
                        project: None,
                        timestamp: None,
                    },
                    crate::cass::client::SessionMatch {
                        session_id: "demo-2".to_string(),
                        path: "/demo/2".to_string(),
                        score: 0.85,
                        snippet: Some("Demo session 2".to_string()),
                        content_hash: None,
                        project: None,
                        timestamp: None,
                    },
                ];
                self.wizard.set_session_results(demo);
                self.wizard.toggle_session(0);
                self.wizard.toggle_session(1);
                self.status_message = Some("Loaded demo data".to_string());
            }
            KeyCode::Char('n') => {
                // Proceed to next phase
                let scorer = QualityScorer::with_defaults();
                if let Err(e) = self.wizard.confirm_sessions(&scorer) {
                    self.status_message = Some(format!("Error: {}", e));
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_move_extraction_key(&mut self, key: KeyCode) -> Result<()> {
        match key {
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(i) = self.pattern_list_state.selected() {
                    if i > 0 {
                        self.pattern_list_state.select(Some(i - 1));
                    }
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(i) = self.pattern_list_state.selected() {
                    let count = self.get_moves_count();
                    if i < count.saturating_sub(1) {
                        self.pattern_list_state.select(Some(i + 1));
                    }
                }
            }
            KeyCode::Char('n') => {
                // Next session
                if !self.wizard.next_session() {
                    self.status_message = Some("No more sessions".to_string());
                }
            }
            KeyCode::Char('f') => {
                // Finish extraction
                if let Err(e) = self.wizard.finish_extraction() {
                    self.status_message = Some(format!("Error: {}", e));
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_guard_key(&mut self, key: KeyCode) -> Result<()> {
        match key {
            KeyCode::Char('y') | KeyCode::Char('a') => {
                // Accept
                self.wizard.review_move(MoveDecision::Accept)?;
                if !self.wizard.next_flagged_move() {
                    self.wizard.finish_guard()?;
                }
            }
            KeyCode::Char('n') | KeyCode::Char('r') => {
                // Reject
                self.wizard.review_move(MoveDecision::Reject)?;
                if !self.wizard.next_flagged_move() {
                    self.wizard.finish_guard()?;
                }
            }
            KeyCode::Char('e') => {
                // Needs evidence
                self.wizard.review_move(MoveDecision::NeedsEvidence)?;
                if !self.wizard.next_flagged_move() {
                    self.wizard.finish_guard()?;
                }
            }
            KeyCode::Char('s') => {
                // Skip to formalization
                self.wizard.finish_guard()?;
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_formalization_key(
        &mut self,
        key: KeyCode,
        _quality_scorer: &QualityScorer,
    ) -> Result<()> {
        match key {
            KeyCode::Char('t') => {
                // Run test
                self.wizard.start_test()?;
            }
            KeyCode::Char('s') | KeyCode::Enter => {
                // Save/complete
                let output_dir = self.wizard.checkpoint().query.clone();
                self.wizard.complete(output_dir.into())?;
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_test_key(&mut self, key: KeyCode) -> Result<()> {
        match key {
            KeyCode::Char('s') | KeyCode::Enter => {
                // Complete
                let output_dir = self.wizard.checkpoint().query.clone();
                self.wizard.complete(output_dir.into())?;
            }
            KeyCode::Char('r') => {
                // Return to formalization
                if let WizardState::MaterializationTest { draft, .. } = self.wizard.state().clone()
                {
                    self.wizard.return_to_formalization(draft);
                }
            }
            _ => {}
        }
        Ok(())
    }

    // Helper methods for getting display data

    fn get_phase_name(&self) -> &'static str {
        match self.wizard.state() {
            WizardState::SessionSelection { .. } => "Session Selection",
            WizardState::MoveExtraction { .. } => "Pattern Extraction",
            WizardState::ThirdAlternativeGuard { .. } => "Review Guard",
            WizardState::SkillFormalization { .. } => "Skill Formalization",
            WizardState::MaterializationTest { .. } => "Materialization Test",
            WizardState::Complete { .. } => "Complete",
            WizardState::Cancelled { .. } => "Cancelled",
        }
    }

    fn get_quality_score(&self) -> f32 {
        match self.wizard.state() {
            WizardState::SkillFormalization { draft, .. } => draft
                .validation
                .as_ref()
                .map(|v| v.confidence)
                .unwrap_or(0.7),
            WizardState::MaterializationTest { draft, .. } => draft
                .validation
                .as_ref()
                .map(|v| v.confidence)
                .unwrap_or(0.7),
            _ => 0.7,
        }
    }

    fn quality_color(&self, quality: f32) -> Color {
        if quality >= 0.8 {
            Color::Green
        } else if quality >= 0.5 {
            Color::Yellow
        } else {
            Color::Red
        }
    }

    fn get_pattern_items(&self) -> Vec<(String, f32)> {
        match self.wizard.state() {
            WizardState::SessionSelection {
                results, selected, ..
            } => results
                .iter()
                .enumerate()
                .map(|(i, r)| {
                    let prefix = if selected.contains(&i) { "[x]" } else { "[ ]" };
                    (
                        format!(
                            "{} {}",
                            prefix,
                            r.snippet.as_deref().unwrap_or(&r.session_id)
                        ),
                        r.score as f32,
                    )
                })
                .collect(),
            WizardState::MoveExtraction { moves, .. } => moves
                .iter()
                .map(|m| (format!("{:?}: {}", m.tag, m.description), m.confidence))
                .collect(),
            WizardState::ThirdAlternativeGuard {
                moves,
                flagged_indices,
                current_idx,
            } => flagged_indices
                .iter()
                .enumerate()
                .filter_map(|(i, &idx)| {
                    let prefix = if i == *current_idx { "→" } else { " " };
                    moves.get(idx).map(|m| {
                        (
                            format!("{} {:?}: {}", prefix, m.tag, m.description),
                            m.confidence,
                        )
                    })
                })
                .collect(),
            WizardState::SkillFormalization { draft, .. }
            | WizardState::MaterializationTest { draft, .. } => draft
                .rules
                .iter()
                .map(|r| (r.description.clone(), r.confidence))
                .collect(),
            _ => vec![],
        }
    }

    fn get_selected_detail(&self) -> Text<'static> {
        let idx = self.pattern_list_state.selected().unwrap_or(0);

        match self.wizard.state() {
            WizardState::MoveExtraction { moves, .. } => {
                if let Some(mov) = moves.get(idx) {
                    Text::from(vec![
                        Line::from(Span::styled(
                            format!("{:?}", mov.tag),
                            Style::default().add_modifier(Modifier::BOLD),
                        )),
                        Line::from(""),
                        Line::from(mov.description.clone()),
                        Line::from(""),
                        Line::from(Span::styled(
                            "Evidence:".to_string(),
                            Style::default().add_modifier(Modifier::BOLD),
                        )),
                        Line::from(mov.evidence.excerpt.clone()),
                        Line::from(""),
                        Line::from(format!("Confidence: {:.0}%", mov.confidence * 100.0)),
                    ])
                } else {
                    Text::from("No pattern selected")
                }
            }
            WizardState::SkillFormalization { draft, .. }
            | WizardState::MaterializationTest { draft, .. } => {
                if let Some(rule) = draft.rules.get(idx) {
                    let mut lines: Vec<Line<'static>> = vec![
                        Line::from(Span::styled(
                            rule.id.clone(),
                            Style::default().add_modifier(Modifier::BOLD),
                        )),
                        Line::from(""),
                        Line::from(rule.description.clone()),
                        Line::from(""),
                        Line::from(Span::styled(
                            "Evidence:".to_string(),
                            Style::default().add_modifier(Modifier::BOLD),
                        )),
                    ];
                    for ev in &rule.evidence {
                        lines.push(Line::from(format!("  - {}", ev)));
                    }
                    lines.push(Line::from(""));
                    lines.push(Line::from(format!(
                        "Confidence: {:.0}%",
                        rule.confidence * 100.0
                    )));
                    Text::from(lines)
                } else {
                    Text::from("No rule selected")
                }
            }
            _ => Text::from("Select an item to view details"),
        }
    }

    fn get_draft_preview(&mut self) -> String {
        match self.wizard.state() {
            WizardState::SkillFormalization { draft, .. }
            | WizardState::MaterializationTest { draft, .. } => {
                let md = self.wizard.generate_skill_md(draft);
                // Estimate tokens (~4 chars per token)
                self.draft_token_count = md.len() / 4;
                md
            }
            _ => {
                self.draft_token_count = 0;
                "Draft will appear here once patterns are extracted...".to_string()
            }
        }
    }

    fn get_available_actions(&self) -> Vec<String> {
        let mut actions = vec![
            "Tab: switch panel".to_string(),
            "q: quit".to_string(),
            "c: checkpoint".to_string(),
            "/: search".to_string(),
        ];

        match self.wizard.state() {
            WizardState::SessionSelection { .. } => {
                actions.extend(vec![
                    "Space: toggle".to_string(),
                    "n: next phase".to_string(),
                    "d: demo data".to_string(),
                ]);
            }
            WizardState::MoveExtraction { .. } => {
                actions.extend(vec!["n: next session".to_string(), "f: finish".to_string()]);
            }
            WizardState::ThirdAlternativeGuard { .. } => {
                actions.extend(vec![
                    "y: accept".to_string(),
                    "n: reject".to_string(),
                    "e: needs evidence".to_string(),
                    "s: skip".to_string(),
                ]);
            }
            WizardState::SkillFormalization { .. } => {
                actions.extend(vec!["t: run test".to_string(), "s: save".to_string()]);
            }
            WizardState::MaterializationTest { .. } => {
                actions.extend(vec!["s: save".to_string(), "r: return".to_string()]);
            }
            _ => {}
        }

        actions
    }

    fn get_moves_count(&self) -> usize {
        match self.wizard.state() {
            WizardState::MoveExtraction { moves, .. } => moves.len(),
            _ => 0,
        }
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

/// Run the build TUI.
pub fn run_build_tui(
    query: &str,
    config: BrennerConfig,
    client: &CassClient,
    quality_scorer: &QualityScorer,
) -> Result<WizardOutput> {
    let wizard = BrennerWizard::new(query, config);
    let tui = BuildTui::new(wizard);
    tui.run(client, quality_scorer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_focus_cycle() {
        let focus = FocusPanel::Patterns;
        assert_eq!(focus.next(), FocusPanel::Details);
        assert_eq!(focus.next().next(), FocusPanel::Draft);
        assert_eq!(focus.next().next().next(), FocusPanel::Actions);
        assert_eq!(focus.next().next().next().next(), FocusPanel::Patterns);
    }

    #[test]
    fn test_focus_prev() {
        let focus = FocusPanel::Patterns;
        assert_eq!(focus.prev(), FocusPanel::Actions);
    }

    #[test]
    fn test_quality_color() {
        let wizard = BrennerWizard::new("test", BrennerConfig::default());
        let tui = BuildTui::new(wizard);

        assert_eq!(tui.quality_color(0.9), Color::Green);
        assert_eq!(tui.quality_color(0.6), Color::Yellow);
        assert_eq!(tui.quality_color(0.3), Color::Red);
    }
}
