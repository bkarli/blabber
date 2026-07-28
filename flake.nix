{
  description = "Blabber Flake";
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
  };
  outputs = { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs { inherit system; };
    in {
      devShells.${system}.default = pkgs.mkShell {
        buildInputs = with pkgs; [
          rustc
          cargo
          clippy
          rust-analyzer

          nodejs_22
          pnpm
          cargo-tauri

          pkg-config
          openssl
          alsa-lib

          webkitgtk_4_1
          gtk3
          libsoup_3
          cairo
          pango
          gdk-pixbuf
          atk
          librsvg
          libappindicator-gtk3
          gsettings-desktop-schemas

          # GStreamer, needed by WebKitGTK for getUserMedia (mic/camera access)
          gst_all_1.gstreamer
          gst_all_1.gst-plugins-base
          gst_all_1.gst-plugins-good
          gst_all_1.gst-plugins-bad
          gst_all_1.gst-plugins-ugly
          gst_all_1.gst-libav

          patchelf
        ];

        shellHook = ''
          export RUST_BACKTRACE=1
          export PKG_CONFIG_PATH="${pkgs.alsa-lib.dev}/lib/pkgconfig:$PKG_CONFIG_PATH"
          export PKG_CONFIG_PATH="${pkgs.webkitgtk_4_1}/lib/pkgconfig:$PKG_CONFIG_PATH"

          export LD_LIBRARY_PATH="${pkgs.lib.makeLibraryPath [
            pkgs.webkitgtk_4_1
            pkgs.gtk3
            pkgs.libsoup_3
            pkgs.cairo
            pkgs.pango
            pkgs.gdk-pixbuf
            pkgs.atk
            pkgs.librsvg
            pkgs.libappindicator-gtk3
          ]}:$LD_LIBRARY_PATH"

          export XDG_DATA_DIRS="${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}:${pkgs.gtk3}/share/gsettings-schemas/${pkgs.gtk3.name}:$XDG_DATA_DIRS"

          export GST_PLUGIN_SYSTEM_PATH_1_0="${pkgs.lib.makeSearchPath "lib/gstreamer-1.0" [
            pkgs.gst_all_1.gstreamer
            pkgs.gst_all_1.gst-plugins-base
            pkgs.gst_all_1.gst-plugins-good
            pkgs.gst_all_1.gst-plugins-bad
            pkgs.gst_all_1.gst-plugins-ugly
            pkgs.gst_all_1.gst-libav
          ]}"

          echo "blabber-gui dev shell ready — cargo tauri dev to launch"
        '';
      };
    };
}
