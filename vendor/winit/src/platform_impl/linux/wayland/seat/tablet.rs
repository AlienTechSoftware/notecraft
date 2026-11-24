//! Pen tablets from `zwp_tablet_manager_v2`.
//!
//! A compositor delivers a pen only to clients that bind the tablet protocol; without it a pen
//! produces no input at all on Wayland. A tool is reported the way the Windows backend reports a
//! pen: hovering moves the cursor (`CursorEntered`, `CursorMoved`, `CursorLeft`), and the tip on
//! the tablet is a touch (`Touch` with `Started`, `Moved`, `Ended`) carrying the pen's pressure as
//! a normalized force. Toolkits that turn the first touch into pointer presses (egui-winit does)
//! therefore draw with it as with a mouse, and see its pressure besides. The buttons on the
//! pen's barrel are mouse buttons (`MouseInput`): the upper one the right button and the lower one
//! the middle, as a compositor makes them for a client without the tablet protocol.
//!
//! Modelled on winit 0.31's `wp_tablet_input_v2`, written against the 0.30 event types.

use std::sync::Mutex;

use sctk::compositor::SurfaceData;
use sctk::globals::GlobalData;
use sctk::reexports::client::globals::{BindError, GlobalList};
use sctk::reexports::client::protocol::wl_seat::WlSeat;
use sctk::reexports::client::protocol::wl_surface::WlSurface;
use sctk::reexports::client::{
    delegate_dispatch, event_created_child, Connection, Dispatch, Proxy, QueueHandle,
};
use sctk::reexports::protocols::wp::tablet::zv2::client::zwp_tablet_manager_v2::ZwpTabletManagerV2;
use sctk::reexports::protocols::wp::tablet::zv2::client::zwp_tablet_pad_group_v2::{
    self, ZwpTabletPadGroupV2,
};
use sctk::reexports::protocols::wp::tablet::zv2::client::zwp_tablet_pad_ring_v2::ZwpTabletPadRingV2;
use sctk::reexports::protocols::wp::tablet::zv2::client::zwp_tablet_pad_strip_v2::ZwpTabletPadStripV2;
use sctk::reexports::protocols::wp::tablet::zv2::client::zwp_tablet_pad_v2::{self, ZwpTabletPadV2};
use sctk::reexports::protocols::wp::tablet::zv2::client::zwp_tablet_seat_v2::{
    self, ZwpTabletSeatV2,
};
use sctk::reexports::client::WEnum;
use sctk::reexports::protocols::wp::tablet::zv2::client::zwp_tablet_tool_v2::{
    ButtonState, Event as ToolEvent, ZwpTabletToolV2,
};
use sctk::reexports::protocols::wp::tablet::zv2::client::zwp_tablet_v2::ZwpTabletV2;

use crate::dpi::LogicalPosition;
use crate::event::{ElementState, Force, MouseButton, Touch, TouchPhase, WindowEvent};
use crate::platform_impl::wayland::state::WinitState;
use crate::platform_impl::wayland::{self, DeviceId};

/// Touch ids of tablet tools start here, well clear of a touchscreen's own ids.
const TOOL_TOUCH_ID_BASE: u64 = 1 << 40;

/// The pen's barrel buttons, as Linux numbers them (`linux/input-event-codes.h`).
const BTN_STYLUS: u32 = 0x14b;
const BTN_STYLUS2: u32 = 0x14c;
const BTN_STYLUS3: u32 = 0x149;

/// The mouse button a barrel button is: the lower one is the middle button, the upper one the
/// right, as libinput and the X Wacom driver have them; a third is the back button.
fn tool_button(code: u32) -> MouseButton {
    match code {
        BTN_STYLUS => MouseButton::Middle,
        BTN_STYLUS2 => MouseButton::Right,
        BTN_STYLUS3 => MouseButton::Back,
        other => MouseButton::Other(other.min(u32::from(u16::MAX)) as u16),
    }
}

/// The `zwp_tablet_manager_v2` global, bound when the compositor has it.
#[derive(Debug)]
pub struct TabletState {
    manager: ZwpTabletManagerV2,
}

impl TabletState {
    /// Bind the tablet manager. Version 1 has every tool event used here; version 2 only adds
    /// the pad's dial, which is not reported.
    pub fn new(
        globals: &GlobalList,
        queue_handle: &QueueHandle<WinitState>,
    ) -> Result<Self, BindError> {
        Ok(Self { manager: globals.bind(queue_handle, 1..=1, GlobalData)? })
    }

