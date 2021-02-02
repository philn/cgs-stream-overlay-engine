
# Introduction

This project hosts a NodeJS application that spawns a GStreamer pipeline in a
dedicated thread. The pipeline can ingest an SRT stream, decode it, overlay a
WebView on top, re-encode the resulting stream and broadcast it to an RTMP
server. A preview (local) stream is available, through the admin WebUI, for
browsers capable of handling HLS, H.264 and AAC.

# Server environment

This was tested on a Ubuntu 20.10 host with the following libraries installed
for [gst-build](https://gitlab.freedesktop.org/gstreamer/gst-build):

```shell

sudo apt install libepoxy-dev libwpewebkit-1.0-dev libsrt-gnutls-dev libx264-dev librtmp-dev clang flex \
  bison ninja-build libfdk-aac-dev

sudo apt build-dep gstreamer-1.0-gl
```

Make sure to not install the Ubuntu libwpebackend-fdo-dev package because the
version shipped in Ubuntu 20.10 doesn't support headless rendering. gst-build
should thus fallback to wpebackend-fdo as a subproject during the build.

# Toy container setup

This was tested with [podman](https://podman.io) but should also work with Docker.

## Build

```shell
podman build -t cgs .
```

## Run

```shell
podman run -it -p 3000:3000 -t cgs . srt://foo rtmp://bar
```

# Old fashioned way

## Streamer build

Install [Neon](https://neon-bindings.com):

```shell
npm i neon-cli
```

And then, build the project:

```shell
neon build --release
```

## How to run this

```shell
export LIBGL_ALWAYS_SOFTWARE=true
node . srt://host:port rtmp://blah
```

# Additional notes

The operator can control overlays from a browser by opening the [admin
interface](http://localhost:3000/admin). By default the pipeline starts paused.
Broadcasting can be started from the admin UI (General section, untick the
`Pause stream` check-box).

The stream preview accessible from the admin UI is an HLS stream encoded in
H.264 and AAC, a browser supporting these codecs is required, Safari should be
able to handle those. Chrome doesn't support native HLS consumption.

# Debugging

If the app doesn't work, the first step is to make sure the SRT stream plays
fine in GStreamer:

```shell
podman -it --entrypoint=bash cgs
GST_DEBUG="3,srt*:6" gst-play-1.0 srt://...
```

In case the app crashes with this kind of message: `fatal runtime error: failed
to initiate panic, error 5`, you need to rebuild neon with the default panic
hook enabled:

```shell
git revert 41af1bfb156410f462539a37c23f0da7bd0a1e91
```

Then rebuild the app in debug mode, by removing `--release` from the `neon
build` command in the Dockerfile. Install gdb in the container and run the app
inside:

```shell
gdb -args node . ...
```

When the crash happens, display the backtrace (`bt` command in gdb).

## TODO

This is a list of the potential improvements that could be made.

- Team name, acronym edition support in admin UI
- admin UI to configure SRT and RTMP URLs
- fallback-switch support
- GL-backed pipeline, if the host has a capable GPU and `LIBGL_ALWAYS_SOFTWARE` is not set.

