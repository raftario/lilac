use lilac::Lilac;
use rodio::{Sink, Source};
use std::{path::PathBuf, process, thread};
use structopt::StructOpt;

trait ResultExt<T> {
    fn unwrap_or_exit(self, code: i32) -> T;
}
impl<T, E: std::fmt::Display> ResultExt<T> for Result<T, E> {
    fn unwrap_or_exit(self, code: i32) -> T {
        match self {
            Ok(v) => v,
            Err(e) => {
                eprintln!("{}", e);
                process::exit(code)
            }
        }
    }
}

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
}

fn main() {
    match Opt::from_args() {
        Opt::Play { file, volume } => {
            let lilac = Lilac::read_file(file).unwrap_or_exit(65);
            println!(
                "Now playing {} by {} on {}",
                lilac.title(),
                lilac.artist(),
                lilac.album(),
            );

            let device = rodio::default_output_device()
                .ok_or("no audio device")
                .unwrap_or_exit(69);
            let sink = Sink::new(&device);

            let source = lilac.source();
            let duration = source.total_duration().unwrap();

            sink.set_volume(volume);
            sink.append(source);
            sink.play();

            thread::sleep(duration);
        }
        Opt::Transcode { input, output } => match input.extension().map(|e| e.to_str().unwrap()) {
            Some("lilac") => {
                println!(
                    "Transcoding LILAC file `{}` to WAV file `{}`",
                    input.display(),
                    output.display(),
                );
                let lilac = Lilac::read_file(input).unwrap_or_exit(65);
                lilac.to_wav_file(output).unwrap_or_exit(70);
            }
            Some("mp3") => {
                println!(
                    "Transcoding MP3 file `{}` to LILAC file `{}`",
                    input.display(),
                    output.display(),
                );
                let lilac = Lilac::from_mp3_file(input).unwrap_or_exit(65);
                lilac.write_file(output).unwrap_or_exit(70);
            }
            Some("flac") => {
                println!(
                    "Transcoding FLAC file `{}` to LILAC file `{}`",
                    input.display(),
                    output.display(),
                );
                let lilac = Lilac::from_flac_file(input).unwrap_or_exit(65);
                lilac.write_file(output).unwrap_or_exit(70);
            }
            Some("ogg") => {
                println!(
                    "Transcoding OGG file `{}` to LILAC file `{}`",
                    input.display(),
                    output.display(),
                );
                let lilac = Lilac::from_ogg_file(input).unwrap_or_exit(65);
                lilac.write_file(output).unwrap_or_exit(70);
            }
            Some("wav") => {
                println!(
                    "Transcoding WAV file `{}` to LILAC file `{}`",
                    input.display(),
                    output.display(),
                );
                let lilac = Lilac::from_wav_file(input).unwrap_or_exit(65);
                lilac.write_file(output).unwrap_or_exit(70);
            }
            _ => {
                eprintln!("unknown input format");
                process::exit(65);
            }
        },
    }
}
