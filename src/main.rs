use pixels::{PixelsBuilder, SurfaceTexture};
use snad_stack::{input::InputHelper, world::World};
use std::{rc::Rc, time::Duration};
use winit::{
    dpi::LogicalSize,
    event::{Event, MouseButton, WindowEvent},
    event_loop::EventLoop,
    window::WindowBuilder,
};

fn main() {
    #[cfg(target_arch = "wasm32")]
    {
        std::panic::set_hook(Box::new(console_error_panic_hook::hook));
        console_log::init_with_level(log::Level::Info).expect("Failed setting logger");
        wasm_bindgen_futures::spawn_local(run());
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        pollster::block_on(run());
    }
}

async fn run() {
    // The cellular automata grid
    let cell_count = LogicalSize::new(600, 400);
    let mut enviornment = World::new(cell_count.width, cell_count.height, 9);
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
        .build_async()
        .await
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
                        (resize.width as u32 / enviornment.density) as usize,
                        (resize.height as u32 / enviornment.density) as usize,
                        e,
                    );
                }
                enviornment.render(pixels.frame_mut());
            }
            Event::MainEventsCleared => {
                for i in 0..3 {
                    enviornment.simulate(3, pixels.frame_mut());
                }
                if let Some((current, previous)) = controller.pixel_position(&enviornment) {
                    enviornment.draw_thick_line(
                        current.x as i32,
                        current.y as i32,
                        previous.x as i32,
                        previous.y as i32,
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
                if let Err(e) = pixels.render() {
                    log::warn!("{e}");
                    control_flow.set_exit();
                } else if let Some((current, previous)) = controller.pixel_position(&enviornment) {
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
