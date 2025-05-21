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
use crate::config_manager::{
    BenchmarkConfigType, ConfigStore, HttpConfigSave, TcpConfigSave, UdsConfigSave,
    get_default_config_path,
};

/// The different pages our TUI can display
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Page {
    Http,
    Tcp,
    Uds,
    Results,
    Configs,
    Help,
}

impl Page {
    fn as_str(&self) -> &'static str {
        match self {
            Page::Http => "HTTP",
            Page::Tcp => "TCP",
            Page::Uds => "UDS",
            Page::Results => "Results",
            Page::Configs => "Configs",
            Page::Help => "Help",
        }
    }

    fn next(&self) -> Self {
        match self {
            Page::Http => Page::Tcp,
            Page::Tcp => Page::Uds,
            Page::Uds => Page::Results,
            Page::Results => Page::Configs,
            Page::Configs => Page::Help,
            Page::Help => Page::Http,
        }
    }

    fn prev(&self) -> Self {
        match self {
            Page::Http => Page::Help,
            Page::Tcp => Page::Http,
            Page::Uds => Page::Tcp,
            Page::Results => Page::Uds,
            Page::Configs => Page::Results,
            Page::Help => Page::Configs,
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
    Normal,    // Like vim's normal mode
    Insert,    // Like vim's insert mode
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum ConfigAction {
    None,
    Save,
    Load,
    Delete,
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
    config_store: ConfigStore,
    config_names: Vec<String>,
    selected_config_index: Option<usize>,
    config_action: ConfigAction,
    config_name_input: String,
}

impl AppState {
    fn new() -> Self {
        // Try to load existing configs
        let config_store = match get_default_config_path() {
            Ok(path) => {
                if path.exists() {
                    match ConfigStore::load(&path) {
                        Ok(store) => store,
                        Err(_) => ConfigStore::new(),
                    }
                } else {
                    ConfigStore::new()
                }
            },
            Err(_) => ConfigStore::new(),
        };

        let config_names = config_store.list();

        Self {
            page: Page::Http,
            http_options: HttpOptions::default(),
            tcp_options: TcpOptions::default(),
            uds_options: UdsOptions::default(),
            focus: FocusField::None,
            mode: AppMode::Normal,
            textarea: TextArea::default(),
            reports: Vec::new(),
            is_running: false,
            current_field_value: String::new(),
            message: None,
            config_store,
            config_names,
            selected_config_index: None,
            config_action: ConfigAction::None,
            config_name_input: String::new(),
        }
    }

    fn save_current_config(&mut self, name: &str) -> Result<()> {
        // Create the appropriate config type based on the current page
        let config = match self.page {
            Page::Http => {
                let http_save = HttpConfigSave {
                    url: self.http_options.url.clone(),
                    method: Some(self.http_options.method.clone()),
                    headers: if self.http_options.headers.is_empty() {
                        None
                    } else {
                        Some(self.http_options.headers.clone())
                    },
                    body: self.http_options.body.clone(),
                    concurrency: Some(self.http_options.concurrency),
                    requests: Some(self.http_options.requests),
                    duration: Some(self.http_options.duration),
                    timeout: Some(self.http_options.timeout),
                    keep_alive: self.http_options.keep_alive,
                };
                BenchmarkConfigType::Http(http_save)
            },
            Page::Tcp => {
                let tcp_save = TcpConfigSave {
                    address: self.tcp_options.address.clone(),
                    data: self.tcp_options.data.clone(),
                    expect: self.tcp_options.expect.clone(),
                    concurrency: Some(self.tcp_options.concurrency),
                    requests: Some(self.tcp_options.requests),
                    duration: Some(self.tcp_options.duration),
                    timeout: Some(self.tcp_options.timeout),
                    keep_alive: self.tcp_options.keep_alive,
                };
                BenchmarkConfigType::Tcp(tcp_save)
            },
            Page::Uds => {
                let uds_save = UdsConfigSave {
                    path: self.uds_options.path.clone(),
                    data: self.uds_options.data.clone(),
                    expect: self.uds_options.expect.clone(),
                    concurrency: Some(self.uds_options.concurrency),
                    requests: Some(self.uds_options.requests),
                    duration: Some(self.uds_options.duration),
                    timeout: Some(self.uds_options.timeout),
                    keep_alive: self.uds_options.keep_alive,
                };
                BenchmarkConfigType::Uds(uds_save)
            },
            _ => return Err(anyhow::anyhow!("Cannot save configuration from this page")),
        };

        // Add the config to the store
        self.config_store.add(name, config);

        // Save the config store to disk
        if let Ok(path) = get_default_config_path() {
            self.config_store.save(path)?;
        }

        // Update the config names list
        self.config_names = self.config_store.list();

        Ok(())
    }

    fn load_config(&mut self, name: &str) -> Result<()> {
        // Get the config from the store
        let config = match self.config_store.get(name) {
            Some(config) => config,
            None => return Err(anyhow::anyhow!("Configuration '{}' not found", name)),
        };

        // Load the config into the appropriate page
        match config {
            BenchmarkConfigType::Http(http_config) => {
                self.http_options.url = http_config.url.clone();
                self.http_options.method = http_config.method.clone().unwrap_or_else(|| "GET".to_string());
                self.http_options.headers = http_config.headers.clone().unwrap_or_default();
                self.http_options.body = http_config.body.clone();
                self.http_options.concurrency = http_config.concurrency.unwrap_or(1);
                self.http_options.requests = http_config.requests.unwrap_or(100);
                self.http_options.duration = http_config.duration.unwrap_or(10);
                self.http_options.timeout = http_config.timeout.unwrap_or(30000);
                self.http_options.keep_alive = http_config.keep_alive;

                // Switch to the HTTP page
                self.page = Page::Http;
            },
            BenchmarkConfigType::Tcp(tcp_config) => {
                self.tcp_options.address = tcp_config.address.clone();
                self.tcp_options.data = tcp_config.data.clone();
                self.tcp_options.expect = tcp_config.expect.clone();
                self.tcp_options.concurrency = tcp_config.concurrency.unwrap_or(1);
                self.tcp_options.requests = tcp_config.requests.unwrap_or(100);
                self.tcp_options.duration = tcp_config.duration.unwrap_or(10);
                self.tcp_options.timeout = tcp_config.timeout.unwrap_or(30000);
                self.tcp_options.keep_alive = tcp_config.keep_alive;

                // Switch to the TCP page
                self.page = Page::Tcp;
            },
            BenchmarkConfigType::Uds(uds_config) => {
                self.uds_options.path = uds_config.path.clone();
                self.uds_options.data = uds_config.data.clone();
                self.uds_options.expect = uds_config.expect.clone();
                self.uds_options.concurrency = uds_config.concurrency.unwrap_or(1);
                self.uds_options.requests = uds_config.requests.unwrap_or(100);
                self.uds_options.duration = uds_config.duration.unwrap_or(10);
                self.uds_options.timeout = uds_config.timeout.unwrap_or(30000);
                self.uds_options.keep_alive = uds_config.keep_alive;

                // Switch to the UDS page
                self.page = Page::Uds;
            },
        }

        Ok(())
    }

    fn delete_config(&mut self, name: &str) -> Result<()> {
        // Remove the config from the store
        if self.config_store.remove(name).is_none() {
            return Err(anyhow::anyhow!("Configuration '{}' not found", name));
        }

        // Save the config store to disk
        if let Ok(path) = get_default_config_path() {
            self.config_store.save(path)?;
        }

        // Update the config names list
        self.config_names = self.config_store.list();

        Ok(())
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
        
        // Make sure cursor is visible after each frame draw
        terminal.show_cursor()?;

        // Handle input
        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                let mut state = app_state.lock().await;
                
                match state.mode {
                    AppMode::Normal => {
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
                            KeyCode::Char('i') => {
                                // Enter insert mode (vim-like)
                                state.mode = AppMode::Insert;
                                
                                // Initialize textarea with value based on focus
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
                                
                                let mut textarea = TextArea::new(vec![state.current_field_value.clone()]);
                                // Configure the textarea for better editing experience
                                textarea.set_hard_tab_indent(false);
                                textarea.set_cursor_line_style(Style::default().add_modifier(Modifier::UNDERLINED));
                                
                                // Use the same title as the field being edited
                                let title = match state.focus {
                                    FocusField::Url => "URL",
                                    FocusField::Method => "Method",
                                    FocusField::Headers => "Headers (key:value)",
                                    FocusField::Body => "Body",
                                    FocusField::Address => "Address (host:port)",
                                    FocusField::Path => "Socket Path",
                                    FocusField::Data => "Data to Send",
                                    FocusField::Expect => "Expected Response (regex)",
                                    FocusField::Concurrency => "Concurrency",
                                    FocusField::Requests => "Requests",
                                    FocusField::Duration => "Duration (seconds)",
                                    FocusField::Timeout => "Timeout (ms)",
                                    FocusField::None => "",
                                };
                                
                                textarea.set_block(Block::default().title(title).borders(Borders::ALL));
                                state.textarea = textarea;
                                
                                // Set cursor to end of text
                                state.textarea.move_cursor(tui_textarea::CursorMove::End);
                            },
                            KeyCode::Enter => {
                                match state.page {
                                    Page::Configs => {
                                        match state.config_action {
                                            ConfigAction::Load => {
                                                if let Some(index) = state.selected_config_index {
                                                    if index < state.config_names.len() {
                                                        let name = state.config_names[index].clone();
                                                        if let Err(e) = state.load_config(&name) {
                                                            state.message = Some(format!("Failed to load config: {}", e));
                                                        } else {
                                                            state.message = Some(format!("Loaded configuration: {}", name));
                                                        }
                                                    }
                                                } else {
                                                    state.message = Some("No configuration selected".to_string());
                                                }
                                                state.config_action = ConfigAction::None;
                                            },
                                            ConfigAction::Save => {
                                                // Start editing the config name
                                                // Keep in normal mode - user needs to press 'i' to edit
                                                state.config_name_input = String::new();
                                                state.message = Some("Press 'i' to enter edit mode".to_string());
                                            },
                                            ConfigAction::Delete => {
                                                if let Some(index) = state.selected_config_index {
                                                    if index < state.config_names.len() {
                                                        let name = state.config_names[index].clone();
                                                        if let Err(e) = state.delete_config(&name) {
                                                            state.message = Some(format!("Failed to delete config: {}", e));
                                                        } else {
                                                            state.message = Some(format!("Deleted configuration: {}", name));
                                                            state.selected_config_index = None;
                                                        }
                                                    }
                                                } else {
                                                    state.message = Some("No configuration selected".to_string());
                                                }
                                                state.config_action = ConfigAction::None;
                                            },
                                            ConfigAction::None => {
                                                // Default to save action when Enter is pressed on Configs page
                                                state.config_action = ConfigAction::Save;
                                                // Keep in normal mode - user needs to press 'i' to edit
                                                state.config_name_input = String::new();
                                                state.message = Some("Press 'i' to enter edit mode".to_string());
                                            },
                                        }
                                    },
                                    _ => {
                                        // Just focus the field but don't enter insert mode yet
                                        // User will need to press 'i' to start editing
                                        state.message = Some("Press 'i' to enter edit mode".to_string());
                                        
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
                                        
                                        let mut textarea = TextArea::new(vec![state.current_field_value.clone()]);
                                        // Configure the textarea for better editing experience
                                        textarea.set_hard_tab_indent(false);
                                        textarea.set_cursor_line_style(Style::default().add_modifier(Modifier::UNDERLINED));
                                        textarea.set_block(Block::default().title(" Editing ").borders(Borders::ALL));
                                        state.textarea = textarea;
                                        // Set cursor to end of text
                                        state.textarea.move_cursor(tui_textarea::CursorMove::End);
                                    }
                                }
                            },
                            _ => {
                                if state.page == Page::Configs {
                                    match key.code {
                                        KeyCode::Up => {
                                            // Navigate up in config list
                                            if let Some(index) = state.selected_config_index {
                                                if index > 0 {
                                                    state.selected_config_index = Some(index - 1);
                                                }
                                            } else if !state.config_names.is_empty() {
                                                state.selected_config_index = Some(state.config_names.len() - 1);
                                            }
                                        },
                                        KeyCode::Down => {
                                            // Navigate down in config list
                                            if let Some(index) = state.selected_config_index {
                                                if index < state.config_names.len() - 1 {
                                                    state.selected_config_index = Some(index + 1);
                                                }
                                            } else if !state.config_names.is_empty() {
                                                state.selected_config_index = Some(0);
                                            }
                                        },
                                        KeyCode::Char('l') | KeyCode::Char('L') => {
                                            state.config_action = ConfigAction::Load;
                                        },
                                        KeyCode::Char('s') | KeyCode::Char('S') => {
                                            state.config_action = ConfigAction::Save;
                                        },
                                        KeyCode::Char('d') | KeyCode::Char('D') => {
                                            state.config_action = ConfigAction::Delete;
                                        },
                                        _ => {}
                                    }
                                } else {
                                    handle_field_navigation(key.code, &mut state);
                                }
                            },
                        }
                    },
                    AppMode::Insert => {
                        match key.code {
                            KeyCode::Esc => {
                                state.mode = AppMode::Normal;
                            },
                            KeyCode::Enter => {
                                if state.page == Page::Configs && state.config_action == ConfigAction::Save {
                                    // Save configuration with entered name
                                    let config_name = state.textarea.lines().join("");
                                    if config_name.is_empty() {
                                        state.message = Some("Please enter a configuration name".to_string());
                                    } else {
                                        if let Err(e) = state.save_current_config(&config_name) {
                                            state.message = Some(format!("Failed to save config: {}", e));
                                        } else {
                                            state.message = Some(format!("Saved configuration: {}", config_name));
                                            state.config_name_input = String::new();
                                            state.config_action = ConfigAction::None;
                                        }
                                    }
                                    state.mode = AppMode::Normal;
                                } else {
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
                                    
                                    state.mode = AppMode::Normal;
                                }
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
        Page::Configs,
        Page::Help].iter().map(|t| {
        Span::styled(t.as_str(), Style::default().fg(Color::White))
    }).collect::<Vec<_>>();

    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).title("ThrustBench Performance Tool"))
        .select(match state.page {
            Page::Http => 0,
            Page::Tcp => 1,
            Page::Uds => 2,
            Page::Results => 3,
            Page::Configs => 4,
            Page::Help => 5,
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
        Page::Configs => render_configs_page(f, chunks[1], &state),
        Page::Help => render_help_page(f, chunks[1]),
    }
    
    // Render the status bar
    let status = match &state.message {
        Some(msg) => msg.clone(),
        None => {
            if state.is_running {
                "Benchmark is running...".to_string()
            } else {
                // Show mode-specific status
                match state.mode {
                    AppMode::Normal => "NORMAL MODE | i: edit | r: run benchmark | q: quit | Tab: switch pages".to_string(),
                    AppMode::Insert => "INSERT MODE | Esc: exit insert mode | Enter: confirm changes".to_string(),
                }
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
    
    // If in insert mode, render the textarea in place of the field
    if let AppMode::Insert = state.mode {
        match state.focus {
            FocusField::Url => {
                let text_area = inner_chunks[0];
                f.render_widget(&state.textarea, text_area);
                
                // Show cursor at position
                let (x, y) = state.textarea.cursor();
                let cursor_x = text_area.x + x as u16 + 1;
                let cursor_y = text_area.y + y as u16 + 1;
                
                // Set cursor position for actual terminal cursor
                f.set_cursor_position((cursor_x, cursor_y));
            },
            FocusField::Method => {
                let text_area = inner_chunks[1];
                f.render_widget(&state.textarea, text_area);
                
                // Show cursor at position
                let (x, y) = state.textarea.cursor();
                let cursor_x = text_area.x + x as u16 + 1;
                let cursor_y = text_area.y + y as u16 + 1;
                
                // Set cursor position for actual terminal cursor
                f.set_cursor_position((cursor_x, cursor_y));
            },
            FocusField::Headers => {
                let text_area = inner_chunks[2];
                f.render_widget(&state.textarea, text_area);
                
                // Show cursor at position
                let (x, y) = state.textarea.cursor();
                let cursor_x = text_area.x + x as u16 + 1;
                let cursor_y = text_area.y + y as u16 + 1;
                
                // Set cursor position for actual terminal cursor
                f.set_cursor_position((cursor_x, cursor_y));
            },
            FocusField::Body => {
                let text_area = inner_chunks[3];
                f.render_widget(&state.textarea, text_area);
                
                // Show cursor at position
                let (x, y) = state.textarea.cursor();
                let cursor_x = text_area.x + x as u16 + 1;
                let cursor_y = text_area.y + y as u16 + 1;
                
                // Set cursor position for actual terminal cursor
                f.set_cursor_position((cursor_x, cursor_y));
            },
            FocusField::Concurrency => {
                let text_area = inner_chunks[4];
                f.render_widget(&state.textarea, text_area);
                
                // Show cursor at position
                let (x, y) = state.textarea.cursor();
                let cursor_x = text_area.x + x as u16 + 1;
                let cursor_y = text_area.y + y as u16 + 1;
                
                // Set cursor position for actual terminal cursor
                f.set_cursor_position((cursor_x, cursor_y));
            },
            FocusField::Requests => {
                let text_area = inner_chunks[5];
                f.render_widget(&state.textarea, text_area);
                
                // Show cursor at position
                let (x, y) = state.textarea.cursor();
                let cursor_x = text_area.x + x as u16 + 1;
                let cursor_y = text_area.y + y as u16 + 1;
                
                // Set cursor position for actual terminal cursor
                f.set_cursor_position((cursor_x, cursor_y));
            },
            FocusField::Duration => {
                let text_area = inner_chunks[6];
                f.render_widget(&state.textarea, text_area);
                
                // Show cursor at position
                let (x, y) = state.textarea.cursor();
                let cursor_x = text_area.x + x as u16 + 1;
                let cursor_y = text_area.y + y as u16 + 1;
                
                // Set cursor position for actual terminal cursor
                f.set_cursor_position((cursor_x, cursor_y));
            },
            FocusField::Timeout => {
                let text_area = inner_chunks[7];
                f.render_widget(&state.textarea, text_area);
                
                // Show cursor at position
                let (x, y) = state.textarea.cursor();
                let cursor_x = text_area.x + x as u16 + 1;
                let cursor_y = text_area.y + y as u16 + 1;
                
                // Set cursor position for actual terminal cursor
                f.set_cursor_position((cursor_x, cursor_y));
            },
            _ => {}
        }
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
    
    // If in insert mode, render the textarea in place of the field
    if let AppMode::Insert = state.mode {
        match state.focus {
            FocusField::Url => {
                let text_area = inner_chunks[0];
                f.render_widget(&state.textarea, text_area);
                
                // Show cursor at position
                let (x, y) = state.textarea.cursor();
                let cursor_x = text_area.x + x as u16 + 1;
                let cursor_y = text_area.y + y as u16 + 1;
                
                // Set cursor position for actual terminal cursor
                f.set_cursor_position((cursor_x, cursor_y));
            },
            FocusField::Method => {
                let text_area = inner_chunks[1];
                f.render_widget(&state.textarea, text_area);
                
                // Show cursor at position
                let (x, y) = state.textarea.cursor();
                let cursor_x = text_area.x + x as u16 + 1;
                let cursor_y = text_area.y + y as u16 + 1;
                
                // Set cursor position for actual terminal cursor
                f.set_cursor_position((cursor_x, cursor_y));
            },
            FocusField::Headers => {
                let text_area = inner_chunks[2];
                f.render_widget(&state.textarea, text_area);
                
                // Show cursor at position
                let (x, y) = state.textarea.cursor();
                let cursor_x = text_area.x + x as u16 + 1;
                let cursor_y = text_area.y + y as u16 + 1;
                
                // Set cursor position for actual terminal cursor
                f.set_cursor_position((cursor_x, cursor_y));
            },
            FocusField::Body => {
                let text_area = inner_chunks[3];
                f.render_widget(&state.textarea, text_area);
                
                // Show cursor at position
                let (x, y) = state.textarea.cursor();
                let cursor_x = text_area.x + x as u16 + 1;
                let cursor_y = text_area.y + y as u16 + 1;
                
                // Set cursor position for actual terminal cursor
                f.set_cursor_position((cursor_x, cursor_y));
            },
            FocusField::Concurrency => {
                let text_area = inner_chunks[4];
                f.render_widget(&state.textarea, text_area);
                
                // Show cursor at position
                let (x, y) = state.textarea.cursor();
                let cursor_x = text_area.x + x as u16 + 1;
                let cursor_y = text_area.y + y as u16 + 1;
                
                // Set cursor position for actual terminal cursor
                f.set_cursor_position((cursor_x, cursor_y));
            },
            FocusField::Requests => {
                let text_area = inner_chunks[5];
                f.render_widget(&state.textarea, text_area);
                
                // Show cursor at position
                let (x, y) = state.textarea.cursor();
                let cursor_x = text_area.x + x as u16 + 1;
                let cursor_y = text_area.y + y as u16 + 1;
                
                // Set cursor position for actual terminal cursor
                f.set_cursor_position((cursor_x, cursor_y));
            },
            FocusField::Duration => {
                let text_area = inner_chunks[6];
                f.render_widget(&state.textarea, text_area);
                
                // Show cursor at position
                let (x, y) = state.textarea.cursor();
                let cursor_x = text_area.x + x as u16 + 1;
                let cursor_y = text_area.y + y as u16 + 1;
                
                // Set cursor position for actual terminal cursor
                f.set_cursor_position((cursor_x, cursor_y));
            },
            FocusField::Timeout => {
                let text_area = inner_chunks[7];
                f.render_widget(&state.textarea, text_area);
                
                // Show cursor at position
                let (x, y) = state.textarea.cursor();
                let cursor_x = text_area.x + x as u16 + 1;
                let cursor_y = text_area.y + y as u16 + 1;
                
                // Set cursor position for actual terminal cursor
                f.set_cursor_position((cursor_x, cursor_y));
            },
            _ => {}
        }
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
    
    // If in insert mode, render the textarea in place of the field
    if let AppMode::Insert = state.mode {
        match state.focus {
            FocusField::Url => {
                let text_area = inner_chunks[0];
                f.render_widget(&state.textarea, text_area);
                
                // Show cursor at position
                let (x, y) = state.textarea.cursor();
                let cursor_x = text_area.x + x as u16 + 1;
                let cursor_y = text_area.y + y as u16 + 1;
                
                // Set cursor position for actual terminal cursor
                f.set_cursor_position((cursor_x, cursor_y));
            },
            FocusField::Method => {
                let text_area = inner_chunks[1];
                f.render_widget(&state.textarea, text_area);
                
                // Show cursor at position
                let (x, y) = state.textarea.cursor();
                let cursor_x = text_area.x + x as u16 + 1;
                let cursor_y = text_area.y + y as u16 + 1;
                
                // Set cursor position for actual terminal cursor
                f.set_cursor_position((cursor_x, cursor_y));
            },
            FocusField::Headers => {
                let text_area = inner_chunks[2];
                f.render_widget(&state.textarea, text_area);
                
                // Show cursor at position
                let (x, y) = state.textarea.cursor();
                let cursor_x = text_area.x + x as u16 + 1;
                let cursor_y = text_area.y + y as u16 + 1;
                
                // Set cursor position for actual terminal cursor
                f.set_cursor_position((cursor_x, cursor_y));
            },
            FocusField::Body => {
                let text_area = inner_chunks[3];
                f.render_widget(&state.textarea, text_area);
                
                // Show cursor at position
                let (x, y) = state.textarea.cursor();
                let cursor_x = text_area.x + x as u16 + 1;
                let cursor_y = text_area.y + y as u16 + 1;
                
                // Set cursor position for actual terminal cursor
                f.set_cursor_position((cursor_x, cursor_y));
            },
            FocusField::Concurrency => {
                let text_area = inner_chunks[4];
                f.render_widget(&state.textarea, text_area);
                
                // Show cursor at position
                let (x, y) = state.textarea.cursor();
                let cursor_x = text_area.x + x as u16 + 1;
                let cursor_y = text_area.y + y as u16 + 1;
                
                // Set cursor position for actual terminal cursor
                f.set_cursor_position((cursor_x, cursor_y));
            },
            FocusField::Requests => {
                let text_area = inner_chunks[5];
                f.render_widget(&state.textarea, text_area);
                
                // Show cursor at position
                let (x, y) = state.textarea.cursor();
                let cursor_x = text_area.x + x as u16 + 1;
                let cursor_y = text_area.y + y as u16 + 1;
                
                // Set cursor position for actual terminal cursor
                f.set_cursor_position((cursor_x, cursor_y));
            },
            FocusField::Duration => {
                let text_area = inner_chunks[6];
                f.render_widget(&state.textarea, text_area);
                
                // Show cursor at position
                let (x, y) = state.textarea.cursor();
                let cursor_x = text_area.x + x as u16 + 1;
                let cursor_y = text_area.y + y as u16 + 1;
                
                // Set cursor position for actual terminal cursor
                f.set_cursor_position((cursor_x, cursor_y));
            },
            FocusField::Timeout => {
                let text_area = inner_chunks[7];
                f.render_widget(&state.textarea, text_area);
                
                // Show cursor at position
                let (x, y) = state.textarea.cursor();
                let cursor_x = text_area.x + x as u16 + 1;
                let cursor_y = text_area.y + y as u16 + 1;
                
                // Set cursor position for actual terminal cursor
                f.set_cursor_position((cursor_x, cursor_y));
            },
            _ => {}
        }
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

fn render_configs_page(
    f: &mut Frame,
    area: Rect,
    state: &AppState,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![
            Constraint::Length(3), // Title
            Constraint::Min(10),   // Config list
            Constraint::Length(3), // Action buttons
            Constraint::Length(3), // Input field (for naming configs)
        ])
        .split(area);

    let configs_block = Block::default()
        .title("Saved Configurations")
        .borders(Borders::ALL);
    f.render_widget(configs_block, area);

    // Title section
    let title = Paragraph::new("Select a configuration to load, or save current settings.")
        .style(Style::default().fg(Color::White));
    f.render_widget(title, chunks[0]);

    // Config list
    if state.config_names.is_empty() {
        let no_configs = Paragraph::new("No saved configurations found.")
            .style(Style::default().fg(Color::Gray))
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(no_configs, chunks[1]);
    } else {
        let configs: Vec<ListItem> = state.config_names.iter().enumerate()
            .map(|(i, name)| {
                let style = if Some(i) == state.selected_config_index {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                // Get the config type
                let config_type = match state.config_store.get(name) {
                    Some(BenchmarkConfigType::Http(_)) => "HTTP",
                    Some(BenchmarkConfigType::Tcp(_)) => "TCP",
                    Some(BenchmarkConfigType::Uds(_)) => "UDS",
                    None => "Unknown",
                };

                ListItem::new(format!("{} [{}]", name, config_type))
                    .style(style)
            })
            .collect();

        let configs_list = List::new(configs)
            .block(Block::default().borders(Borders::ALL));

        f.render_widget(configs_list, chunks[1]);
    }

    // Action buttons
    let action_style = Style::default().fg(Color::White);
    let selected_style = Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD);

    let load_button_style = if state.config_action == ConfigAction::Load {
        selected_style
    } else {
        action_style
    };

    let save_button_style = if state.config_action == ConfigAction::Save {
        selected_style
    } else {
        action_style
    };

    let delete_button_style = if state.config_action == ConfigAction::Delete {
        selected_style
    } else {
        action_style
    };

    let action_buttons = vec![
        Span::styled("[L]oad", load_button_style),
        Span::raw("  "),
        Span::styled("[S]ave", save_button_style),
        Span::raw("  "),
        Span::styled("[D]elete", delete_button_style),
    ];

    let action_paragraph = Paragraph::new(Line::from(action_buttons))
        .block(Block::default().borders(Borders::ALL));

    f.render_widget(action_paragraph, chunks[2]);

    // Config name input field (only shown when saving)
    if state.config_action == ConfigAction::Save {
        let name_input_style = Style::default().fg(Color::Yellow);

        let name_input = Paragraph::new(state.config_name_input.clone())
            .style(name_input_style)
            .block(Block::default().borders(Borders::ALL).title("Configuration name"));

        f.render_widget(name_input, chunks[3]);

        // If we're in insert mode, render the textarea with cursor
        if state.mode == AppMode::Insert {
            let text_area = chunks[3];
            
            // Render the TextArea widget
            f.render_widget(&state.textarea, text_area);
            
            // Show cursor position
            let (x, y) = state.textarea.cursor();
            f.set_cursor_position((text_area.x + x as u16 + 1, text_area.y + y as u16 + 1));
        }
    }
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
        Line::from(" - i: Enter edit mode for the selected field"),
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

