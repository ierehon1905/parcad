# ParCAD's host without a window, from a published Linux x86_64 release, as one
# server for many accounts. Building, running and every variable: README.md
# beside this file.
#
# PARCAD_SOURCE picks where the binaries come from: `release` (the default)
# downloads the release named by PARCAD_VERSION and checks it against that
# release's SHA256SUMS.txt; `local` copies an unpacked
# parcad-cli-x86_64-unknown-linux-gnu archive handed in as the build context
# named `archive`, which is how a build is tried before its release exists.

ARG PARCAD_SOURCE=release

FROM debian:bookworm-slim AS release
ARG PARCAD_VERSION=0.0.14
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

# `docker build --build-arg PARCAD_SOURCE=local --build-context archive=<dir>`,
# the directory being the unpacked archive: parcad, parcad-occt-worker,
# examples/ and licenses/ at its top.
FROM scratch AS local
COPY --from=archive / /opt/parcad

FROM ${PARCAD_SOURCE} AS parcad

FROM debian:bookworm-slim
ARG PARCAD_VERSION=0.0.14
LABEL org.opencontainers.image.title="parcad" \
      org.opencontainers.image.version="${PARCAD_VERSION}" \
      org.opencontainers.image.source="https://github.com/ierehon1905/parcad" \
      org.opencontainers.image.licenses="AGPL-3.0-only"

# What ldd names for the two binaries beyond libc and libm: libgcc_s for both,
# libstdc++ for the worker.
RUN apt-get update \
 && apt-get install -y --no-install-recommends libstdc++6 libgcc-s1 ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --create-home --home-dir /home/parcad parcad \
 && mkdir -p /data/tenants /etc/parcad \
 && chown -R parcad:parcad /data

COPY --from=parcad /opt/parcad /opt/parcad
COPY --chmod=0755 entrypoint.sh /usr/local/bin/parcad-entrypoint
RUN ln -s /opt/parcad/parcad /usr/local/bin/parcad \
 && mkdir /opt/parcad/no-seed \
 && test -x /opt/parcad/parcad -a -x /opt/parcad/parcad-occt-worker -a -d /opt/parcad/examples

# The seed folder is named because a CLI looks for its examples in
# ../Resources, a bundle's layout. Each account is seeded from it on its first
# request.
ENV PARCAD_SEED_DIR=/opt/parcad/examples

USER parcad
WORKDIR /home/parcad
VOLUME /data
EXPOSE 4242

HEALTHCHECK --interval=10s --timeout=3s --start-period=10s --retries=3 \
  CMD ["bash", "-c", "exec 3<>/dev/tcp/127.0.0.1/4242 && printf 'GET /api/health HTTP/1.0\\r\\nHost: 127.0.0.1\\r\\n\\r\\n' >&3 && head -n1 <&3 | grep -q ' 200 '"]

ENTRYPOINT ["/usr/local/bin/parcad-entrypoint"]
