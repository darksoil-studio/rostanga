{
  description = "Template for Holochain app development";

  inputs = {
    nixpkgs.follows = "holochain/nixpkgs";

    versions.url = "github:holochain/holochain?dir=versions/weekly";

    holochain = {
      url = "github:holochain/holochain";
      inputs.versions.follows = "versions";
    };

    rust-overlay.url = "github:oxalica/rust-overlay";
    android-nixpkgs = {
      # url = "github:tadfisher/android-nixpkgs";

      # The main branch follows the "canary" channel of the Android SDK
      # repository. Use another android-nixpkgs branch to explicitly
      # track an SDK release channel.
      #
      url = "github:tadfisher/android-nixpkgs/stable";
      # url = "github:tadfisher/android-nixpkgs/beta";
      # url = "github:tadfisher/android-nixpkgs/preview";
      # url = "github:tadfisher/android-nixpkgs/canary";

      # If you have nixpkgs as an input, this will replace the "nixpkgs" input
      # for the "android" flake.
      #
      inputs.nixpkgs.follows = "nixpkgs";
    };

  };

  outputs = inputs:
    inputs.holochain.inputs.flake-parts.lib.mkFlake
      {
        inherit inputs;
      }
      {
        systems = builtins.attrNames inputs.holochain.devShells;
        perSystem =
          { inputs'
          , config
          , pkgs
          , system
          , lib
          , self'
          , ...
          }:      
          let
            overlays = [ (import inputs.rust-overlay) ];
            rustPkgs = import pkgs.path {
              inherit system overlays;
            };
            rust = rustPkgs.rust-bin.stable."1.71.1".default.override {
              extensions = [ "rust-src" ];
              targets = [ 
                "armv7-linux-androideabi"
                "x86_64-linux-android"
                "i686-linux-android"
                "aarch64-unknown-linux-musl"
                "wasm32-unknown-unknown"
                "x86_64-pc-windows-gnu"
                "x86_64-unknown-linux-musl"
                "x86_64-apple-darwin"
                "aarch64-linux-android"
                "aarch64-apple-ios"
              ];
            };

          in {
            devShells.default = pkgs.mkShell {

              inputsFrom = [ inputs'.holochain.devShells.holonix ];
              packages = (with pkgs; [
                nodejs-18_x
                # more packages go here
                cargo-nextest
              ])
              ++ ([
                rust
              ])
              ;
              
              buildInputs = (with pkgs; [
                openssl
                # this is required for glib-networking
                glib
                gradle
                jdk17
              ])
              ++ (lib.optionals pkgs.stdenv.isLinux
                (with pkgs; [
                  webkitgtk_4_1.dev
                  gdk-pixbuf
                  gtk3
                  # Video/Audio data composition framework tools like "gst-inspect", "gst-launch" ...
                  gst_all_1.gstreamer
                  # Common plugins like "filesrc" to combine within e.g. gst-launch
                  gst_all_1.gst-plugins-base
                  # Specialized plugins separated by quality
                  gst_all_1.gst-plugins-good
                  gst_all_1.gst-plugins-bad
                  gst_all_1.gst-plugins-ugly
                  # Plugins to reuse ffmpeg to play almost every video format
                  gst_all_1.gst-libav
                  # Support the Video Audio (Hardware) Acceleration API
                  gst_all_1.gst-vaapi
                  libsoup_3
                ]))
              ++ lib.optionals pkgs.stdenv.isDarwin
                (with pkgs; [
                  darwin.apple_sdk.frameworks.Security
                  darwin.apple_sdk.frameworks.CoreServices
                  darwin.apple_sdk.frameworks.CoreFoundation
                  darwin.apple_sdk.frameworks.Foundation
                  darwin.apple_sdk.frameworks.AppKit
                  darwin.apple_sdk.frameworks.WebKit
                  darwin.apple_sdk.frameworks.Cocoa
                ])
              ;

              nativeBuildInputs = (with pkgs; [
                perl
                pkg-config
                makeWrapper
              ])
              ++ (lib.optionals pkgs.stdenv.isLinux
                (with pkgs; [
                  wrapGAppsHook
                ]))
              ++ (lib.optionals pkgs.stdenv.isDarwin [
                pkgs.xcbuild
                pkgs.libiconv
              ])
              ;

              shellHook = ''
                export NDK_HOME=$ANDROID_SDK_ROOT/ndk-bundle
                export GIO_MODULE_DIR=${pkgs.glib-networking}/lib/gio/modules/
                export GIO_EXTRA_MODULES=${pkgs.glib-networking}/lib/gio/modules
                export WEBKIT_DISABLE_COMPOSITING_MODE=1

                export RUSTFLAGS+=" -C link-arg=$(gcc -print-libgcc-file-name)"
              '';
            };
          };
      };
}
