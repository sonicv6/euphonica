use libc;
use std::{
    f32::consts::PI, fs::{File, OpenOptions}, io::{self, prelude::*, BufReader, Error, SeekFrom}, os::unix::fs::OpenOptionsExt, sync::Arc
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

/// Blackman-Harris 4-term window, for reducing scalloping loss in FFT.
/// Implementation taken from crate spectrum-analyzer v0.5.2, refactored
/// to run in-place.
fn blackman_harris_4term_inplace(samples: &mut [f32]) {
    const ALPHAS: [f32; 4] = [0.35875, -0.48829, 0.14128, -0.01168];
    let samples_len_f32 = samples.len() as f32;

    for i in 0..samples.len() {
        let mut acc = 0.0;

        // Will result in something like that:
        /* ALPHA0
            + ALPHA1 * ((2.0 * PI * *samples[i])/samples_len_f32).cos()
            + ALPHA2 * ((4.0 * PI * *samples[i])/samples_len_f32).cos()
            + ALPHA3 * ((6.0 * PI * *samples[i])/samples_len_f32).cos()
        */

        for alpha_i in 0..ALPHAS.len() {
            // in 1. iter. 0PI, then 2PI, then 4 PI, then 6 PI
            let two_pi_iteration = 2.0 * alpha_i as f32 * PI;
            let sample = samples[i];
            acc += ALPHAS[alpha_i] * ((two_pi_iteration * sample) / samples_len_f32).cos();
        }
        samples[i] = acc;
    }
}


pub fn try_open_pipe(path: &str) -> Result<BufReader<File>, Error> {
    Ok(BufReader::new(open_named_pipe_readonly(path)?))
}

fn parse_to_float(buf: [u8; 4], format: &AudioFormat, is_le: bool) -> f32 {
    // Currently MPD only supports streaming PCM through FIFO so we need not handle DSD here.
    // DSD is overkill for decorative things like these anyway.
    if format.bits == 0 {
        // 32bit float. Should already be -1 to 1, no need for normalisation.
        if buf.len() != 4 {
            panic!("Invalid FIFO data: configured to interpret as float32, got less than 4 bytes per sample");
        }
        if is_le {
            f32::from_le_bytes(buf)
        }
        else {
            f32::from_be_bytes(buf)
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
            i32::from_le_bytes(buf) as f32 / max_val
        }
        else {
            i32::from_be_bytes(buf) as f32 / max_val
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
    samples_left: &mut [f32],
    samples_right: &mut [f32],
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
        samples_left[idx] = parse_to_float(buf.try_into().unwrap(), format, is_le);
        // Right channel
        let mut buf = vec![0u8; bytes_per_sample as usize];
        reader.read_exact(&mut buf)?;
        buf.resize(4, 0); // pad MSB with zeros
        samples_right[idx] = parse_to_float(buf.try_into().unwrap(), format, is_le);
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
    input_buf: &mut [f32],
    output_buf: &mut Vec<f32>,
    n_bins: u32,
    bin_mode: FftBinMode,
    min_freq: f32,
    max_freq: f32
) {
    let samples_len_f32 = input_buf.len() as f32;
    blackman_harris_4term_inplace(input_buf);

    let fft_res = spectrum_analyzer::samples_fft_to_spectrum(
        input_buf,
        format.rate,
        spectrum_analyzer::FrequencyLimit::Range(min_freq, max_freq),
        None,
        None
    );
    let spectrum = fft_res.data();
    output_buf.clear();
    // Compute magnitudes. This might trigger mem allocations
    // if bin count has changed.
    for _ in 0..n_bins {
        output_buf.push(0.0);
    }

    // Determine which bin this frequency falls into.
    // Each bin's value is the average of magnitudes of all the frequencies therein.
    let mut prev_count: u32 = 0;
    let mut prev_bin: usize = 0;
    match bin_mode {
        FftBinMode::Linear => {
            let spacing = (max_freq - min_freq) / (n_bins as f32);
            for i in 0..spectrum.len() {
                let (freq, x) = spectrum[i];
                // Each bin's range is an interval open to the right, except for the last.
                let bin_idx: usize = (((freq.val() - min_freq) / spacing).floor() as usize).min((n_bins - 1) as usize);
                // println!("f={},\tbin={}", freq, bin_idx);
                // rustfft does not normalise by itself.
                // output_buf[bin_idx] = output_buf[bin_idx].max(x.val());
                output_buf[bin_idx] += x.val();
                if prev_bin != bin_idx {
                    if prev_count > 0 && bin_idx > 0 {
                        // Done summing the previous bin => average it
                        output_buf[prev_bin] /= prev_count as f32;
                    }
                    prev_count = 1;
                    prev_bin = bin_idx;
                }
                else {
                    prev_count += 1;
                }
                // Average last bin
                output_buf[prev_bin] /= prev_count as f32;
            }
        }
        FftBinMode::Logarithmic => {
            // Evenly-spaced but in logarithmic scale
            let log_base = (max_freq.log10() - min_freq.log10()) / (n_bins as f32);
            for i in 0..spectrum.len() {
                let (freq, x) = spectrum[i];
                // Same logic as linear, but converted to logarithmic first
                let bin_idx: usize = (
                    ((freq.val().log10() - min_freq.log10()) / log_base).floor().max(0.0) as usize
                ).min((n_bins - 1) as usize);
                // println!("f={},\tbin={}", freq, bin_idx);
                // rustfft does not normalise by itself.
                // output_buf[bin_idx] = output_buf[bin_idx].max(x.val());
                output_buf[bin_idx] += x.val();
                if prev_bin != bin_idx {
                    if prev_count > 1 && bin_idx > 0 {
                        // Done summing the previous bin => average it
                        output_buf[prev_bin] /= (prev_count as f32).log10().max(1.0);
                    }
                    prev_count = 1;
                    prev_bin = bin_idx;
                }
                else {
                    prev_count += 1;
                }
                // Average last bin
                output_buf[prev_bin] /= (prev_count as f32).log10().max(1.0);
            }
        }
    }
    // for i in 0..output_buf.len() {
    //     // y-axis should be logarithmic to mirror dB scale
    //     output_buf[i] = (output_buf[i] / norm_fac).max(1.0).log10();
    // }
    // println!("{:?}", &output_buf);
}
