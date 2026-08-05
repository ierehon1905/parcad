# Tauri's build script registers a rerun-if-changed on this directory. When it
# does not exist, cargo treats the build script as permanently dirty and reruns
# it — and the bun bundle inside it — on every build, costing ~16 s a time.
# The directory is deliberately empty of capability files: adding one would
# grant permissions the app does not currently have.
