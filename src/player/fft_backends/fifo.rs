use gio::{self, prelude::*};
use std::{
    cell::RefCell, str::FromStr, sync::{Arc, Mutex, RwLock}, thread, time::Duration
};

use mpd::status::AudioFormat;

use crate::utils::settings_manager;
use super::backend::{FftStatus, FftBackend};

#[derive(Default, Debug)]
pub struct FifoFftBackend {
    fft_status: Arc<RwLock<FftStatus>>,
    fft_handle: RefCell<Option<gio::JoinHandle<()>>>
}

impl FftBackend for FifoFftBackend {
    fn start(&self, output: Arc<Mutex<(Vec<f32>, Vec<f32>)>>) -> Result<(), ()> {
        let curr_status: FftStatus = *self.fft_status.read().unwrap();
        println!("Current status: {:?}", curr_status);
        if curr_status != FftStatus::Reading && curr_status != FftStatus::Stopping {
            let fft_status = self.fft_status.clone();
            let fft_handle = gio::spawn_blocking(move || {
                println!("Starting FIFO backend");
                let settings = settings_manager();
                let player_settings = settings.child("player");
                // Will require starting a new thread to account for path and format changes
                if let Ok(format) = AudioFormat::from_str(
                    settings.child("client").string("mpd-fifo-format").as_str(),
                ) {
                    // These settings require a restart
                    let n_samples = player_settings.uint("visualizer-fft-samples") as usize;
                    let n_bins = player_settings.uint("visualizer-spectrum-bins") as usize;
                    if let Ok(mut reader) = super::fft::try_open_pipe(
                        settings.child("client").string("mpd-fifo-path").as_str(),
                        &format,
                        n_samples,
                    ) {
                        // Allocate the following once only
                        let mut fft_buf_left: Vec<f32> = vec![0.0; n_samples];
                        let mut fft_buf_right: Vec<f32> = vec![0.0; n_samples];
                        let mut curr_step_left: Vec<f32> = vec![0.0; n_bins];
                        let mut curr_step_right: Vec<f32> = vec![0.0; n_bins];
                        'outer: loop {
                            // These should be applied on-the-fly
                            let bin_mode =
                                if player_settings.boolean("visualizer-spectrum-use-log-bins") {
                                    super::fft::BinMode::Logarithmic
                                } else {
                                    super::fft::BinMode::Linear
                                };
                            let fps = player_settings.uint("visualizer-fps") as f32;
                            let min_freq =
                                player_settings.uint("visualizer-spectrum-min-hz") as f32;
                            let max_freq =
                                player_settings.uint("visualizer-spectrum-max-hz") as f32;
                            let curr_step_weight = player_settings
                                .double("visualizer-spectrum-curr-step-weight")
                                as f32;
                            match super::fft::get_stereo_pcm(
                                &mut fft_buf_left,
                                &mut fft_buf_right,
                                &mut reader,
                                &format,
                                fps,
                                true,
                            ) {
                                Ok(()) => {
                                    // Compute outside of mutex lock please
                                    super::fft::get_magnitudes(
                                        &format,
                                        &mut fft_buf_left,
                                        &mut curr_step_left,
                                        n_bins as u32,
                                        bin_mode,
                                        min_freq,
                                        max_freq,
                                    );
                                    super::fft::get_magnitudes(
                                        &format,
                                        &mut fft_buf_right,
                                        &mut curr_step_right,
                                        n_bins as u32,
                                        bin_mode,
                                        min_freq,
                                        max_freq,
                                    );
                                    // Replace last frame
                                    if let Ok(mut output_lock) = output.lock() {
                                        if output_lock.0.len() != n_bins
                                            || output_lock.1.len() != n_bins
                                        {
                                            output_lock.0.clear();
                                            output_lock.1.clear();
                                            for _ in 0..n_bins {
                                                output_lock.0.push(0.0);
                                                output_lock.1.push(0.0);
                                            }
                                        }
                                        for i in 0..n_bins {
                                            output_lock.0[i] = curr_step_left[i] * curr_step_weight
                                                + output_lock.0[i] * (1.0 - curr_step_weight);
                                            output_lock.1[i] = curr_step_right[i]
                                                * curr_step_weight
                                                + output_lock.1[i] * (1.0 - curr_step_weight);
                                        }
                                        // println!("FFT L: {:?}\tR: {:?}", &output_lock.0, &output_lock.1);
                                    } else {
                                        panic!("Unable to lock FFT data mutex");
                                    }
                                }
                                Err(e) => match e.kind() {
                                    std::io::ErrorKind::UnexpectedEof
                                    | std::io::ErrorKind::WouldBlock => {
                                        *fft_status.write().unwrap() = FftStatus::ValidNotReading;
                                    }
                                    _ => {
                                        println!("FFT ERR: {:?}", &e);
                                        break 'outer;
                                    }
                                },
                            }
                            // Placed here such that we can use the first iteration to verify
                            // that the settings are correct.
                            let curr_status = *fft_status.read().unwrap();
                            if curr_status == FftStatus::Stopping {
                                println!("Stopping thread...");
                                return;
                            } else if curr_status != FftStatus::Reading {
                                *fft_status.write().unwrap() = FftStatus::Reading;
                            }
                            thread::sleep(Duration::from_millis((1000.0 / fps).floor() as u64));
                        }
                    }
                }
                // All graceful thread shutdowns are inside the loop. If we've reached here then
                // it's an error.
                *fft_status.write().unwrap() = FftStatus::Invalid;
            });
            self.fft_handle.replace(Some(fft_handle));
            return Ok(());
        }
        else {
            println!("Another FIFO thread is already running");
        }
        Err(())
    }

    fn stop(&self) {
        if let Some(handle) = self.fft_handle.take() {
            *self.fft_status.write().unwrap() = FftStatus::Stopping;
            let stop_future = glib::MainContext::default().spawn_local(
                async move {
                    let _ = handle.await;
                }
            );
            let _ = glib::MainContext::default().block_on(stop_future);
            // In case the thread is dead to begin with
            *self.fft_status.write().unwrap() = FftStatus::ValidNotReading;
        }
    }

    fn status(&self) -> FftStatus {
        *self.fft_status.read().unwrap()
    }
}
