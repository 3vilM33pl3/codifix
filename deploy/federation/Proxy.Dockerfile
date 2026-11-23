ARG PROXY_IMAGE
FROM ${PROXY_IMAGE}
# The gateway edge listens on 8443. Strip the upstream binary's low-port file
# capability so execution works under cap_drop ALL, not by granting more power.
RUN setcap -r /usr/bin/caddy
