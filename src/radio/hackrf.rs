//! HackRF device control.
//!
//! This module provides a high-level interface for controlling HackRF devices
//! using the `waverave_hackrf` crate. Falls back to demo mode at runtime if no
//! HackRF hardware is detected.

use anyhow::{Context, Result};
use std::sync::mpsc::Sender;
use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    Arc, Mutex,
};
use std::thread::{self, JoinHandle};

use crate::app::RadioEvent;
use crate::capture::{Capture, RfModulation, StoredLevelDuration};

use super::demodulator::Demodulator;
use super::demodulator::FmDemodulator;
use super::demodulator::LevelDuration;

use waverave_hackrf::{HackRf, open_hackrf};

/// Sample rate for HackRF (2 MHz is good for keyfob signals)
const SAMPLE_RATE: u32 = 2_000_000;

/// Shared gain/amp settings that can be updated while receiving
#[derive(Debug, Clone, Copy, PartialEq)]
struct GainSettings {
    lna_gain: u32,
    vga_gain: u32,
    amp_enabled: bool,
}

impl Default for GainSettings {
    fn default() -> Self {
        Self {
            lna_gain: 32,
            vga_gain: 32,
            amp_enabled: false,
        }
    }
}

/// HackRF controller for receiving and transmitting signals
pub struct HackRfController {
    /// Event sender for notifying the app
    event_tx: Sender<RadioEvent>,
    /// Whether we're currently receiving
    receiving: Arc<AtomicBool>,
    /// Receiver thread handle
    rx_thread: Option<JoinHandle<()>>,
    /// Current frequency
    frequency: Arc<Mutex<u32>>,
    /// AM/OOK demodulator
    demodulator_am: Arc<Mutex<Demodulator>>,
    /// FM/2FSK demodulator
    demodulator_fm: Arc<Mutex<FmDemodulator>>,
    /// Whether HackRF is available
    hackrf_available: bool,
    /// Shared gain settings (read by receiver thread)
    gain_settings: Arc<Mutex<GainSettings>>,
    /// RSSI (f32 bits) written by RX callback, read by UI - never blocks
    rssi_value: Arc<AtomicU32>,
    /// The USB FD device (if running in Termux)
    usb_fd_device: Option<nusb::Device>,
}

impl HackRfController {
    /// Create a new HackRF controller
    pub fn new(event_tx: Sender<RadioEvent>, usb_fd_device: Option<nusb::Device>) -> Result<Self> {
        let demodulator_am = Demodulator::new(SAMPLE_RATE);
        let demodulator_fm = FmDemodulator::new(SAMPLE_RATE);

        // Check if HackRF is available
        let hackrf_available = check_hackrf_available(usb_fd_device.as_ref());

        if hackrf_available {
            tracing::info!("HackRF device detected");
        } else {
            tracing::warn!("HackRF not detected - running in demo mode");
        }

        Ok(Self {
            event_tx,
            receiving: Arc::new(AtomicBool::new(false)),
            rx_thread: None,
            frequency: Arc::new(Mutex::new(433_920_000)),
            demodulator_am: Arc::new(Mutex::new(demodulator_am)),
            demodulator_fm: Arc::new(Mutex::new(demodulator_fm)),
            hackrf_available,
            gain_settings: Arc::new(Mutex::new(GainSettings::default())),
            rssi_value: Arc::new(AtomicU32::new(0)),
            usb_fd_device,
        })
    }

    /// Shared atomic for RSSI (f32::to_bits); UI reads so callback never blocks on channel.
    pub fn rssi_source(&self) -> Arc<AtomicU32> {
        self.rssi_value.clone()
    }

    /// Check if HackRF is available
    #[allow(dead_code)]
    pub fn is_available(&self) -> bool {
        self.hackrf_available
    }

    /// HackRF supports transmit.
    pub fn supports_tx(&self) -> bool {
        true
    }

