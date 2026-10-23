//! Native audio receive/playout policy.
//!
//! Video stays on the 2 ms low-latency path; audio gets its own jitter budget,
//! clock-synced playout, Opus packet-loss concealment, and error isolation so a
//! failing audio device can never stall the shared bundled RTP transport.

use crate::gstreamer_backend::send_log;
use crate::gstreamer_config::{audio_clock_sync_enabled, audio_latency_ms};
use crate::gstreamer_pipeline::{set_property_from_str_if_supported, set_property_if_supported};
use crate::protocol::Event;
use gst::prelude::*;
use gstreamer as gst;
use std::sync::mpsc::Sender;

const AUDIO_QUEUE_MAX_TIME_NS: u64 = 200_000_000;
/// audiobasesink ring buffer size/segment, in microseconds.
const AUDIO_SINK_BUFFER_TIME_US: i64 = 200_000;
const AUDIO_SINK_LATENCY_TIME_US: i64 = 10_000;
const MAX_UPSTREAM_HOPS: usize = 24;

pub(crate) fn is_rtp_audio_pad(pad: &gst::Pad) -> bool {
    let caps = pad.current_caps().unwrap_or_else(|| pad.query_caps(None));
    caps.structure(0).is_some_and(|structure| {
        structure.name() == "application/x-rtp"
            && structure
                .get::<String>("media")
                .is_ok_and(|media| media == "audio")
    })
}

/// Give the audio stream's rtpjitterbuffer its own latency budget and enable
/// lost-packet events so the Opus decoder can conceal gaps.
pub(crate) fn configure_audio_jitterbuffer(
    webrtc_src_pad: &gst::Pad,
    event_sender: &Option<Sender<Event>>,
) {
    let Some(jitterbuffer) = find_upstream_jitterbuffer(webrtc_src_pad) else {
        send_log(
            event_sender,
            "warn",
            "Could not locate the audio rtpjitterbuffer; audio keeps the shared low-latency budget.".to_owned(),
        );
        return;
    };

    let latency_ms = audio_latency_ms();
    set_property_if_supported(&jitterbuffer, "latency", latency_ms);
    set_property_if_supported(&jitterbuffer, "do-lost", true);
    // The jitterbuffer task also runs depay + Opus decode for this stream.
    boost_streaming_thread(&jitterbuffer, event_sender);
    send_log(
        event_sender,
        "info",
        format!(
            "Configured native audio jitter buffer {}: latency={latency_ms}ms doLost=true clockSync={}.",
            jitterbuffer.name(),
            audio_clock_sync_enabled()
        ),
    );
}

/// Walk upstream from a webrtcbin src pad (through ghost/proxy pads and
/// single-input elements such as rtpptdemux or FEC decoders) to the
/// rtpjitterbuffer that feeds it.
fn find_upstream_jitterbuffer(pad: &gst::Pad) -> Option<gst::Element> {
    let mut current = pad.clone();
    for _ in 0..MAX_UPSTREAM_HOPS {
        if let Some(ghost) = current.downcast_ref::<gst::GhostPad>() {
            current = ghost.target()?;
            continue;
        }
        if let Some(owner) = current
            .parent()
            .and_then(|parent| parent.downcast::<gst::Pad>().ok())
        {
            // Internal proxy of a ghost sink pad: leave the bin through its peer.
            current = owner.peer()?;
            continue;
        }

        let element = current.parent_element()?;
        if element
            .factory()
            .is_some_and(|factory| factory.name() == "rtpjitterbuffer")
        {
            return Some(element);
        }
        current = element.sink_pads().into_iter().next()?.peer()?;
    }
    None
}

/// Enable Opus packet-loss concealment on whatever decoder decodebin plugs.
pub(crate) fn configure_audio_decoder(decodebin: &gst::Element) {
    let Some(bin) = decodebin.downcast_ref::<gst::Bin>() else {
        return;
    };
    bin.connect_deep_element_added(|_bin, _sub_bin, element| {
        if element
            .factory()
            .is_some_and(|factory| factory.name() == "opusdec")
        {
            set_property_if_supported(element, "plc", true);
        }
    });
}

/// Raise the priority of the streaming thread that pushes out of `element`'s
/// src pad. Software video decode can saturate a dual-core CPU; audio threads
/// are cheap but must not wait behind it or playout underruns. Only these
/// threads are boosted (not the whole process) so Electron's signaling keeps
/// its share of CPU.
fn boost_streaming_thread(element: &gst::Element, event_sender: &Option<Sender<Event>>) {
    let Some(src_pad) = element.static_pad("src") else {
        return;
    };
    let name = element.name();
    let event_sender = event_sender.clone();
    src_pad.add_probe(gst::PadProbeType::BUFFER, move |_pad, _info| {
        if raise_current_thread_priority() {
            send_log(
                &event_sender,
                "debug",
                format!("Raised native audio streaming thread priority for {name}."),
            );
        }
        gst::PadProbeReturn::Remove
    });
}

#[cfg(target_os = "windows")]
fn raise_current_thread_priority() -> bool {
    type Handle = *mut core::ffi::c_void;
    const THREAD_PRIORITY_HIGHEST: i32 = 2;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> Handle;
        fn SetThreadPriority(thread: Handle, priority: i32) -> i32;
    }

    unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST) != 0 }
}

