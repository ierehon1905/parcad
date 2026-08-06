use occt_sys::{occt_include_path, occt_lib_path};

fn main() {
    let target = std::env::var("TARGET").expect("No TARGET environment variable defined");
    let is_windows = target.to_lowercase().contains("windows");
    let is_windows_gnu = target.to_lowercase().contains("windows-gnu");

    println!("cargo:rustc-link-search=native={}", occt_lib_path().to_str().unwrap());
    println!("cargo:rustc-link-lib=static=TKMath");
    println!("cargo:rustc-link-lib=static=TKernel");
    println!("cargo:rustc-link-lib=static=TKFeat");
    println!("cargo:rustc-link-lib=static=TKGeomBase");
    println!("cargo:rustc-link-lib=static=TKG2d");
    println!("cargo:rustc-link-lib=static=TKG3d");
    println!("cargo:rustc-link-lib=static=TKTopAlgo");
    println!("cargo:rustc-link-lib=static=TKGeomAlgo");
    println!("cargo:rustc-link-lib=static=TKBRep");
    println!("cargo:rustc-link-lib=static=TKPrim");
    // OCCT 8.0 consolidated data exchange behind the DE framework: the four
    // TKSTEP* toolkits became TKDESTEP and TKSTL became TKDESTL, both of which
    // now sit on XCAF and so drag in the OCAF and visualisation toolkits below.
    println!("cargo:rustc-link-lib=static=TKDESTEP");
    println!("cargo:rustc-link-lib=static=TKDESTL");
    println!("cargo:rustc-link-lib=static=TKDE");
    println!("cargo:rustc-link-lib=static=TKXCAF");
    println!("cargo:rustc-link-lib=static=TKVCAF");
    println!("cargo:rustc-link-lib=static=TKCAF");
    println!("cargo:rustc-link-lib=static=TKLCAF");
    println!("cargo:rustc-link-lib=static=TKCDF");
    println!("cargo:rustc-link-lib=static=TKV3d");
    println!("cargo:rustc-link-lib=static=TKService");
    println!("cargo:rustc-link-lib=static=TKMesh");
    println!("cargo:rustc-link-lib=static=TKShHealing");
    println!("cargo:rustc-link-lib=static=TKFillet");
    println!("cargo:rustc-link-lib=static=TKBool");
    println!("cargo:rustc-link-lib=static=TKBO");
    println!("cargo:rustc-link-lib=static=TKOffset");
    println!("cargo:rustc-link-lib=static=TKXSBase");

    if is_windows {
        println!("cargo:rustc-link-lib=dylib=user32");
    }

    let mut build = cxx_build::bridge("src/lib.rs");

    if is_windows_gnu {
        build.define("OCC_CONVERT_SIGNALS", "TRUE");
    }

    build
        .cpp(true)
        // OCCT 8.0 requires C++17 — its headers use constexpr and mutable-state
        // idioms that a C++11 compile rejects outright. Upstream asked for
        // c++11 because it shipped OCCT 7.7.
        .flag_if_supported("-std=c++17")
        .define("_USE_MATH_DEFINES", "TRUE")
        .include(occt_include_path())
        .include("include")
        .compile("wrapper");

    println!("cargo:rustc-link-lib=static=wrapper");

    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=include/wrapper.hxx");
}
