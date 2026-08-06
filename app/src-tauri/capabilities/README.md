# Tauri's build script registers a rerun-if-changed on this directory. When it
# does not exist, cargo treats the build script as permanently dirty and reruns
# it — and the bun bundle inside it — on every build, costing ~16 s a time.
#
# One capability lives here, and only one: session.json grants the main window
# core:event:allow-listen, because the live session's Tauri transport is an
# event and a webview cannot subscribe to one without it. Everything else the
# frontend calls is an app command, which the ACL does not gate — keep it that
# way, and say why in the capability file if another one ever becomes
# unavoidable.
