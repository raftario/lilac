use anyhow::Context;
use crossterm::event::KeyCode;
use crossterm::{
    event::{self, Event as TerminalEvent, KeyEvent},
    terminal::{EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use lilac::Lilac;
use rayon::prelude::*;
use rodio::{Sink, Source};
use std::{
    io::{self, Write},
    path::{Path, PathBuf},
    process,
    sync::mpsc::{self, Sender},
    thread,
    time::{Duration, Instant},
};
use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout, Rect},
    style::{self, Color, Style},
    widgets, Frame, Terminal,
};

const TICK_RATE: Duration = Duration::from_millis(100);

static BOLD: Style = Style::new().modifier(style::Modifier::BOLD);
static WHITE: Style = Style::new().fg(Color::White);

struct Queue {
    songs: Vec<(Lilac, PathBuf)>,
    cursor: usize,
}
struct QueueEl<'a> {
    idx: usize,
    lilac: &'a Lilac,
}

impl Queue {
    fn new<'a, P>(files: &'a [P]) -> Result<Self, lilac::Error>
    where
        P: AsRef<Path> + Sync,
        &'a [P]: IntoParallelIterator<Item = &'a P>,
    {
        Ok(Self {
            songs: files
                .par_iter()
                .filter_map(|f| match Lilac::read_file(&f) {
                    Ok(l) => Some((l, f.as_ref().to_owned())),
                    Err(e) => {
                        io::stderr().lock().write_fmt(format_args!("{}", e)).ok();
                        None
                    }
                })
                .collect(),
            cursor: 0,
        })
    }
    fn is_empty(&self) -> bool {
        self.songs.is_empty()
    }

    fn current(&self) -> QueueEl {
        let (l, _) = &self.songs[self.cursor];
        QueueEl {
            idx: self.cursor,
            lilac: l,
        }
    }
    fn files(&self) -> Vec<&str> {
        self.songs
            .iter()
            .map(|(_, p)| p.file_stem().unwrap().to_str().unwrap())
            .collect()
    }

    fn next(&mut self) -> bool {
        if self.cursor == self.songs.len() - 1 {
            return false;
        }

        self.cursor += 1;
        true
    }
    fn prev(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }

        self.cursor -= 1;
        true
    }
}

struct Stopwatch {
    time: Duration,
    started: Instant,
    running: bool,
}

impl Stopwatch {
    fn new() -> Self {
        Self {
            time: Duration::new(0, 0),
            started: Instant::now(),
            running: false,
        }
    }

    fn start(&mut self) {
        if self.running {
            return;
        }
        self.running = true;
        self.started = Instant::now();
    }
    fn stop(&mut self) {
        if !self.running {
            return;
        }
        self.running = false;
        self.time += self.started.elapsed();
    }

    fn reset(&mut self) {
        self.time = Duration::new(0, 0);
        self.started = Instant::now();
    }

    fn time(&self) -> Duration {
        if self.running {
            self.time + self.started.elapsed()
        } else {
            self.time
        }
    }
}

