use anyhow::Context;
use lilac::Lilac;
use rodio::{Sink, Source};
use std::{path::PathBuf, process, thread};
use structopt::StructOpt;

type Result = anyhow::Result<()>;
const OK: Result = Result::Ok(());

mod interactive;
mod transcode;

#[derive(StructOpt)]
#[structopt(about)]
enum Opt {
    /// Plays a LILAC file
    Play {
        /// File to play
        #[structopt(name = "FILE")]
        file: PathBuf,
        /// Playback volume
        #[structopt(short, long, name = "VOLUME", default_value = "1.0")]
        volume: f32,
    },
    /// Transcodes a file to or from LILAC
    Transcode {
        /// Input file
        #[structopt(name = "INPUT")]
        input: PathBuf,
        /// Output file
        #[structopt(name = "OUTPUT")]
        output: PathBuf,
    },

    #[structopt(external_subcommand)]
    Interactive(Vec<String>),
}

fn main() {
    if let Err(e) = match Opt::from_args() {
        Opt::Play { file, volume } => play(file, volume),
        Opt::Transcode { input, output } => transcode::main(input, output),
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
