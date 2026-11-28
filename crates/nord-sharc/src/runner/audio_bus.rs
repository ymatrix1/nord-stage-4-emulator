//! Inter-chip audio bus of the LLE system (bean `ns4-lle4`): stereo frames produced by the tone
//! chips (ORG, PNO, SY1SY2, SMP) travel into the FX cores and out to a host stereo buffer, with no
//! WAV hand-over in between.
//!
//! The real path is SPORT/DAI audio DMA between the chips (`dsp-peripherals-audio.md`: the transmit
//! side never appears as a DSP access, so the transfer is modelled by its effect, like the ARM
//! messages of [`super::msg`]). Model, everything ASSUMED until a bus capture exists:
//!
//! - one **tick** = one base period of the system (the pass period of the fastest chip); a node
//!   with `every() = k` runs a pass on every k-th tick;
//! - every node reads `in_frames()` stereo frames per pass from its input FIFO (zero padded and
//!   counted in [`AudioBus::underruns`] when the FIFO is short) and returns the frames its pass
//!   produced;
//! - frames produced in tick `t` reach their destination at the end of tick `t`, so a consumer
//!   sees them from tick `t + 1` on: one pass of latency per hop, whatever the node order, which
//!   keeps the result independent of how the nodes are scheduled (sequential here, threads later);
//! - several routes into one destination add sample by sample, each times its `gain`;
//! - [`Dest::Host`] appends to the host stereo buffer, which is the system output.
//!
//! Nothing here allocates on a real-time path: the LLE system is an offline oracle (no real time,
//! `docs/research/sharc-lle-realtime.md`), so plain `Vec`s are used.
use std::collections::VecDeque;

/// One stereo frame `[left, right]`.
pub type Frame = [f32; 2];

/// A chip (or any test double) the bus can run.
pub trait BusNode {
    /// Stereo frames the node consumes per pass (0 for a pure source).
    fn in_frames(&self) -> usize;
    /// Runs a pass on every `every()`-th tick (at least 1).
    fn every(&self) -> u32 {
        1
    }
    /// The core behind the node, for inspection (None for a test double).
    fn core_mut(&mut self) -> Option<&mut crate::core::Core> {
        None
    }
    /// Runs one pass with `input` (exactly `in_frames()` frames) and returns the frames produced.
    fn run_pass(&mut self, input: &[Frame]) -> Result<Vec<Frame>, String>;
    /// Queues one ARM message (`word1` = DM word or FIFO address, `word2` = value) for delivery
    /// before the node's next pass (the live path of `ns4-lle5`). `false` when the node takes none.
    fn post(&mut self, _word1: u32, _word2: u32) -> bool {
        false
    }
}

/// Where a route delivers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dest {
    /// Input FIFO of node `n`.
    Node(usize),
    /// The host stereo buffer.
    Host,
}

/// One connection of the bus: the output of node `from` times `gain` into `to`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Route {
    pub from: usize,
    pub to: Dest,
    pub gain: f32,
}

/// The host stereo output.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stereo {
    pub left: Vec<f32>,
    pub right: Vec<f32>,
}

/// Nodes, routes, FIFOs and the host buffer.
pub struct AudioBus {
    nodes: Vec<Box<dyn BusNode>>,
    routes: Vec<Route>,
    fifos: Vec<VecDeque<Frame>>,
    /// The system output so far.
    pub host: Stereo,
    /// Ticks run so far.
    pub tick: u64,
    /// Frames each node was short of at its passes (zero padded).
    pub underruns: Vec<u64>,
    /// Host wall time spent in each node's passes, nanoseconds (a measurement, not part of the
    /// deterministic output).
    pub node_ns: Vec<u64>,
    /// What each node took in and put out so far (input side, output side).
    pub node_levels: Vec<NodeLevels>,
}

/// Level statistics of one signal point: peak, RMS and first non-zero tick over the finite samples;
/// `non_finite` counts the NaN and infinite ones, which are left out of the other fields, so a node
/// that outputs NaN does not read as a clean 0.0.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Level {
    pub peak: f32,
    sumsq: f64,
    pub samples: u64,
    /// NaN or infinite samples seen (not in `peak`, RMS or `samples`).
    pub non_finite: u64,
    /// First tick with a non-zero sample.
    pub first_nonzero_tick: Option<u64>,
}

impl Level {
    /// Folds `frames` seen at `tick`.
    pub fn add(&mut self, frames: &[Frame], tick: u64) {
        for x in frames.iter().flatten() {
            if !x.is_finite() {
                self.non_finite += 1;
                continue;
            }
            self.peak = self.peak.max(x.abs());
            self.sumsq += f64::from(*x).powi(2);
            self.samples += 1;
            if *x != 0.0 && self.first_nonzero_tick.is_none() {
                self.first_nonzero_tick = Some(tick);
            }
        }
    }

    /// RMS over all samples folded so far.
    pub fn rms(&self) -> f64 {
        (self.sumsq / self.samples.max(1) as f64).sqrt()
    }
}

