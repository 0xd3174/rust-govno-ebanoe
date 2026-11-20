{
  description = "Minimal dear-imgui-rs 0.22 demo: native winit and wasm32 browser";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { nixpkgs, ... }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
    in
    {
      devShells.${system}.default = pkgs.mkShell {
        packages = with pkgs; [
          rustc
          cargo
          clang
          lld
          wasm-bindgen-cli
          binaryen
          python3
          pkg-config
          libxkbcommon
          wayland
          libX11
          libXcursor
          libXi
          libXrandr
          mesa
          vulkan-loader
        ];

        env.LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [
          pkgs.wayland
          pkgs.libxkbcommon
          pkgs.libX11
          pkgs.libXcursor
          pkgs.libXi
          pkgs.libXrandr
          pkgs.vulkan-loader
          pkgs.mesa
        ];
      };
    };
}
