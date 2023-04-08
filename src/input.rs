use crate::{cells::CellType, world::World};

use winit::{
    dpi::PhysicalPosition,
    event::{ElementState, Event, MouseButton, MouseScrollDelta, VirtualKeyCode, WindowEvent},
};

// A Useful abstraction for winit event_loop events
pub struct InputHelper {
    mouse_states: [bool; 3],
    pub previous: Option<PhysicalPosition<f32>>,
    pub current_mouse: Option<PhysicalPosition<f32>>,
    selection_size: f32,
    pub material: CellType,
}
impl InputHelper {
    pub fn new() -> Self {
        Self {
            mouse_states: [false; 3],
            previous: None,
            current_mouse: None,
            selection_size: 8.0,
            material: CellType::Sand,
        }
    }
    // Called for every event which comes from winit's event loop
    pub fn hook_events(&mut self, events: &Event<()>) {
        if let Event::WindowEvent { event, .. } = events {
            match event {
                WindowEvent::KeyboardInput { input, .. } => {
                    if let Some(key_code) = input.virtual_keycode {
                        self.material = match key_code {
                            VirtualKeyCode::A => CellType::Air,
                            VirtualKeyCode::S => CellType::Sand,
                            VirtualKeyCode::W => CellType::Water,
                            VirtualKeyCode::R => CellType::Stone,
                            // If the key is not recognized, keep currently selected material
                            _ => self.material,
                        };
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
                        self.selection_size -= pos.y as f32 / 16.0;
                    }
                    // Scroll wheel
                    if let MouseScrollDelta::LineDelta(_, y) = delta {
                        self.selection_size -= y;
                    }
                    self.selection_size = self.selection_size.clamp(1.0, 128.0);
                }
                _ => {}
            }
        }
        if let Event::RedrawRequested(_) = events {
            // Calculating here because many mouse move events may be sent in a single frame
            self.previous = self.current_mouse;
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
    fn convert_position(position: PhysicalPosition<f32>, d: u32) -> PhysicalPosition<u32> {
        let position = position.cast::<u32>();
        PhysicalPosition::new(position.x / d, position.y / d)
    }
    // An easy way to get cell coordinates (u32) from mouse position (f32)
    pub fn pixel_position(
        &mut self,
        world: &World,
    ) -> Option<(PhysicalPosition<u32>, PhysicalPosition<u32>)> {
        let density = world.density;
        match (self.current_mouse, self.previous) {
            (Some(current), Some(previous)) => {
                let current = Self::convert_position(current, density);
                let previous = Self::convert_position(previous, density);
                Some((current, previous))
            }
            (Some(current), None) => {
                let current = Self::convert_position(current, density);
                Some((current, current))
            }
            _ => None,
        }
    }
}
