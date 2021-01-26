use crate::command::Command;
use crate::event::Event;
use futures::{channel::mpsc, StreamExt};
use glib::WeakRef;
use gst::{self, prelude::*};
use std::error;
use std::sync::{Arc, Mutex};
use std::thread;

pub struct Streamer {
    pipeline: gst::Pipeline,
    root_directory: std::string::String,
    rtmp_url: std::string::String,
}

impl Streamer {
    pub fn new(
        ingest_uri: std::string::String,
        root_directory: std::string::String,
        rtmp_url: std::string::String,
    ) -> Result<Self, Box<dyn error::Error>> {
        //let uri = "testbin://audio,volume=0.5,is-live=1+video,pattern=ball,is-live=1";
        let pipeline = gst::parse_launch(&format!(
            "compositor name=vmixer audiomixer name=amixer \
             wpesrc name=wpesrc location=http://127.0.0.1:3000 draw-background=0 \
             ! capsfilter name=wpecaps caps=\"video/x-raw,format=BGRA\" ! queue ! vmixer. \
             uridecodebin3 name=decoder uri={uri}",
            uri = ingest_uri
        ))?;

        // Upcast to a gst::Pipeline as the above function could've also returned an arbitrary
        // gst::Element if a different string was passed
        let pipeline = pipeline
            .downcast::<gst::Pipeline>()
            .expect("Couldn't downcast pipeline");

        // Request that the pipeline forwards us all messages, even those that it would otherwise
        // aggregate first
        pipeline.set_property_message_forward(true);

        Ok(Streamer {
            pipeline,
            root_directory,
            rtmp_url,
        })
    }

    fn setup_decoder(&self) {
        let decodebin = self
            .pipeline
            .get_by_name("decoder")
            .expect("decodebin not found");

        let pipeline_weak = self.pipeline.downgrade();
        decodebin.connect_pad_added(move |_decodebin, src_pad| {
            let pipeline = upgrade_weak!(pipeline_weak);
            let caps = src_pad.get_stream().unwrap().get_caps().unwrap();
            let s = caps.get_structure(0).unwrap();
            let name = s.get_name();

            if name.starts_with("audio/") {
                let mixer = pipeline
                    .get_by_name("amixer")
                    .expect("audio mixer not found");

                let convert = gst::ElementFactory::make("audioconvert", None).unwrap();
                let resample = gst::ElementFactory::make("audioresample", None).unwrap();
                let queue = gst::ElementFactory::make("queue", None).unwrap();
                pipeline.add_many(&[&convert, &resample, &queue]).unwrap();
                gst::Element::link_many(&[&convert, &resample, &queue, &mixer]).unwrap();

                queue.sync_state_with_parent().unwrap();
                resample.sync_state_with_parent().unwrap();
                convert.sync_state_with_parent().unwrap();

                let sink_pad = convert
                    .get_static_pad("sink")
                    .expect("Mixer sink pad request failed");
                src_pad.link(&sink_pad).expect("pad link failed");
            } else if name.starts_with("video/") {
                let mixer = pipeline
                    .get_by_name("vmixer")
                    .expect("video mixer not found");

                let sink_pad = mixer
                    .get_request_pad("sink_%u")
                    .expect("Mixer sink pad request failed");

                let width = s.get_some::<i32>("width").unwrap();
                let height = s.get_some::<i32>("height").unwrap();

                sink_pad
                    .set_properties(&[(&"zorder", &0u32), (&"width", &width), (&"height", &height)])
                    .expect("sink pad properties configuration failed");

                // Make sure the webview dimensions match the video stream dimensions.
                let wpe_caps_filter = pipeline
                    .get_by_name("wpecaps")
                    .expect("wpesrc capsfilter not found");
                let caps = gst::Caps::builder("video/x-raw")
                    .field("width", &width)
                    .field("height", &height)
                    .build();
                wpe_caps_filter.set_property("caps", &caps).unwrap();
                let wpe_src = pipeline.get_by_name("wpesrc").expect("wpesrc not found");
                let wpe_caps_sink_pad = wpe_src.get_static_pad("src").unwrap();
                wpe_caps_sink_pad.send_event(gst::event::Reconfigure::new());

                src_pad.link(&sink_pad).expect("pad link failed");
            }
        });

        self.pipeline.set_state(gst::State::Paused).unwrap();
    }

