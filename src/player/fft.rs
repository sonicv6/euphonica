use libc;
use std::{
    io::{self, prelude::*, BufReader, Error},
    fs::{OpenOptions, File},
    os::unix::fs::OpenOptionsExt
};
use rustfft::{
    num_complex::Complex,
    Fft
};

use mpd::status::AudioFormat;

fn open_named_pipe_readonly(path: &str) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_RDONLY)  // from os::unix::fs::OpenOptionsExt, i.e. on Unix-like systems only.
        .open(path)
}

// #[derive(PartialEq, Eq, Clone, Debug)]
// pub enum BitDepth {
//     Float32,
//     Int32,
//     Int16,
//     Int8
// }

// impl BitDepth {
//     pub fn bytes_per_sample(&self) -> u32 {
//         match self {
//             Self::Float32 | Self::Int32 => 4,
//             Self::Int16 => 2,
//             Self::Int8 => 1
//         }
//     }

//     pub fn is_float(&self) -> bool {
//         self == &Self::Float32
//     }

//     pub fn from_mpd_format(format_str: &str) -> Result<Self, ()> {
//         let parts: Vec<&str> = format_str.split(':').collect();
//         if parts.len() != 3 {
//             return Err(()); // Invalid format string
//         }

//         match parts[1] {
//             "f" => Ok(BitDepth::Float32),
//             "8" => Ok(BitDepth::Int8),
//             "16" => Ok(BitDepth::Int16),
//             "32" => Ok(BitDepth::Int32),
//             _ => Err(()), // Unsupported bit depth
//         }
//     }
// }

pub fn try_open_pipe(path: &str) -> Result<BufReader<File>, Error> {
    Ok(BufReader::new(open_named_pipe_readonly(path)?))
}

fn parse_to_float_complex(buf: [u8; 4], format: &AudioFormat, is_le: bool) -> Complex<f32> {
    // Currently MPD only supports streaming PCM through FIFO so we need not handle DSD here.
    // DSD is overkill for decorative things like these anyway.
    if format.bits == 0 {
        // 32bit float
        if buf.len() != 4 {
            panic!("Invalid FIFO data: configured to interpret as float32, got less than 4 bytes per sample");
        }
        if is_le {
            Complex { re: f32::from_le_bytes(buf), im: 0.0 }
        }
        else {
            Complex { re: f32::from_be_bytes(buf), im: 0.0 }
        }
    } else {
        if is_le {
            Complex { re: i32::from_le_bytes(buf) as f32, im: 0.0 }
        }
        else {
            Complex { re: i32::from_be_bytes(buf) as f32, im: 0.0 }
        }
    }
}

/// Read the specified number of samples, then return them as 32-bit float.
///
/// Here we assume the left and right channels are given one after the other as follows:
/// L1 R1 L2 R2 L3 R3 ...
/// That is, each sample consists of two numbers. Each number may also span multiple bytes.
/// Little-endianness is assumed for simplicity. TODO: maybe support big-endian too?
pub fn get_stereo_pcm(
    samples_left: &mut [Complex<f32>],
    samples_right: &mut [Complex<f32>],
    reader: &mut BufReader<File>,
    format: &AudioFormat,
    is_le: bool
) -> Result<(), std::io::Error> {
    let num_samples = samples_left.len();
    if num_samples != samples_right.len() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Given sample buffers' lengths do not match"
        ));
    }
    let bytes_per_sample = format.bits / 8;
    for idx in 0..num_samples {
        // Left channel
        let mut buf = vec![0u8; bytes_per_sample as usize];
        reader.read_exact(&mut buf)?;
        buf.resize(4, 0); // pad MSB with zeros
        samples_left[idx] = parse_to_float_complex(buf.try_into().unwrap(), format, is_le);
        // Right channel
        let mut buf = vec![0u8; bytes_per_sample as usize];
        reader.read_exact(&mut buf)?;
        buf.resize(4, 0); // pad MSB with zeros
        samples_right[idx] = parse_to_float_complex(buf.try_into().unwrap(), format, is_le);
    }
    Ok(())
}