#[cfg(not(target_os = "windows"))]
fn raise_current_thread_priority() -> bool {
    false
}

pub(crate) fn audio_sink_chain_factories() -> Vec<(&'static str, Option<bool>)> {
    let mut factories = Vec::with_capacity(5);
    // Swallow downstream audio errors (device removed, format renegotiation)
    // so they do not propagate into rtpbin and stop video on the same transport.
    if gst::ElementFactory::find("errorignore").is_some() {
        factories.push(("errorignore", None));
    }
    factories.extend([
        ("queue", None),
        ("audioconvert", None),
        ("audioresample", None),
        ("autoaudiosink", Some(audio_clock_sync_enabled())),
    ]);
    factories
}

pub(crate) fn configure_error_ignore(element: &gst::Element) {
    set_property_if_supported(element, "ignore-error", true);
    set_property_if_supported(element, "ignore-notlinked", true);
    set_property_if_supported(element, "ignore-notnegotiated", true);
}

pub(crate) fn configure_audio_queue(element: &gst::Element, event_sender: &Option<Sender<Event>>) {
    // The queue task feeds the audio device.
    boost_streaming_thread(element, event_sender);
    // Time-bounded instead of 2 buffers: absorb short sink stalls without
    // dropping audio, while still capping stale audio at 200 ms.
    set_property_if_supported(element, "max-size-buffers", 0u32);
    set_property_if_supported(element, "max-size-bytes", 0u32);
    set_property_if_supported(element, "max-size-time", AUDIO_QUEUE_MAX_TIME_NS);
    set_property_from_str_if_supported(element, "leaky", "downstream");
}

pub(crate) fn configure_audio_sink(element: &gst::Element) {
    configure_audio_output_element(element);
    // autoaudiosink creates the real device sink (wasapi2sink, ...) when it
    // goes to READY; configure it as it is added.
    if let Some(bin) = element.downcast_ref::<gst::Bin>() {
        bin.connect_element_added(|_bin, child| configure_audio_output_element(child));
    }
}

fn configure_audio_output_element(element: &gst::Element) {
    set_property_if_supported(element, "sync", audio_clock_sync_enabled());
    set_property_if_supported(element, "async", false);
    set_property_if_supported(element, "enable-last-sample", false);
    set_property_if_supported(element, "buffer-time", AUDIO_SINK_BUFFER_TIME_US);
    set_property_if_supported(element, "latency-time", AUDIO_SINK_LATENCY_TIME_US);
    // wasapi2sink: warn instead of erroring on device loss/IO failures.
    set_property_if_supported(element, "continue-on-error", true);
    // WASAPI low-latency mode shrinks the device buffer; slow CPUs underrun.
    set_property_if_supported(element, "low-latency", false);
}

/// The audio sink can only answer latency queries once it has negotiated, so
/// ask the pipeline to recalculate latency when the first buffer arrives.
pub(crate) fn watch_audio_playout(sink: &gst::Element) {
    let Some(sink_pad) = sink.static_pad("sink") else {
        return;
    };
    let sink = sink.downgrade();
    sink_pad.add_probe(gst::PadProbeType::BUFFER, move |_pad, _info| {
        if let Some(sink) = sink.upgrade() {
            let _ = sink.post_message(gst::message::Latency::builder().src(&sink).build());
        }
        gst::PadProbeReturn::Remove
    });
}

/// Bus handler for LATENCY messages. Applications own latency redistribution;
/// without it the clock-synced audio sink would treat every buffer as late.
pub(crate) fn handle_latency_message(
    pipeline: &gst::Pipeline,
    message: &gst::Message,
    event_sender: &Option<Sender<Event>>,
) {
    let audio_sink = message
        .src()
        .and_then(|src| src.clone().downcast::<gst::Element>().ok())
        .filter(is_audio_sink);

    match pipeline.recalculate_latency() {
        Ok(()) => {
            if audio_sink.is_some() {
                send_log(
                    event_sender,
                    "info",
                    format!(
                        "Native audio playout latency configured: pipelineLatency={:?}.",
                        pipeline.latency()
                    ),
                );
            }
        }
        Err(error) => {
            let Some(sink) = audio_sink else {
                send_log(
                    event_sender,
                    "debug",
                    format!("GStreamer latency recalculation deferred: {error}."),
                );
                return;
            };
            // The negotiated audio sink could not be given a latency: fall back
            // to unsynchronized playout rather than dropping every buffer as late.
            set_audio_sink_sync(&sink, false);
            send_log(
                event_sender,
                "warn",
                format!(
                    "Native audio latency configuration failed ({error}); falling back to unsynchronized audio playout."
                ),
            );
        }
    }
}

fn is_audio_sink(element: &gst::Element) -> bool {
    element
        .factory()
        .is_some_and(|factory| factory.klass().contains("Sink/Audio"))
}

fn set_audio_sink_sync(sink: &gst::Element, enabled: bool) {
    set_property_if_supported(sink, "sync", enabled);
    if let Some(bin) = sink.downcast_ref::<gst::Bin>() {
        for child in bin.children() {
            set_property_if_supported(&child, "sync", enabled);
        }
    }
}
