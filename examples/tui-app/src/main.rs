//! Example TUI application with ratatui.
//!
//! Run with: cargo run --example tui-app
//!
//! This demonstrates the TUI transport bridge for terminal applications.

use std::io;

use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame, Terminal,
};

fn main() -> io::Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Run app
    let result = run_app(&mut terminal);

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

struct App {
    output: Vec<String>,
    input: String,
    scroll: u16,
}

impl App {
    fn new() -> Self {
        Self {
            output: vec![
                "Remote Agents Core - TUI Example".to_string(),
                "================================".to_string(),
                "".to_string(),
                "This demonstrates the TUI transport bridge.".to_string(),
                "Type a message and press Enter to echo it.".to_string(),
                "Press 'q' or Ctrl+C to quit.".to_string(),
                "".to_string(),
            ],
            input: String::new(),
            scroll: 0,
        }
    }

    fn handle_input(&mut self, c: char) {
        self.input.push(c);
    }

    fn handle_backspace(&mut self) {
        self.input.pop();
    }

    fn handle_enter(&mut self) {
        if !self.input.is_empty() {
            let msg = std::mem::take(&mut self.input);
            self.output.push(format!("> {msg}"));
            // Echo with a simulated response
            self.output.push(format!("  [echo] {msg}"));
        }
    }

    fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    fn scroll_down(&mut self) {
        self.scroll = self.scroll.saturating_add(1);
    }
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
    let mut app = App::new();

    loop {
        terminal.draw(|f| ui(f, &app))?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key {
                    KeyEvent {
                        code: KeyCode::Char('c'),
                        modifiers: KeyModifiers::CONTROL,
                        ..
                    } => return Ok(()),
                    KeyEvent {
                        code: KeyCode::Char('q'),
                        modifiers: KeyModifiers::NONE,
                        ..
                    } if app.input.is_empty() => return Ok(()),
                    KeyEvent {
                        code: KeyCode::Char(c),
                        ..
                    } => app.handle_input(c),
                    KeyEvent {
                        code: KeyCode::Backspace,
                        ..
                    } => app.handle_backspace(),
                    KeyEvent {
                        code: KeyCode::Enter,
                        ..
                    } => app.handle_enter(),
                    KeyEvent {
                        code: KeyCode::Up, ..
                    } => app.scroll_up(),
                    KeyEvent {
                        code: KeyCode::Down,
                        ..
                    } => app.scroll_down(),
                    _ => {}
                }
            }
        }
    }
}

fn ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),    // Output
            Constraint::Length(3), // Input
            Constraint::Length(1), // Status
        ])
        .split(f.area());

    // Output area
    let output_text: Vec<Line> = app.output.iter().map(|s| Line::from(s.as_str())).collect();

    let output = Paragraph::new(output_text)
        .block(Block::default().borders(Borders::ALL).title("Output"))
        .scroll((app.scroll, 0));
    f.render_widget(output, chunks[0]);

    // Input area
    let input = Paragraph::new(app.input.as_str())
        .style(Style::default().fg(Color::Yellow))
        .block(Block::default().borders(Borders::ALL).title("Input"));
    f.render_widget(input, chunks[1]);

    // Set cursor
    f.set_cursor_position((
        chunks[1].x + app.input.len() as u16 + 1,
        chunks[1].y + 1,
    ));

    // Status bar
    let status = Paragraph::new(Line::from(vec![
        Span::raw(" Press "),
        Span::styled("q", Style::default().fg(Color::Yellow)),
        Span::raw(" to quit | "),
        Span::styled("Up/Down", Style::default().fg(Color::Yellow)),
        Span::raw(" to scroll "),
    ]));
    f.render_widget(status, chunks[2]);
}
