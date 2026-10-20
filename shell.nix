{
  pkgs ? import <nixpkgs> { },
}:

pkgs.mkShell {
  packages = [
    pkgs.rustc
    pkgs.cargo
    pkgs.raylib
    pkgs.glfw
    pkgs.pkg-config
    pkgs.llvmPackages.libclang
    pkgs.rustPlatform.bindgenHook
    pkgs.inter
  ];
}
