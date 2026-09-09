# Build only the dedicated gateway, never the general Memory app.
# Supply reviewed, compatible, digest-pinned Rust and OS base images explicitly.
ARG RUST_BUILD_IMAGE
ARG GATEWAY_RUNTIME_IMAGE
FROM ${RUST_BUILD_IMAGE} AS build
WORKDIR /src
COPY . .
RUN cargo build --release -p mem-federation-runtime --bin memory-federation-gateway --locked
FROM ${GATEWAY_RUNTIME_IMAGE}
COPY --from=build /src/target/release/memory-federation-gateway /usr/local/bin/memory-federation-gateway
USER 65532:65532
ENTRYPOINT ["/usr/local/bin/memory-federation-gateway"]
CMD ["/config/gateway.json"]
