fn main() {
    cxx_build::bridges(["src/history.rs", "src/sweep.rs"])
        .cpp(true)
        // OCCT 8.0 requires C++17; see vendor/opencascade-sys/build.rs.
        .std("c++17")
        .include(occt_sys::occt_include_path())
        .include(".")
        .compile("parcad_opencascade_history");

    println!("cargo:rerun-if-changed=src/history.rs");
    println!("cargo:rerun-if-changed=include/history.hxx");
    println!("cargo:rerun-if-changed=src/sweep.rs");
    println!("cargo:rerun-if-changed=include/sweep.hxx");
}
