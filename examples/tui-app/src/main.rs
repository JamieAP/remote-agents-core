//! Example TUI application with ratatui and PTY integration.
//!
//! Run with: cargo run -p tui-app-example
//!
//! This demonstrates the TUI transport bridge for terminal applications.

use std::{io, path::PathBuf, time::Duration};

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
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame, Terminal,
};
use remote_agents_pty::PtyService;
use tokio::sync::mpsc;
use uuid::Uuid;

#[tokio::main]
async fn main() -> io::Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Run app
    let result = run_app(&mut terminal).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(e) = result {
        eprintln!("Error: {e}");
    }

    Ok(())
}

struct App {
    output_lines: Vec<String>,
    input: String,
    scroll: u16,
    session_id: Option<Uuid>,
    status: String,
}

impl App {
    fn new() -> Self {
        Self {
            output_lines: vec![
                "Remote Agents Core - TUI Example".to_string(),
                "================================".to_string(),
                "".to_string(),
                "Starting PTY session...".to_string(),
            ],
            input: String::new(),
            scroll: 0,
            session_id: None,
            status: "Initializing...".to_string(),
        }
    }

    fn add_output(&mut self, text: &str) {
        // Split by newlines and add each line
        for line in text.split('\n') {
            // Strip carriage returns and control sequences for display
            let clean: String = line
                .chars()
                .filter(|c| !c.is_control() || *c == '\t')
                .collect();
            if !clean.is_empty() || !self.output_lines.last().map_or(true, |l| l.is_empty()) {
                self.output_lines.push(clean);
            }
        }
        // Auto-scroll to bottom
        let visible_lines = 20u16; // approximate
        if self.output_lines.len() as u16 > visible_lines {
            self.scroll = (self.output_lines.len() as u16).saturating_sub(visible_lines);
        }
    }

    fn handle_input(&mut self, c: char) {
        self.input.push(c);
    }

    fn handle_backspace(&mut self) {
        self.input.pop();
    }

    fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    fn scroll_down(&mut self) {
        self.scroll = self.scroll.saturating_add(1);
    }
}

async fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
    let mut app = App::new();

    // Create PTY service and session
    let pty_service = PtyService::new();
    let working_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    // Get terminal size
    let size = terminal.size()?;
    let cols = size.width.saturating_sub(2); // Account for borders
    let rows = size.height.saturating_sub(6); // Account for input area and status

    let (session_id, mut pty_output) = match pty_service
        .create_session(working_dir, cols, rows)
        .await
    {
        Ok((id, output)) => {
            app.session_id = Some(id);
            app.status = format!("Connected (session: {})", &id.to_string()[..8]);
            app.add_output("");
            app.add_output("PTY session started. Type commands and press Enter.");
            app.add_output("Press Ctrl+C to quit.");
            app.add_output("");
            (id, output)
        }
        Err(e) => {
            app.status = format!("Failed: {e}");
            app.add_output(&format!("Failed to create PTY session: {e}"));
            // Run in degraded mode without PTY
            loop {
                terminal.draw(|f| ui(f, &app))?;
                if event::poll(Duration::from_millis(100))? {
                    if let Event::Key(KeyEvent {
                        code: KeyCode::Char('c'),
                        modifiers: KeyModifiers::CONTROL,
                        ..
                    }) = event::read()?
                    {
                        return Ok(());
                    }
                }
            }
        }
    };

    // Channel for PTY output
    let (output_tx, mut output_rx) = mpsc::unbounded_channel::<String>();

    // Spawn task to receive PTY output
    tokio::spawn(async move {
        while let Some(data) = pty_output.recv().await {
            if let Ok(text) = String::from_utf8(data) {
                let _ = output_tx.send(text);
            }
        }
    });

    loop {
        // Check for PTY output
        while let Ok(text) = output_rx.try_recv() {
            app.add_output(&text);
        }

        terminal.draw(|f| ui(f, &app))?;

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                match key {
                    KeyEvent {
                        code: KeyCode::Char('c'),
                        modifiers: KeyModifiers::CONTROL,
                        ..
                    } => {
                        // Cleanup PTY session
                        let _ = pty_service.close_session(session_id).await;
                        return Ok(());
                    }
                    KeyEvent {
                        code: KeyCode::Char(c),
                        modifiers: KeyModifiers::NONE | KeyModifiers::SHIFT,
                        ..
                    } => {
                        app.handle_input(c);
                    }
                    KeyEvent {
                        code: KeyCode::Backspace,
                        ..
                    } => {
                        app.handle_backspace();
                    }
                    KeyEvent {
                        code: KeyCode::Enter,
                        ..
                    } => {
                        if !app.input.is_empty() {
                            let input = std::mem::take(&mut app.input);
                            // Send input + newline to PTY
                            let cmd = format!("{}\n", input);
                            if let Err(e) = pty_service.write(session_id, cmd.as_bytes()).await {
                                app.add_output(&format!("[Error sending: {e}]"));
                            }
                        }
                    }
                    KeyEvent {
                        code: KeyCode::Up,
                        modifiers: KeyModifiers::NONE,
                        ..
                    } => app.scroll_up(),
                    KeyEvent {
                        code: KeyCode::Down,
                        modifiers: KeyModifiers::NONE,
                        ..
                    } => app.scroll_down(),
                    KeyEvent {
                        code: KeyCode::PageUp,
                        ..
                    } => {
                        app.scroll = app.scroll.saturating_sub(10);
                    }
                    KeyEvent {
                        code: KeyCode::PageDown,
                        ..
                    } => {
                        app.scroll = app.scroll.saturating_add(10);
                    }
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
    let output_text: Vec<Line> = app
        .output_lines
        .iter()
        .map(|s| Line::from(s.as_str()))
        .collect();

    let output = Paragraph::new(output_text)
        .block(Block::default().borders(Borders::ALL).title("Output"))
        .wrap(Wrap { trim: false })
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
    let status_style = if app.status.starts_with("Connected") {
        Style::default().fg(Color::Green)
    } else if app.status.starts_with("Failed") {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::Yellow)
    };

    let status = Paragraph::new(Line::from(vec![
        Span::raw(" "),
        Span::styled(&app.status, status_style),
        Span::raw(" | "),
        Span::styled("Ctrl+C", Style::default().fg(Color::Yellow)),
        Span::raw(" quit | "),
        Span::styled("Up/Down/PgUp/PgDn", Style::default().fg(Color::Yellow)),
        Span::raw(" scroll "),
    ]));
    f.render_widget(status, chunks[2]);
}
