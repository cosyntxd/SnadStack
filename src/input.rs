use crate::cells::CellType;
use pixels::Pixels;
use winit::{
    dpi::PhysicalPosition,
    event::{ElementState, Event, MouseButton, MouseScrollDelta, VirtualKeyCode, WindowEvent},
};

// A Useful abstraction for winit event_loop events
pub struct InputHelper {
    mouse_states: [bool; 3],
    pub current_mouse: PhysicalPosition<f32>,
    selection_size: f32,
    pub material: CellType,
}
impl InputHelper {
    pub fn new() -> Self {
        Self {
            mouse_states: [false; 3],
            current_mouse: PhysicalPosition::new(-1.0, -1.0),
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
                    self.current_mouse = position.cast();
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
    // An easy way to get cell coordinates (usize) from mouse position (f32)
    pub fn pixel_position(&mut self, pixels: &Pixels) -> Option<(usize, usize)> {
        pixels
            .window_pos_to_pixel((self.current_mouse.x, self.current_mouse.y))
            .ok()
    }
}
