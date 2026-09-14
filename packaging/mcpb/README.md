# The MCP bundle and its registry entry

`manifest.json` turns the release's CLI tarball into
`parcad-mcp-<arch>-apple-darwin.mcpb`, an MCP bundle. The release workflow packs
it with the official `mcpb` packer, because a plain `zip` writes directory
entries its unpacker fails on. `server.json` lists that bundle in the MCP
Registry as `io.github.ierehon1905/parcad`.

The bundle runs `parcad mcp` from its own directory, with the worker and the
seed parts beside it, so it needs no Homebrew install.

## On each release

1. Bump the version everywhere; `packaging_manifests_carry_the_crate_version`
   fails until `manifest.json`, `server.json` and the plugin manifests all
   carry it.
2. After the release is published, copy the `.mcpb` checksum from
   `SHA256SUMS.txt` into `server.json`'s `fileSha256`.
3. From this directory:

```bash
mcp-publisher validate
mcp-publisher login github    # once per machine, as the owner of ierehon1905
mcp-publisher publish
```

The registry refuses a bundle URL without "mcp" in it, which is why the file is
not called `parcad-*.mcpb`. It also checks `fileSha256` against the download.