/// Input and output [`Level`] of a node.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NodeLevels {
    pub input: Level,
    pub output: Level,
}

impl AudioBus {
    /// A bus over `nodes` (indexed in order) and `routes`; a route that names a missing node is
    /// an error.
    pub fn new(nodes: Vec<Box<dyn BusNode>>, routes: Vec<Route>) -> Result<Self, String> {
        let n = nodes.len();
        for r in &routes {
            let bad = |i: usize| format!("route {r:?}: node {i} does not exist ({n} nodes)");
            if r.from >= n {
                return Err(bad(r.from));
            }
            if let Dest::Node(t) = r.to {
                if t >= n {
                    return Err(bad(t));
                }
            }
        }
        Ok(Self {
            fifos: vec![VecDeque::new(); n],
            underruns: vec![0; n],
            node_ns: vec![0; n],
            node_levels: vec![NodeLevels::default(); n],
            nodes,
            routes,
            host: Stereo::default(),
            tick: 0,
        })
    }

    /// Frames waiting in the input FIFO of node `n`.
    pub fn queued(&self, n: usize) -> usize {
        self.fifos[n].len()
    }

    /// The `every()` of node `n` (1 for a missing node).
    pub fn every(&self, n: usize) -> u32 {
        self.nodes.get(n).map_or(1, |x| x.every())
    }

    /// Hands an ARM message to node `n` (see [`BusNode::post`]); `false` when it takes none.
    pub fn post(&mut self, n: usize, word1: u32, word2: u32) -> bool {
        self.nodes.get_mut(n).is_some_and(|x| x.post(word1, word2))
    }

    /// Puts `frames` in front of node `n`'s FIFO contents (start-up fill, host input).
    pub fn push_input(&mut self, n: usize, frames: &[Frame]) {
        self.fifos[n].extend(frames.iter().copied());
    }

    /// One tick: the due nodes run their passes, then the produced frames are routed.
    pub fn run_tick(&mut self) -> Result<(), String> {
        let mut produced: Vec<Option<Vec<Frame>>> = vec![None; self.nodes.len()];
        for (i, node) in self.nodes.iter_mut().enumerate() {
            if !self.tick.is_multiple_of(u64::from(node.every().max(1))) {
                continue;
            }
            let need = node.in_frames();
            let mut input = Vec::with_capacity(need);
            while input.len() < need {
                match self.fifos[i].pop_front() {
                    Some(f) => input.push(f),
                    None => {
                        input.push([0.0; 2]);
                        self.underruns[i] += 1;
                    }
                }
            }
            let t0 = std::time::Instant::now();
            let out = node
                .run_pass(&input)
                .map_err(|e| format!("tick {} node {i}: {e}", self.tick))?;
            self.node_ns[i] += t0.elapsed().as_nanos() as u64;
            self.node_levels[i].input.add(&input, self.tick);
            self.node_levels[i].output.add(&out, self.tick);
            produced[i] = Some(out);
        }
        let mut to_node: Vec<Vec<Frame>> = vec![Vec::new(); self.nodes.len()];
        let mut to_host: Vec<Frame> = Vec::new();
        for r in &self.routes {
            let Some(out) = &produced[r.from] else {
                continue;
            };
            let acc = match r.to {
                Dest::Node(t) => &mut to_node[t],
                Dest::Host => &mut to_host,
            };
            if acc.len() < out.len() {
                acc.resize(out.len(), [0.0; 2]);
            }
            for (a, f) in acc.iter_mut().zip(out) {
                a[0] += r.gain * f[0];
                a[1] += r.gain * f[1];
            }
        }
        for (fifo, frames) in self.fifos.iter_mut().zip(to_node) {
            fifo.extend(frames);
        }
        for f in to_host {
            self.host.left.push(f[0]);
            self.host.right.push(f[1]);
        }
        self.tick += 1;
        Ok(())
    }

    /// Runs `ticks` ticks.
    pub fn run(&mut self, ticks: u32) -> Result<(), String> {
        (0..ticks).try_for_each(|_| self.run_tick())
    }