    /// Start receiving at the specified frequency
    pub fn start_receiving(&mut self, frequency: u32) -> Result<()> {
        if self.receiving.load(Ordering::SeqCst) {
            return Ok(());
        }

        *self.frequency.lock().unwrap() = frequency;
        self.receiving.store(true, Ordering::SeqCst);

        let receiving = self.receiving.clone();
        let event_tx = self.event_tx.clone();
        let freq = self.frequency.clone();
        let demodulator_am = self.demodulator_am.clone();
        let demodulator_fm = self.demodulator_fm.clone();
        let hackrf_available = self.hackrf_available;
        let gain_settings = self.gain_settings.clone();
        let rssi_value = self.rssi_value.clone();
        let usb_fd_device = self.usb_fd_device.clone();

        self.rx_thread = Some(thread::spawn(move || {
            if hackrf_available {
                if let Err(e) = run_receiver_hackrf(
                    receiving.clone(),
                    event_tx.clone(),
                    freq,
                    demodulator_am,
                    demodulator_fm,
                    gain_settings,
                    rssi_value,
                    usb_fd_device,
                )
                {
                    let _ = event_tx.send(RadioEvent::Error(format!("Receiver error: {}", e)));
                }
            } else {
                run_demo_receiver(receiving, event_tx, freq);
            }
        }));

        tracing::info!("Started receiving at {} Hz", frequency);
        Ok(())
    }

    /// Stop receiving
    pub fn stop_receiving(&mut self) -> Result<()> {
        self.receiving.store(false, Ordering::SeqCst);

        if let Some(handle) = self.rx_thread.take() {
            let _ = handle.join();
        }

        tracing::info!("Stopped receiving");
        Ok(())
    }

    /// Transmit a signal
    pub fn transmit(&self, signal: &[LevelDuration], frequency: u32) -> Result<()> {
        if !self.hackrf_available {
            tracing::warn!("Cannot transmit: HackRF not available (demo mode)");
            return Ok(());
        }

        let _was_receiving = self.receiving.load(Ordering::SeqCst);

        transmit_signal_hackrf(signal, frequency, self.usb_fd_device.as_ref())?;

        Ok(())
    }

    /// Update frequency (can be done live)
    pub fn set_frequency(&mut self, frequency: u32) -> Result<()> {
        *self.frequency.lock().unwrap() = frequency;
        Ok(())
    }

    /// Update LNA gain live
    pub fn set_lna_gain(&mut self, gain: u32) -> Result<()> {
        if let Ok(mut settings) = self.gain_settings.lock() {
            settings.lna_gain = gain;
        }
        Ok(())
    }

    /// Update VGA gain live
    pub fn set_vga_gain(&mut self, gain: u32) -> Result<()> {
        if let Ok(mut settings) = self.gain_settings.lock() {
            settings.vga_gain = gain;
        }
        Ok(())
    }

    /// Update AMP enable live
    pub fn set_amp_enable(&mut self, enabled: bool) -> Result<()> {
        if let Ok(mut settings) = self.gain_settings.lock() {
            settings.amp_enabled = enabled;
        }
        Ok(())
    }
}

impl Drop for HackRfController {
    fn drop(&mut self) {
        self.receiving.store(false, Ordering::SeqCst);
        if let Some(handle) = self.rx_thread.take() {
            let _ = handle.join();
        }
    }
}

/// Open HackRf either from the FD device or by scanning.
fn open_device(usb_fd_device: Option<&nusb::Device>) -> Result<HackRf> {
    if let Some(dev) = usb_fd_device {
        HackRf::from_nusb_device(dev.clone(), 0, waverave_hackrf::HackRfType::One).map_err(|e| anyhow::anyhow!(e))
    } else {
        open_hackrf().map_err(|e| anyhow::anyhow!(e))
    }
}

/// Check if HackRF is available
fn check_hackrf_available(usb_fd_device: Option<&nusb::Device>) -> bool {
    match open_device(usb_fd_device) {
        Ok(_) => {
            tracing::debug!("HackRF opened successfully");
            true
        }
        Err(e) => {
            tracing::debug!("HackRF not available: {:?}", e);
            match std::process::Command::new("hackrf_info")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
            {
                Ok(status) => status.success(),
                Err(_) => false,
            }
        }
    }
}

