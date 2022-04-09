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

    // uridecodebin3 works for reconnect where uridecodebin does not
    let uridecodebin = gst::ElementFactory::make("uridecodebin3", Some("uridecodebin3"))?;
    uridecodebin.try_set_property("uri", uri)?;
    uridecodebin.connect("source-setup", false, move |args| {
        let source = args[1].get::<gst::Element>().unwrap();
        if source.class().type_().name() == "GstRTSPSrc" {
            // set protocol to TCP
            source
                .try_set_property("protocols", gst_rtsp::RTSPLowerTrans::TCP)
                .unwrap();
        }

        None
    });

    // create a sink and add a probe so can verify buffers are being received from uridecodebin3 (i.e. connection is working)
    let fakesink = gst::ElementFactory::make("fakesink", Some("fakesink"))?;
    fakesink
        .static_pad("sink")
        .unwrap()
        .add_probe(gst::PadProbeType::BUFFER, |_, probe_info| {
            if let Some(gst::PadProbeData::Buffer(_)) = probe_info.data {
                println!("fakesink received buffer");
            }
            gst::PadProbeReturn::Ok
        });

    pipeline.add_many(&[&uridecodebin, &fakesink])?;

    // link to fakesink after connection but only for video_*
    let pipeline_weak = pipeline.downgrade();
    uridecodebin.connect_pad_added(move |_, src_pad| {
        if src_pad.name().starts_with("video_") {
            let pipeline = match pipeline_weak.upgrade() {
                Some(pipeline) => pipeline,
                None => return,
            };

            let fakesink = pipeline.by_name("fakesink").unwrap();
            let sink_pad = fakesink.static_pad("sink").unwrap();
            src_pad.link(&sink_pad).unwrap();
        }
    });

    pipeline.set_state(gst::State::Playing)?;

    bus.add_watch(move |_, message| {
        if let Some(src) = message.src() {
            let parent = src.parent();

            let grandparent = parent.clone().map(|parent| parent.parent()).unwrap_or(None);

            if src.name() == "uridecodebin3"
                || parent
                    .map(|parent| parent.name() == "uridecodebin3")
                    .unwrap_or(false)
                || grandparent
                    .map(|grandparent| grandparent.name() == "uridecodebin3")
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