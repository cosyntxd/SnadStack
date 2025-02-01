use pixels::{PixelsBuilder, SurfaceTexture};
use std::rc::Rc;
use winit::{
    dpi::LogicalSize,
    event::{Event, MouseButton, WindowEvent},
    event_loop::EventLoop,
    window::WindowBuilder,
};

use crate::simulate::world::World;

use super::input::InputHelper;

pub fn run() {
    // The cellular automata grid
    let cell_count = LogicalSize::new(600, 400);
    let mut enviornment = World::new(cell_count.width, cell_count.height, 12);
    // Event handlers
    let event_loop = EventLoop::new();

    // Window
    let window = WindowBuilder::new()
        .with_title("Snad Stack")
        .with_inner_size(cell_count)
        .with_min_inner_size(cell_count)
        .build(&event_loop)
        .expect("Could not instantiate window");

    let mut controller = InputHelper::new(enviornment.density, &window);

    let window = Rc::new(window);

    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        use winit::platform::web::WindowExtWebSys;

        let window = Rc::clone(&window);

        // Select element to append canvas to
        let target_div = web_sys::window()
            .and_then(|win| win.document())
            .and_then(|win| win.body())
            .expect("Failed to select div");

        target_div
            .append_child(&web_sys::Element::from(window.canvas()))
            .ok()
            .expect("Failed to append canvas");

        // Get client window size
        let get_element_size = || {
            let client_window = web_sys::window().unwrap();
            LogicalSize::new(
                client_window.inner_width().unwrap().as_f64().unwrap() - 256.0,
                client_window.inner_height().unwrap().as_f64().unwrap(),
            )
        };

        window.set_inner_size(get_element_size());

        // Register resize event to resize window
        let closure = wasm_bindgen::closure::Closure::wrap(Box::new(move |_e: web_sys::Event| {
            window.set_inner_size(get_element_size())
        }) as Box<dyn FnMut(_)>);
        web_sys::window()
            .unwrap()
            .add_event_listener_with_callback("resize", closure.as_ref().unchecked_ref())
            .expect("Failed to register resize event");
        closure.forget();
    }

    // Create window surface
    let mut pixels = {
        let window_size = window.inner_size();
        let surface_texture =
            SurfaceTexture::new(window_size.width, window_size.height, window.as_ref());
        PixelsBuilder::new(
            cell_count.width as u32,
            cell_count.height as u32,
            surface_texture,
        )
        .build()
        .expect("Could not instantiate Pixels")
    };
    enviornment.render(pixels.frame_mut());

    // Run Every frame
    event_loop.run(move |event, _, control_flow| {
        // println!("{event:?}");
        enviornment.render(pixels.frame_mut());

        control_flow.set_poll();
        controller.hook_events(&event);
        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                control_flow.set_exit();
            }
            Event::WindowEvent {
                event: WindowEvent::Resized(size),
                ..
            } => {
                let width = size.width / enviornment.density;
                let height = size.height / enviornment.density;
                pixels
                    .resize_surface(size.width, size.height)
                    .expect("Failed to resize surface");
                pixels
                    .resize_buffer(width, height)
                    .expect("Failed to resize buffer");
                let (resize, expand) = controller.resized();
                if let Some(e) = expand {
                    println!("{width:?} {height:?}");
                    enviornment.resize(
                        (resize.width / enviornment.density) as usize,
                        (resize.height / enviornment.density) as usize,
                        e,
                    );
                }
                enviornment.render(pixels.frame_mut());
            }
            Event::MainEventsCleared => {
                for i in 0..1 {
                    enviornment.simulate(2, pixels.frame_mut());
                }
                if let Some((current, previous)) = controller.pixel_position(&enviornment) {
                    enviornment.draw_thick_line(
                        current.x,
                        current.y,
                        previous.x,
                        previous.y,
                        controller.selection_size(),
                        controller.material,
                        controller.mouse_clicked(MouseButton::Left),
                        true,
                        pixels.frame_mut(),
                    );
                }
                window.request_redraw();
            }

            Event::RedrawRequested(_) => {
                if let Some((current, previous)) = controller.pixel_position(&enviornment) {
                    enviornment.draw_thick_line(
                        current.x,
                        current.y,
                        previous.x,
                        previous.y,
                        controller.selection_size(),
                        controller.material,
                        false,
                        true,
                        pixels.frame_mut(),
                    );
                }
                if let Err(e) = pixels.render() {
                    log::warn!("{e}");
                    control_flow.set_exit();
                }
                if let Some((current, previous)) = controller.pixel_position(&enviornment) {
                    enviornment.draw_thick_line(
                        current.x,
                        current.y,
                        previous.x,
                        previous.y,
                        controller.selection_size(),
                        controller.material,
                        false,
                        false,
                        pixels.frame_mut(),
                    );
                }
            }
            _ => {}
        }
    });
}
//
