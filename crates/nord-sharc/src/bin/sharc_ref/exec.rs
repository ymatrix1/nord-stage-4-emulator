//! Scenario executor: drives a [`Core`] and collects the captured series.
use crate::script::{parse_line, substitute, Cmd};
use crate::signal::generate;
use crate::voice::voice_pokes;
use nord_sharc::core::Core;

/// One captured output series (a name and its f32 samples, in capture order).
#[derive(Debug, Clone, PartialEq)]
pub struct Series {
    pub name: String,
    pub samples: Vec<f32>,
}

/// Result of a scenario run.
#[derive(Debug, Default)]
pub struct Render {
    pub meta: Vec<(String, String)>,
    pub series: Vec<Series>,
    /// Steps executed by every `run` command, in order.
    pub run_steps: Vec<u64>,
}

pub struct Runner {
    pub core: Core,
    pub render: Render,
    /// Next free word of the command FIFO (0 until the first `msg`).
    pub fifo_wr: u32,
    /// First word of the command FIFO ring (`isr::FIFO_BASE` unless a `fifo` command moved it).
    pub fifo_base: u32,
    /// An interrupt stopped at a `break` pc: (start step, saved IMASK, vector).
    pub irq_pending: Option<(u64, u32, u8)>,
}

impl Runner {
    pub fn new(image: &[u8], lenient: bool) -> Self {
        let mut core = Core::new(image);
        core.lenient = lenient;
        Runner {
            core,
            render: Render::default(),
            fifo_wr: 0,
            fifo_base: crate::isr::FIFO_BASE,
            irq_pending: None,
        }
    }

    fn series_mut(&mut self, name: &str) -> &mut Series {
        if let Some(k) = self.render.series.iter().position(|s| s.name == name) {
            return &mut self.render.series[k];
        }
        self.render.series.push(Series {
            name: name.to_string(),
            samples: Vec::new(),
        });
        self.render.series.last_mut().expect("just pushed")
    }

    fn poke(&mut self, addr: u32, value: u32) -> Result<(), String> {
        self.core
            .dm_write(addr, value)
            .map_err(|a| format!("bad data address 0x{a:x}"))
    }

    /// Executes `cmds`; a `Loop` substitutes its variable into each body line and parses it.
    pub fn exec(&mut self, cmds: &[Cmd]) -> Result<(), String> {
        for cmd in cmds {
            self.exec_one(cmd)?;
        }
        Ok(())
    }

    fn exec_one(&mut self, cmd: &Cmd) -> Result<(), String> {
        match cmd {
            Cmd::Meta(k, v) => self.render.meta.push((k.clone(), v.clone())),
            Cmd::Warmup {
                entry,
                steps,
                flags,
            } => {
                for (n, high) in flags {
                    self.core.set_flag_input(*n, *high);
                }
                self.core.pc = *entry;
                if let Some(f) = self.core.run(*steps) {
                    return Err(format!("warmup fault: {f:?}"));
                }
            }
            Cmd::Poke(addr, value) => self.poke(*addr, *value)?,
            Cmd::Fill {
                addr,
                count,
                kind,
                opts,
            } => {
                let n0 = crate::signal::num(opts, "n0", 0.0)? as usize;
                let stride = crate::signal::num(opts, "stride", 1.0)? as u32;
                let samples = generate(kind, n0 + count, opts)?;
                for (k, v) in samples[n0..].iter().enumerate() {
                    self.poke(addr + stride * k as u32, v.to_bits())?;
                }
            }
            Cmd::Voice(k, fields) => {
                for (addr, value) in voice_pokes(*k, fields)? {
                    self.poke(addr, value)?;
                }
            }
            Cmd::Set(code, value) => self.core.write_ureg(*code, *value),
            Cmd::Run { entry, max_steps } => {
                let depth = self.core.return_addresses().len();
                let start = self.core.steps;
                self.core.enter_handler(*entry);
                while self.core.return_addresses().len() > depth {
                    if self.core.steps - start >= *max_steps {
                        return Err(format!(
                            "handler 0x{entry:x} did not return within {max_steps} steps (pc 0x{:x})",
                            self.core.pc
                        ));
                    }
                    self.core
                        .step()
                        .map_err(|f| format!("fault in run: {f:?}"))?;
                }
                self.render.run_steps.push(self.core.steps - start);
            }
            Cmd::Irq {
                vector,
                max_steps,
                break_pc,
            } => self.run_irq(*vector, *max_steps, *break_pc)?,
            Cmd::Resume { max_steps } => self.resume_irq(*max_steps)?,
            Cmd::Msg { op, addr, value } => self.push_msg(*op, *addr, *value)?,
            Cmd::Dev(addr, value) => self.core.preset_device(*addr, *value),
            Cmd::Fifo(base) => {
                self.fifo_base = *base;
                self.fifo_wr = 0;
            }
            Cmd::Capture {
                name,
                addr,
                count,
                stride,
                part,
            } => {
                let mut words = Vec::with_capacity(*count);
                for k in 0..*count as u32 {
                    let a = addr + stride * k;
                    let w = self
                        .core
                        .dm_read(a)
                        .map_err(|a| format!("bad capture address 0x{a:x}"))?;
                    words.push(part.pick(w));
                }
                self.series_mut(name).samples.extend(words);
            }
            Cmd::Loop { count, var, body } => {
                for i in 0..i64::from(*count) {
                    for line in body {
                        self.exec_one(&parse_line(&substitute(line, var, i)?)?)?;
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::parse_script;

    fn runner() -> Runner {
        Runner::new(&[0u8; 16], false)
    }

    #[test]
    fn poke_fill_and_capture_round_trip() {
        let mut r = runner();
        let cmds = parse_script(
            "poke 0xA0000 f0.5\nfill 0xA0001 3 ramp start=1 step=1 stride=2\ncapture a 0xA0000 1\ncapture b 0xA0001 3 stride=2\n",
        )
        .unwrap();
        r.exec(&cmds).unwrap();
        assert_eq!(r.render.series[0].samples, vec![0.5]);
        assert_eq!(r.render.series[1].samples, vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn loop_substitutes_into_fill_options() {
        let mut r = runner();
        let cmds =
            parse_script("loop 2\nfill 0xA0000 2 ramp step=1 n0={i*2}\ncapture s 0xA0000 2\nend\n")
                .unwrap();
        r.exec(&cmds).unwrap();
        assert_eq!(r.render.series[0].samples, vec![0.0, 1.0, 2.0, 3.0]);
    }

    #[test]
    fn set_voice_and_device_space_work() {
        let mut r = runner();
        let cmds = parse_script(
            "set R3 5
voice 0 gain=0.5 ratio=1
capture g 0x92408 2
poke 0x7fffffff f1.0
capture d 0x7fffffff 1
",
        )
        .unwrap();
        r.exec(&cmds).unwrap();
        assert_eq!(r.render.series[0].samples, vec![0.5, 0.5]);
        assert_eq!(r.render.series[1].samples, vec![1.0]);
        assert_eq!(r.core.read_ureg(3), 5);
        assert!(r.exec(&parse_script("voice 0 bogus=1").unwrap()).is_err());
    }

    #[test]
    fn run_returns_from_a_handler_and_meta_is_kept() {
        // image of zeros: short word 0 decodes as NOP; an RTI is needed to return, so a bounded
        // run reports the missing return instead of hanging
        let mut r = runner();
        let cmds = parse_script("meta rate 48000\nrun 0 5").unwrap();
        assert!(r.exec(&cmds).is_err());
        assert_eq!(r.render.meta[0], ("rate".into(), "48000".into()));
    }
}
