use crate::{
    embeddings::{default_model, embed_text, resolve_config},
    extractors::parser_status,
    indexer::{search_memory, status},
    settings::set_settings,
};
use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Tabs, Wrap},
};
use std::{collections::HashMap, io::Stdout, time::Duration};

const PROVIDERS: &[&str] = &["local", "openai", "openrouter", "ollama"];
const TABS: &[&str] = &["Config", "Parsers", "Status", "Search"];

#[derive(Default)]
struct App {
    tab: usize,
    provider_state: ListState,
    search_query: String,
    search_results: Vec<String>,
    message: String,
}

pub fn run() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let mut app = App::default();
    app.provider_state.select(Some(0));
    let outcome = main_loop(&mut terminal, &mut app);
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;
    outcome
}

fn main_loop(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|frame| render(frame, app))?;
        if !event::poll(Duration::from_millis(250))? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
            break;
        }
        match (app.tab, key.code) {
            (_, KeyCode::Tab) => app.tab = (app.tab + 1) % TABS.len(),
            (_, KeyCode::BackTab) => app.tab = (app.tab + TABS.len() - 1) % TABS.len(),
            (0, KeyCode::Char('q')) | (1, KeyCode::Char('q')) | (2, KeyCode::Char('q')) => break,
            (0, KeyCode::Up) => move_selection(&mut app.provider_state, -1, PROVIDERS.len()),
            (0, KeyCode::Down) => move_selection(&mut app.provider_state, 1, PROVIDERS.len()),
            (0, KeyCode::Enter) => apply_provider(app),
            (3, KeyCode::Char(c)) => app.search_query.push(c),
            (3, KeyCode::Backspace) => {
                app.search_query.pop();
            }
            (3, KeyCode::Enter) => run_search(app),
            (3, KeyCode::Esc) => app.search_query.clear(),
            _ => {}
        }
    }
    Ok(())
}

fn render(frame: &mut ratatui::Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1), Constraint::Length(3)])
        .split(frame.area());

    let titles: Vec<Line> = TABS.iter().map(|t| Line::from(*t)).collect();
    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).title("local-ai-memory"))
        .select(app.tab)
        .style(Style::default().fg(Color::Gray))
        .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
    frame.render_widget(tabs, chunks[0]);

    match app.tab {
        0 => render_config(frame, chunks[1], app),
        1 => render_parsers(frame, chunks[1]),
        2 => render_status(frame, chunks[1]),
        3 => render_search(frame, chunks[1], app),
        _ => {}
    }

    let footer = Paragraph::new(footer_hint(app))
        .style(Style::default().fg(Color::DarkGray))
        .block(Block::default().borders(Borders::ALL));
    frame.render_widget(footer, chunks[2]);
}

fn footer_hint(app: &App) -> String {
    let base = match app.tab {
        0 => "Tab: switch panel · ↑↓: pick provider · Enter: apply · q: quit",
        1 => "Tab: switch panel · q: quit",
        2 => "Tab: switch panel · q: quit",
        3 => "Tab: switch panel · type query · Enter: search · Esc: clear · Ctrl-C: quit",
        _ => "Tab: switch panel · q: quit",
    };
    if app.message.is_empty() {
        base.to_string()
    } else {
        format!("{} · {}", base, app.message)
    }
}

fn render_config(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);

    let items: Vec<ListItem> = PROVIDERS.iter().map(|p| ListItem::new(*p)).collect();
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Provider"))
        .highlight_style(Style::default().fg(Color::Black).bg(Color::LightYellow).add_modifier(Modifier::BOLD))
        .highlight_symbol("» ");
    let mut state = app.provider_state.clone();
    frame.render_stateful_widget(list, columns[0], &mut state);

    let active = resolve_config(None, &HashMap::new(), true)
        .map(|cfg| {
            format!(
                "Provider: {}\nModel:    {}\nDims:     {}\nBase URL: {}\nAPI key:  {}",
                cfg.provider,
                cfg.model,
                cfg.dimensions.map(|d| d.to_string()).unwrap_or_else(|| "auto".to_string()),
                if cfg.base_url.is_empty() { "(default)".to_string() } else { cfg.base_url },
                if cfg.api_key.is_some() { "set" } else { "missing" }
            )
        })
        .unwrap_or_else(|e| format!("error resolving config: {e}"));
    let detail = Paragraph::new(active)
        .block(Block::default().borders(Borders::ALL).title("Active embedding config"))
        .wrap(Wrap { trim: true });
    frame.render_widget(detail, columns[1]);
}

fn render_parsers(frame: &mut ratatui::Frame, area: Rect) {
    let status = parser_status();
    let text = serde_json::to_string_pretty(&status).unwrap_or_default();
    let widget = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title("Parser status"))
        .wrap(Wrap { trim: false });
    frame.render_widget(widget, area);
}

fn render_status(frame: &mut ratatui::Frame, area: Rect) {
    let text = status(None)
        .map(|s| serde_json::to_string_pretty(&s).unwrap_or_default())
        .unwrap_or_else(|e| format!("error: {e}"));
    let widget = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title("Index status"))
        .wrap(Wrap { trim: false });
    frame.render_widget(widget, area);
}

fn render_search(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    let input = Paragraph::new(Line::from(vec![Span::raw(&app.search_query)]))
        .block(Block::default().borders(Borders::ALL).title("Query"));
    frame.render_widget(input, rows[0]);

    let items: Vec<ListItem> = app
        .search_results
        .iter()
        .map(|line| ListItem::new(line.clone()))
        .collect();
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Results"));
    frame.render_widget(list, rows[1]);
}

fn move_selection(state: &mut ListState, delta: isize, len: usize) {
    if len == 0 {
        return;
    }
    let current = state.selected().unwrap_or(0) as isize;
    let next = (current + delta).rem_euclid(len as isize) as usize;
    state.select(Some(next));
}

fn apply_provider(app: &mut App) {
    let Some(index) = app.provider_state.selected() else {
        return;
    };
    let provider = PROVIDERS[index];
    let values: Vec<(&str, String)> = vec![
        ("embedding.provider", provider.to_string()),
        ("embedding.default_model", default_model(provider).to_string()),
        (
            "embedding.cloud_enabled",
            if provider == "local" { "false".to_string() } else { "true".to_string() },
        ),
    ];
    match set_settings(&values, None) {
        Ok(()) => app.message = format!("provider set to {provider}; reindex required"),
        Err(error) => app.message = format!("error: {error}"),
    }
}

fn run_search(app: &mut App) {
    let query = app.search_query.trim();
    if query.is_empty() {
        app.search_results.clear();
        app.message = "empty query".to_string();
        return;
    }
    // Touch embed_text path so missing API keys surface in the message.
    let _ = embed_text(query, None, &HashMap::new()).err().map(|e| {
        app.message = format!("embed error: {e}");
    });
    match search_memory(query, "low", Some(10), &HashMap::new(), None) {
        Ok(rows) => {
            app.search_results = rows
                .into_iter()
                .enumerate()
                .map(|(i, row)| {
                    let snippet = row.text.split_whitespace().collect::<Vec<_>>().join(" ");
                    let snippet = if snippet.len() > 160 { &snippet[..160] } else { &snippet };
                    format!("{}. {} (score {:.3}) — {}", i + 1, row.citation, row.score, snippet)
                })
                .collect();
            app.message = format!("{} results", app.search_results.len());
        }
        Err(error) => {
            app.search_results.clear();
            app.message = format!("error: {error}");
        }
    }
}