    fn setup_outgoing_streams(&self) -> Result<(), glib::BoolError> {
        let bin_description = &format!(
            "flvmux streamable=1 name=mux ! rtmp2sink enable-last-sample=0 async-connect=0 location=\"{location}\" \
             queue name=stream-vqueue ! videoconvert ! x264enc tune=zerolatency ! tee name=vtee ! queue ! mux.video \
             queue name=stream-aqueue ! audioconvert ! audioresample ! fdkaacenc ! tee name=atee ! queue ! mux.audio \
             hlssink2 name=hlssink location={hls_root}/segment%05d.ts playlist-location={hls_root}/playlist.m3u8 target-duration=3 \
             vtee. ! queue ! h264parse ! hlssink.video \
             atee. ! queue ! aacparse ! hlssink.audio \
             ",
            location = self.rtmp_url,
            hls_root = self.root_directory,
        );
        let bin = gst::parse_bin_from_description_with_name(bin_description, false, "stream-bin")
            .unwrap();

        let video_queue = bin
            .get_by_name("stream-vqueue")
            .expect("No stream-vqueue found");
        let video_mixer = self.pipeline.get_by_name("vmixer").expect("No mixer found");

        self.pipeline.add(&bin).expect("Failed to add bin");

        let srcpad = video_mixer
            .get_static_pad("src")
            .expect("Failed to get src pad from mixer");
        let sinkpad = video_queue
            .get_static_pad("sink")
            .expect("Failed to get sink pad from recording bin");

        if let Ok(video_ghost_pad) = gst::GhostPad::with_target(Some("video_sink"), &sinkpad) {
            bin.add_pad(&video_ghost_pad).unwrap();
            srcpad.link(&video_ghost_pad).unwrap();
        }

        let audio_mixer = self
            .pipeline
            .get_by_name("amixer")
            .expect("No audio mixer found");

        let audio_srcpad = audio_mixer
            .get_static_pad("src")
            .expect("Failed to get src pad from audio mixer");

        let audio_queue = bin
            .get_by_name("stream-aqueue")
            .expect("No stream-aqueue found");

        let audio_sinkpad = audio_queue
            .get_static_pad("sink")
            .expect("Failed to get sink pad from audio_queue");

        if let Ok(audio_ghost_pad) = gst::GhostPad::with_target(Some("audio_sink"), &audio_sinkpad)
        {
            bin.add_pad(&audio_ghost_pad).unwrap();
            audio_srcpad.link(&audio_ghost_pad).unwrap();
        }

        bin.sync_state_with_parent()
    }

    fn start(&self) -> Result<gst::StateChangeSuccess, gst::StateChangeError> {
        self.pipeline.set_state(gst::State::Playing)
    }

    fn stop(&self) -> Result<gst::StateChangeSuccess, gst::StateChangeError> {
        self.pipeline.set_state(gst::State::Null)
    }

    fn handle_command(&self, command: Command) {
        match command {
            Command::Pause => {
                self.pipeline.set_state(gst::State::Paused).unwrap();
            }
            Command::Resume => {
                self.start().unwrap();
            }
        }
    }
}

async fn message_handler(
    main_loop: glib::MainLoop,
    pipeline_weak: WeakRef<gst::Pipeline>,
    mut tx: mpsc::Sender<Event>,
) {
    let pipeline = upgrade_weak!(pipeline_weak);
    let bus = pipeline.get_bus().unwrap();
    let mut messages = bus.stream();
    while let Some(msg) = messages.next().await {
        use gst::MessageView;

        match msg.view() {
            MessageView::Eos(..) => {
                tx.try_send(Event::Eos {}).expect("Send failed");
                main_loop.quit();
            }
            MessageView::StateChanged(state) => {
                if let Some(element) = msg.get_src() {
                    if element == pipeline {
                        let new_state = state.get_current();
                        let bin_ref = pipeline.upcast_ref::<gst::Bin>();
                        let filename =
                            format!("streamer-{:#?}_to_{:#?}", state.get_old(), new_state);
                        bin_ref.debug_to_dot_file_with_ts(gst::DebugGraphDetails::all(), filename);
                        tx.try_send(Event::StateChanged { state: new_state })
                            .expect("Send failed");
                    }
                }
            }
            MessageView::Error(err) => {
                if let Some(element) = msg.get_src() {
                    let bin_ref = element.downcast_ref::<gst::Bin>().unwrap();
                    let filename = format!("streamer-error");
                    bin_ref.debug_to_dot_file_with_ts(gst::DebugGraphDetails::all(), filename);
                }
                tx.try_send(Event::Error {
                    message: err.get_error().to_string(),
                    stack: format!("{:?}", err.get_debug().unwrap()),
                })
                .expect("Send failed");
                main_loop.quit();
            }
            _ => (),
        }
    }
}

pub fn event_thread(streamer: Streamer) -> (mpsc::Receiver<Event>, mpsc::Sender<Command>) {
    // Create sending and receiving channels for the event data
    let (tx, events_rx) = mpsc::channel(1000);
    let (command_tx, mut command_rx) = mpsc::channel(1000);

    let streamer = Arc::new(Mutex::new(streamer));

    thread::spawn(move || {
        let ctx = glib::MainContext::default();
        ctx.push_thread_default();
        let mainloop = glib::MainLoop::new(Some(&ctx), false);

        // Input commands handling
        let streamer_weak = Arc::downgrade(&streamer);
        let command_handler = async move {
            while let Some(input_command) = command_rx.next().await {
                if let Some(streamer) = streamer_weak.upgrade() {
                    if let Ok(s) = streamer.lock() {
                        s.handle_command(input_command);
                    }
                }
            }
        };
        ctx.spawn_local(command_handler);

        if let Ok(streamer) = streamer.lock() {
            let pipeline_weak = streamer.pipeline.downgrade();
            let mainloop_clone = mainloop.clone();
            streamer.setup_decoder();
            streamer.setup_outgoing_streams().unwrap();
            ctx.spawn_local(message_handler(mainloop_clone, pipeline_weak, tx));
        }

        mainloop.run();

        if let Ok(streamer) = streamer.lock() {
            streamer.stop().unwrap();
        }
        ctx.pop_thread_default();
    });

    (events_rx, command_tx)
}
