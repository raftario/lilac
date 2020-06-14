use lilac::Lilac;
use std::{path::PathBuf, process};
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
        Opt::Play { file } => {
            let lilac = Lilac::read_file(file).unwrap_or_exit(65);
            println!(
                "Now playing {} by {} on {}",
                lilac.title(),
                lilac.artist(),
                lilac.album(),
            );
            lilac.play().unwrap_or_exit(69);
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
