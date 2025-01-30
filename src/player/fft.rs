use libc;
use std::{
    fs::{File, OpenOptions}, io::{self, prelude::*, BufReader, Error, SeekFrom}, os::unix::fs::OpenOptionsExt, sync::Arc
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
        // 32bit float. Should already be -1 to 1, no need for normalisation.
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
        // Assume signed magnitudes since that's what normal LPCM is
        let max_val: f32 = match format.bits {
            32 => std::i32::MAX as f32,
            16 => std::i16::MAX as f32,
            8 => std::i8::MAX as f32,
            _ => unimplemented!()
        };
        if is_le {
            Complex { re: i32::from_le_bytes(buf) as f32 / max_val, im: 0.0 }
        }
        else {
            Complex { re: i32::from_be_bytes(buf) as f32 / max_val, im: 0.0 }
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

pub enum FftBinMode {
    Linear,
    Logarithmic
}

/// Get frequency domain of stereo PCM samples.
///
/// This function may reallocate output_buf to match input_buf's size.
/// However, input_buf and scratch_buf must be of the same size from
/// the beginning.
///
/// Only frequencies within the human hearing range (20Hz to 20kHz) will
/// be kept.
///
/// This function also bins the magnitudes together to reduce the number
/// of output frequencies.
pub fn get_magnitudes(
    format: &AudioFormat,
    input_buf: &mut [Complex<f32>],
    output_buf: &mut Vec<f32>,
    scratch_buf: &mut [Complex<f32>],
    fft: Arc<dyn Fft<f32>>,
    n_bins: u32,
    bin_mode: FftBinMode
) {
    fft.process_with_scratch(input_buf, scratch_buf);
    output_buf.clear();
    // Compute magnitudes. This might trigger mem allocations
    // if bin count has changed.
    for _ in 0..n_bins {
        output_buf.push(0.0);
    }
    // Determine which bin this frequency falls into.
    // Each bin's value is the maximum magnitude of all the frequencies falling
    // within it (looks nicer).
    match bin_mode {
        FftBinMode::Linear => {
            let spacing = (20000.0 - 20.0) / (n_bins as f32);
            for (i, x) in input_buf.iter().enumerate() {
                let freq: f32 = (i as f32) * (format.rate as f32) / (input_buf.len() as f32);
                if freq >= 20.0 && freq <= 20000.0 {
                    // Each bin's range is an interval open to the right, except for the last.
                    let bin_idx: usize = (((freq - 20.0) / spacing).floor() as usize).min((n_bins - 1) as usize);
                    // println!("f={},\tbin={}", freq, bin_idx);
                    // rustfft does not normalise by itself.
                    let x_norm = x / (output_buf.len() as f32).sqrt();
                    output_buf[bin_idx] = output_buf[bin_idx].max((x_norm.re * x_norm.re + x_norm.im * x_norm.im).sqrt());
                }
            }
        }
        FftBinMode::Logarithmic => {
            // Evenly-spaced but in logarithmic scale
            let log_base = (20000.0_f32.log10() - 20.0_f32.log10()) / (n_bins as f32);
            for (i, x) in input_buf.iter().enumerate() {
                let freq: f32 = (i as f32) * (format.rate as f32) / (input_buf.len() as f32);
                if freq >= 20.0 && freq <= 20000.0 {
                    // Same logic as linear, but converted to logarithmic first
                    let bin_idx: usize = (
                        ((freq.log10() - 20.0_f32.log10()) / log_base).floor().max(0.0) as usize
                    ).min((n_bins - 1) as usize);
                    // println!("f={},\tbin={}", freq, bin_idx);
                    // rustfft does not normalise by itself.
                    let x_norm = x / (output_buf.len() as f32).sqrt();
                    output_buf[bin_idx] = output_buf[bin_idx].max((x_norm.re * x_norm.re + x_norm.im * x_norm.im).sqrt());
                }
            }
        }
    }
}
