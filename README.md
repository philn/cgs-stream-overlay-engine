
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

# Streamer build

Install [Neon](https://neon-bindings.com):

```shell
npm i neon-cli
```

And then, build the project:

```shell
neon build --release
```

# How to run this

```shell
export LIBGL_ALWAYS_SOFTWARE=true
node . srt://host:port rtmp://blah
```

The operator can control overlays from a browser by opening the [admin interface](http://localhost:3000/admin).

The stream preview accessible from the admin UI is an HLS stream encoded in
H.264 and AAC, a browser supporting these codecs is required, Safari should be
able to handle those. Chrome doesn't support native HLS consumption.

## TODO

This is a list of the potential improvements that could be made.

- Team name, acronym edition support in admin UI
- admin UI to configure SRT and RTMP URLs
- fallback-switch support
- GL-backed pipeline, if the host has a capable GPU and `LIBGL_ALWAYS_SOFTWARE` is not set.
- Docker file for easier testing and deployment