pub fn main(files: Vec<String>) -> crate::Result {
    println!("Loading...");
    let mut queue = Queue::new(&files)?;
    if queue.is_empty() {
        return crate::OK;
    }
    let device = rodio::default_output_device().context("No audio output device")?;

    crossterm::terminal::enable_raw_mode()?;

    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    terminal.backend_mut().execute(EnterAlternateScreen)?;
    terminal.hide_cursor()?;

    terminal.clear()?;

    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        if let Err(e) = poll(tx) {
            eprintln!("{:#}", e);
            process::exit(1);
        }
    });

    let mut stopwatch = Stopwatch::new();

    let source = queue.current().lilac.clone().source();
    let mut sink = Sink::new(&device);

    let mut state = State {
        controls: ControlsState {
            playback: PlaybackState {
                playing: false,
                played: Duration::new(0, 0),
                duration: source.total_duration().unwrap(),
            },
            volume: VolumeState(100),
        },
        info: InfoState::read(&queue),
    };

    sink.set_volume(state.controls.volume.0 as f32 / 100.0);
    sink.append(source);
    sink.pause();

    macro_rules! reset {
        () => {{
            sink.stop();
            sink = Sink::new(&device);

            let source = queue.current().lilac.clone().source();
            state.controls.playback.played = Duration::new(0, 0);
            state.controls.playback.duration = source.total_duration().unwrap();
            state.info = InfoState::read(&queue);

            sink.set_volume(state.controls.volume.0 as f32 / 100.0);
            sink.append(source);
            if state.controls.playback.playing {
                sink.play();
            } else {
                sink.pause();
            }

            stopwatch.reset()
        }};
    }

    loop {
        terminal.draw(|mut f| draw(&mut f, &state))?;
        match rx.recv()? {
            Event::Input(KeyEvent { code, .. }) => match code {
                KeyCode::Char(' ') => {
                    state.controls.playback.playing = !state.controls.playback.playing;
                    if state.controls.playback.playing {
                        sink.play();
                        stopwatch.start();
                    } else {
                        sink.pause();
                        stopwatch.stop();
                    }
                }

                KeyCode::Right => {
                    if !queue.next() {
                        continue;
                    }
                    reset!();
                }
                KeyCode::Left => {
                    if stopwatch.time() < Duration::from_secs(2) {
                        queue.prev();
                    }
                    reset!();
                }

                KeyCode::Up => {
                    if state.controls.volume.0 < 100 {
                        state.controls.volume.0 += 1;
                        sink.set_volume(state.controls.volume.0 as f32 / 100.0);
                    }
                }
                KeyCode::Down => {
                    if state.controls.volume.0 > 0 {
                        state.controls.volume.0 -= 1;
                        sink.set_volume(state.controls.volume.0 as f32 / 100.0);
                    }
                }

                KeyCode::Esc => break,
                _ => continue,
            },

            Event::Tick => {
                state.controls.playback.played = stopwatch.time();
                if state.controls.playback.played >= state.controls.playback.duration
                    && state.controls.playback.playing
                {
                    if queue.next() {
                        reset!();
                    } else {
                        while queue.prev() {}

                        state.controls.playback.playing = false;
                        sink.pause();
                        stopwatch.stop();
                        reset!();
                    }
                }
            }
        }
    }

    terminal.show_cursor()?;
    terminal.backend_mut().execute(LeaveAlternateScreen)?;

    crossterm::terminal::disable_raw_mode()?;

    crate::OK
}

enum Event<T> {
    Input(T),
    Tick,
}

fn poll(tx: Sender<Event<KeyEvent>>) -> crate::Result {
    let mut last_tick = Instant::now();
    loop {
        if event::poll(TICK_RATE - last_tick.elapsed())? {
            if let TerminalEvent::Key(k) = event::read()? {
                tx.send(Event::Input(k))?;
                if let KeyEvent {
                    code: KeyCode::Esc, ..
                } = k
                {
                    break crate::OK;
                }
            }
        }
        if last_tick.elapsed() >= TICK_RATE {
            tx.send(Event::Tick)?;
            last_tick = Instant::now();
        }
    }
}

struct State {
    controls: ControlsState,
    info: InfoState,
}
struct ControlsState {
    playback: PlaybackState,
    volume: VolumeState,
}
struct PlaybackState {
    playing: bool,
    played: Duration,
    duration: Duration,
}
struct VolumeState(u16);
struct InfoState {
    metadata: MetadataState,
    queue: QueueState,
}
struct MetadataState {
    title: String,
    artist: String,
    album: String,

    channels: u16,
    sample_rate: u32,
    bit_depth: u32,
}
struct QueueState {
    queue: Vec<String>,
    current: usize,
}

impl InfoState {
    fn read(q: &Queue) -> Self {
        let QueueEl { idx, lilac } = q.current();
        Self {
            metadata: MetadataState::read(lilac),
            queue: QueueState {
                queue: q.files().into_iter().map(ToOwned::to_owned).collect(),
                current: idx,
            },
        }
    }
}
impl MetadataState {
    fn read(l: &Lilac) -> Self {
        Self {
            title: l.title().to_owned(),
            artist: l.artist().to_owned(),
            album: l.album().to_owned(),
            channels: l.channels,
            sample_rate: l.sample_rate,
            bit_depth: l.bit_depth,
        }
    }
}

