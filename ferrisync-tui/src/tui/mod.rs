mod app;
mod ui;
pub mod screens;
use ferrisync_core::sync_engine::pairing::PairingManager;
use ferrisync_core::sync_engine::SyncEngine;
use ferrisync_core::DeviceInfo;
use ferrisync_core::storage::Storage;
use std::path::PathBuf;
use std::sync::Arc;
use app::App;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io::stdout;

pub async fn run_tui(
    engine: Arc<SyncEngine>,
    pairing: PairingManager,
    storage: Arc<Storage>,
    device_info: DeviceInfo,
    data_dir: &PathBuf,
) -> anyhow::Result<()> {
    enable_raw_mode()?;
    let mut stdout = stdout();
    stdout.execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(engine, pairing, storage, device_info, data_dir.clone());

    let res = run_loop(&mut terminal, &mut app).await;

    disable_raw_mode()?;
    terminal.backend_mut().execute(LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(e) = res {
        eprintln!("TUI error: {e}");
    }

    Ok(())
}

async fn run_loop(terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>, app: &mut App) -> anyhow::Result<()> {
    loop {
        terminal.draw(|f| ui::render(f, app))?;

        if event::poll(std::time::Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            if app.confirm_quit {
                                return Ok(());
                            }
                            app.confirm_quit = true;
                        }
                        KeyCode::Char('y') if app.confirm_quit => {
                            return Ok(());
                        }
                        KeyCode::Char('n') if app.confirm_quit => {
                            app.confirm_quit = false;
                        }
                        KeyCode::Char('1') => app.set_tab(0),
                        KeyCode::Char('2') => app.set_tab(1),
                        KeyCode::Char('3') => app.set_tab(2),
                        KeyCode::Char('4') => app.set_tab(3),
                        KeyCode::Tab => {
                            let next = (app.active_tab + 1) % 4;
                            app.set_tab(next);
                        }
                        KeyCode::Enter => {
                            app.handle_enter().await;
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        app.refresh().await;
    }
}
