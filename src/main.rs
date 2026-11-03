//! `dps150`: argument parsing and wiring of the layers.

use std::io::{self, Stdout};
use std::panic;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use app::{Action, Effect, Key, Model, Outcome, PortInfo, Shutdown};
use device::{Command, Device, LinkConfig, Message};
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
        Ok(status) => ExitCode::from(status),
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(1)
        }
    }
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// Returns the exit status: 0, or 1 when the output could not be confirmed
/// OFF (SAF-11).
fn run(cli: &app::Cli) -> Result<u8, String> {
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
    model.set_safety(config.safety());
    let names_file = app::names_path(env("XDG_CONFIG_HOME").as_deref(), env("HOME").as_deref());
    let names = app::load_names(names_file.as_deref());
    model.set_names(names.names, names.writable);
    // The first notice wins; both files are reported on the first screen.
    if let Some(notice) = loaded.notice.or(names.notice) {
        model.notify(notice);
    }
    // The device thread has already delivered the first snapshot.
    while let Ok(message) = device.messages().try_recv() {
        model.apply(message);
    }

    let stop = Arc::new(AtomicBool::new(false));
    for signal in [
        signal_hook::consts::SIGTERM,
        signal_hook::consts::SIGHUP,
        signal_hook::consts::SIGINT,
    ] {
        signal_hook::flag::register(signal, Arc::clone(&stop))
            .map_err(|e| format!("dps150: cannot handle signals: {e}"))?;
    }

    let mut terminal = TerminalGuard::enter().map_err(|e| format!("dps150: terminal: {e}"))?;
    let outcome = event_loop(
        &mut terminal,
        &mut model,
        &device,
        theme,
        &stop,
        names_file.as_deref(),
    );
    // Leave the alternate screen first: the line below is the last thing
    // written (SAF-11).
    drop(terminal);
    let outcome = outcome.map_err(|e| format!("dps150: {e}"))?;
    if let Some(line) = &outcome.line {
        println!("{line}");
    }
    Ok(outcome.status)
}

/// Starts the quit sequence and sends its first actions (SAF-11, SAF-14).
fn begin_shutdown(
    model: &Model,
    device: &Device,
    now: std::time::Duration,
    force_off: bool,
) -> Shutdown {
    let (shutdown, actions) = model.begin_shutdown(now, force_off);
    run_actions(device, actions);
    shutdown
}

fn run_actions(device: &Device, actions: Vec<Action>) {
    for action in actions {
        match action {
            Action::Write(write) => device.send(Command::Write(write)),
            Action::Close => device.send(Command::Close),
        }
    }
}

fn event_loop(
    terminal: &mut TerminalGuard,
    model: &mut Model,
    device: &Device,
    theme: ui::Theme,
    stop: &AtomicBool,
    names_file: Option<&std::path::Path>,
) -> io::Result<Outcome> {
    let begin = Instant::now();
    let mut last_draw = begin;
    let mut dirty = true;
    // Once a quit has started, the sequence runs while the screen stays up.
    let mut shutdown: Option<Shutdown> = None;
    loop {
        let now = begin.elapsed();
        let before = model.clone();
        model.tick(now);
        if *model != before {
            dirty = true;
        }
        // `SIGTERM`, `SIGHUP`, `SIGINT`: quit without a prompt, by the On exit
        // setting (SAF-11).
        if stop.load(Ordering::Relaxed) && shutdown.is_none() {
            shutdown = Some(begin_shutdown(model, device, now, false));
        }
        while let Ok(message) = device.messages().try_recv() {
            if let (Some(sequence), Message::Event(event)) = (shutdown.as_mut(), &message) {
                run_actions(device, sequence.event(now, event));
            }
            model.apply(message);
            dirty = true;
        }
        if let Some(sequence) = shutdown.as_mut() {
            run_actions(device, sequence.tick(now));
            if let Some(outcome) = sequence.finished() {
                return Ok(outcome.clone());
            }
        } else {
            // Writes no key caused, such as the OFF queued during a link loss,
            // go out first after a reconnect (LNK-04).
            for write in model.drain_writes() {
                device.send(Command::Write(write));
            }
        }
        // Below the floor only OFF and Ctrl-C work, and nothing pending
        // survives (SAF-09, SAF-10).
        if let Ok((cols, rows)) = ratatui::crossterm::terminal::size() {
            model.set_too_small(ui::Tier::select(cols, rows) == ui::Tier::TooSmall);
        }
        if event::poll(FRAME)? {
            match event::read()? {
                Event::Key(key) if key.kind != KeyEventKind::Release => {
                    if let Some(key) = map_key(key) {
                        dirty = true;
                        if let Some(sequence) = shutdown.as_mut() {
                            // Only a second Ctrl-C does anything while quitting:
                            // it stops the wait.
                            if key == Key::CtrlC {
                                sequence.abort();
                            }
                        } else {
                            let effect = model.on_key(key);
                            save_names_if_changed(model, names_file);
                            match effect {
                                Effect::Quit => {
                                    shutdown = Some(begin_shutdown(model, device, now, false));
                                }
                                Effect::QuitOff => {
                                    shutdown = Some(begin_shutdown(model, device, now, true));
                                }
                                Effect::Reconnect => device.send(Command::Reconnect),
                                Effect::Write(write) => device.send(Command::Write(write)),
                                Effect::Writes(writes) => {
                                    for write in writes {
                                        device.send(Command::Write(write));
                                    }
                                }
                                Effect::None => {}
                            }
                        }
                    }
                }
                // A resize re-evaluates the tier at once and keeps the state.
                Event::Resize(..) => dirty = true,
                _ => {}
            }
        }
        if let Some(outcome) = shutdown.as_ref().and_then(Shutdown::finished) {
            return Ok(outcome.clone());
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

/// Writes `presets.toml` after a rename or a clear (CFG-02). A failure is a
/// notice; the names stay for the session (APP-05).
fn save_names_if_changed(model: &mut Model, path: Option<&std::path::Path>) {
    let Some(names) = model.take_names_to_save() else {
        return;
    };
    let Some(path) = path else {
        model.notify("Names not saved: no home directory");
        return;
    };
    if let Err(error) = app::save_names(path, &names) {
        model.notify(format!("Could not save presets.toml: {error}"));
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
