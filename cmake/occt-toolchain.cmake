# Make OpenCASCADE compile optimised.
#
# The problem this solves: `occt-sys` builds OCCT through the `cmake` crate,
# which resolves CMAKE_BUILD_TYPE to `Debug` here even under `cargo build
# --release`. The whole geometry kernel then compiles at -O0 — measured, not
# assumed: the generated flags for TKBO, the boolean-operations library, carried
# `-g` and no `-O` at all, and booleans took several times longer than they
# should.
#
# Why this file rather than CXXFLAGS: the `cmake` crate deliberately strips every
# `-O` argument before forwarding compiler flags ("let cmake deal with
# optimization/debuginfo"), so no environment variable can carry the setting
# through. A toolchain file is the one hook it passes along untouched, because
# CMake reads it before the project is configured.
#
# Why not simply force a Release build, which would be the obvious fix: OCCT's
# Release configuration also defines `No_Exception`, which turns its
# `Standard_Failure` raises into no-ops. A fillet radius larger than the material
# would stop being a catchable failure and become undefined behaviour — and every
# error message this project produces for bad geometry depends on those
# exceptions being thrown. Staying in the Debug configuration and only changing
# the optimisation level keeps the checks and the exceptions.
#
# Override the level with PARCAD_OCCT_OPT (e.g. -O2) before a clean rebuild.

# -O2 rather than the -O3 OCCT's own Release uses, because the two were measured
# and there is nothing between them:
#
#              build   mesh        build   mesh
#   enclosure   -O2 43     4        -O3 44     4     (ms, median of 7)
#   bracket     -O2 39    28        -O3 39    28
#
# Identical inside the noise, and -O3 costs 1.2 MB more in the worker binary.
# Unsurprising for OCCT: it is pointer-chasing C++ over topology graphs, so the
# extra unrolling and vectorisation -O3 buys has almost nothing to work on.
# For reference, the same parts at -O0 were 203/22 and 197/181 — the step that
# matters is turning optimisation on at all, not which level.
if (DEFINED ENV{PARCAD_OCCT_OPT})
  set(_parcad_opt "$ENV{PARCAD_OCCT_OPT}")
else ()
  set(_parcad_opt "-O2")
endif ()

# FORCE, because OCCT's CMakeLists sets these itself and would otherwise win.
set(CMAKE_C_FLAGS_DEBUG   "-g ${_parcad_opt}" CACHE STRING "parcad: optimised debug build" FORCE)
set(CMAKE_CXX_FLAGS_DEBUG "-g ${_parcad_opt}" CACHE STRING "parcad: optimised debug build" FORCE)
