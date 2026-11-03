//! `dps150`: argument parsing and wiring of the layers.

use std::io::{self, Stdout};
use std::panic;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use app::{Effect, Key, Model, PortInfo};
use device::{Command, Device, LinkConfig};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::cursor::{Hide, Show};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

/// A redraw is due at least this often (APP.md §2), and at most about 30 a
/// second: the event wait below is the coalescing window.
const REDRAW: Duration = Duration::from_secs(1);
const FRAME: Duration = Duration::from_millis(33);

fn main() -> ExitCode {
    let cli = match app::parse_args(std::env::args_os()) {
        Ok(cli) => cli,
        // Usage error: status 2; --help and --version: status 0 (APP-06).
        Err(error) => error.exit(),
    };
    match run(&cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(1)
        }
    }
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

fn run(cli: &app::Cli) -> Result<(), String> {
    // Everything that can fail runs before the alternate screen (APP-01,
    // UI-10): there is no start screen.
    if !app::locale::is_utf8(
        env("LC_ALL").as_deref(),
        env("LC_CTYPE").as_deref(),
        env("LANG").as_deref(),
    ) {
        return Err("dps150 needs a UTF-8 terminal".into());
    }
    let path = app::config_path(env("XDG_CONFIG_HOME").as_deref(), env("HOME").as_deref());
    let loaded = app::load_config(path.as_deref());
    let config = loaded.config;

    let ports = serialport::available_ports()
        .unwrap_or_default()
        .into_iter()
        .map(|port| PortInfo {
            usb: match port.port_type {
                serialport::SerialPortType::UsbPort(usb) => Some((usb.vid, usb.pid)),
                _ => None,
            },
            path: port.port_name,
        })
        .collect::<Vec<_>>();
    let port =
        app::resolve_port(cli.port.as_deref(), &config, &ports).map_err(|e| e.to_string())?;

    let link = LinkConfig {
        refresh_hz: config.refresh_hz,
        reconnect: config.reconnect,
    };
    let device = Device::start(&port, link).map_err(|e| e.to_string())?;

    let theme = ui::Theme::new(ui::detect_depth(
        env("NO_COLOR").as_deref(),
        config.colors,
        env("COLORTERM").as_deref(),
        env("TERM").as_deref(),
    ));
    let mut model = Model::new(port, config.reconnect);
    if let Some(notice) = loaded.notice {
        model.notify(notice);
    }
    // The device thread has already delivered the first snapshot.
    while let Ok(message) = device.messages().try_recv() {
        model.apply(message);
    }

    let stop = Arc::new(AtomicBool::new(false));
    for signal in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGHUP] {
        signal_hook::flag::register(signal, Arc::clone(&stop))
            .map_err(|e| format!("dps150: cannot handle signals: {e}"))?;
    }

    let mut terminal = TerminalGuard::enter().map_err(|e| format!("dps150: terminal: {e}"))?;
    event_loop(&mut terminal, &mut model, &device, theme, &stop).map_err(|e| format!("dps150: {e}"))
}

fn event_loop(
    terminal: &mut TerminalGuard,
    model: &mut Model,
    device: &Device,
    theme: ui::Theme,
    stop: &AtomicBool,
) -> io::Result<()> {
    let begin = Instant::now();
    let mut last_draw = begin;
    let mut dirty = true;
    loop {
        let before = model.clone();
        model.tick(begin.elapsed());
        if *model != before {
            dirty = true;
        }
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        while let Ok(message) = device.messages().try_recv() {
            model.apply(message);
            dirty = true;
        }
        if event::poll(FRAME)? {
            match event::read()? {
                Event::Key(key) if key.kind != KeyEventKind::Release => {
                    if let Some(key) = map_key(key) {
                        dirty = true;
                        match model.on_key(key) {
                            Effect::Quit => return Ok(()),
                            Effect::Reconnect => device.send(Command::Reconnect),
                            Effect::Write(write) => device.send(Command::Write(write)),
                            Effect::None => {}
                        }
                    }
                }
                // A resize re-evaluates the tier at once and keeps the state.
                Event::Resize(..) => dirty = true,
                _ => {}
            }
        }
        if dirty || last_draw.elapsed() >= REDRAW {
            terminal
                .0
                .draw(|frame| ui::render(model, frame.buffer_mut(), theme))?;
            last_draw = Instant::now();
            dirty = false;
        }
    }
}

fn map_key(key: KeyEvent) -> Option<Key> {
    let control = key.modifiers.contains(KeyModifiers::CONTROL);
    Some(match key.code {
        KeyCode::Char('c') if control => Key::CtrlC,
        KeyCode::Char('x') if control => Key::CtrlX,
        KeyCode::Char(ch) if !control => Key::Char(ch),
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Tab => Key::Tab,
        KeyCode::Enter => Key::Enter,
        KeyCode::Esc => Key::Esc,
        KeyCode::Backspace => Key::Backspace,
        _ => return None,
    })
}

/// Raw mode, the alternate screen and a hidden cursor, all undone on drop,
/// on a panic and on a signal (UI-11).
struct TerminalGuard(Terminal<CrosstermBackend<Stdout>>);

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        // The hook restores the terminal first, then reports; nothing is sent
        // to the device on a panic (APP-07).
        let previous = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            restore();
            eprintln!("dps150: internal error, output state unknown");
            previous(info);
        }));
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, Hide)?;
        Ok(Self(Terminal::new(CrosstermBackend::new(stdout))?))
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore();
    }
}

fn restore() {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), Show, LeaveAlternateScreen);
}
