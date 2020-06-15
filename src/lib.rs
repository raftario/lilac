use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{BufReader, BufWriter, Read, Write},
    path::Path,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error: {0}")]
    IO(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[cfg(feature = "playback")]
    #[error("playback error: {0}")]
    Playback(&'static str),

    #[cfg(feature = "mp3")]
    #[error("mp3 error: {0}")]
    Mp3(#[from] minimp3::Error),

    #[cfg(feature = "flac")]
    #[error("flac error: {0}")]
    Flac(#[from] claxon::Error),

    #[cfg(feature = "ogg")]
    #[error("ogg error: {0}")]
    Ogg(#[from] lewton::VorbisError),

    #[cfg(feature = "wav")]
    #[error("wav error: {0}")]
    Wav(#[from] hound::Error),
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Lilac {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub year: Option<i32>,
    pub album: Option<String>,
    pub track: Option<u32>,

    pub channels: u16,
    pub sample_rate: u32,
    pub bit_depth: u32,

    samples: Vec<i32>,
}
impl Lilac {
    pub fn read<R: Read>(reader: R) -> Result<Self, Error> {
        serde_json::from_reader(reader).map_err(Into::into)
    }
    pub fn read_file<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        Self::read(BufReader::new(File::open(path)?))
    }

    pub fn write<W: Write>(&self, writer: W) -> Result<(), Error> {
        serde_json::to_writer_pretty(writer, self).map_err(Into::into)
    }
    pub fn write_file<P: AsRef<Path>>(&self, path: P) -> Result<(), Error> {
        self.write(BufWriter::new(File::create(path)?))
    }

    pub fn title(&self) -> &str {
        self.title.as_ref().map(AsRef::as_ref).unwrap_or("Unknown")
    }
    pub fn artist(&self) -> &str {
        self.artist.as_ref().map(AsRef::as_ref).unwrap_or("Unknown")
    }
    pub fn album(&self) -> &str {
        self.album.as_ref().map(AsRef::as_ref).unwrap_or("Unknown")
    }
}

#[cfg(feature = "playback")]
mod playback {
    use crate::{Error, Lilac};
    use rodio::{source::Source, Device};
    use std::{cmp::Ordering, thread, time::Duration};

    impl Lilac {
        pub fn source(self) -> impl Source<Item = f32> {
            let min = (2u32.pow(self.bit_depth - 1)) as f32;
            let max = (2u32.pow(self.bit_depth - 1) - 1) as f32;

            let samples_len = self.samples.len();

            LilacSource {
                channels: self.channels,
                sample_rate: self.sample_rate,

                samples: self.samples.into_iter().map(move |s| match s.cmp(&0) {
                    Ordering::Less => (s as f32 / min),
                    Ordering::Equal => 0.0,
                    Ordering::Greater => (s as f32 / max),
                }),

                duration: Duration::from_millis(
                    samples_len as u64 / self.channels as u64 / (self.sample_rate / 1000) as u64,
                ),
            }
        }

        pub fn play(self) -> Result<(), Error> {
            let device = rodio::default_output_device().ok_or(Error::Playback(
                "couldn't detect default audio playback device",
            ))?;
            self.play_on(&device);
            Ok(())
        }
        pub fn play_on(self, device: &Device) {
            let source = self.source();
            let duration = source.total_duration().unwrap();
            rodio::play_raw(device, source);
            thread::sleep(duration);
        }
    }

    struct LilacSource<T: Iterator<Item = f32>> {
        channels: u16,
        sample_rate: u32,

        samples: T,

        duration: Duration,
    }
    impl<T: Iterator<Item = f32>> Iterator for LilacSource<T> {
        type Item = f32;

        #[inline]
        fn next(&mut self) -> Option<Self::Item> {
            self.samples.next()
        }
    }
    impl<T: Iterator<Item = f32>> Source for LilacSource<T> {
        #[inline]
        fn current_frame_len(&self) -> Option<usize> {
            None
        }
        #[inline]
        fn channels(&self) -> u16 {
            self.channels
        }
        #[inline]
        fn sample_rate(&self) -> u32 {
            self.sample_rate
        }
        #[inline]
        fn total_duration(&self) -> Option<Duration> {
            Some(self.duration)
        }
    }
}

#[cfg(feature = "mp3")]
mod mp3 {
    use crate::{Error, Lilac};
    use minimp3::Decoder;
    use std::{
        fs::File,
        io::{BufReader, Read},
        path::Path,
    };

    impl Lilac {
        pub fn from_mp3<R: Read>(reader: R) -> Result<Self, Error> {
            let mut reader = Decoder::new(reader);
            let mut samples = Vec::new();

            let first_frame = reader.next_frame()?;
            let channels = first_frame.channels as u16;
            let sample_rate = first_frame.sample_rate as u32;
            samples.extend(first_frame.data.into_iter().map(|s| s as i32));

            loop {
                match reader.next_frame() {
                    Ok(f) => samples.extend(f.data.into_iter().map(|s| s as i32)),
                    Err(e) => match e {
                        minimp3::Error::Eof => break,
                        _ => return Err(e.into()),
                    },
                }
            }

            Ok(Lilac {
                title: None,
                artist: None,
                year: None,
                album: None,
                track: None,
                channels,
                sample_rate,
                bit_depth: 16,
                samples,
            })
        }

        pub fn from_mp3_file<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
            Self::from_mp3(BufReader::new(File::open(path)?))
        }
    }
}

#[cfg(feature = "flac")]
mod flac {
    use crate::{Error, Lilac};
    use claxon::FlacReader;
    use std::{
        fs::File,
        io::{BufReader, Read},
        path::Path,
    };

    impl Lilac {
        pub fn from_flac<R: Read>(reader: R) -> Result<Self, Error> {
            let mut reader = FlacReader::new(reader)?;

            let info = reader.streaminfo();

            let title = reader.get_tag("TITLE").next().map(ToOwned::to_owned);
            let artist = {
                let artists: Vec<&str> = reader.get_tag("ARTIST").collect();
                if !artists.is_empty() {
                    Some(artists.join(", "))
                } else {
                    None
                }
            };
            let album = reader.get_tag("ALBUM").next().map(ToOwned::to_owned);
            let track = match reader.get_tag("TRACKNUMBER").next() {
                Some(tn) => match tn.parse() {
                    Ok(tn) => Some(tn),
                    Err(_) => None,
                },
                None => None,
            };

            Ok(Lilac {
                title,
                artist,
                year: None,
                album,
                track,

                channels: info.channels as u16,
                sample_rate: info.sample_rate,
                bit_depth: info.bits_per_sample,

                samples: reader.samples().collect::<Result<_, _>>()?,
            })
        }

        pub fn from_flac_file<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
            Self::from_flac(BufReader::new(File::open(path)?))
        }
    }
}

#[cfg(feature = "ogg")]
mod ogg {
    use crate::{Error, Lilac};
    use lewton::inside_ogg::OggStreamReader;
    use std::{
        fs::File,
        io::{BufReader, Read, Seek},
        path::Path,
    };

    impl Lilac {
        pub fn from_ogg<R: Read + Seek>(reader: R) -> Result<Self, Error> {
            let mut reader = OggStreamReader::new(reader)?;

            let mut title = None;
            let mut artists = Vec::new();
            let mut album = None;
            let mut track = None;
            for (k, v) in &reader.comment_hdr.comment_list {
                let uk = k.to_ascii_uppercase();
                if uk == "TITLE" && title.is_none() {
                    title = Some(v.clone());
                } else if uk == "ARTIST" {
                    artists.push(v.as_ref());
                } else if uk == "ALBUM" && album.is_none() {
                    album = Some(v.clone());
                } else if uk == "TRACKNUMBER" && track.is_none() {
                    if let Ok(tn) = v.parse() {
                        track = Some(tn);
                    }
                }
            }
            let artist = if !artists.is_empty() {
                Some(artists.join(", "))
            } else {
                None
            };

            let mut samples = Vec::new();
            while let Some(packet) = reader.read_dec_packet_itl()? {
                samples.extend(packet.into_iter().map(|s| s as i32));
            }

            Ok(Lilac {
                title,
                artist,
                year: None,
                album,
                track,

                channels: reader.ident_hdr.audio_channels as u16,
                sample_rate: reader.ident_hdr.audio_sample_rate,
                bit_depth: 16,

                samples,
            })
        }

        pub fn from_ogg_file<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
            Self::from_ogg(BufReader::new(File::open(path)?))
        }
    }
}

#[cfg(feature = "wav")]
mod wav {
    use crate::{Error, Lilac};
    use hound::{SampleFormat, WavReader, WavSpec, WavWriter};
    use std::{
        fs::File,
        io::{BufReader, BufWriter, Read, Seek, Write},
        path::Path,
    };

    impl Lilac {
        pub fn from_wav<R: Read>(reader: R) -> Result<Self, Error> {
            let mut reader = WavReader::new(reader)?;

            let spec = reader.spec();
            let samples = reader.samples().collect::<Result<_, _>>()?;

            Ok(Lilac {
                title: None,
                artist: None,
                year: None,
                album: None,
                track: None,
                channels: spec.channels,
                sample_rate: spec.sample_rate,
                bit_depth: spec.bits_per_sample as u32,
                samples,
            })
        }

        pub fn from_wav_file<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
            Self::from_wav(BufReader::new(File::open(path)?))
        }

        pub fn to_wav<W: Write + Seek>(&self, writer: W) -> Result<(), Error> {
            let spec = WavSpec {
                channels: self.channels,
                sample_rate: self.sample_rate,
                bits_per_sample: self.bit_depth as u16,
                sample_format: SampleFormat::Int,
            };

            let mut writer = WavWriter::new(writer, spec)?;
            for sample in self.samples.iter().copied() {
                writer.write_sample(sample)?;
            }

            writer.finalize().map_err(Into::into)
        }

        pub fn to_wav_file<P: AsRef<Path>>(&self, path: P) -> Result<(), Error> {
            self.to_wav(BufWriter::new(File::create(path)?))
        }
    }
}
