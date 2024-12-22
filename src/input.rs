use crate::{cells::CellType, world::World};

use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::{ElementState, Event, MouseButton, MouseScrollDelta, VirtualKeyCode, WindowEvent},
    window::Window,
};
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct CenterLocation {
    pub x: i32,
    pub y: i32,
}
impl CenterLocation {
    pub fn new() -> Self {
        Self { x: 0, y: 0 }
    }
    pub fn from(size: &PhysicalSize<u32>, density: u32) -> Self {
        Self {
            x: (size.width as i32 / density as i32),
            y: (size.height as i32 / density as i32),
        }
    }
    pub fn difference(&mut self, rhs: CenterLocation) -> Self {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}

// A Useful abstraction for winit event_loop events
pub struct InputHelper {
    mouse_states: [bool; 3],
    max_size: f32,
    pub previous: Option<PhysicalPosition<f32>>,
    pub current_mouse: Option<PhysicalPosition<f32>>,
    selection_size: f32,
    pub material: CellType,
    env_density: u32,
    current_window_size: PhysicalSize<u32>,
    prev_center: CenterLocation,
    curr_center: CenterLocation,
}
impl InputHelper {
    pub fn new(env_density: u32, win: &Window) -> Self {
        let current_window_size = win.inner_size();
        Self {
            mouse_states: [false; 3],
            previous: None,
            current_mouse: None,
            selection_size: 8.0,
            material: CellType::Sand,
            max_size: 16.0,
            env_density,
            current_window_size,
            prev_center: CenterLocation::new(),
            curr_center: CenterLocation::new(),
        }
    }
    // Called for every event which comes from winit's event loop
    pub fn hook_events(&mut self, events: &Event<()>) {
        if let Event::WindowEvent { event, .. } = events {
            match event {
                WindowEvent::KeyboardInput { input, .. } => {
                    if let Some(key_code) = input.virtual_keycode {
                        self.material.switch_if_valid(key_code)
                    }
                }
                WindowEvent::MouseInput { state, button, .. } => {
                    let button_state = state == &ElementState::Pressed;
                    let index = Self::mouse_button_to_int(button);
                    self.mouse_states[index] = button_state;
                }
                WindowEvent::CursorMoved { position, .. } => {
                    self.current_mouse = Some(position.cast());
                }
                WindowEvent::MouseWheel { delta, .. } => {
                    // Touchpad/trackpad
                    if let MouseScrollDelta::PixelDelta(pos) = delta {
                        self.selection_size -= pos.y as f32 * self.max_size / 2000.0;
                    }
                    // Scroll wheel
                    if let MouseScrollDelta::LineDelta(_, y) = delta {
                        self.selection_size -= y * self.max_size / 10.0;
                    }
                    self.selection_size = self.selection_size.clamp(1.0, self.max_size);
                }
                WindowEvent::Resized(size) => {
                    self.current_window_size = size.clone();
                    self.curr_center = CenterLocation::from(size, self.env_density);
                    self.max_size = (size.width.max(size.width) / self.env_density) as f32;
                }
                _ => {}
            }
        }
        if let Event::RedrawEventsCleared = events {
            // Calculating here because many mouse move events may be sent in a single frame
            self.previous = self.current_mouse;
            self.prev_center = self.curr_center;
        }
    }
    fn mouse_button_to_int(button: &MouseButton) -> usize {
        match button {
            MouseButton::Left => 0,
            MouseButton::Middle => 1,
            MouseButton::Right => 2,
            MouseButton::Other(_) => 0,
        }
    }
    pub fn mouse_clicked(&mut self, button: MouseButton) -> bool {
        let index = Self::mouse_button_to_int(&button);
        self.mouse_states[index as usize]
    }
    pub fn selection_size(&mut self) -> isize {
        self.selection_size as isize
    }
    fn convert_position(&self, position: PhysicalPosition<f32>) -> PhysicalPosition<i32> {
        let d = self.env_density as i32;
        let position = position.cast::<i32>();
        let curr = self.current_window_size;
        PhysicalPosition::new(
            (position.x - ((curr.width % self.env_density) / 2) as i32) / d,
            (position.y - ((curr.height % self.env_density) / 2) as i32) / d,
        )
    }
    // An easy way to get cell coordinates (u32) from mouse position (f32)
    pub fn pixel_position(
        &mut self,
        _world: &World,
    ) -> Option<(PhysicalPosition<i32>, PhysicalPosition<i32>)> {
        match (self.current_mouse, self.previous) {
            (Some(current), Some(previous)) => {
                let current = self.convert_position(current);
                let previous = self.convert_position(previous);
                Some((current, previous))
            }
            (Some(current), None) => {
                let current = self.convert_position(current);
                Some((current, current))
            }
            _ => None,
        }
    }

    pub fn resized(&mut self) -> (PhysicalSize<u32>, Option<CenterLocation>) {
        let diff = self.curr_center.difference(self.prev_center);
        let diff = if diff == CenterLocation::default() {
            None
        } else {
            Some(diff)
        };
        (self.current_window_size.cast::<u32>(), (diff))
    }
}
