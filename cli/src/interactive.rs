use anyhow::Context;
use console::{Alignment, Key, Term};
use lilac::Lilac;
use rayon::prelude::*;
use rodio::{Sink, Source};
use std::{
    io::Write,
    process,
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::{Duration, Instant},
};

const PLAY: &str = "PLAY";
const PAUSE: &str = "PAUSE";

const BLOCKS: [char; 8] = ['▏', '▎', '▍', '▌', '▋', '▊', '▉', '█'];

pub fn main(queue: Vec<String>) -> crate::Result {
    if queue.is_empty() {
        return crate::OK;
    }

    let mut term = Term::stdout();
    let (rows, cols) = match (term.size_checked(), term.features().is_attended()) {
        (Some((r, c)), true) => (r as usize, c as usize),
        _ => return crate::OK,
    };

    term.hide_cursor()?;
    term.clear_screen()?;
    term.write_line("Loading...")?;

    let queue: Vec<Lilac> = queue
        .into_par_iter()
        .map(Lilac::read_file)
        .collect::<Result<_, _>>()?;
    let mut idx = 0;
    let mut current = &queue[idx];

    term.hide_cursor()?;
    term.clear_screen()?;

    let first_row = 1;
    let first_col = 2;
    let last_row = rows - 2;
    let last_col = cols - 3;
    let usable_cols = last_col - first_col;

    let play_pause_pos = (first_col, last_row);
    let play_pos_len = PLAY.len().max(PAUSE.len());

    let progress_pos = (play_pause_pos.0 + play_pos_len + 1, last_row);
    let progress_len = usable_cols - play_pos_len - 1 - 1 - 5; // play/pause + padding -- padding + timestamp

    let timestamp_pos = (progress_pos.0 + progress_len + 2, last_row);

    let meta_pos = (first_col, first_row);
    let meta_len = usable_cols - 10;

    let volume_pos = (usable_cols - 9, first_row);

    let (tx, rx) = mpsc::channel();
    let progress_term = term.clone();
    thread::spawn(move || {
        if let Err(e) = progress_fn(rx, progress_term, progress_pos, progress_len, timestamp_pos) {
            eprintln!("{:#}", e);
            process::exit(1);
        }
    });

    let device = rodio::default_output_device().context("no audio device")?;
    let mut sink = Sink::new(&device);
    sink.set_volume(1.0);
    sink.pause();
    let mut paused = true;

    print_meta(
        current.title(),
        current.artist(),
        current.album(),
        &mut term,
        meta_pos,
        meta_len,
        (idx, queue.len()),
    )?;

    term.move_cursor_to(play_pause_pos.0, play_pause_pos.1)?;
    term.write_fmt(format_args!("{:width$}", PLAY, width = play_pos_len))?;

    let mut volume = 1.0;
    print_volume(volume, volume_pos, &mut term)?;

    let source = current.clone().source();
    tx.send(ProgressEvent::Reset(source.total_duration().unwrap()))?;
    sink.append(source);

    macro_rules! skip {
        () => {{
            current = &queue[idx];

            print_meta(
                current.title(),
                current.artist(),
                current.album(),
                &mut term,
                meta_pos,
                meta_len,
                (idx, queue.len()),
            )?;

            let source = current.clone().source();
            tx.send(ProgressEvent::Reset(source.total_duration().unwrap()))?;
            sink.stop();
            sink = Sink::new(&device);
            sink.append(source);
            sink.pause();

            if !paused {
                sink.play();
            }
        }};
    }

    loop {
        match term.read_key()? {
            Key::Escape => break,
            Key::Char(' ') => {
                paused = !paused;
                if paused {
                    sink.pause();
                } else {
                    sink.play();
                }

                term.move_cursor_to(play_pause_pos.0, play_pause_pos.1)?;
                term.write_fmt(format_args!(
                    "{:width$}",
                    if paused { PAUSE } else { PLAY },
                    width = play_pos_len,
                ))?;
                term.flush()?;

                tx.send(if paused {
                    ProgressEvent::Pause
                } else {
                    ProgressEvent::Play
                })?;
            }
            Key::ArrowRight => {
                if idx == queue.len() - 1 {
                    continue;
                }
                idx += 1;
                skip!();
            }
            Key::ArrowLeft => {
                if idx != 0 {
                    idx -= 1;
                }
                skip!();
            }
            Key::ArrowUp => {
                if volume < 2.0 {
                    volume += 0.05;
                    sink.set_volume(volume);
                    print_volume(volume, volume_pos, &mut term)?;
                }
            }
            Key::ArrowDown => {
                if volume > 0.0 {
                    volume -= 0.05;
                    sink.set_volume(volume);
                    print_volume(volume, volume_pos, &mut term)?;
                }
            }
            _ => continue,
        }
    }

    term.clear_screen()?;
    term.show_cursor()?;
    crate::OK
}