    /// The tablets, tools and pads of `seat`.
    pub fn get_tablet_seat(
        &self,
        seat: &WlSeat,
        queue_handle: &QueueHandle<WinitState>,
    ) -> ZwpTabletSeatV2 {
        self.manager.get_tablet_seat(seat, queue_handle, ())
    }
}

/// What one tool is doing, gathered until the compositor's `frame` says it is complete.
#[derive(Debug, Default)]
pub struct TabletToolData {
    inner: Mutex<TabletToolDataInner>,
}

#[derive(Debug, Default)]
struct TabletToolDataInner {
    /// Events of the frame in progress.
    pending: Vec<ToolStep>,
    /// The surface the tool is over, between proximity in and out.
    surface: Option<WlSurface>,
    /// Where the tool is, in the surface's logical coordinates.
    position: (f64, f64),
    /// The latest pressure, 0 to 65535, once the tool has reported one.
    pressure: Option<u32>,
    /// True while the tip is on the tablet.
    down: bool,
    /// True when the frame in progress changed the pressure.
    pressure_changed: bool,
}

#[derive(Debug, Clone, PartialEq)]
enum ToolStep {
    Enter(WlSurface),
    Moved,
    Down,
    Up,
    Left,
    /// A barrel button pressed (true) or let go.
    Button(MouseButton, bool),
}

impl TabletToolDataInner {
    fn force(&self) -> Option<Force> {
        self.pressure.map(|p| Force::Normalized(f64::from(p.min(65535)) / 65535.0))
    }
}

impl Dispatch<ZwpTabletManagerV2, GlobalData, WinitState> for TabletState {
    fn event(
        _: &mut WinitState,
        _: &ZwpTabletManagerV2,
        _: <ZwpTabletManagerV2 as Proxy>::Event,
        _: &GlobalData,
        _: &Connection,
        _: &QueueHandle<WinitState>,
    ) {
        // The manager has no events.
    }
}

impl Dispatch<ZwpTabletSeatV2, (), WinitState> for TabletState {
    event_created_child!(WinitState, ZwpTabletSeatV2, [
        zwp_tablet_seat_v2::EVT_TABLET_ADDED_OPCODE => (ZwpTabletV2, ()),
        zwp_tablet_seat_v2::EVT_TOOL_ADDED_OPCODE => (ZwpTabletToolV2, TabletToolData::default()),
        zwp_tablet_seat_v2::EVT_PAD_ADDED_OPCODE => (ZwpTabletPadV2, ()),
    ]);

    fn event(
        _: &mut WinitState,
        _: &ZwpTabletSeatV2,
        _: <ZwpTabletSeatV2 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<WinitState>,
    ) {
        // Tablets, tools and pads arrive as the new objects above.
    }
}

impl Dispatch<ZwpTabletV2, (), WinitState> for TabletState {
    fn event(
        _: &mut WinitState,
        tablet: &ZwpTabletV2,
        event: <ZwpTabletV2 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<WinitState>,
    ) {
        if let sctk::reexports::protocols::wp::tablet::zv2::client::zwp_tablet_v2::Event::Removed =
            event
        {
            tablet.destroy();
        }
    }
}

impl Dispatch<ZwpTabletToolV2, TabletToolData, WinitState> for TabletState {
    fn event(
        state: &mut WinitState,
        tool: &ZwpTabletToolV2,
        event: ToolEvent,
        data: &TabletToolData,
        _: &Connection,
        _: &QueueHandle<WinitState>,
    ) {
        let mut inner = data.inner.lock().unwrap();
        match event {
            ToolEvent::ProximityIn { surface, .. } => inner.pending.push(ToolStep::Enter(surface)),
            ToolEvent::ProximityOut => inner.pending.push(ToolStep::Left),
            ToolEvent::Down { .. } => inner.pending.push(ToolStep::Down),
            ToolEvent::Up => inner.pending.push(ToolStep::Up),
            ToolEvent::Button { button, state, .. } => {
                let pressed = matches!(state, WEnum::Value(ButtonState::Pressed));
                inner.pending.push(ToolStep::Button(tool_button(button), pressed));
            },
            ToolEvent::Motion { x, y } => {
                inner.position = (x, y);
                inner.pending.push(ToolStep::Moved);
            },
            ToolEvent::Pressure { pressure } => {
                inner.pressure = Some(pressure);
                inner.pressure_changed = true;
            },
            ToolEvent::Removed => {
                drop(inner);
                tool.destroy();
            },
            ToolEvent::Frame { .. } => {
                let pending = std::mem::take(&mut inner.pending);
                let steps = frame_steps(pending, std::mem::take(&mut inner.pressure_changed), inner.down);
                for step in steps {
                    if let ToolStep::Enter(surface) = &step {
                        inner.surface = Some(surface.clone());
                    }
                    deliver(state, tool, &mut inner, step);
                }
            },
            _ => (),
        }
    }
}

