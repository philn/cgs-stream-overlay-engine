use gst;

/// Represents the data that will be received by the `poll` method. It may
/// include different types of data or be replaced with a more simple type,
/// e.g., `Vec<u8>`.
pub enum Event {
    Error { message: String, stack: String },
    StateChanged { state: gst::State },
    Eos {},
}
