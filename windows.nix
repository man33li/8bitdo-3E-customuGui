# Linux-hosted Windows GNU cross toolchain. No rustup downloads required.
{ pkgs ? import (builtins.fetchTarball {
    url = "https://github.com/NixOS/nixpkgs/archive/26f4107e47e3ad0ffd0db220469ba83b6f6d96e8.tar.gz";
    sha256 = "sha256-OaQwAtPMTjQiY2sT/2/62UbSPhzP/0aDnWzMm/NCiUw=";
  }) { }
}:
let
  cross = pkgs.pkgsCross.mingwW64;
  raylib = pkgs.fetchzip {
    url = "https://github.com/raysan5/raylib/archive/refs/tags/5.5.tar.gz";
    sha256 = "1dhy9ghbwvz0s434j03rfa2l6wxcfj028vlkk1xbf5q97vin5pr7";
  };
  clay = pkgs.fetchzip {
    url = "https://github.com/nicbarker/clay/archive/e6cc36941ab2af5d81107617039d6f527a1c660b.tar.gz";
    sha256 = "0cz3mdnixpp3invpdcxbmrblwl44zfvyjbqcczwgx01i1422ni64";
  };
in pkgs.mkShell {
  packages = [ cross.buildPackages.rustc pkgs.cargo cross.stdenv.cc pkgs.llvmPackages.libclang ];
  RUSTC = "${cross.buildPackages.rustc}/bin/rustc";
  CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = "${cross.stdenv.cc}/bin/x86_64-w64-mingw32-gcc";
  CC_x86_64_pc_windows_gnu = "${cross.stdenv.cc}/bin/x86_64-w64-mingw32-gcc";
  AR_x86_64_pc_windows_gnu = "${cross.stdenv.cc.bintools}/bin/x86_64-w64-mingw32-ar";
  # Nixpkgs' Windows Rust std links the static winpthreads archive explicitly.
  CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS = "-L native=${cross.windows.pthreads}/lib -C link-arg=-static -C link-arg=-static-libgcc";
  LIBCLANG_PATH = "${pkgs.lib.getLib pkgs.llvmPackages.libclang}/lib";
  MINGW_INCLUDE_DIR = "${pkgs.lib.getDev cross.stdenv.cc.libc}/include";
  RAYLIB_SRC = "${raylib}";
  CLAY_DIR = "${clay}";
}
