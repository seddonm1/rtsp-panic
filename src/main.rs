use gst::prelude::*;
use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    gst::init()?;

    let args: Vec<_> = env::args().collect();
    let uri: &str = if args.len() == 2 {
        args[1].as_ref()
    } else {
        println!("Usage: uridecodebin file_path");
        std::process::exit(-1)
    };

    let pipeline = gst::Pipeline::new(None);
    let bus = pipeline.bus().unwrap();
    let main_loop = gst::glib::MainLoop::new(None, false);

    let uridecodebin = gst::ElementFactory::make("uridecodebin", Some("uridecodebin"))?;
    uridecodebin.try_set_property("uri", uri)?;

    // create a sink and add a probe so can verify buffers are being received from uridecodebin (i.e. connection is working)
    let fakesink = gst::ElementFactory::make("fakesink", Some("fakesink"))?;
    fakesink
        .static_pad("sink")
        .unwrap()
        .add_probe(gst::PadProbeType::BUFFER, |_, probe_info| {
            if let Some(gst::PadProbeData::Buffer(ref _buffer)) = probe_info.data {
                println!("fakesink received buffer");
            }

            gst::PadProbeReturn::Ok
        });

    pipeline.add_many(&[&uridecodebin, &fakesink])?;

    // link to fakesink after connection
    let pipeline_weak = pipeline.downgrade();
    uridecodebin.connect_pad_added(move |source_element, _src_pad| {
        let pipeline = match pipeline_weak.upgrade() {
            Some(pipeline) => pipeline,
            None => return,
        };

        let fakesink = pipeline.by_name("fakesink").unwrap();
        source_element.link(&fakesink).unwrap();
    });

    pipeline.set_state(gst::State::Playing)?;

    bus.add_watch(move |_, message| {
        println!("{:?}", message);

        // listen for any errors/warnings from uridecodebin (or children) (like disconnection or failure to connect events)
        if let Some(src) = message.src() {
            let parent = src.parent();

            if src.name() == "uridecodebin"
                || parent
                    .map(|parent| parent.name() == "uridecodebin")
                    .unwrap_or(false)
            {
                match message.view() {
                    gst::MessageView::Error(_) | gst::MessageView::Warning(_) => {
                        // try to reconnect
                        uridecodebin.set_state(gst::State::Null).unwrap();
                        std::thread::sleep(std::time::Duration::from_millis(3000));
                        uridecodebin.set_state(gst::State::Playing).unwrap();
                    }
                    _ => (),
                }
            }
        }

        gst::glib::Continue(true)
    })?;

    // block
    main_loop.run();

    pipeline.set_state(gst::State::Null)?;

    Ok(())
}
