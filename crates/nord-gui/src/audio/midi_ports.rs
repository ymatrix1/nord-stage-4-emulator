//! MIDI input port list and selection shared between the GUI and the MIDI-in thread.
//!
//! The MIDI thread (`io.rs`) owns the `midir` connection. The GUI reads a snapshot of the port
//! names and asks for a rescan or another port through a channel; no lock is ever taken on the
//! audio thread (this state lives on the UI and MIDI threads only).

use crossbeam_channel::{unbounded, Receiver, Sender};
use std::sync::{Arc, Mutex};

/// Which port the MIDI thread should listen to.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum MidiChoice {
    /// The first port found (the behaviour before the selector existed).
    #[default]
    First,
    /// No MIDI input.
    Off,
    /// The port with this name, when present; otherwise nothing is connected.
    Port(String),
}

/// Request from the GUI to the MIDI thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MidiCtl {
    /// Re-read the port list (hot plug) and reconnect if the chosen port changed.
    Rescan,
    Select(MidiChoice),
}

/// What the GUI shows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MidiPortsView {
    pub names: Vec<String>,
    pub choice: MidiChoice,
    /// Name of the connected port.
    pub connected: Option<String>,
    /// Last problem ("No MIDI input devices found", connect error), empty when fine.
    pub status: String,
}

/// Index of the port to open for `choice`, if any.
pub fn pick_port(names: &[String], choice: &MidiChoice) -> Option<usize> {
    match choice {
        MidiChoice::First => (!names.is_empty()).then_some(0),
        MidiChoice::Off => None,
        MidiChoice::Port(n) => names.iter().position(|x| x == n),
    }
}

/// Cloneable GUI side of the port list.
#[derive(Clone)]
pub struct MidiPorts {
    view: Arc<Mutex<MidiPortsView>>,
    tx: Sender<MidiCtl>,
}

/// The MIDI thread's side.
pub struct MidiPortsWorker {
    view: Arc<Mutex<MidiPortsView>>,
    pub rx: Receiver<MidiCtl>,
}

/// Creates the two halves.
pub fn midi_ports() -> (MidiPorts, MidiPortsWorker) {
    let view = Arc::new(Mutex::new(MidiPortsView::default()));
    let (tx, rx) = unbounded();
    (
        MidiPorts {
            view: Arc::clone(&view),
            tx,
        },
        MidiPortsWorker { view, rx },
    )
}

impl MidiPorts {
    /// Copy of what the worker last published.
    pub fn snapshot(&self) -> MidiPortsView {
        self.view.lock().map(|v| v.clone()).unwrap_or_default()
    }

    pub fn rescan(&self) {
        let _ = self.tx.send(MidiCtl::Rescan);
    }

    pub fn select(&self, choice: MidiChoice) {
        if let Ok(mut v) = self.view.lock() {
            v.choice = choice.clone();
        }
        let _ = self.tx.send(MidiCtl::Select(choice));
    }
}

impl MidiPortsWorker {
    /// Publishes a scan result.
    pub fn publish(
        &self,
        names: Vec<String>,
        choice: MidiChoice,
        connected: Option<String>,
        status: String,
    ) {
        if let Ok(mut v) = self.view.lock() {
            *v = MidiPortsView {
                names,
                choice,
                connected,
                status,
            };
        }
    }
}

/// "MIDI input" control group: port combo box, Rescan button and the status line.
pub fn show(ui: &mut egui::Ui, ports: &MidiPorts) {
    let v = ports.snapshot();
    ui.horizontal(|ui| {
        ui.label("MIDI input");
        let shown = match (&v.choice, &v.connected) {
            (MidiChoice::Off, _) => "Off".to_string(),
            (_, Some(n)) => n.clone(),
            (MidiChoice::Port(n), None) => format!("{n} (not found)"),
            (MidiChoice::First, None) => "No device".to_string(),
        };
        egui::ComboBox::from_id_salt("ns4_midi_in_port")
            .selected_text(shown)
            .width(260.0)
            .show_ui(ui, |ui| {
                let mut pick = None;
                if ui
                    .selectable_label(v.choice == MidiChoice::First, "First available")
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    pick = Some(MidiChoice::First);
                }
                for n in &v.names {
                    if ui
                        .selectable_label(v.choice == MidiChoice::Port(n.clone()), n)
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        pick = Some(MidiChoice::Port(n.clone()));
                    }
                }
                if ui
                    .selectable_label(v.choice == MidiChoice::Off, "Off")
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    pick = Some(MidiChoice::Off);
                }
                if let Some(c) = pick {
                    ports.select(c);
                }
            })
            .response
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if ui
            .button("Rescan")
            .on_hover_text("Look for MIDI keyboards plugged in after the start")
            .clicked()
        {
            ports.rescan();
        }
        if !v.status.is_empty() {
            ui.weak(&v.status);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names() -> Vec<String> {
        vec!["Keys A".to_string(), "Keys B".to_string()]
    }

    #[test]
    fn pick_port_follows_the_choice() {
        let n = names();
        assert_eq!(pick_port(&n, &MidiChoice::First), Some(0));
        assert_eq!(pick_port(&[], &MidiChoice::First), None);
        assert_eq!(pick_port(&n, &MidiChoice::Off), None);
        assert_eq!(pick_port(&n, &MidiChoice::Port("Keys B".into())), Some(1));
        assert_eq!(pick_port(&n, &MidiChoice::Port("Gone".into())), None);
    }

    #[test]
    fn gui_requests_reach_the_worker_and_the_view_round_trips() {
        let (ports, worker) = midi_ports();
        assert_eq!(ports.snapshot(), MidiPortsView::default());
        ports.rescan();
        ports.select(MidiChoice::Port("Keys B".into()));
        let got: Vec<MidiCtl> = worker.rx.try_iter().collect();
        assert_eq!(
            got,
            [
                MidiCtl::Rescan,
                MidiCtl::Select(MidiChoice::Port("Keys B".into()))
            ]
        );
        worker.publish(
            names(),
            MidiChoice::First,
            Some("Keys A".into()),
            String::new(),
        );
        let v = ports.snapshot();
        assert_eq!(v.names.len(), 2);
        assert_eq!(v.connected.as_deref(), Some("Keys A"));
    }

    #[test]
    fn selector_draws_with_and_without_devices() {
        let (ports, worker) = midi_ports();
        let ctx = egui::Context::default();
        for connected in [None, Some("Keys A".to_string())] {
            worker.publish(names(), MidiChoice::First, connected, "x".into());
            let _ = ctx.run(Default::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| show(ui, &ports));
            });
        }
    }
}
