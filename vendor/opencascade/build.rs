fn main() {
    cxx_build::bridge("src/history.rs")
        .cpp(true)
        // OCCT 8.0 requires C++17; see vendor/opencascade-sys/build.rs.
        .flag_if_supported("-std=c++17")
        .include(occt_sys::occt_include_path())
        .include(".")
        .compile("parcad_opencascade_history");

    println!("cargo:rerun-if-changed=src/history.rs");
    println!("cargo:rerun-if-changed=include/history.hxx");
}