/// Run a demo receiver (no actual HackRF)
fn run_demo_receiver(
    receiving: Arc<AtomicBool>,
    _event_tx: Sender<RadioEvent>,
    _frequency: Arc<Mutex<u32>>,
) {
    tracing::info!("Demo receiver thread started (no HackRF)");

    while receiving.load(Ordering::SeqCst) {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    tracing::info!("Demo receiver thread stopped");
}

fn pairs_to_stored(pairs: &[LevelDuration]) -> Vec<StoredLevelDuration> {
    pairs
        .iter()
        .map(|p| StoredLevelDuration {
            level: p.level,
            duration_us: p.duration_us,
        })
        .collect()
}

/// Compute average magnitude of IQ buffer
fn compute_rssi(buffer: &[num_complex::Complex<i8>]) -> f32 {
    if buffer.is_empty() {
        return 0.0;
    }
    let sum_mag: f32 = buffer
        .iter()
        .map(|c| {
            let i = c.re as f32 / 128.0;
            let q = c.im as f32 / 128.0;
            (i * i + q * q).sqrt()
        })
        .sum();
    sum_mag / buffer.len() as f32
}

/// Process a buffer of samples
fn process_samples(
    buffer: &[num_complex::Complex<i8>],
    current_freq: u32,
    rssi_value: &Arc<AtomicU32>,
    demodulator_am: &Arc<Mutex<Demodulator>>,
    demodulator_fm: &Arc<Mutex<FmDemodulator>>,
    capture_id: &std::sync::atomic::AtomicU32,
    event_tx: &Sender<RadioEvent>,
) {
    rssi_value.store(compute_rssi(buffer).to_bits(), Ordering::Relaxed);

    let samples: Vec<i8> = buffer.iter().flat_map(|c| [c.re, c.im]).collect();

    if let Ok(mut demod) = demodulator_am.lock() {
        if let Some(pairs) = demod.process_samples(&samples) {
            let id = capture_id.fetch_add(1, Ordering::SeqCst);
            let capture = Capture::from_pairs_with_rf(
                id,
                current_freq,
                pairs_to_stored(&pairs),
                Some(RfModulation::AM),
            );
            let _ = event_tx.send(RadioEvent::SignalCaptured(capture));
        }
    }
    if let Ok(mut demod) = demodulator_fm.lock() {
        if let Some(pairs) = demod.process_samples(&samples) {
            let id = capture_id.fetch_add(1, Ordering::SeqCst);
            let capture = Capture::from_pairs_with_rf(
                id,
                current_freq,
                pairs_to_stored(&pairs),
                Some(RfModulation::FM),
            );
            let _ = event_tx.send(RadioEvent::SignalCaptured(capture));
        }
    }
}

/// Run the receiver loop with actual HackRF using waverave-hackrf
fn run_receiver_hackrf(
    receiving: Arc<AtomicBool>,
    event_tx: Sender<RadioEvent>,
    frequency: Arc<Mutex<u32>>,
    demodulator_am: Arc<Mutex<Demodulator>>,
    demodulator_fm: Arc<Mutex<FmDemodulator>>,
    gain_settings: Arc<Mutex<GainSettings>>,
    rssi_value: Arc<AtomicU32>,
    usb_fd_device: Option<nusb::Device>,
) -> Result<()> {
    tracing::info!("HackRF receiver thread starting...");

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("Failed to build tokio runtime")?;

    rt.block_on(async {
        let hackrf = open_device(usb_fd_device.as_ref())
            .context("Failed to open HackRF device")?;

        let freq = *frequency.lock().unwrap();
        let initial_gains = *gain_settings.lock().unwrap();
        tracing::info!(
            "Configuring HackRF: freq={} Hz, sample_rate={} Hz, LNA={} dB, VGA={} dB, AMP={}",
            freq, SAMPLE_RATE, initial_gains.lna_gain, initial_gains.vga_gain, initial_gains.amp_enabled
        );

        hackrf.set_sample_rate(SAMPLE_RATE as f64).await.context("Failed to set sample rate")?;
        hackrf.set_freq(freq as u64).await.context("Failed to set frequency")?;
        hackrf.set_lna_gain(initial_gains.lna_gain as u16).await.context("Failed to set LNA gain")?;
        hackrf.set_vga_gain(initial_gains.vga_gain as u16).await.context("Failed to set RXVGA gain")?;
        hackrf.set_amp_enable(initial_gains.amp_enabled).await.context("Failed to enable amp")?;

        tracing::info!("HackRF configured, starting RX (AM + FM demodulators)...");

        let mut rx = hackrf.start_rx(8192).await.context("Failed to start RX")?;

        let capture_id = std::sync::atomic::AtomicU32::new(0);
        let mut applied = initial_gains;

        while receiving.load(Ordering::SeqCst) {
            // Apply live gain settings
            if let Ok(current) = gain_settings.lock() {
                if current.lna_gain != applied.lna_gain {
                    applied.lna_gain = current.lna_gain;
                }
                if current.vga_gain != applied.vga_gain {
                    applied.vga_gain = current.vga_gain;
                }
                if current.amp_enabled != applied.amp_enabled {
                    applied.amp_enabled = current.amp_enabled;
                }
            }

            // Ensure we keep submitting buffers
            while rx.pending() < 4 {
                rx.submit();
            }

            // Await a buffer with timeout
            let timeout = tokio::time::sleep(std::time::Duration::from_millis(50));
            tokio::select! {
                result = rx.next_complete() => {
                    match result {
                        Ok(buf) => {
                            let data = buf.samples();

                            process_samples(
                                data,
                                freq,
                                &rssi_value,
                                &demodulator_am,
                                &demodulator_fm,
                                &capture_id,
                                &event_tx,
                            );
                        }
                        Err(e) => {
                            tracing::warn!("RX error: {:?}", e);
                        }
                    }
                }
                _ = timeout => {
                    // Timeout hit, continue loop to check `receiving` flag
                }
            }
        }

        rx.stop().await.context("Failed to stop RX")?;
        Ok::<(), anyhow::Error>(())
    })?;

    tracing::info!("HackRF receiver thread stopped");
    Ok(())
}

/// Transmit a signal via HackRF
fn transmit_signal_hackrf(signal: &[LevelDuration], frequency: u32, usb_fd_device: Option<&nusb::Device>) -> Result<()> {
    tracing::info!("Starting HackRF transmission at maximum power...");

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("Failed to build tokio runtime")?;

    rt.block_on(async {
        let hackrf = open_device(usb_fd_device)
            .context("Failed to open HackRF device")?;

        hackrf.set_sample_rate(SAMPLE_RATE as f64).await.context("Failed to set sample rate")?;
        hackrf.set_freq(frequency as u64).await.context("Failed to set frequency")?;
        hackrf.set_txvga_gain(47).await.context("Failed to set TXVGA gain")?;
        hackrf.set_amp_enable(true).await.context("Failed to enable amp")?;

        let tx_samples = generate_tx_samples(signal, SAMPLE_RATE);
        let total_samples = tx_samples.len();
        tracing::debug!("Generated {} TX samples", total_samples);

        let mut tx = hackrf.start_tx(8192).await.context("Failed to start TX")?;

        let mut buf_chunk = Vec::with_capacity(8192);
        for &(i, q) in &tx_samples {
            buf_chunk.push(num_complex::Complex::new(i, q));
            if buf_chunk.len() == buf_chunk.capacity() {
                let mut buf = tx.get_buffer();
                buf.clear();
                buf.extend_from_slice(&buf_chunk);
                tx.submit(buf);
                buf_chunk.clear();
            }
        }

        if !buf_chunk.is_empty() {
            let pad_len = buf_chunk.capacity() - buf_chunk.len();
            for _ in 0..pad_len {
                buf_chunk.push(num_complex::Complex::new(0, 0));
            }
            let mut buf = tx.get_buffer();
            buf.clear();
            buf.extend_from_slice(&buf_chunk);
            tx.submit(buf);
        }

        tx.stop().await.context("Failed to stop TX")?;
        Ok::<(), anyhow::Error>(())
    })?;

    tracing::info!("Transmission complete");
    Ok(())
}

/// Generate TX samples from level/duration pairs
fn generate_tx_samples(signal: &[LevelDuration], sample_rate: u32) -> Vec<(i8, i8)> {
    let mut samples = Vec::new();
    let samples_per_us = sample_rate as f64 / 1_000_000.0;

    for ld in signal {
        let num_samples = (ld.duration_us as f64 * samples_per_us) as usize;
        let value: i8 = if ld.level { 127 } else { 0 };

        for _ in 0..num_samples {
            samples.push((value, 0));
        }
    }

    samples
}
