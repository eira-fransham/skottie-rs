extern crate skia_safe as skia;

use clap::{App, Arg};
use either::Either;
use glutin::dpi::LogicalSize;
#[cfg(windows)]
use glutin::platform::windows::WindowBuilderExtWindows;
use glutin::{
    event::{Event, KeyboardInput, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
    ContextBuilder, GlRequest,
};
use skia::{
    gpu::{gl::FramebufferInfo, BackendRenderTarget, SurfaceOrigin},
    ColorType, Surface,
};
use std::{convert::TryInto, time};

type WindowedContext = glutin::ContextWrapper<glutin::PossiblyCurrent, glutin::window::Window>;

fn main() {
    const WIDTH: usize = 800;
    const HEIGHT: usize = 600;

    let matches = App::new("Lottie Viewer")
        .arg(
            Arg::with_name("INPUT")
                .help("Sets the lottie file to play")
                .required(true)
                .index(1),
        )
        .get_matches();
    let filename = std::path::Path::new(matches.value_of_os("INPUT").unwrap());

    // Calculate the right logical size of the window.
    let event_loop = EventLoop::new();
    let logical_window_size = LogicalSize::new(WIDTH as f64, HEIGHT as f64);

    // Open a window.
    let window_builder = WindowBuilder::new()
        .with_title("Minimal example")
        .with_inner_size(logical_window_size);
    #[cfg(windows)]
    let window_builder = window_builder.with_drag_and_drop(false);

    let gl_context = ContextBuilder::new()
        .with_gl(GlRequest::GlThenGles {
            opengl_version: (4, 6),
            opengles_version: (3, 1),
        })
        .with_multisampling(0)
        .with_hardware_acceleration(Some(true))
        .build_windowed(window_builder, &event_loop)
        .unwrap();

    // Load OpenGL, and make the context current.
    let gl_context = unsafe { gl_context.make_current().unwrap() };

    gl::load_with(|name| gl_context.get_proc_address(name));

    let mut gr_context = skia::gpu::Context::new_gl(None, None).unwrap();

    let fb_info = {
        let mut fboid: gl::types::GLint = 0;
        unsafe { gl::GetIntegerv(gl::FRAMEBUFFER_BINDING, &mut fboid) };

        FramebufferInfo {
            fboid: fboid.try_into().unwrap(),
            format: skia::gpu::gl::Format::RGB565.into(),
        }
    };

    fn create_surface(
        windowed_context: &WindowedContext,
        fb_info: &FramebufferInfo,
        gr_context: &mut skia::gpu::Context,
    ) -> skia::Surface {
        let pixel_format = windowed_context.get_pixel_format();
        let size = windowed_context.window().inner_size();
        let backend_render_target = BackendRenderTarget::new_gl(
            (
                size.width.try_into().unwrap(),
                size.height.try_into().unwrap(),
            ),
            pixel_format.multisampling.map(|s| s.try_into().unwrap()),
            pixel_format.stencil_bits.try_into().unwrap(),
            *fb_info,
        );
        Surface::from_backend_render_target(
            gr_context,
            &backend_render_target,
            SurfaceOrigin::BottomLeft,
            ColorType::RGB565,
            skia::ColorSpace::new_srgb(),
            &skia::SurfaceProps::with_options(Default::default(), skia::PixelGeometry::RGBH),
        )
        .unwrap()
    }

    let mut surface = create_surface(&gl_context, &fb_info, &mut gr_context);
    let sf = gl_context.window().scale_factor() as f32;
    surface.canvas().scale((sf, sf));

    let mut last = time::Instant::now();

    let mut now = time::Instant::now();
    let start = now;

    let num_frames = 1000;
    let mut times = Vec::with_capacity(num_frames);
    times.push(now - last);

    last = now;

    let mut file = std::fs::File::open(filename).unwrap();
    let mut to_render = match filename
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .as_ref()
        .map(|s| &s[..])
    {
        Some("json") | Some("lottie") => skia::animation::Animation::read(&mut file)
            .map(Either::Left)
            .expect("Failed to open lottie file"),
        Some("svg") => skia::svg::SvgDom::read(&mut file)
            .map(Either::Right)
            .expect("Failed to open lottie file"),
        other => panic!("Unrecognized filetype: {:?}", other),
    };

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;

        if times.len() >= num_frames {
            let avg = times.drain(..).take(num_frames).sum::<time::Duration>() / num_frames as u32;

            println!(
                "{:?} fps",
                time::Duration::new(1, 0).as_nanos() / avg.as_nanos()
            );
        }

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            }
            | Event::WindowEvent {
                event:
                    WindowEvent::KeyboardInput {
                        input:
                            KeyboardInput {
                                virtual_keycode: Some(VirtualKeyCode::Escape),
                                ..
                            },
                        ..
                    },
                ..
            } => {
                *control_flow = ControlFlow::Exit;
            }
            Event::WindowEvent {
                event: WindowEvent::Resized(physical_size),
                ..
            } => {
                gl_context.resize(physical_size);
                surface = create_surface(&gl_context, &fb_info, &mut gr_context);
            }
            Event::RedrawRequested(_) => {
                if let Either::Left(animation) = &mut to_render {
                    let dur = animation.duration();
                    animation.seek_time::<()>((now - start).as_secs_f64() % dur);
                }

                {
                    let canvas = surface.canvas();
                    canvas.clear(0xff_ff_ff_ff);

                    match &to_render {
                        Either::Left(animation) => animation.render(canvas, None),
                        Either::Right(svg) => svg.render(canvas),
                    }

                    canvas.flush();
                }

                gl_context.swap_buffers().unwrap();

                now = time::Instant::now();
                let this_dt = now - last;
                times.push(this_dt);
                last = now;
            }
            Event::MainEventsCleared => {
                gl_context.window().request_redraw();
            }
            _ => {}
        }
    });
}
