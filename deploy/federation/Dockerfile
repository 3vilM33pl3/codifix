# Build only the dedicated gateway, never the general Codifix app.
# Supply reviewed, compatible, digest-pinned Rust and OS base images explicitly.
ARG RUST_BUILD_IMAGE
ARG GATEWAY_RUNTIME_IMAGE
FROM ${RUST_BUILD_IMAGE} AS build
WORKDIR /src
COPY . .
RUN cargo build --release -p codifix-federation-runtime --bin codifix-federation-gateway --locked
FROM ${GATEWAY_RUNTIME_IMAGE}
COPY --from=build /src/target/release/codifix-federation-gateway /usr/local/bin/codifix-federation-gateway
USER 65532:65532
ENTRYPOINT ["/usr/local/bin/codifix-federation-gateway"]
CMD ["/config/gateway.json"]
