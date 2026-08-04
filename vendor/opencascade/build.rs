fn main() {
    cxx_build::bridge("src/history.rs")
        .cpp(true)
        .flag_if_supported("-std=c++11")
        .include(occt_sys::occt_include_path())
        .include(".")
        .compile("parcad_opencascade_history");

    println!("cargo:rerun-if-changed=src/history.rs");
    println!("cargo:rerun-if-changed=include/history.hxx");
}