fn draw<T: Backend>(f: &mut Frame<T>, s: &State) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(5), Constraint::Length(1)].as_ref())
        .vertical_margin(2)
        .split(f.size());

    draw_controls(f, &s.controls, chunks[1]);
    draw_info(f, &s.info, chunks[0]);
}

fn draw_controls<T: Backend>(f: &mut Frame<T>, s: &ControlsState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(75), Constraint::Percentage(25)].as_ref())
        .horizontal_margin(2)
        .split(area);

    draw_playback(f, &s.playback, chunks[0]);
    draw_volume(f, &s.volume, chunks[1]);
}

fn draw_playback<T: Backend>(f: &mut Frame<T>, s: &PlaybackState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [
                Constraint::Length(6),
                Constraint::Min(1),
                Constraint::Length(6),
            ]
            .as_ref(),
        )
        .horizontal_margin(2)
        .split(area);

    let play_pause_text = [widgets::Text::styled(
        if s.playing { "PLAY  " } else { "PAUSE " },
        BOLD,
    )];
    let play_pause = widgets::Paragraph::new(play_pause_text.iter()).wrap(false);
    f.render_widget(play_pause, chunks[0]);

    let timeline = widgets::Gauge::default()
        .ratio((s.played.as_secs_f64() / s.duration.as_secs_f64()).min(1.0))
        .label("")
        .style(WHITE);
    f.render_widget(timeline, chunks[1]);

    let played = s.played.as_secs();
    let timestamp_text = [widgets::Text::styled(
        format!(" {:02}:{:02}", played / 60, played % 60),
        BOLD,
    )];
    let timestamp = widgets::Paragraph::new(timestamp_text.iter()).wrap(false);
    f.render_widget(timestamp, chunks[2]);
}

fn draw_volume<T: Backend>(f: &mut Frame<T>, s: &VolumeState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(1), Constraint::Length(4)].as_ref())
        .horizontal_margin(2)
        .split(area);

    let gauge = widgets::Gauge::default()
        .percent(s.0)
        .label("")
        .style(WHITE);
    f.render_widget(gauge, chunks[0]);

    let level_text = [widgets::Text::Styled(format!(" {:3}", s.0).into(), BOLD)];
    let level = widgets::Paragraph::new(level_text.iter()).wrap(false);
    f.render_widget(level, chunks[1]);
}

fn draw_info<T: Backend>(f: &mut Frame<T>, s: &InfoState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(75), Constraint::Percentage(25)].as_ref())
        .horizontal_margin(4)
        .split(area);

    draw_metadata(f, &s.metadata, chunks[0]);
    draw_queue(f, &s.queue, chunks[1]);
}

fn draw_metadata<T: Backend>(f: &mut Frame<T>, s: &MetadataState, area: Rect) {
    let text = [
        widgets::Text::styled(&s.title, BOLD),
        widgets::Text::raw(format!("\n{}", s.artist)),
        widgets::Text::raw(format!("\n{}", s.album)),
        widgets::Text::raw(format!(
            "\n\n{} bits {} at {} Hz",
            s.bit_depth,
            match s.channels {
                1 => "mono",
                2 => "stereo",
                _ => "polyphonic",
            },
            s.sample_rate,
        )),
    ];
    f.render_widget(widgets::Paragraph::new(text.iter()).wrap(true), area);
}

fn draw_queue<T: Backend>(f: &mut Frame<T>, s: &QueueState, area: Rect) {
    let items = s.queue.iter().map(widgets::Text::raw);
    let mut state = widgets::ListState::default();
    state.select(Some(s.current));
    f.render_stateful_widget(
        widgets::List::new(items).highlight_style(BOLD),
        area,
        &mut state,
    );
}