/// The steps a frame reports. A frame that changed only the pressure while the
/// tip is down moves the touch where it is, so pressing harder without moving
/// reaches the window; a frame that moves, or puts the tip down or up, already
/// carries the new pressure.
fn frame_steps(mut pending: Vec<ToolStep>, pressure_changed: bool, down: bool) -> Vec<ToolStep> {
    let reports_contact =
        pending.iter().any(|step| matches!(step, ToolStep::Moved | ToolStep::Down | ToolStep::Up));
    if pressure_changed && down && !reports_contact {
        pending.push(ToolStep::Moved);
    }
    pending
}

/// Report one step of a tool over one of our windows.
fn deliver(
    state: &mut WinitState,
    tool: &ZwpTabletToolV2,
    inner: &mut TabletToolDataInner,
    step: ToolStep,
) {
    let leaving = matches!(step, ToolStep::Left);
    let Some(surface) = inner.surface.clone() else { return };
    if leaving {
        inner.surface = None;
    }

    // Only a window's own surface: over its decorations or a subsurface the tool is not ours.
    let is_toplevel = surface
        .data::<SurfaceData>()
        .is_some_and(|data| data.parent_surface().map_or(true, |parent| *parent == surface));
    if !is_toplevel {
        return;
    }
    let window_id = wayland::make_wid(&surface);
    let scale_factor = match state.windows.get_mut().get(&window_id) {
        Some(window) => window.lock().unwrap().scale_factor(),
        None => {
            // Nothing can be told to a window that is gone, but the tool must not stay
            // down: the next window it hovers would get moves without a start.
            if matches!(step, ToolStep::Up | ToolStep::Left) {
                inner.down = false;
            }
            if leaving {
                inner.pressure = None;
            }
            return;
        },
    };
    let location = LogicalPosition::new(inner.position.0, inner.position.1).to_physical(scale_factor);
    let device_id = crate::event::DeviceId(crate::platform_impl::DeviceId::Wayland(DeviceId));
    let touch = |phase: TouchPhase, force: Option<Force>| {
        WindowEvent::Touch(Touch {
            device_id,
            phase,
            location,
            force,
            id: TOOL_TOUCH_ID_BASE + u64::from(tool.id().protocol_id()),
        })
    };

    let sink = &mut state.events_sink;
    match step {
        ToolStep::Enter(_) => {
            sink.push_window_event(WindowEvent::CursorEntered { device_id }, window_id);
            sink.push_window_event(WindowEvent::CursorMoved { device_id, position: location }, window_id);
        },
        ToolStep::Moved if inner.down => {
            sink.push_window_event(touch(TouchPhase::Moved, inner.force()), window_id)
        },
        ToolStep::Moved => {
            sink.push_window_event(WindowEvent::CursorMoved { device_id, position: location }, window_id)
        },
        ToolStep::Down => {
            inner.down = true;
            sink.push_window_event(touch(TouchPhase::Started, inner.force()), window_id);
        },
        ToolStep::Up => {
            if inner.down {
                inner.down = false;
                sink.push_window_event(touch(TouchPhase::Ended, inner.force()), window_id);
            }
        },
        ToolStep::Left => {
            if inner.down {
                inner.down = false;
                sink.push_window_event(touch(TouchPhase::Cancelled, inner.force()), window_id);
            }
            inner.pressure = None;
            sink.push_window_event(WindowEvent::CursorLeft { device_id }, window_id);
        },
        ToolStep::Button(button, pressed) => {
            let state = if pressed { ElementState::Pressed } else { ElementState::Released };
            sink.push_window_event(WindowEvent::MouseInput { device_id, state, button }, window_id);
        },
    }
}

