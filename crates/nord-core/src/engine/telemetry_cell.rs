//! Lock-free single-writer / many-reader cell for [`Telemetry`] (a seqlock over atomic words).
//!
//! Spec: `docs/specs/telemetry.md`. The audio thread calls [`TelemetryCell::publish`]; GUI threads
//! call [`TelemetryCell::read`]. Neither side locks, waits or allocates; the payload is stored as
//! `AtomicU32` words, so there is no data race (and no `unsafe`). A reader that overlaps a write
//! sees an odd or changed sequence number and retries, so it can never return a torn snapshot.

use super::telemetry::{Telemetry, WORDS};
use std::sync::atomic::{fence, AtomicU32, Ordering};

/// Reader retry budget; the writer holds the odd sequence for a few dozen stores, so one or two
/// retries are the practical maximum.
const MAX_ATTEMPTS: usize = 64;

/// Seqlock cell holding the newest [`Telemetry`]. Exactly one thread may call [`Self::publish`].
#[derive(Debug)]
pub struct TelemetryCell {
    seq: AtomicU32,
    words: [AtomicU32; WORDS],
}

impl Default for TelemetryCell {
    fn default() -> Self {
        Self::new()
    }
}

impl TelemetryCell {
    /// Empty cell: [`Self::read`] returns `None` until the first publish.
    pub fn new() -> Self {
        Self {
            seq: AtomicU32::new(0),
            words: std::array::from_fn(|_| AtomicU32::new(0)),
        }
    }

    /// Stores `t` (writer thread only). Wait-free, allocation-free.
    pub fn publish(&self, t: &Telemetry) {
        let words = t.to_words();
        let s = self.seq.load(Ordering::Relaxed);
        self.seq.store(s.wrapping_add(1), Ordering::Relaxed); // odd: write in progress
        fence(Ordering::Release);
        for (slot, w) in self.words.iter().zip(words) {
            slot.store(w, Ordering::Relaxed);
        }
        self.seq.store(s.wrapping_add(2), Ordering::Release); // even: stable
    }

    /// Number of completed publishes (wraps at `u32::MAX / 2`).
    pub fn publishes(&self) -> u32 {
        self.seq.load(Ordering::Relaxed) / 2
    }

    /// Newest consistent snapshot, or `None` before the first publish (or after
    /// `MAX_ATTEMPTS` consecutive overlaps with the writer).
    pub fn read(&self) -> Option<Telemetry> {
        for _ in 0..MAX_ATTEMPTS {
            let s1 = self.seq.load(Ordering::Acquire);
            if s1 == 0 {
                return None;
            }
            if s1 & 1 == 1 {
                std::hint::spin_loop();
                continue;
            }
            let mut words = [0u32; WORDS];
            for (w, slot) in words.iter_mut().zip(&self.words) {
                *w = slot.load(Ordering::Relaxed);
            }
            fence(Ordering::Acquire);
            if self.seq.load(Ordering::Relaxed) == s1 {
                return Some(Telemetry::from_words(&words));
            }
        }
        None
    }
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    #[test]
    fn empty_cell_reads_none_then_the_published_value() {
        let cell = TelemetryCell::new();
        assert!(cell.read().is_none());
        let mut t = Telemetry::default();
        t.bpm = 97.5;
        cell.publish(&t);
        assert_eq!(cell.read(), Some(t));
        assert_eq!(cell.publishes(), 1);
        t.bpm = 98.0;
        cell.publish(&t);
        assert_eq!(cell.read().map(|x| x.bpm), Some(98.0));
        assert_eq!(cell.publishes(), 2);
    }

    /// A snapshot whose every field is derived from `n`; a torn read mixes two values of `n`.
    fn stamped(n: u32) -> Telemetry {
        let mut t = Telemetry::default();
        t.midi_in_count = n;
        t.midi_out_count = n.wrapping_mul(3);
        t.midi_out_dropped = !n;
        t.layer_level = [n as f32; 7];
        t.held_notes = [n; 4];
        t.bpm = n as f32;
        t
    }

    fn is_consistent(t: &Telemetry) -> bool {
        let n = t.midi_in_count;
        t.midi_out_count == n.wrapping_mul(3)
            && t.midi_out_dropped == !n
            && t.layer_level.iter().all(|&l| l == n as f32)
            && t.held_notes == [n; 4]
            && t.bpm == n as f32
    }

    /// Stress: one writer republishes as fast as it can while readers verify every snapshot.
    #[test]
    fn concurrent_readers_never_see_a_torn_snapshot() {
        let cell = Arc::new(TelemetryCell::new());
        let stop = Arc::new(AtomicBool::new(false));
        let writer = {
            let (cell, stop) = (Arc::clone(&cell), Arc::clone(&stop));
            std::thread::spawn(move || {
                let mut n = 1u32;
                while !stop.load(Ordering::Relaxed) {
                    // 2^24 keeps `n as f32` exact, so the float fields compare equal.
                    cell.publish(&stamped(n));
                    n = if n >= 1 << 24 { 1 } else { n + 1 };
                }
                n
            })
        };
        let readers: Vec<_> = (0..3)
            .map(|_| {
                let cell = Arc::clone(&cell);
                std::thread::spawn(move || {
                    let mut ok = 0u32;
                    let t0 = std::time::Instant::now();
                    while t0.elapsed() < std::time::Duration::from_millis(400) {
                        if let Some(t) = cell.read() {
                            assert!(is_consistent(&t), "torn snapshot: {t:?}");
                            ok += 1;
                        }
                    }
                    ok
                })
            })
            .collect();
        let reads: u32 = readers.into_iter().map(|r| r.join().unwrap()).sum();
        stop.store(true, Ordering::Relaxed);
        let last = writer.join().unwrap();
        assert!(reads > 1000, "readers made only {reads} consistent reads");
        assert!(last > 1, "writer never advanced");
    }
}
