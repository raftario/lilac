use anyhow::Context;
use lilac::Lilac;
use rodio::{Sink, Source};
use std::{path::PathBuf, process, thread};
use structopt::StructOpt;

type Result = anyhow::Result<()>;
const OK: Result = Result::Ok(());

mod interactive;
mod transcode;

/// LILAC playback and transcoding utility
///
/// If neither of the subcommands are detected,
/// opens an interactive player and load the procided files.
#[derive(StructOpt)]
enum Opt {
    /// Plays a LILAC file
    Play {
        /// File to play
        #[structopt(name = "FILE")]
        file: PathBuf,
        /// Playback volume
        ///
        /// Should be anywhere between 0.0 and 1.0 inclusively
        #[structopt(short, long, name = "VOLUME", default_value = "1.0")]
        volume: f32,
    },
    /// Transcodes a file to or from LILAC
    ///
    /// Supports transcoding from MP3, FLAC,
    /// OGG and WAV, and transcoding to WAV.
    /// Input and output formats are automatically inferred
    Transcode {
        /// Glob matching the input files
        #[structopt(name = "GLOB")]
        glob: String,
        /// Output files naming pattern
        ///
        /// %F is replaced with the input filename without extension,
        /// %E with the output format extension,
        /// %e with the input format extension,
        /// %T with the song title,
        /// %A with the song artist,
        /// %a with the song album.
        #[structopt(name = "PATTERN", default_value = "%F.%E")]
        output: String,
    },

    #[structopt(external_subcommand)]
    Interactive(Vec<String>),
}

fn main() {
    if let Err(e) = match Opt::from_args() {
        Opt::Play { file, volume } => play(file, volume),
        Opt::Transcode { glob, output } => transcode::main(glob, output),
        Opt::Interactive(queue) => interactive::main(queue),
    } {
        eprintln!("{:#}", e);
        process::exit(1);
    }
}

fn play(file: PathBuf, volume: f32) -> Result {
    let lilac = Lilac::read_file(file)?;
    println!(
        "Now playing {} by {} on {}",
        lilac.title(),
        lilac.artist(),
        lilac.album(),
    );

    let device = rodio::default_output_device().context("no audio device")?;
    let sink = Sink::new(&device);

    let source = lilac.source();
    let duration = source.total_duration().unwrap();

    sink.set_volume(volume);
    sink.append(source);
    sink.play();

    thread::sleep(duration);
    OK
}
