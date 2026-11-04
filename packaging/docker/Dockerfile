# ParCAD's host without a window, from a published Linux x86_64 release.
# Building, running and every variable: README.md beside this file.

FROM debian:bookworm-slim AS fetch
ARG PARCAD_VERSION=0.0.13
ARG PARCAD_REPO=ierehon1905/parcad
RUN apt-get update \
 && apt-get install -y --no-install-recommends curl ca-certificates \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /fetch
RUN set -eu; \
    asset=parcad-cli-x86_64-unknown-linux-gnu.tar.gz; \
    base="https://github.com/${PARCAD_REPO}/releases/download/v${PARCAD_VERSION}"; \
    curl -fsSLO "$base/SHA256SUMS.txt" \
      || { echo "no release v${PARCAD_VERSION} at github.com/${PARCAD_REPO}; set PARCAD_VERSION to a published version, without the v" >&2; exit 1; }; \
    grep "  $asset\$" SHA256SUMS.txt > expected.txt \
      || { echo "v${PARCAD_VERSION} has no $asset; set PARCAD_VERSION to a release that carries the Linux CLI archive" >&2; exit 1; }; \
    curl -fsSLO "$base/$asset"; \
    sha256sum -c expected.txt; \
    tar -xzf "$asset"; \
    mv parcad-cli-x86_64-unknown-linux-gnu /opt/parcad

FROM debian:bookworm-slim
ARG PARCAD_VERSION=0.0.13
LABEL org.opencontainers.image.title="parcad" \
      org.opencontainers.image.version="${PARCAD_VERSION}" \
      org.opencontainers.image.source="https://github.com/ierehon1905/parcad" \
      org.opencontainers.image.licenses="AGPL-3.0-only"

# What ldd names for the two binaries beyond libc and libm: libgcc_s for both,
# libstdc++ for the worker.
RUN apt-get update \
 && apt-get install -y --no-install-recommends libstdc++6 libgcc-s1 socat ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --create-home --home-dir /home/parcad parcad \
 && mkdir -p /data/parts /data/export \
 && chown -R parcad:parcad /data

COPY --from=fetch /opt/parcad /opt/parcad
COPY --chmod=0755 entrypoint.sh /usr/local/bin/parcad-entrypoint
RUN ln -s /opt/parcad/parcad /usr/local/bin/parcad \
 && mkdir /opt/parcad/no-seed

# The host's own port, behind the forwarder on 4242; `docker exec … parcad
# call` reads the same variable, so it finds the host. The seed folder is named
# because a CLI looks for its examples in ../Resources, a bundle's layout.
ENV PARCAD_HTTP_PORT=4243 \
    PARCAD_PROJECTS_DIR=/data/parts \
    PARCAD_EXPORT_DIR=/data/export \
    PARCAD_SEED_DIR=/opt/parcad/examples

USER parcad
WORKDIR /home/parcad
VOLUME /data
EXPOSE 4242

HEALTHCHECK --interval=10s --timeout=3s --start-period=10s --retries=3 \
  CMD ["bash", "-c", "exec 3<>/dev/tcp/127.0.0.1/4242 && printf 'GET /api/health HTTP/1.0\\r\\nHost: 127.0.0.1\\r\\n\\r\\n' >&3 && head -n1 <&3 | grep -q ' 200 '"]

ENTRYPOINT ["/usr/local/bin/parcad-entrypoint"]
