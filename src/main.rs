use winit::{
    window::WindowBuilder,
    event::{Event, WindowEvent, MouseButton},
    event_loop::EventLoop,
    dpi::LogicalSize,
};
use pixels::{
    PixelsBuilder,
    SurfaceTexture
};
use snad_stack::{
    cells::{Cell, CellType},
    world::World,
    input::InputHelper,
};

fn main() {
    // The cellular automata grid
    let cell_count = LogicalSize::new(200, 100);
    let mut enviornment = World::new(cell_count.width, cell_count.height, 16);

    // Event handlers
    let event_loop = EventLoop::new();
    let mut controller = InputHelper::new();

    // Window
    let window = WindowBuilder::new()
            .with_title("Snad Stack")
            .with_inner_size(cell_count)
            .with_min_inner_size(LogicalSize::new(150,75))
            .build(&event_loop)
            .expect("Could not instantiate window");

    // Create window surface
    let mut pixels = {
        let window_size = window.inner_size();
        let surface_texture = SurfaceTexture::new(
            window_size.width, window_size.height, &window);
        PixelsBuilder::new(cell_count.width as u32, cell_count.height as u32, surface_texture)
            .build()
            .expect("Could not instantiate Pixels")
    };

    // Run Every frame
    event_loop.run(move |event, _, control_flow| {
        control_flow.set_poll();
        controller.hook_events(&event);
        match event {
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                control_flow.set_exit();
            },
            Event::WindowEvent { event: WindowEvent::Resized(size), .. } => {
                let width = size.width / enviornment.density;
                let height = size.height / enviornment.density;
                pixels.resize_surface(size.width, size.height)
                    .expect("Failed to resize surface");
                pixels.resize_buffer(width, height)
                    .expect("Failed to resize buffer");
                enviornment.resize(width, height);
            },
            Event::MainEventsCleared => {
                if let Some((mouse_x, mouse_y)) = controller.pixel_position(&pixels) {
                    let mut cell = & mut enviornment.grid[mouse_y][mouse_x];
                    cell.selected = true;
                    if controller.mouse_clicked(MouseButton::Left) {
                        cell.rgb = Cell::rgb_ranges(CellType::Sand)
                    }
                    if controller.mouse_clicked(MouseButton::Right) {
                        cell.rgb = Cell::rgb_ranges(CellType::Air)
                    }
                    #[cfg(debug_assertions)]{
                        println!("{:?}", (mouse_x, mouse_y))
                    }
                }
                enviornment.render(pixels.get_frame_mut());
                window.request_redraw();
            },
            Event::RedrawRequested(_) => {
                if pixels.render().is_err(){
                    control_flow.set_exit();
                }
            }
            _ => {}
        }
    });
}