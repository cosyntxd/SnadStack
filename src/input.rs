use winit::{
    dpi::PhysicalPosition,
    event::{Event, WindowEvent, MouseButton, ElementState}
};
use pixels::Pixels;

// A Useful abstraction for winit event_loop events
pub struct InputHelper {
    mouse_states: [bool; 3],
    pub current_mouse: PhysicalPosition<f32>,
}
impl InputHelper {
    pub fn new() -> Self {
        Self {
            mouse_states: [false; 3],
            current_mouse: PhysicalPosition::new(-1.0, -1.0)
        }
    }
    // Called for every event which comes from winit's event loop
    pub fn hook_events(&mut self, events: &Event::<()>) {
        if let Event::WindowEvent {event, .. } = events {
            match event {
                WindowEvent::MouseInput { state, button, ..} => {
                    let button_state = state == &ElementState::Pressed;
                    let index = Self::mouse_button_to_int(button);
                    self.mouse_states[index] = button_state;
                },
                WindowEvent::CursorMoved { position, ..} => {
                    self.current_mouse = position.cast();
                },
                _ => {}
            }
        }
    }
    fn mouse_button_to_int(button: &MouseButton) -> usize {
        match button {
            MouseButton::Left   => 0,
            MouseButton::Middle => 1,
            MouseButton::Right  => 2,
            MouseButton::Other(_) => 0,
        }
    }
    pub fn mouse_clicked(&mut self, button: MouseButton) -> bool {
        let index = Self::mouse_button_to_int(&button);
        self.mouse_states[index as usize]
    }
    // An easy way to get cell coordinates (usize) from mouse position (f32)
    pub fn pixel_position(&mut self, pixels: &Pixels) -> Option<(usize, usize)> {
        pixels.window_pos_to_pixel((self.current_mouse.x, self.current_mouse.y)).ok()
    }
}