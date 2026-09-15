# The toolchain for OpenCASCADE built to WebAssembly: Emscripten's own, then
# the optimisation flags every native build gets, then Wasm exception handling.
#
# The file name has to end in "mscripten.cmake": OCCT's CMakeLists recognises an
# Emscripten build by matching CMAKE_TOOLCHAIN_FILE against that pattern.

include("$ENV{EMSDK}/upstream/emscripten/cmake/Modules/Platform/Emscripten.cmake")

# The same -O2 inside the Debug configuration as native, and for the same
# reason: Release defines No_Exception, which turns every Standard_Failure the
# fillet boundary catches into undefined behaviour. See docs/GOTCHAS.md.
include("${CMAKE_CURRENT_LIST_DIR}/../cmake/occt-toolchain.cmake")

# Native Wasm exceptions, which Rust's wasm32-unknown-emscripten target also
# unwinds with; the two must agree or a C++ throw aborts at the Rust boundary.
# OCCT appends its own -fexceptions afterwards, which -fwasm-exceptions outranks.
string(APPEND CMAKE_C_FLAGS_INIT " -fwasm-exceptions")
string(APPEND CMAKE_CXX_FLAGS_INIT " -fwasm-exceptions")

# OCC_CONVERT_SIGNALS puts a setjmp inside every OCC_CATCH_SIGNALS try block, to
# turn a SIGSEGV into a Standard_Failure once OSD::SetSignal installs a handler.
# Nothing in parcad calls SetSignal, and wasm has no signals at all; what it did
# do was make clang emit invalid wasm for setjmp inside a Wasm-EH try
# ("br_table: label arity inconsistent", ShapeUpgrade_ShapeDivide::Perform).
# OCCT adds the define itself; -U in the flags comes after it on every command.
string(APPEND CMAKE_CXX_FLAGS_INIT " -UOCC_CONVERT_SIGNALS")
