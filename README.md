# snack

a stupid fast and lightweight Slack client built with Rust and [Iced](https://iced.rs/)

## run

install GStreamer 1.14 or newer first. on macOS:

```sh
brew install gstreamer
```

on Ubuntu/Debian:

```sh
sudo apt install libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \
  gstreamer1.0-{libav,plugins-base,plugins-good,plugins-bad,plugins-ugly}
```

windows installers are available from the
[GStreamer project](https://gstreamer.freedesktop.org/download/).

clone the repo locally and run the following command to start the app:

```sh
cargo run
```
