{
  description = "Minimal dear-imgui-rs 0.22 demo: native winit and wasm32 browser";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { nixpkgs, ... }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
      runtimeLibs = with pkgs; [
        wayland
        libxkbcommon
        libX11
        libXcursor
        libXi
        libXrandr
        vulkan-loader
        mesa
      ];
    in
    {
      devShells.${system} = {
        default = pkgs.mkShell {
          packages = with pkgs; [
            rustc
            cargo
            clang
            lld
            wasm-bindgen-cli
            binaryen
            wasm-tools
            python3
            pkg-config
          ] ++ runtimeLibs;

          env.LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath runtimeLibs;
        };

        # Builds the Emscripten provider from the dear-imgui-rs repository checkout.
        provider = pkgs.mkShell {
          packages = with pkgs; [
            rustc
            cargo
            wasm-bindgen-cli
            binaryen
            wasm-tools
            emscripten
            lld
            clang
            python3
            pkg-config
          ];
        };
      };
    };
}
