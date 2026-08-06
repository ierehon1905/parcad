# Patches applied to OpenCASCADE

`../OCCT` is a pristine export of an upstream tag and must stay that way. Every
change parcad makes to the kernel lives here, as a numbered `NNNN-name.patch`,
and `build.rs` applies them in filename order to a staged copy under `OUT_DIR`.

That separation is the whole point: after dropping in a new OCCT tag, a patch
that no longer fits fails the build loudly instead of vanishing into a tree
nobody can diff against upstream any more.

## Adding one

1. Edit the staged tree or a scratch copy until the change is right, measuring
   the result — `PARCAD_CHECK_VALIDITY=1` and the `eval/` corpus, not eyeballing.
2. Produce the diff against the pristine tree:

   ```
   diff -ru vendor/occt-sys/OCCT <your-edited-tree> \
     > vendor/occt-sys/patches/0001-short-name.patch
   ```

   Paths must be `-p1`-strippable, i.e. `a/src/...` / `b/src/...` or the
   equivalent from `diff -ru` run one level above the tree.
3. Give the patch a header comment saying what it fixes, how it was measured,
   and its upstream status — issue number if reported, "not yet reported" if
   not. A patch nobody offered upstream is a patch we carry forever.
4. Record it in `../PARCAD-CHANGES.md`.

## Upgrading OCCT

Replace `../OCCT` with the new tag's export, then build. Any patch that rejects
names itself in the error. Rebase it, or delete it if upstream fixed the bug —
and if it is gone upstream, say so in `PARCAD-CHANGES.md` rather than silently
dropping it.

## Current series

Empty, and an empty `patches/` is itself the claim that `../OCCT` is upstream
and nothing else — which `tools/occt-import.sh` verifies. See
`../PARCAD-CHANGES.md` for the one patch that has lived here and why it left.
