# The MCP bundle and its registry entry

`manifest.json` turns the release's CLI tarball into
`parcad-mcp-<target triple>.mcpb`, an MCP bundle, one per platform. The release workflow packs
it with the official `mcpb` packer, because a plain `zip` writes directory
entries its unpacker fails on. `server.json.in` lists those bundles in the MCP
Registry as `io.github.ierehon1905/parcad`.

The bundle starts through `packaging/plugin/scripts/parcad-mcp`, the plugin's
own launcher, so it runs the installed parcad (Homebrew or the app) whenever
there is one. The `parcad`, worker and seed parts it carries are used only when
nothing is installed, which is what lets a registry install work on a bare Mac.

## On each release

`server.json.in` is rendered from the release's checksums and published to the
registry by `.github/workflows/publish.yml`, logged in through GitHub OIDC — no
token to expire. It lists one bundle per platform; each bundle's
`manifest.json` names the one platform its binaries run on. By hand, from a
rendered `server.json` (see `../README.md`):

```bash
mcp-publisher validate
mcp-publisher login github
mcp-publisher publish
```

The registry refuses a bundle URL without "mcp" in it, which is why the file is
not called `parcad-*.mcpb`. It also checks `fileSha256` against the download.
