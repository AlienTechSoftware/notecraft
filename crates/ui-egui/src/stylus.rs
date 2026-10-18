//! Pen pressure, as far as the window layer hands it to us.
//!
//! This is the measuring end of the pen and drawing work (`work\requests\pen-and-drawing.md`),
//! not the drawing end. It answers one question: does a real pen's pressure reach this
//! window, and what is it worth?
//!
//! What the stack below us does, read in the sources of the versions we build against
//! (eframe and egui 0.36.2, egui-winit 0.36.2, winit 0.30.13):
//!
//! - **Windows**: winit reads `WM_POINTER`, recognises `PT_PEN` and normalises
//!   `POINTER_PEN_INFO::pressure` over 1024 levels; egui-winit forwards that as
//!   `egui::Event::Touch { force }` beside the ordinary pointer events, so the pointer still
//!   moves the caret and the force arrives alongside it.
//! - **macOS, Linux and Wayland**: winit drops the tablet fields entirely, so no force
//!   arrives and a pen is indistinguishable from a mouse here. Covering those needs a
//!   per-platform reader, which is decision 1 of the request.
//! - **Tilt, barrel rotation and the eraser end**: not exposed by winit on any platform.
//!   The word "tilt" does not appear in its source.
//!
//! So on Windows this reports real pressure, and elsewhere it honestly reports a mouse.
//! Nothing here guesses: a device that sends no force is a mouse and writes at full
//! pressure, which is what a mouse should do.

/// What the pen is doing now.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    /// 0 to 1. A mouse, and any device that reports no force, is always 1.
    pub pressure: f32,
    /// A device that reports pressure has been used in this window.
    pub pen: bool,
    /// A contact is on the glass now.
    pub down: bool,
}

impl Default for Sample {
    fn default() -> Self {
        Self { pressure: 1.0, pen: false, down: false }
    }
}

/// Follows the touch events of a window and keeps the latest pen reading.
#[derive(Clone, Debug, Default)]
pub struct Stylus {
    pen: bool,
    pressure: f32,
    down: bool,
    highest: f32,
    samples: u64,
}

impl Stylus {
    /// Takes this frame's input events. Call it once per frame, before anything asks.
    pub fn update(&mut self, events: &[egui::Event]) {
        for e in events {
            let egui::Event::Touch { phase, force, .. } = e else { continue };
            // A force at all means the device reports pressure: that is a pen, and from
            // then on this window believes what it says, including a reading of zero.
            let reported = force.filter(|f| f.is_finite()).map(|f| f.clamp(0.0, 1.0));
            if reported.is_some() {
                self.pen = true;
            }
            match phase {
                egui::TouchPhase::Start | egui::TouchPhase::Move => {
                    self.down = true;
                    if self.pen {
                        self.pressure = reported.unwrap_or(0.0);
                        self.highest = self.highest.max(self.pressure);
                        self.samples += 1;
                    }
                }
                egui::TouchPhase::End | egui::TouchPhase::Cancel => {
                    self.down = false;
                    self.pressure = 0.0;
                }
            }
        }
    }

    /// The reading now. A window that has never seen a pen answers like a mouse.
    pub fn sample(&self) -> Sample {
        if !self.pen {
            return Sample { pressure: 1.0, pen: false, down: self.down };
        }
        Sample { pressure: self.pressure, pen: true, down: self.down }
    }

    /// The hardest this pen has been pressed in this window, and how many readings have
    /// arrived. Both are for the diagnostic: a pen that only ever reports 0 or 1 is a pen
    /// the platform is not reading properly.
    pub fn seen(&self) -> (f32, u64) {
        (self.highest, self.samples)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(phase: egui::TouchPhase, force: Option<f32>) -> egui::Event {
        egui::Event::Touch { device_id: egui::TouchDeviceId(0), id: egui::TouchId(0), phase, pos: egui::Pos2::ZERO, force }
    }

    #[test]
    fn a_window_with_no_pen_writes_at_full_pressure() {
        let mut s = Stylus::default();
        assert_eq!(s.sample(), Sample { pressure: 1.0, pen: false, down: false });
        // A touch screen that reports no force is not a pen, and must not write at nothing.
        s.update(&[touch(egui::TouchPhase::Start, None), touch(egui::TouchPhase::Move, None)]);
        assert_eq!(s.sample(), Sample { pressure: 1.0, pen: false, down: true });
        assert_eq!(s.seen(), (0.0, 0));
    }

    #[test]
    fn a_pen_is_believed_once_it_reports_a_force() {
        let mut s = Stylus::default();
        s.update(&[touch(egui::TouchPhase::Start, Some(0.25)), touch(egui::TouchPhase::Move, Some(0.8))]);
        assert_eq!(s.sample(), Sample { pressure: 0.8, pen: true, down: true });
        assert_eq!(s.seen(), (0.8, 2), "the hardest press and how many readings arrived");

        // Once a pen is known, a reading with no force is a reading of nothing, not a mouse:
        // winit reports a press lighter than the pen's first level that way.
        s.update(&[touch(egui::TouchPhase::Move, None)]);
        assert_eq!(s.sample().pressure, 0.0);
        assert!(s.sample().pen, "it is still a pen");

        // Lifting ends the contact.
        s.update(&[touch(egui::TouchPhase::End, Some(0.1))]);
        assert_eq!(s.sample(), Sample { pressure: 0.0, pen: true, down: false });
    }

    #[test]
    fn junk_readings_do_not_get_through() {
        let mut s = Stylus::default();
        s.update(&[touch(egui::TouchPhase::Start, Some(f32::NAN))]);
        assert!(!s.sample().pen, "a NaN is not a reading");
        s.update(&[touch(egui::TouchPhase::Move, Some(9.0)), touch(egui::TouchPhase::Move, Some(-3.0))]);
        let p = s.sample().pressure;
        assert!((0.0..=1.0).contains(&p), "pressure stays in range: {p}");
        assert_eq!(s.seen().0, 1.0, "the out of range high reading was clamped, not dropped");
    }
}
