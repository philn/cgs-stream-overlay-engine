#[macro_use]
mod macros;
mod command;
mod event;
mod pipeline;

extern crate glib;
extern crate gstreamer as gst;
use crate::event::Event;
use crate::pipeline::{event_thread, Streamer};

use futures::channel::mpsc;
use std::sync::{Arc, Mutex};

use neon::context::{Context, TaskContext};
use neon::object::Object;
use neon::result::JsResult;
use neon::task::Task;
use neon::types::{JsFunction, JsString, JsUndefined, JsValue};
use neon::{declare_types, register_module};

/// Reading from a channel `Receiver` is a blocking operation. This struct
/// wraps the data required to perform a read asynchronously from a libuv
/// thread.
pub struct EventEmitterTask(Arc<Mutex<mpsc::Receiver<Event>>>);

/// Implementation of a neon `Task` for `EventEmitterTask`. This task reads
/// from the events channel and calls a JS callback with the data.
impl Task for EventEmitterTask {
    type Output = Option<Event>;
    type Error = String;
    type JsEvent = JsValue;

    /// The work performed on the `libuv` thread. First acquire a lock on
    /// the receiving thread and then return the received data.
    /// In practice, this should never need to wait for a lock since it
    /// should only be executed one at a time by the `EventEmitter` class.
    fn perform(&self) -> Result<Self::Output, Self::Error> {
        let mut rx = self
            .0
            .lock()
            .map_err(|_| "Could not obtain lock on receiver".to_string())?;

        // Attempt to read from the channel. Block for at most 100 ms.
        match rx.try_next() {
            Ok(event) => Ok(event),
            Err(_) => Ok(None),
        }
    }

    /// After the `perform` method has returned, the `complete` method is
    /// scheduled on the main thread. It is responsible for converting the
    /// Rust data structure into a JS object.
    fn complete(
        self,
        mut cx: TaskContext,
        event: Result<Self::Output, Self::Error>,
    ) -> JsResult<Self::JsEvent> {
        // Receive the event or return early with the error
        let event = event.or_else(|err| cx.throw_error(&err.to_string()))?;

        // Timeout occured, return early with `undefined
        let event = match event {
            Some(event) => event,
            None => return Ok(JsUndefined::new().upcast()),
        };

        // Create an empty object `{}`
        let o = cx.empty_object();

        // Creates an object of the shape `{ "event": string, ...data }`
        match event {
            Event::Error { message, stack } => {
                let event_name = cx.string("error");
                let event_message = cx.string(message);
                let event_stack = cx.string(stack);

                o.set(&mut cx, "event", event_name)?;
                o.set(&mut cx, "message", event_message)?;
                o.set(&mut cx, "stack", event_stack)?;
            }
            Event::StateChanged { state } => {
                let event_name = cx.string("stateChanged");
                let event_state = cx.string(format!("{:?}", state).to_string());

                o.set(&mut cx, "event", event_name)?;
                o.set(&mut cx, "state", event_state)?;
            }
            Event::Eos {} => {
                let event_name = cx.string("eos");

                o.set(&mut cx, "event", event_name)?;
            }
        }

        Ok(o.upcast())
    }
}

/// Rust struct that holds the data required by the `JsEventEmitter` class.
#[derive(Clone)]
pub struct EventEmitter {
    // Since the `Receiver` is sent to a thread and mutated, it must be
    // `Send + Sync`. Since, correct usage of the `poll` interface should
    // only have a single concurrent consume, we guard the channel with a
    // `Mutex`.
    events: Arc<Mutex<mpsc::Receiver<Event>>>,

    command_sender: Arc<Mutex<mpsc::Sender<command::Command>>>,
}

// Implementation of the `JsEventEmitter` class. This is the only public
// interface of the Rust code. It exposes the `poll` and `shutdown` methods
// to JS.
declare_types! {
    pub class JsEventEmitter for EventEmitter {
        // Called by the `JsEventEmitter` constructor
        init(mut cx) {
            gst::init().expect("could not init() gstreamer libs");

            let ingest_url = cx.argument::<JsString>(0)?.value();
            let root_directory = cx.argument::<JsString>(1)?.value();
            let rtmp_url = cx.argument::<JsString>(2)?.value();
            let streamer =
                Streamer::new(ingest_url, root_directory, rtmp_url).map_err(|err| panic!(err.to_string()))?;

            // Start work in a separate thread
            let (rx, command_sender) = event_thread(streamer);

            // Construct a new `EventEmitter` to be wrapped by the class.
            let emitter = EventEmitter {
                events: Arc::new(Mutex::new(rx)),
                command_sender: Arc::new(Mutex::new(command_sender)),
            };
            Ok(emitter)
        }

        // This method should be called by JS to receive data. It accepts a
        // `function (err, data)` style asynchronous callback. It may be called
        // in a loop, but care should be taken to only call it once at a time.
        method poll(mut cx) {
            // The callback to be executed when data is available
            let cb = cx.argument::<JsFunction>(0)?;
            let this = cx.this();

            // Create an asynchronously `EventEmitterTask` to receive data
            let events = cx.borrow(&this, |emitter| Arc::clone(&emitter.events));
            let emitter = EventEmitterTask(events);

            // Schedule the task on the `libuv` thread pool
            emitter.schedule(cb);

            // The `poll` method does not return any data.
            Ok(JsUndefined::new().upcast())
        }

        method pause(mut cx) {
            let this = cx.this();

            if let Ok(mut tx) = cx.borrow(&this, |emitter| Arc::clone(&emitter.command_sender)).lock() {
                tx.try_send(command::Command::Pause)
                    .or_else(|err| cx.throw_error(&err.to_string()))?;
            }

            Ok(JsUndefined::new().upcast())
        }

        method play(mut cx) {
            let this = cx.this();

            if let Ok(mut tx) = cx.borrow(&this, |emitter| Arc::clone(&emitter.command_sender)).lock() {
                tx.try_send(command::Command::Resume)
                    .or_else(|err| cx.throw_error(&err.to_string()))?;
            }

            Ok(JsUndefined::new().upcast())
        }
    }
}

// Expose the neon objects as a node module
register_module!(mut cx, {
    // Expose the `JsEventEmitter` class as `EventEmitter`.
    cx.export_class::<JsEventEmitter>("EventEmitter")?;

    Ok(())
});