    /// The nodes, for inspection after a run.
    pub fn nodes_mut(&mut self) -> &mut [Box<dyn BusNode>] {
        &mut self.nodes
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Source that emits `frames` frames of a counter (left = n, right = -n) per pass.
    pub struct Counter {
        pub n: f32,
        pub frames: usize,
        pub every: u32,
    }

    impl BusNode for Counter {
        fn in_frames(&self) -> usize {
            0
        }
        fn every(&self) -> u32 {
            self.every
        }
        fn run_pass(&mut self, _: &[Frame]) -> Result<Vec<Frame>, String> {
            Ok((0..self.frames)
                .map(|_| {
                    self.n += 1.0;
                    [self.n, -self.n]
                })
                .collect())
        }
    }

    /// Consumer that doubles its input.
    pub struct Doubler {
        pub frames: usize,
        pub every: u32,
    }

    impl BusNode for Doubler {
        fn in_frames(&self) -> usize {
            self.frames
        }
        fn every(&self) -> u32 {
            self.every
        }
        fn run_pass(&mut self, input: &[Frame]) -> Result<Vec<Frame>, String> {
            Ok(input.iter().map(|f| [2.0 * f[0], 2.0 * f[1]]).collect())
        }
    }

    fn counter(frames: usize, every: u32) -> Box<dyn BusNode> {
        Box::new(Counter {
            n: 0.0,
            frames,
            every,
        })
    }

    fn route(from: usize, to: Dest, gain: f32) -> Route {
        Route { from, to, gain }
    }

    #[test]
    fn a_hop_costs_one_tick_and_the_first_pass_underruns() {
        let nodes: Vec<Box<dyn BusNode>> = vec![
            counter(2, 1),
            Box::new(Doubler {
                frames: 2,
                every: 1,
            }),
        ];
        let routes = vec![route(0, Dest::Node(1), 1.0), route(1, Dest::Host, 1.0)];
        let mut bus = AudioBus::new(nodes, routes).unwrap();
        bus.run(3).unwrap();
        // tick 0: Doubler sees zeros (underrun of 2 frames); ticks 1, 2 see the counter frames
        assert_eq!(bus.underruns, vec![0, 2]);
        assert_eq!(bus.host.left, vec![0.0, 0.0, 2.0, 4.0, 6.0, 8.0]);
        assert_eq!(bus.host.right, vec![0.0, 0.0, -2.0, -4.0, -6.0, -8.0]);
    }

    #[test]
    fn routes_add_with_their_gains_and_a_slow_consumer_drains_its_fifo() {
        let nodes: Vec<Box<dyn BusNode>> = vec![
            counter(2, 1),
            counter(2, 1),
            Box::new(Doubler {
                frames: 4,
                every: 2,
            }),
        ];
        let routes = vec![
            route(0, Dest::Node(2), 1.0),
            route(1, Dest::Node(2), 0.5),
            route(2, Dest::Host, 1.0),
        ];
        let mut bus = AudioBus::new(nodes, routes).unwrap();
        bus.push_input(2, &[[1.0, 1.0]; 4]); // start-up fill: no underrun on tick 0
        bus.run(4).unwrap();
        assert_eq!(bus.underruns[2], 0);
        // tick 0: input = fill (x2 = 2.0); tick 2 input = 4 frames of (a + 0.5 b) from ticks 0, 1:
        // tick 0 frames 1,2 (a) + 0.5 * (1,2) (b) = 1.5, 3.0; tick 1 frames 3,4 -> 4.5, 6.0
        assert_eq!(bus.host.left, vec![2.0, 2.0, 2.0, 2.0, 3.0, 6.0, 9.0, 12.0]);
        assert_eq!(
            bus.queued(2),
            4,
            "ticks 2 and 3 refilled it for the pass of tick 4"
        );
    }

    #[test]
    fn bad_routes_and_failing_nodes_are_reported() {
        let n = |k: usize| (0..k).map(|_| counter(1, 1)).collect::<Vec<_>>();
        assert!(AudioBus::new(n(1), vec![route(1, Dest::Host, 1.0)]).is_err());
        assert!(AudioBus::new(n(1), vec![route(0, Dest::Node(3), 1.0)]).is_err());
        struct Bad;
        impl BusNode for Bad {
            fn in_frames(&self) -> usize {
                0
            }
            fn run_pass(&mut self, _: &[Frame]) -> Result<Vec<Frame>, String> {
                Err("stuck".into())
            }
        }
        let mut bus = AudioBus::new(vec![Box::new(Bad)], vec![]).unwrap();
        let e = bus.run_tick().unwrap_err();
        assert!(e.contains("tick 0 node 0: stuck"), "{e}");
    }

    #[test]
    fn nodes_without_a_message_path_refuse_posts_and_the_bus_times_the_nodes() {
        let mut bus = AudioBus::new(vec![counter(1, 1)], vec![]).unwrap();
        assert!(!bus.post(0, 1, 2), "a test double takes no ARM message");
        assert!(!bus.post(7, 1, 2), "no such node");
        bus.run(3).unwrap();
        assert_eq!(bus.node_ns.len(), 1);
        let l = bus.node_levels[0];
        assert!(
            l.output.peak > 0.0 && l.output.rms() > 0.0,
            "the counter source is not silent"
        );
        assert_eq!((l.output.first_nonzero_tick, l.input.samples), (Some(0), 0));
        let mut z = Level::default();
        z.add(&[[0.0, f32::NAN], [f32::INFINITY, 0.0]], 4);
        assert_eq!((z.peak, z.samples, z.first_nonzero_tick), (0.0, 2, None));
        assert_eq!(
            z.non_finite, 2,
            "NaN and inf are counted, not read as silence"
        );
    }
}
