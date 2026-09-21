{
  pkgs,
  lib,
  config,
  inputs,
  ...
}:

{
  env = {
    # WebKitGTK's glib does TLS through the glib-networking GIO module; without
    # it every https:// load inside the webview fails (addon icons, screenshots)
    # while plain curl works. dconf stays listed so GSettings keeps its backend.
    GIO_EXTRA_MODULES = lib.makeSearchPath "lib/gio/modules" (
      with pkgs;
      [
        glib-networking
        dconf.lib
      ]
    );
    # GTK reads the desktop's font and DPI settings through GSettings, so the
    # GTK and desktop schemas have to be on XDG_DATA_DIRS. Without them GTK
    # never learns the screen resolution: `gdk_screen_get_resolution` keeps its
    # -1 "unset" sentinel, WebKitGTK divides that by 96 for the device pixel
    # ratio, and the webview lays the UI out against a negative scale — a
    # negative viewport, a 9000000px root font size, and the whole app rendered
    # as a smear a few pixels wide in the middle of the window.
    # WebKitGTK plays media through GStreamer and looks its plugins up on this
    # path. Without it the webview logs "GStreamer element appsink not found"
    # at startup and any <video>/<audio> in the UI fails outright. appsink is
    # in -base; -good/-bad carry the common codecs, -libav the ffmpeg rest.
    GST_PLUGIN_SYSTEM_PATH_1_0 = lib.makeSearchPath "lib/gstreamer-1.0" (
      with pkgs.gst_all_1;
      [
        gstreamer
        gst-plugins-base
        gst-plugins-good
        gst-plugins-bad
        gst-libav
      ]
    );
    XDG_DATA_DIRS = lib.concatStringsSep ":" [
      "${pkgs.gtk3}/share/gsettings-schemas/${pkgs.gtk3.name}"
      "${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}"
      "$XDG_DATA_DIRS"
    ];
  };

  packages = with pkgs; [
    # Rust workflow
    bacon
    fd
    just
    git
    mold # fast linker
    pkg-config
    watchexec
    mprocs

    # Tauri v2 native deps (Linux webview + shell)
    gtk3
    webkitgtk_4_1
    libsoup_3
    openssl
    glib-networking

    # Tauri CLI so `cargo tauri dev` works inside the shell
    cargo-tauri
  ];

  languages.rust = {
    enable = true;
    toolchainFile = ./rust-toolchain.toml;
  };

  # The UI (./ui) — a Vite/React app driven with npm.
  languages.javascript.enable = true;

  process.manager.implementation = "mprocs";

  # `cargo tauri dev` starts the UI dev server itself (see the beforeDevCommand
  # in src-tauri/tauri.conf.json), so this is the whole app in one process.
  processes.app.exec = "cargo tauri dev";

  enterTest = ''
    cargo test --manifest-path src-tauri/Cargo.toml
  '';
}
