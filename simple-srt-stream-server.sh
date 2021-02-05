#!/bin/bash

#export SHELL=bash
#eval `gst-env --builddir ~/gst-build/groovy-build/ --only-environment`

# export GST_DEBUG=3
echo "Starting stream on srt://localhost:8000"
gst-launch-1.0 mpegtsmux name=mux ! queue ! srtsink uri=srt://:8000 wait-for-connection=0 \
               videotestsrc ! video/x-raw,width=600,height=400 ! timeoverlay ! videoconvert ! x264enc tune=zerolatency ! queue ! mux. \
               audiotestsrc ! audioconvert ! audioresample ! fdkaacenc ! queue ! aacparse ! queue ! mux.