impl Dispatch<ZwpTabletPadV2, (), WinitState> for TabletState {
    event_created_child!(WinitState, ZwpTabletPadV2, [
        zwp_tablet_pad_v2::EVT_GROUP_OPCODE => (ZwpTabletPadGroupV2, ()),
    ]);

    fn event(
        _: &mut WinitState,
        pad: &ZwpTabletPadV2,
        event: <ZwpTabletPadV2 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<WinitState>,
    ) {
        // The pad's buttons, rings and strips are not reported.
        if let zwp_tablet_pad_v2::Event::Removed = event {
            pad.destroy();
        }
    }
}

impl Dispatch<ZwpTabletPadGroupV2, (), WinitState> for TabletState {
    event_created_child!(WinitState, ZwpTabletPadGroupV2, [
        zwp_tablet_pad_group_v2::EVT_RING_OPCODE => (ZwpTabletPadRingV2, ()),
        zwp_tablet_pad_group_v2::EVT_STRIP_OPCODE => (ZwpTabletPadStripV2, ()),
    ]);

    fn event(
        _: &mut WinitState,
        _: &ZwpTabletPadGroupV2,
        _: <ZwpTabletPadGroupV2 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<WinitState>,
    ) {
    }
}

impl Dispatch<ZwpTabletPadRingV2, (), WinitState> for TabletState {
    fn event(
        _: &mut WinitState,
        _: &ZwpTabletPadRingV2,
        _: <ZwpTabletPadRingV2 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<WinitState>,
    ) {
    }
}

impl Dispatch<ZwpTabletPadStripV2, (), WinitState> for TabletState {
    fn event(
        _: &mut WinitState,
        _: &ZwpTabletPadStripV2,
        _: <ZwpTabletPadStripV2 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<WinitState>,
    ) {
    }
}

delegate_dispatch!(WinitState: [ZwpTabletManagerV2: GlobalData] => TabletState);
delegate_dispatch!(WinitState: [ZwpTabletSeatV2: ()] => TabletState);
delegate_dispatch!(WinitState: [ZwpTabletV2: ()] => TabletState);
delegate_dispatch!(WinitState: [ZwpTabletToolV2: TabletToolData] => TabletState);
delegate_dispatch!(WinitState: [ZwpTabletPadV2: ()] => TabletState);
delegate_dispatch!(WinitState: [ZwpTabletPadGroupV2: ()] => TabletState);
delegate_dispatch!(WinitState: [ZwpTabletPadRingV2: ()] => TabletState);
delegate_dispatch!(WinitState: [ZwpTabletPadStripV2: ()] => TabletState);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_barrel_buttons_are_the_middle_and_right_mouse_buttons() {
        assert_eq!(tool_button(0x14b), MouseButton::Middle);
        assert_eq!(tool_button(0x14c), MouseButton::Right);
        assert_eq!(tool_button(0x149), MouseButton::Back);
        assert_eq!(tool_button(0x150), MouseButton::Other(0x150));
    }

    #[test]
    fn a_frame_with_a_button_reports_it() {
        let press = ToolStep::Button(MouseButton::Right, true);
        assert_eq!(frame_steps(vec![press.clone()], false, false), vec![press]);
    }

    #[test]
    fn pressing_harder_without_moving_moves_the_touch_where_it_is() {
        assert_eq!(frame_steps(vec![], true, true), vec![ToolStep::Moved]);
    }

    #[test]
    fn a_frame_that_already_reports_the_tip_carries_the_pressure() {
        assert_eq!(frame_steps(vec![ToolStep::Moved], true, true), vec![ToolStep::Moved]);
        assert_eq!(frame_steps(vec![ToolStep::Down], true, false), vec![ToolStep::Down]);
        assert_eq!(frame_steps(vec![ToolStep::Up], true, true), vec![ToolStep::Up]);
    }

    #[test]
    fn pressure_with_the_tip_up_or_unchanged_adds_nothing() {
        assert_eq!(frame_steps(vec![], true, false), vec![]);
        assert_eq!(frame_steps(vec![], false, true), vec![]);
        assert_eq!(frame_steps(vec![ToolStep::Left], false, true), vec![ToolStep::Left]);
    }
}
