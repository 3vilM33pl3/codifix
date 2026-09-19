# Package previously reviewed local release binaries without rebuilding or
# downloading dependencies. This is not a signed release or image attestation.
# The build context must contain only this file and the two named binaries.
ARG GATEWAY_RUNTIME_IMAGE
FROM ${GATEWAY_RUNTIME_IMAGE}
ARG GATEWAY_SHA256
ARG ENROLL_SHA256
COPY memory-federation-gateway memory-federation-enroll /usr/local/bin/
RUN test "${#GATEWAY_SHA256}" = 64 && test "${#ENROLL_SHA256}" = 64 \
    && printf '%s  %s\n' "$GATEWAY_SHA256" /usr/local/bin/memory-federation-gateway \
        "$ENROLL_SHA256" /usr/local/bin/memory-federation-enroll | sha256sum --check --strict \
    && chmod 0555 /usr/local/bin/memory-federation-gateway /usr/local/bin/memory-federation-enroll
USER 65532:65532
ENTRYPOINT ["/usr/local/bin/memory-federation-gateway"]
CMD ["/config/gateway.json"]