fn print_meta(
    title: &str,
    artist: &str,
    album: &str,
    term: &mut Term,
    pos: (usize, usize),
    len: usize,
    i_of_n: (usize, usize),
) -> crate::Result {
    macro_rules! pad {
        ($s:expr) => {
            console::pad_str($s, len, Alignment::Left, Some("..."))
        };
    }

    term.move_cursor_to(pos.0, pos.1)?;
    term.write_str(&pad!(&console::style(title).bold().to_string()))?;

    term.move_cursor_to(pos.0, pos.1 + 1)?;
    term.write_str(&pad!(artist))?;

    term.move_cursor_to(pos.0, pos.1 + 2)?;
    term.write_str(&pad!(album))?;

    term.move_cursor_to(pos.0, pos.1 + 4)?;
    term.write_fmt(format_args!("{:02} / {:02}", i_of_n.0 + 1, i_of_n.1))?;

    term.flush()?;

    crate::OK
}

fn print_volume(volume: f32, pos: (usize, usize), term: &mut Term) -> crate::Result {
    term.move_cursor_to(pos.0, pos.1)?;
    let volume_blocks = (volume * 20.0) as usize;
    let div = volume_blocks / 8;
    let rem = volume_blocks % 8;
    for _ in 0..div {
        term.write_fmt(format_args!("{}", BLOCKS[7]))?;
    }
    if div < 5 {
        term.write_fmt(format_args!("{}", BLOCKS[rem]))?;
        if div < 4 {
            for _ in 0..(5 - (div + 1)) {
                term.write_str(" ")?;
            }
        }
    }
    term.write_fmt(format_args!(" {:3}", volume_blocks * 5))?;
    term.flush()?;

    crate::OK
}

enum ProgressEvent {
    Play,
    Pause,
    Reset(Duration),
}

fn progress_fn(
    rx: Receiver<ProgressEvent>,
    mut term: Term,
    pos: (usize, usize),
    len: usize,
    timestamp_pos: (usize, usize),
) -> crate::Result {
    let total_blocks = len * 8;

    let mut total_duration = Duration::from_secs_f64(0.0);
    let mut blocks_per_second = 1.0;
    let mut interval = 0.01;

    let mut paused = true;
    let mut last_play = Instant::now();
    let mut played = last_play.elapsed();

    macro_rules! display {
        () => {{
            played += last_play.elapsed();
            last_play = Instant::now();

            let progress = (blocks_per_second * played.as_secs_f64()).round() as usize;
            if progress >= 8 {
                term.move_cursor_to(pos.0 + (progress / 8) - 1, pos.1)?;
                term.write_fmt(format_args!("{}{}", BLOCKS[7], BLOCKS[progress % 8],))?;
            } else {
                term.move_cursor_to(pos.0 + (progress / 8), pos.1)?;
                term.write_fmt(format_args!("{}", BLOCKS[progress % 8]))?;
            }

            let played_seconds = played.as_secs();
            term.move_cursor_to(timestamp_pos.0, timestamp_pos.1)?;
            term.write_fmt(format_args!(
                "{:02}:{:02}",
                played_seconds / 60,
                played_seconds % 60,
            ))?;

            term.flush()?;
        }};
    }

    loop {
        thread::sleep(Duration::from_secs_f64(interval));

        match rx.try_recv() {
            Ok(e) => match e {
                ProgressEvent::Play => {
                    paused = false;
                    last_play = Instant::now();
                }
                ProgressEvent::Pause => {
                    paused = true;
                }
                ProgressEvent::Reset(d) => {
                    total_duration = d;
                    blocks_per_second = total_blocks as f64 / total_duration.as_secs_f64();
                    interval = (1.0 / blocks_per_second).min(1.0);

                    played = Duration::from_secs_f64(0.0);
                    term.move_cursor_to(pos.0, pos.1)?;
                    term.write_str(&console::pad_str(" ", len, Alignment::Left, None))?;
                    display!()
                }
            },
            Err(e) => {
                if let TryRecvError::Disconnected = e {
                    break crate::OK;
                }
            }
        }

        if paused || played >= total_duration {
            continue;
        }
        display!()
    }
}
