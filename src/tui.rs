use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Tabs, Wrap},
    Terminal, Frame,
};
use std::{io, sync::Arc};
use tokio::sync::Mutex;
use tui_textarea::TextArea;

use crate::report::BenchmarkReport;

/// The different pages our TUI can display
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Page {
    Http,
    Tcp,
    Uds,
    Results,
    Help,
}

impl Page {
    fn as_str(&self) -> &'static str {
        match self {
            Page::Http => "HTTP",
            Page::Tcp => "TCP",
            Page::Uds => "UDS",
            Page::Results => "Results",
            Page::Help => "Help",
        }
    }

    fn next(&self) -> Self {
        match self {
            Page::Http => Page::Tcp,
            Page::Tcp => Page::Uds,
            Page::Uds => Page::Results,
            Page::Results => Page::Help,
            Page::Help => Page::Http,
        }
    }

    fn prev(&self) -> Self {
        match self {
            Page::Http => Page::Help,
            Page::Tcp => Page::Http,
            Page::Uds => Page::Tcp,
            Page::Results => Page::Uds,
            Page::Help => Page::Results,
        }
    }
}

#[derive(Clone)]
struct HttpOptions {
    url: String,
    method: String,
    headers: Vec<String>,
    body: Option<String>,
    concurrency: usize,
    requests: usize,
    duration: u64,
    timeout: u64,
    keep_alive: bool,
}

impl Default for HttpOptions {
    fn default() -> Self {
        Self {
            url: String::new(),
            method: "GET".to_string(),
            headers: Vec::new(),
            body: None,
            concurrency: 1,
            requests: 100,
            duration: 10,
            timeout: 30000,
            keep_alive: false,
        }
    }
}

#[derive(Clone)]
struct TcpOptions {
    address: String,
    data: Option<String>,
    expect: Option<String>,
    concurrency: usize,
    requests: usize,
    duration: u64,
    timeout: u64,
    keep_alive: bool,
}

impl Default for TcpOptions {
    fn default() -> Self {
        Self {
            address: String::new(),
            data: None,
            expect: None,
            concurrency: 1,
            requests: 100,
            duration: 10,
            timeout: 30000,
            keep_alive: false,
        }
    }
}

#[derive(Clone)]
struct UdsOptions {
    path: String,
    data: Option<String>,
    expect: Option<String>,
    concurrency: usize,
    requests: usize,
    duration: u64,
    timeout: u64,
    keep_alive: bool,
}

