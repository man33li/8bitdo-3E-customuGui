use std::{env, fs, path::PathBuf};

fn main() {
    for name in ["CLAY_DIR", "RAYLIB_SRC", "MINGW_INCLUDE_DIR"] {
        println!("cargo:rerun-if-env-changed={name}");
    }
    let clay = PathBuf::from(env::var_os("CLAY_DIR").unwrap_or_else(|| "/home/knd/AI/clay".into()));
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let windows = env::var("CARGO_CFG_TARGET_OS").unwrap() == "windows";
    let mut includes = vec![clay.clone(), PathBuf::from("src/gui")];
    let linux_libraries = if windows {
        let source = PathBuf::from(env::var_os("RAYLIB_SRC").expect("RAYLIB_SRC required; use nix-shell windows.nix"));
        includes.extend([source.join("src"), source.join("src/external/glfw/include")]);
        None
    } else {
        let raylib = pkg_config::Config::new().atleast_version("5.5").cargo_metadata(false).probe("raylib")
            .expect("raylib 5.5 is required; build inside nix-shell");
        let glfw = pkg_config::Config::new().atleast_version("3.4").cargo_metadata(false).probe("glfw3")
            .expect("GLFW 3.4 is required for native window/input coordinates; build inside nix-shell");
        includes.extend(raylib.include_paths.iter().chain(&glfw.include_paths).cloned());
        Some([raylib, glfw])
    };
    for path in [clay.join("clay.h"), clay.join("renderers/raylib/clay_renderer_raylib.c"), PathBuf::from("src/gui/bridge.c"), PathBuf::from("src/gui/bridge.h")] {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    let mut bridge = cc::Build::new();
    bridge.file("src/gui/bridge.c").includes(&includes).flag_if_supported("-std=c11");
    bridge.compile("ultimate_clay");
    // Static libraries must follow the bridge that references them.
    if windows {
        let source = PathBuf::from(env::var_os("RAYLIB_SRC").unwrap()).join("src");
        let mut raylib = cc::Build::new();
        raylib.include(&source).include(source.join("external/glfw/include"))
            .define("PLATFORM_DESKTOP", None).define("GRAPHICS_API_OPENGL_33", None)
            .flag_if_supported("-std=gnu99").warnings(false);
        for module in ["rcore", "rshapes", "rtextures", "rtext", "rmodels", "raudio", "utils", "rglfw"] {
            raylib.file(source.join(format!("{module}.c")));
        }
        raylib.compile("raylib");
        for name in ["opengl32", "gdi32", "winmm", "shell32"] { println!("cargo:rustc-link-lib={name}"); }
    } else if let Some(libraries) = linux_libraries {
        for library in libraries {
            for path in &library.link_paths { println!("cargo:rustc-link-search=native={}", path.display()); }
            for name in &library.libs { println!("cargo:rustc-link-lib={name}"); }
        }
    }
    let mut bindings = bindgen::Builder::default()
        .header("src/gui/bridge.h")
        .allowlist_type("Clay_.*|Ultimate_.*|Font|Texture.*|Image|Color|Vector2|Camera2D|Rectangle|KeyboardKey|GamepadButton|ConfigFlags|TraceLogLevel")
        .allowlist_function("Clay_.*|Ultimate_.*|.*Window.*|.*Drawing|ClearBackground|.*Mouse.*|.*Key.*|.*Gamepad.*|GetScreenWidth|GetScreenHeight|GetTime|WaitTime|PollInputEvents|.*Font.*|.*Texture.*|.*Image.*|BeginMode2D|EndMode2D|SetConfigFlags|SetTargetFPS|SetExitKey|SetTraceLogLevel")
        .allowlist_function("MemFree|GetRenderWidth|GetRenderHeight")
        .allowlist_var("CLAY_.*|FLAG_.*|KEY_.*|MOUSE_.*|GAMEPAD_.*|TEXTURE_FILTER_.*|LOG_.*")
        .derive_default(true).layout_tests(false).generate_comments(false).prepend_enum_name(false)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()));
    for include in &includes { bindings = bindings.clang_arg(format!("-I{}", include.display())); }
    if windows {
        bindings = bindings.clang_arg("--target=x86_64-w64-windows-gnu")
            .clang_arg(format!("-isystem{}", env::var("MINGW_INCLUDE_DIR").expect("MINGW_INCLUDE_DIR required; use nix-shell windows.nix")));
    }
    bindings.generate().expect("generate Clay/raylib FFI").write_to_file(out.join("gui_ffi.rs")).unwrap();
    fs::metadata("assets/fonts/InterVariable.ttf").expect("bundled Inter font missing");
}
