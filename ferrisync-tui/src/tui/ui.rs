use super::app::App;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Tabs, Wrap};
use ratatui::Frame;

const TAB_NAMES: &[&str] = &[" Dashboard ", " Devices ", " Folders ", " Activity "];

pub fn render(f: &mut Frame, app: &App) {
    let area = f.area();

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);

    render_tabs(f, layout[0], app);
    render_content(f, layout[1], app);
    render_status(f, layout[2], app);
    render_help(f, layout[3], app);
}

fn render_tabs(f: &mut Frame, area: Rect, app: &App) {
    let titles: Vec<Line> = TAB_NAMES
        .iter()
        .enumerate()
        .map(|(i, name)| {
            if i == app.active_tab {
                Line::from(Span::styled(
                    *name,
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ))
            } else {
                Line::from(Span::styled(*name, Style::default().fg(Color::White)))
            }
        })
        .collect();

    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).title(" FerriSync "))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD));
    f.render_widget(tabs, area);
}

fn render_content(f: &mut Frame, area: Rect, app: &App) {
    match app.active_tab {
        0 => render_dashboard(f, area, app),
        1 => render_devices(f, area, app),
        2 => render_folders(f, area, app),
        3 => render_log(f, area, app),
        _ => {}
    }
}

fn render_dashboard(f: &mut Frame, area: Rect, app: &App) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(6), Constraint::Min(1)])
        .split(area);

    let info = vec![
        Line::from(format!("Device ID:  {}", app.device_info.id)),
        Line::from(format!("Device name: {}", app.device_info.name)),
        Line::from(format!(
            "Paired devices: {}",
            app.devices.len()
        )),
        Line::from(format!("Sync folders: {}", app.folders.len())),
        Line::from(format!("Status: {}", app.status_message)),
        Line::from(format!("Data dir: {}", app.data_dir.display())),
    ];

    let info_widget = Paragraph::new(info)
        .block(Block::default().borders(Borders::ALL).title(" System Info "))
        .wrap(Wrap { trim: false });
    f.render_widget(info_widget, layout[0]);

    let devices: Vec<ListItem> = app
        .devices
        .iter()
        .map(|(id, name, last_seen)| {
            let last = last_seen.map_or("never".to_string(), |ts| {
                chrono::DateTime::from_timestamp(ts, 0)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                    .unwrap_or_else(|| ts.to_string())
            });
            ListItem::new(format!("{name} ({id}) — last seen: {last}"))
        })
        .collect();

    let list = List::new(devices)
        .block(Block::default().borders(Borders::ALL).title(" Devices "))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD));
    f.render_widget(list, layout[1]);
}

fn render_devices(f: &mut Frame, area: Rect, app: &App) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(3)])
        .split(area);

    let items: Vec<ListItem> = app
        .devices
        .iter()
        .map(|(id, name, last_seen)| {
            let last = last_seen.map_or("never".to_string(), |ts| {
                chrono::DateTime::from_timestamp(ts, 0)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                    .unwrap_or_else(|| ts.to_string())
            });
            ListItem::new(format!("{name} ({id}) — last seen: {last}"))
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Paired Devices "))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD));
    f.render_widget(list, layout[0]);

    let input = Paragraph::new(format!("Enter IP: {} (press Enter to pair)", app.pairing_ip))
        .block(Block::default().borders(Borders::ALL).title(" Pair New Device "))
        .wrap(Wrap { trim: false });
    f.render_widget(input, layout[1]);
}

fn render_folders(f: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app
        .folders
        .iter()
        .map(|(id, path, dev_id, dir, last_sync)| {
            let last = last_sync.map_or("never".to_string(), |ts| {
                chrono::DateTime::from_timestamp(ts, 0)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                    .unwrap_or_else(|| ts.to_string())
            });
            ListItem::new(format!("[{id}] {path} ↔ {dev_id} ({dir}) — last sync: {last}"))
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Sync Folders "))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD));
    f.render_widget(list, area);
}

fn render_log(f: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app
        .log_entries
        .iter()
        .rev()
        .map(|entry| ListItem::new(entry.clone()))
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Activity Log "))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD));
    f.render_widget(list, area);
}

fn render_status(f: &mut Frame, area: Rect, app: &App) {
    let msg = if app.confirm_quit {
        " Press 'y' to quit, 'n' to cancel "
    } else {
        &app.status_message
    };

    let status = Paragraph::new(Line::from(Span::styled(
        msg,
        Style::default().fg(if app.confirm_quit { Color::Yellow } else { Color::Green }),
    )))
    .block(Block::default().borders(Borders::ALL));
    f.render_widget(status, area);
}

fn render_help(f: &mut Frame, area: Rect, _app: &App) {
    let help = Line::from(Span::styled(
        " [1]Dashboard [2]Devices [3]Folders [4]Log | Tab: Next | Enter: Action | q: Quit ",
        Style::default().fg(Color::DarkGray),
    ));
    let help_widget = Paragraph::new(help);
    f.render_widget(help_widget, area);
}