impl Default for UdsOptions {
    fn default() -> Self {
        Self {
            path: String::new(),
            data: None,
            expect: None,
            concurrency: 1,
            requests: 100,
            duration: 10,
            timeout: 30000,
            keep_alive: false,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum FocusField {
    None,
    Url,
    Method,
    Headers,
    Body,
    Address,
    Path,
    Data,
    Expect,
    Concurrency,
    Requests,
    Duration,
    Timeout,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum AppMode {
    Navigation,
    Editing,
}

struct AppState {
    page: Page,
    http_options: HttpOptions,
    tcp_options: TcpOptions,
    uds_options: UdsOptions,
    focus: FocusField,
    mode: AppMode,
    textarea: TextArea<'static>,
    reports: Vec<BenchmarkReport>,
    is_running: bool,
    current_field_value: String,
    message: Option<String>,
}

impl AppState {
    fn new() -> Self {
        Self {
            page: Page::Http,
            http_options: HttpOptions::default(),
            tcp_options: TcpOptions::default(),
            uds_options: UdsOptions::default(),
            focus: FocusField::None,
            mode: AppMode::Navigation,
            textarea: TextArea::default(),
            reports: Vec::new(),
            is_running: false,
            current_field_value: String::new(),
            message: None,
        }
    }
}

pub async fn run_tui() -> Result<()> {
    // Set up terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app state
    let app_state = Arc::new(Mutex::new(AppState::new()));
    let app_state_clone = app_state.clone();

    // Start the main loop
    let res = run_app(&mut terminal, app_state_clone).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("Error: {}", err);
    }

    Ok(())
}

async fn run_app(
    terminal: &mut Terminal<impl ratatui::backend::Backend>,
    app_state: Arc<Mutex<AppState>>,
) -> Result<()> {
    loop {
        // Draw the UI
        terminal.draw(|f| ui(f, &app_state))?;

        // Handle input
        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                let mut state = app_state.lock().await;
                
                match state.mode {
                    AppMode::Navigation => {
                        match key.code {
                            KeyCode::Char('q') => return Ok(()),
                            KeyCode::Tab => state.page = state.page.next(),
                            KeyCode::BackTab => state.page = state.page.prev(),
                            KeyCode::Right => state.page = state.page.next(),
                            KeyCode::Left => state.page = state.page.prev(),
                            KeyCode::Char('r') => {
                                // Run benchmark
                                if !state.is_running {
                                    let app_state_clone = app_state.clone();
                                    tokio::spawn(async move {
                                        run_benchmark(app_state_clone).await;
                                    });
                                    state.is_running = true;
                                    state.message = Some("Benchmark started...".to_string());
                                }
                            },
                            KeyCode::Enter => {
                                // Enter edit mode for the current field
                                state.mode = AppMode::Editing;
                                
                                // Initialize textarea with current value
                                state.current_field_value = match state.focus {
                                    FocusField::Url => state.http_options.url.clone(),
                                    FocusField::Method => state.http_options.method.clone(),
                                    FocusField::Headers => state.http_options.headers.join("\n"),
                                    FocusField::Body => state.http_options.body.clone().unwrap_or_default(),
                                    FocusField::Address => state.tcp_options.address.clone(),
                                    FocusField::Path => state.uds_options.path.clone(),
                                    FocusField::Data => match state.page {
                                        Page::Tcp => state.tcp_options.data.clone().unwrap_or_default(),
                                        Page::Uds => state.uds_options.data.clone().unwrap_or_default(),
                                        _ => String::new(),
                                    },
                                    FocusField::Expect => match state.page {
                                        Page::Tcp => state.tcp_options.expect.clone().unwrap_or_default(),
                                        Page::Uds => state.uds_options.expect.clone().unwrap_or_default(),
                                        _ => String::new(),
                                    },
                                    FocusField::Concurrency => match state.page {
                                        Page::Http => state.http_options.concurrency.to_string(),
                                        Page::Tcp => state.tcp_options.concurrency.to_string(),
                                        Page::Uds => state.uds_options.concurrency.to_string(),
                                        _ => String::new(),
                                    },
                                    FocusField::Requests => match state.page {
                                        Page::Http => state.http_options.requests.to_string(),
                                        Page::Tcp => state.tcp_options.requests.to_string(),
                                        Page::Uds => state.uds_options.requests.to_string(),
                                        _ => String::new(),
                                    },
                                    FocusField::Duration => match state.page {
                                        Page::Http => state.http_options.duration.to_string(),
                                        Page::Tcp => state.tcp_options.duration.to_string(),
                                        Page::Uds => state.uds_options.duration.to_string(),
                                        _ => String::new(),
                                    },
                                    FocusField::Timeout => match state.page {
                                        Page::Http => state.http_options.timeout.to_string(),
                                        Page::Tcp => state.tcp_options.timeout.to_string(),
                                        Page::Uds => state.uds_options.timeout.to_string(),
                                        _ => String::new(),
                                    },
                                    FocusField::None => String::new(),
                                };
                                
                                state.textarea = TextArea::new(vec![state.current_field_value.clone()]);
                            },
                            _ => handle_field_navigation(key.code, &mut state),
                        }
                    },
                    AppMode::Editing => {
                        match key.code {
                            KeyCode::Esc => {
                                state.mode = AppMode::Navigation;
                            },
                            KeyCode::Enter => {
                                // Save the changes and return to navigation mode
                                let content = state.textarea.lines().join("\n");
                                
                                match state.focus {
                                    FocusField::Url => state.http_options.url = content,
                                    FocusField::Method => state.http_options.method = content,
                                    FocusField::Headers => {
                                        state.http_options.headers = content
                                            .lines()
                                            .map(|s| s.to_string())
                                            .filter(|s| !s.is_empty())
                                            .collect();
                                    },
                                    FocusField::Body => {
                                        state.http_options.body = if content.is_empty() {
                                            None
                                        } else {
                                            Some(content)
                                        };
                                    },
                                    FocusField::Address => state.tcp_options.address = content,
                                    FocusField::Path => state.uds_options.path = content,
                                    FocusField::Data => {
                                        match state.page {
                                            Page::Tcp => state.tcp_options.data = if content.is_empty() {
                                                None
                                            } else {
                                                Some(content)
                                            },
                                            Page::Uds => state.uds_options.data = if content.is_empty() {
                                                None
                                            } else {
                                                Some(content)
                                            },
                                            _ => {}
                                        }
                                    },
                                    FocusField::Expect => {
                                        match state.page {
                                            Page::Tcp => state.tcp_options.expect = if content.is_empty() {
                                                None
                                            } else {
                                                Some(content)
                                            },
                                            Page::Uds => state.uds_options.expect = if content.is_empty() {
                                                None
                                            } else {
                                                Some(content)
                                            },
                                            _ => {}
                                        }
                                    },
                                    FocusField::Concurrency => {
                                        let value = content.parse::<usize>().unwrap_or(1);
                                        match state.page {
                                            Page::Http => state.http_options.concurrency = value,
                                            Page::Tcp => state.tcp_options.concurrency = value,
                                            Page::Uds => state.uds_options.concurrency = value,
                                            _ => {}
                                        }
                                    },
                                    FocusField::Requests => {
                                        let value = content.parse::<usize>().unwrap_or(100);
                                        match state.page {
                                            Page::Http => state.http_options.requests = value,
                                            Page::Tcp => state.tcp_options.requests = value,
                                            Page::Uds => state.uds_options.requests = value,
                                            _ => {}
                                        }
                                    },
                                    FocusField::Duration => {
                                        let value = content.parse::<u64>().unwrap_or(10);
                                        match state.page {
                                            Page::Http => state.http_options.duration = value,
                                            Page::Tcp => state.tcp_options.duration = value,
                                            Page::Uds => state.uds_options.duration = value,
                                            _ => {}
                                        }
                                    },
                                    FocusField::Timeout => {
                                        let value = content.parse::<u64>().unwrap_or(30000);
                                        match state.page {
                                            Page::Http => state.http_options.timeout = value,
                                            Page::Tcp => state.tcp_options.timeout = value,
                                            Page::Uds => state.uds_options.timeout = value,
                                            _ => {}
                                        }
                                    },
                                    FocusField::None => {}
                                }
                                
                                state.mode = AppMode::Navigation;
                            },
                            _ => {
                                if let KeyCode::Char(c) = key.code {
                                    state.textarea.insert_char(c);
                                } else if key.code == KeyCode::Backspace {
                                    state.textarea.delete_char();
                                } else if key.code == KeyCode::Delete {
                                    state.textarea.delete_next_char();
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn handle_field_navigation(key: KeyCode, state: &mut AppState) {
    match key {
        KeyCode::Up | KeyCode::Down => {
            match (state.page, state.focus, key) {
                // HTTP page navigation
                (Page::Http, FocusField::None, KeyCode::Down) => state.focus = FocusField::Url,
                (Page::Http, FocusField::Url, KeyCode::Down) => state.focus = FocusField::Method,
                (Page::Http, FocusField::Method, KeyCode::Down) => state.focus = FocusField::Headers,
                (Page::Http, FocusField::Headers, KeyCode::Down) => state.focus = FocusField::Body,
                (Page::Http, FocusField::Body, KeyCode::Down) => state.focus = FocusField::Concurrency,
                (Page::Http, FocusField::Concurrency, KeyCode::Down) => state.focus = FocusField::Requests,
                (Page::Http, FocusField::Requests, KeyCode::Down) => state.focus = FocusField::Duration,
                (Page::Http, FocusField::Duration, KeyCode::Down) => state.focus = FocusField::Timeout,
                
                (Page::Http, FocusField::Timeout, KeyCode::Up) => state.focus = FocusField::Duration,
                (Page::Http, FocusField::Duration, KeyCode::Up) => state.focus = FocusField::Requests,
                (Page::Http, FocusField::Requests, KeyCode::Up) => state.focus = FocusField::Concurrency,
                (Page::Http, FocusField::Concurrency, KeyCode::Up) => state.focus = FocusField::Body,
                (Page::Http, FocusField::Body, KeyCode::Up) => state.focus = FocusField::Headers,
                (Page::Http, FocusField::Headers, KeyCode::Up) => state.focus = FocusField::Method,
                (Page::Http, FocusField::Method, KeyCode::Up) => state.focus = FocusField::Url,
                (Page::Http, FocusField::Url, KeyCode::Up) => state.focus = FocusField::None,
                
                // TCP page navigation
                (Page::Tcp, FocusField::None, KeyCode::Down) => state.focus = FocusField::Address,
                (Page::Tcp, FocusField::Address, KeyCode::Down) => state.focus = FocusField::Data,
                (Page::Tcp, FocusField::Data, KeyCode::Down) => state.focus = FocusField::Expect,
                (Page::Tcp, FocusField::Expect, KeyCode::Down) => state.focus = FocusField::Concurrency,
                (Page::Tcp, FocusField::Concurrency, KeyCode::Down) => state.focus = FocusField::Requests,
                (Page::Tcp, FocusField::Requests, KeyCode::Down) => state.focus = FocusField::Duration,
                (Page::Tcp, FocusField::Duration, KeyCode::Down) => state.focus = FocusField::Timeout,
                
                (Page::Tcp, FocusField::Timeout, KeyCode::Up) => state.focus = FocusField::Duration,
                (Page::Tcp, FocusField::Duration, KeyCode::Up) => state.focus = FocusField::Requests,
                (Page::Tcp, FocusField::Requests, KeyCode::Up) => state.focus = FocusField::Concurrency,
                (Page::Tcp, FocusField::Concurrency, KeyCode::Up) => state.focus = FocusField::Expect,
                (Page::Tcp, FocusField::Expect, KeyCode::Up) => state.focus = FocusField::Data,
                (Page::Tcp, FocusField::Data, KeyCode::Up) => state.focus = FocusField::Address,
                (Page::Tcp, FocusField::Address, KeyCode::Up) => state.focus = FocusField::None,
                
                // UDS page navigation
                (Page::Uds, FocusField::None, KeyCode::Down) => state.focus = FocusField::Path,
                (Page::Uds, FocusField::Path, KeyCode::Down) => state.focus = FocusField::Data,
                (Page::Uds, FocusField::Data, KeyCode::Down) => state.focus = FocusField::Expect,
                (Page::Uds, FocusField::Expect, KeyCode::Down) => state.focus = FocusField::Concurrency,
                (Page::Uds, FocusField::Concurrency, KeyCode::Down) => state.focus = FocusField::Requests,
                (Page::Uds, FocusField::Requests, KeyCode::Down) => state.focus = FocusField::Duration,
                (Page::Uds, FocusField::Duration, KeyCode::Down) => state.focus = FocusField::Timeout,
                
                (Page::Uds, FocusField::Timeout, KeyCode::Up) => state.focus = FocusField::Duration,
                (Page::Uds, FocusField::Duration, KeyCode::Up) => state.focus = FocusField::Requests,
                (Page::Uds, FocusField::Requests, KeyCode::Up) => state.focus = FocusField::Concurrency,
                (Page::Uds, FocusField::Concurrency, KeyCode::Up) => state.focus = FocusField::Expect,
                (Page::Uds, FocusField::Expect, KeyCode::Up) => state.focus = FocusField::Data,
                (Page::Uds, FocusField::Data, KeyCode::Up) => state.focus = FocusField::Path,
                (Page::Uds, FocusField::Path, KeyCode::Up) => state.focus = FocusField::None,
                
                _ => {}
            }
        },
        _ => {},
    }
}

fn ui(f: &mut Frame, app_state: &Arc<Mutex<AppState>>) {
    // Try to lock state. If we can't, just return and try again next frame
    let Ok(state) = app_state.try_lock() else {
        return;
    };
    
    // Create a layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(f.area());

    // Create the tabs
    let titles = [Page::Http, 
        Page::Tcp, 
        Page::Uds, 
        Page::Results, 
        Page::Help].iter().map(|t| {
        Span::styled(t.as_str(), Style::default().fg(Color::White))
    }).collect::<Vec<_>>();
    
    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).title("Vibe Benchmark Tool"))
        .select(match state.page {
            Page::Http => 0,
            Page::Tcp => 1,
            Page::Uds => 2,
            Page::Results => 3,
            Page::Help => 4,
        })
        .style(Style::default().fg(Color::White))
        .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
        
    f.render_widget(tabs, chunks[0]);
    
    // Render the content based on the current tab
    match state.page {
        Page::Http => render_http_page(f, chunks[1], &state),
        Page::Tcp => render_tcp_page(f, chunks[1], &state),
        Page::Uds => render_uds_page(f, chunks[1], &state),
        Page::Results => render_results_page(f, chunks[1], &state),
        Page::Help => render_help_page(f, chunks[1]),
    }
    
    // Render the status bar
    let status = match &state.message {
        Some(msg) => msg.clone(),
        None => {
            if state.is_running {
                "Benchmark is running...".to_string()
            } else {
                "Press 'r' to run benchmark | 'q' to quit | Tab to switch pages".to_string()
            }
        }
    };
    
    let status_bar = Paragraph::new(status)
        .style(Style::default().fg(Color::White));
        
    f.render_widget(status_bar, chunks[2]);
}

fn render_http_page(
    f: &mut Frame,
    area: Rect,
    state: &AppState,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![
            Constraint::Min(0),
        ])
        .split(area);
    
    let inner_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(5),
            Constraint::Length(5),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
        ])
        .split(chunks[0]);
    
    let http_config = Block::default()
        .title("HTTP Benchmark Configuration")
        .borders(Borders::ALL);
    f.render_widget(http_config, chunks[0]);

    // URL field
    let url_style = if state.focus == FocusField::Url {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    
    let url_widget = Paragraph::new(state.http_options.url.clone())
        .style(url_style)
        .block(Block::default().borders(Borders::ALL).title("URL"));
        
    f.render_widget(url_widget, inner_chunks[0]);
    
    // Method field
    let method_style = if state.focus == FocusField::Method {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    
    let method_widget = Paragraph::new(state.http_options.method.clone())
        .style(method_style)
        .block(Block::default().borders(Borders::ALL).title("Method"));
        
    f.render_widget(method_widget, inner_chunks[1]);
    
    // Headers field
    let headers_style = if state.focus == FocusField::Headers {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    
    let headers = state.http_options.headers.iter()
        .map(|h| ListItem::new(h.as_str()))
        .collect::<Vec<_>>();
        
    let headers_widget = List::new(headers)
        .style(headers_style)
        .block(Block::default().borders(Borders::ALL).title("Headers (key:value)"));
        
    f.render_widget(headers_widget, inner_chunks[2]);
    
    // Body field
    let body_style = if state.focus == FocusField::Body {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    
    let body_content = state.http_options.body.clone().unwrap_or_default();
    let body_widget = Paragraph::new(body_content)
        .style(body_style)
        .block(Block::default().borders(Borders::ALL).title("Body"));
        
    f.render_widget(body_widget, inner_chunks[3]);
    
    // Concurrency field
    let concurrency_style = if state.focus == FocusField::Concurrency {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    
    let concurrency_widget = Paragraph::new(state.http_options.concurrency.to_string())
        .style(concurrency_style)
        .block(Block::default().borders(Borders::ALL).title("Concurrency"));
        
    f.render_widget(concurrency_widget, inner_chunks[4]);
    
    // Requests field
    let requests_style = if state.focus == FocusField::Requests {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    
    let requests_widget = Paragraph::new(state.http_options.requests.to_string())
        .style(requests_style)
        .block(Block::default().borders(Borders::ALL).title("Requests"));
        
    f.render_widget(requests_widget, inner_chunks[5]);
    
    // Duration field
    let duration_style = if state.focus == FocusField::Duration {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    
    let duration_widget = Paragraph::new(state.http_options.duration.to_string())
        .style(duration_style)
        .block(Block::default().borders(Borders::ALL).title("Duration (seconds)"));
        
    f.render_widget(duration_widget, inner_chunks[6]);
    
    // Timeout field
    let timeout_style = if state.focus == FocusField::Timeout {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    
    let timeout_widget = Paragraph::new(state.http_options.timeout.to_string())
        .style(timeout_style)
        .block(Block::default().borders(Borders::ALL).title("Timeout (ms)"));
        
    f.render_widget(timeout_widget, inner_chunks[7]);
    
    // If in edit mode, render the textarea over everything
    if let AppMode::Editing = state.mode {
        f.render_widget(
            &state.textarea,
            centered_rect(60, 20, area)
        );
    }
}

fn render_tcp_page(
    f: &mut Frame,
    area: Rect,
    state: &AppState,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![
            Constraint::Min(0),
        ])
        .split(area);
    
    let inner_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![
            Constraint::Length(3),
            Constraint::Length(5),
            Constraint::Length(5),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
        ])
        .split(chunks[0]);
    
    let tcp_config = Block::default()
        .title("TCP Benchmark Configuration")
        .borders(Borders::ALL);
    f.render_widget(tcp_config, chunks[0]);

    // Address field
    let address_style = if state.focus == FocusField::Address {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    
    let address_widget = Paragraph::new(state.tcp_options.address.clone())
        .style(address_style)
        .block(Block::default().borders(Borders::ALL).title("Address (host:port)"));
        
    f.render_widget(address_widget, inner_chunks[0]);
    
    // Data field
    let data_style = if state.focus == FocusField::Data {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    
    let data_content = state.tcp_options.data.clone().unwrap_or_default();
    let data_widget = Paragraph::new(data_content)
        .style(data_style)
        .block(Block::default().borders(Borders::ALL).title("Data to Send"));
        
    f.render_widget(data_widget, inner_chunks[1]);
    
    // Expect field
    let expect_style = if state.focus == FocusField::Expect {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    
    let expect_content = state.tcp_options.expect.clone().unwrap_or_default();
    let expect_widget = Paragraph::new(expect_content)
        .style(expect_style)
        .block(Block::default().borders(Borders::ALL).title("Expected Response (regex)"));
        
    f.render_widget(expect_widget, inner_chunks[2]);
    
    // Concurrency field
    let concurrency_style = if state.focus == FocusField::Concurrency {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    
    let concurrency_widget = Paragraph::new(state.tcp_options.concurrency.to_string())
        .style(concurrency_style)
        .block(Block::default().borders(Borders::ALL).title("Concurrency"));
        
    f.render_widget(concurrency_widget, inner_chunks[3]);
    
    // Requests field
    let requests_style = if state.focus == FocusField::Requests {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    
    let requests_widget = Paragraph::new(state.tcp_options.requests.to_string())
        .style(requests_style)
        .block(Block::default().borders(Borders::ALL).title("Requests"));
        
    f.render_widget(requests_widget, inner_chunks[4]);
    
    // Duration field
    let duration_style = if state.focus == FocusField::Duration {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    
    let duration_widget = Paragraph::new(state.tcp_options.duration.to_string())
        .style(duration_style)
        .block(Block::default().borders(Borders::ALL).title("Duration (seconds)"));
        
    f.render_widget(duration_widget, inner_chunks[5]);
    
    // Timeout field
    let timeout_style = if state.focus == FocusField::Timeout {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    
    let timeout_widget = Paragraph::new(state.tcp_options.timeout.to_string())
        .style(timeout_style)
        .block(Block::default().borders(Borders::ALL).title("Timeout (ms)"));
        
    f.render_widget(timeout_widget, inner_chunks[6]);
    
    // If in edit mode, render the textarea over everything
    if let AppMode::Editing = state.mode {
        f.render_widget(
            &state.textarea,
            centered_rect(60, 20, area)
        );
    }
}

fn render_uds_page(
    f: &mut Frame,
    area: Rect,
    state: &AppState,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![
            Constraint::Min(0),
        ])
        .split(area);
    
    let inner_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![
            Constraint::Length(3),
            Constraint::Length(5),
            Constraint::Length(5),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
        ])
        .split(chunks[0]);
    
    let uds_config = Block::default()
        .title("Unix Domain Socket Benchmark Configuration")
        .borders(Borders::ALL);
    f.render_widget(uds_config, chunks[0]);

    // Path field
    let path_style = if state.focus == FocusField::Path {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    
    let path_widget = Paragraph::new(state.uds_options.path.clone())
        .style(path_style)
        .block(Block::default().borders(Borders::ALL).title("Socket Path"));
        
    f.render_widget(path_widget, inner_chunks[0]);
    
    // Data field
    let data_style = if state.focus == FocusField::Data {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    
    let data_content = state.uds_options.data.clone().unwrap_or_default();
    let data_widget = Paragraph::new(data_content)
        .style(data_style)
        .block(Block::default().borders(Borders::ALL).title("Data to Send"));
        
    f.render_widget(data_widget, inner_chunks[1]);
    
    // Expect field
    let expect_style = if state.focus == FocusField::Expect {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    
    let expect_content = state.uds_options.expect.clone().unwrap_or_default();
    let expect_widget = Paragraph::new(expect_content)
        .style(expect_style)
        .block(Block::default().borders(Borders::ALL).title("Expected Response (regex)"));
        
    f.render_widget(expect_widget, inner_chunks[2]);
    
    // Concurrency field
    let concurrency_style = if state.focus == FocusField::Concurrency {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    
    let concurrency_widget = Paragraph::new(state.uds_options.concurrency.to_string())
        .style(concurrency_style)
        .block(Block::default().borders(Borders::ALL).title("Concurrency"));
        
    f.render_widget(concurrency_widget, inner_chunks[3]);
    
    // Requests field
    let requests_style = if state.focus == FocusField::Requests {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    
    let requests_widget = Paragraph::new(state.uds_options.requests.to_string())
        .style(requests_style)
        .block(Block::default().borders(Borders::ALL).title("Requests"));
        
    f.render_widget(requests_widget, inner_chunks[4]);
    
    // Duration field
    let duration_style = if state.focus == FocusField::Duration {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    
    let duration_widget = Paragraph::new(state.uds_options.duration.to_string())
        .style(duration_style)
        .block(Block::default().borders(Borders::ALL).title("Duration (seconds)"));
        
    f.render_widget(duration_widget, inner_chunks[5]);
    
    // Timeout field
    let timeout_style = if state.focus == FocusField::Timeout {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    
    let timeout_widget = Paragraph::new(state.uds_options.timeout.to_string())
        .style(timeout_style)
        .block(Block::default().borders(Borders::ALL).title("Timeout (ms)"));
        
    f.render_widget(timeout_widget, inner_chunks[6]);
    
    // If in edit mode, render the textarea over everything
    if let AppMode::Editing = state.mode {
        f.render_widget(
            &state.textarea,
            centered_rect(60, 20, area)
        );
    }
}

fn render_results_page(
    f: &mut Frame,
    area: Rect,
    state: &AppState,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![
            Constraint::Min(0),
        ])
        .split(area);
    
    let results_block = Block::default()
        .title("Benchmark Results")
        .borders(Borders::ALL);
    f.render_widget(results_block, chunks[0]);

    if state.reports.is_empty() {
        let no_results = Paragraph::new("No benchmark results available. Run a benchmark first.")
            .style(Style::default().fg(Color::Gray));
        f.render_widget(no_results, chunks[0]);
        return;
    }

    // Get the latest report
    let report = &state.reports[state.reports.len() - 1];
    
    let content = vec![
        Line::from(vec![
            Span::styled("Target: ", Style::default().fg(Color::White)),
            Span::styled(&report.target, Style::default().fg(Color::Yellow))
        ]),
        Line::from(vec![
            Span::styled("Protocol: ", Style::default().fg(Color::White)),
            Span::styled(&report.protocol, Style::default().fg(Color::Yellow))
        ]),
        Line::from(vec![
            Span::styled("Concurrency: ", Style::default().fg(Color::White)),
            Span::styled(report.concurrency.to_string(), Style::default().fg(Color::Yellow))
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Request Statistics:", Style::default().fg(Color::White).add_modifier(Modifier::BOLD))
        ]),
        Line::from(vec![
            Span::styled("Total Requests: ", Style::default().fg(Color::White)),
            Span::styled(report.total_requests.to_string(), Style::default().fg(Color::Yellow))
        ]),
        Line::from(vec![
            Span::styled("Successful Requests: ", Style::default().fg(Color::White)),
            Span::styled(report.successful_requests.to_string(), Style::default().fg(Color::Green))
        ]),
        Line::from(vec![
            Span::styled("Failed Requests: ", Style::default().fg(Color::White)),
            Span::styled(report.failed_requests.to_string(), Style::default().fg(Color::Red))
        ]),
        Line::from(vec![
            Span::styled("Requests/sec: ", Style::default().fg(Color::White)),
            Span::styled(format!("{:.2}", report.requests_per_second), Style::default().fg(Color::Green))
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Timing Statistics:", Style::default().fg(Color::White).add_modifier(Modifier::BOLD))
        ]),
        Line::from(vec![
            Span::styled("Total Time: ", Style::default().fg(Color::White)),
            Span::styled(format!("{:?}", report.total_time), Style::default().fg(Color::Yellow))
        ]),
        Line::from(vec![
            Span::styled("Average Response Time: ", Style::default().fg(Color::White)),
            Span::styled(format!("{:?}", report.avg_response_time), Style::default().fg(Color::Yellow))
        ]),
        Line::from(vec![
            Span::styled("Min Response Time: ", Style::default().fg(Color::White)),
            Span::styled(format!("{:?}", report.min_response_time), Style::default().fg(Color::Yellow))
        ]),
        Line::from(vec![
            Span::styled("Max Response Time: ", Style::default().fg(Color::White)),
            Span::styled(format!("{:?}", report.max_response_time), Style::default().fg(Color::Yellow))
        ]),
        Line::from(vec![
            Span::styled("p50 Response Time: ", Style::default().fg(Color::White)),
            Span::styled(format!("{:?}", report.p50_response_time), Style::default().fg(Color::Yellow))
        ]),
        Line::from(vec![
            Span::styled("p90 Response Time: ", Style::default().fg(Color::White)),
            Span::styled(format!("{:?}", report.p90_response_time), Style::default().fg(Color::Yellow))
        ]),
        Line::from(vec![
            Span::styled("p95 Response Time: ", Style::default().fg(Color::White)),
            Span::styled(format!("{:?}", report.p95_response_time), Style::default().fg(Color::Yellow))
        ]),
        Line::from(vec![
            Span::styled("p99 Response Time: ", Style::default().fg(Color::White)),
            Span::styled(format!("{:?}", report.p99_response_time), Style::default().fg(Color::Yellow))
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Transfer Statistics:", Style::default().fg(Color::White).add_modifier(Modifier::BOLD))
        ]),
        Line::from(vec![
            Span::styled("Bytes Sent: ", Style::default().fg(Color::White)),
            Span::styled(format!("{} bytes", report.bytes_sent), Style::default().fg(Color::Yellow))
        ]),
        Line::from(vec![
            Span::styled("Bytes Received: ", Style::default().fg(Color::White)),
            Span::styled(format!("{} bytes", report.bytes_received), Style::default().fg(Color::Yellow))
        ]),
    ];
    
    let report_widget = Paragraph::new(content)
        .block(Block::default())
        .wrap(Wrap { trim: true });

    f.render_widget(report_widget, chunks[0]);
}

fn render_help_page(
    f: &mut Frame,
    area: Rect,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![
            Constraint::Min(0),
        ])
        .split(area);
    
    let help_block = Block::default()
        .title("Help")
        .borders(Borders::ALL);
    f.render_widget(help_block, chunks[0]);

    let content = vec![
        Line::from(vec![
            Span::styled("Benchmark Tool Help", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))
        ]),
        Line::from(""),
        Line::from("This tool helps you benchmark HTTP, TCP, and Unix Domain Socket servers."),
        Line::from(""),
        Line::from(vec![
            Span::styled("Navigation:", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
        ]),
        Line::from(" - Tab/Left/Right: Switch between tabs"),
        Line::from(" - Up/Down: Navigate through fields"),
        Line::from(" - Enter: Edit the selected field"),
        Line::from(" - Esc: Exit edit mode"),
        Line::from(" - r: Run the configured benchmark"),
        Line::from(" - q: Quit the application"),
        Line::from(""),
        Line::from(vec![
            Span::styled("Benchmark Types:", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
        ]),
        Line::from(" - HTTP: Benchmark HTTP/HTTPS servers"),
        Line::from(" - TCP: Benchmark TCP servers (like Redis, Valkey)"),
        Line::from(" - UDS: Benchmark Unix Domain Socket servers"),
        Line::from(""),
        Line::from(vec![
            Span::styled("Common Configuration:", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
        ]),
        Line::from(" - Concurrency: Number of concurrent connections"),
        Line::from(" - Requests: Total number of requests to perform"),
        Line::from(" - Duration: Maximum duration of the benchmark in seconds"),
        Line::from(" - Timeout: Timeout for each request in milliseconds"),
        Line::from(""),
        Line::from(vec![
            Span::styled("Results:", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
        ]),
        Line::from(" - The Results tab shows the outcome of the most recent benchmark"),
        Line::from(" - Includes request rate, response times, and transfer statistics"),
    ];
    
    let help_widget = Paragraph::new(content)
        .block(Block::default())
        .wrap(Wrap { trim: true });

    f.render_widget(help_widget, chunks[0]);
}

async fn run_benchmark(app_state: Arc<Mutex<AppState>>) {
    let page;
    let http_options;
    let tcp_options;
    let uds_options;
    
    // Get a copy of the options to work with
    {
        let state = app_state.lock().await;
        page = state.page;
        http_options = state.http_options.clone();
        tcp_options = state.tcp_options.clone();
        uds_options = state.uds_options.clone();
    }
    
    // Run the appropriate benchmark
    let result = match page {
        Page::Http => {
            if http_options.url.is_empty() {
                let mut state = app_state.lock().await;
                state.message = Some("Error: URL cannot be empty".to_string());
                state.is_running = false;
                return;
            }
            
            let config = crate::config::HttpConfig::new(
                http_options.url,
                Some(http_options.method),
                Some(http_options.headers),
                http_options.body.as_deref().map(|s| s.to_string()),
                None, // body_file
                Some(http_options.concurrency),
                Some(http_options.requests),
                Some(http_options.duration),
                Some(http_options.timeout),
                http_options.keep_alive,
            );
            
            let runner = crate::runner::HttpRunner::new(config);
            runner.run().await
        },
        Page::Tcp => {
            if tcp_options.address.is_empty() {
                let mut state = app_state.lock().await;
                state.message = Some("Error: Address cannot be empty".to_string());
                state.is_running = false;
                return;
            }
            
            let config = crate::config::TcpConfig::new(
                tcp_options.address,
                tcp_options.data,
                None, // data_file
                tcp_options.expect,
                Some(tcp_options.concurrency),
                Some(tcp_options.requests),
                Some(tcp_options.duration),
                Some(tcp_options.timeout),
                tcp_options.keep_alive,
            );
            
            let runner = crate::runner::TcpRunner::new(config);
            runner.run().await
        },
        Page::Uds => {
            if uds_options.path.is_empty() {
                let mut state = app_state.lock().await;
                state.message = Some("Error: Socket path cannot be empty".to_string());
                state.is_running = false;
                return;
            }
            
            let config = crate::config::UdsConfig::new(
                std::path::PathBuf::from(uds_options.path),
                uds_options.data,
                None, // data_file
                uds_options.expect,
                Some(uds_options.concurrency),
                Some(uds_options.requests),
                Some(uds_options.duration),
                Some(uds_options.timeout),
                uds_options.keep_alive,
            );
            
            let runner = crate::runner::UdsRunner::new(config);
            runner.run().await
        },
        _ => {
            let mut state = app_state.lock().await;
            state.message = Some("Error: Cannot run benchmark from this page".to_string());
            state.is_running = false;
            return;
        }
    };
    
    // Update the app state with the result
    let mut state = app_state.lock().await;
    state.is_running = false;
    
    match result {
        Ok(report) => {
            state.reports.push(report);
            state.message = Some("Benchmark completed successfully".to_string());
            state.page = Page::Results;
        },
        Err(e) => {
            state.message = Some(format!("Benchmark failed: {}", e));
        }
    }
}

// Helper function for the TUI layout
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}