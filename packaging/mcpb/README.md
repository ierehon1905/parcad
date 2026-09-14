# The MCP bundle and its registry entry

`manifest.json` turns the release's CLI tarball into
`parcad-mcp-<arch>-apple-darwin.mcpb`, an MCP bundle. The release workflow packs
it with the official `mcpb` packer, because a plain `zip` writes directory
entries its unpacker fails on. `server.json` lists that bundle in the MCP
Registry as `io.github.ierehon1905/parcad`.

The bundle starts through `packaging/plugin/scripts/parcad-mcp`, the plugin's
own launcher, so it runs the installed parcad (Homebrew or the app) whenever
there is one. The `parcad`, worker and seed parts it carries are used only when
nothing is installed, which is what lets a registry install work on a bare Mac.

## On each release

1. Bump the version everywhere; `packaging_manifests_carry_the_crate_version`
   fails until `manifest.json`, `server.json` and the plugin manifests all
   carry it.
2. After the release is published, copy the `.mcpb` checksum from
   `SHA256SUMS.txt` into `server.json`'s `fileSha256`.
3. From this directory:

```bash
mcp-publisher validate
MCP_GITHUB_TOKEN="$(gh auth token --user ierehon1905)" mcp-publisher login github
mcp-publisher publish
```

Log in right before publishing: the registry's token expires within the hour,
and a stale one fails the publish with a 401. The token route needs no browser;
plain `mcp-publisher login github` is a device-code flow that needs a terminal.

The registry refuses a bundle URL without "mcp" in it, which is why the file is
not called `parcad-*.mcpb`. It also checks `fileSha256` against the download.
