use lilac::Lilac;
use std::path::PathBuf;
use std::process;

pub fn main(input: PathBuf, output: PathBuf) -> crate::Result {
    match input.extension().map(|e| e.to_str().unwrap()) {
        Some("lilac") => {
            println!(
                "Transcoding LILAC file `{}` to WAV file `{}`",
                input.display(),
                output.display(),
            );
            let lilac = Lilac::read_file(input)?;
            lilac.to_wav_file(output)?;
        }
        Some("mp3") => {
            println!(
                "Transcoding MP3 file `{}` to LILAC file `{}`",
                input.display(),
                output.display(),
            );
            let lilac = Lilac::from_mp3_file(input)?;
            lilac.write_file(output)?;
        }
        Some("flac") => {
            println!(
                "Transcoding FLAC file `{}` to LILAC file `{}`",
                input.display(),
                output.display(),
            );
            let lilac = Lilac::from_flac_file(input)?;
            lilac.write_file(output)?;
        }
        Some("ogg") => {
            println!(
                "Transcoding OGG file `{}` to LILAC file `{}`",
                input.display(),
                output.display(),
            );
            let lilac = Lilac::from_ogg_file(input)?;
            lilac.write_file(output)?;
        }
        Some("wav") => {
            println!(
                "Transcoding WAV file `{}` to LILAC file `{}`",
                input.display(),
                output.display(),
            );
            let lilac = Lilac::from_wav_file(input)?;
            lilac.write_file(output)?;
        }
        _ => {
            eprintln!("unknown input format");
            process::exit(65);
        }
    }
    crate::OK
}
