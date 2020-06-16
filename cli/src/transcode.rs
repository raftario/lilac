use anyhow::Context;
use lilac::Lilac;
use rayon::prelude::*;
use std::{
    fs::{self, File},
    io::{BufReader, Read, Seek, SeekFrom},
    path::PathBuf,
};

static MP3_MAGIC_NUMBERS: &[&[u8]] = &[&[0xFF, 0xFB], &[0xFF, 0xF3], &[0xFF, 0xF2], b"ID3"];
static FLAC_MAGIC_NUMBER: &[u8] = b"fLaC";
static OGG_MAGIC_NUMBER: &[u8] = b"OggS";
static WAV_MAGIC_NUMBER: &[u8] = b"WAVE";
const WAV_MAGIC_NUMBER_OFFSET: usize = 8;

pub fn main(glob: String, output: String, keep: bool) -> crate::Result {
    let files = glob::glob(&glob)?;
    let results: Vec<anyhow::Result<(PathBuf, PathBuf)>> = files
        .par_bridge()
        .map(|r| transcode(r?, &output, keep))
        .collect();
    for r in results {
        match r {
            Ok((i, o)) => println!("`{}` -> `{}`", i.display(), o.display()),
            Err(e) => eprintln!("{:#}", e),
        }
    }

    crate::OK
}

enum Format {
    Lilac,
    Mp3,
    Flac,
    Ogg,
    Wav,
}

fn transcode(filename: PathBuf, output: &str, keep: bool) -> anyhow::Result<(PathBuf, PathBuf)> {
    let reader = BufReader::new(File::open(&filename)?);

    let (lilac, format) = match filename
        .extension()
        .map(|e| e.to_str().map(|e| e.to_lowercase()))
    {
        Some(Some(s)) => match s.as_ref() {
            "lilac" => (Lilac::read(reader)?, Format::Lilac),
            "mp3" => (Lilac::from_mp3(reader)?, Format::Mp3),
            "flac" => (Lilac::from_flac(reader)?, Format::Flac),
            "ogg" => (Lilac::from_ogg(reader)?, Format::Ogg),
            "wav" => (Lilac::from_wav(reader)?, Format::Wav),
            _ => detect(reader)?,
        },
        _ => detect(reader)?,
    };

    let output = output
        .replace(
            "%F",
            filename
                .file_stem()
                .context("Invalid filename")?
                .to_string_lossy()
                .as_ref(),
        )
        .replace(
            "%E",
            match format {
                Format::Lilac => "wav",
                _ => "lilac",
            },
        )
        .replace(
            "%e",
            match format {
                Format::Lilac => "lilac",
                Format::Mp3 => "mp3",
                Format::Flac => "flac",
                Format::Ogg => "ogg",
                Format::Wav => "wav",
            },
        )
        .replace("%T", lilac.title())
        .replace("%A", lilac.artist())
        .replace("%a", lilac.album());
    let outfile = filename
        .parent()
        .map(|p| p.join(&output))
        .unwrap_or_else(|| PathBuf::from(output));

    if let Some(p) = outfile.parent() {
        fs::create_dir_all(p)?;
    }

    match format {
        Format::Lilac => lilac.to_wav_file(&outfile)?,
        _ => lilac.write_file(&outfile)?,
    }

    if !keep {
        fs::remove_file(&filename)?;
    }
    Ok((filename, outfile))
}

fn detect<R: Read + Seek>(mut reader: R) -> anyhow::Result<(Lilac, Format)> {
    let magic_numer_len = MP3_MAGIC_NUMBERS
        .iter()
        .fold(0, |max, n| max.max(n.len()))
        .max(FLAC_MAGIC_NUMBER.len())
        .max(OGG_MAGIC_NUMBER.len())
        .max(WAV_MAGIC_NUMBER_OFFSET + WAV_MAGIC_NUMBER.len());
    let mut magic_number = vec![0; magic_numer_len];

    reader.read_exact(&mut magic_number)?;
    reader.seek(SeekFrom::Start(0))?;

    let result = if MP3_MAGIC_NUMBERS
        .iter()
        .any(|n| &magic_number[..n.len()] == *n)
    {
        (Lilac::from_mp3(reader)?, Format::Mp3)
    } else if FLAC_MAGIC_NUMBER == &magic_number[..FLAC_MAGIC_NUMBER.len()] {
        (Lilac::from_flac(reader)?, Format::Flac)
    } else if OGG_MAGIC_NUMBER == &magic_number[..OGG_MAGIC_NUMBER.len()] {
        (Lilac::from_ogg(reader)?, Format::Ogg)
    } else if WAV_MAGIC_NUMBER == &magic_number[WAV_MAGIC_NUMBER_OFFSET..WAV_MAGIC_NUMBER.len()] {
        (Lilac::from_wav(reader)?, Format::Wav)
    } else {
        (Lilac::read(reader)?, Format::Lilac)
    };
    Ok(result)
}
