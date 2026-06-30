#!/usr/bin/env bash
# Print the default local-dev environment for one binary, as `export` lines on
# stdout. Read them as a reference, or eval them to load them into your shell:
#
#   scripts/print_env.sh filter                    # just print the vars
#   eval "$(scripts/print_env.sh filter)"     && cargo run -p filter
#   eval "$(scripts/print_env.sh controller)" && cargo run -p controller
#
# Both roles use a shared throwaway token (override SINKHOLE_DEV_TOKEN), so a filter
# and controller loaded this way talk to each other out of the box. DEBUG config for
# local poking only — never production. Reference: docs/configuration/env.md.
set -euo pipefail

TOKEN="${SINKHOLE_DEV_TOKEN:-dev-token}"

case "${1:-}" in
    filter)
        # The filter stores only the token hash; the controller uses the plaintext.
        hash="$(printf %s "$TOKEN" | sha256sum | awk '{print $1}')"
        cat <<EOF
export SINKHOLE_FILTER_UPSTREAM_HOST=127.0.0.1
export SINKHOLE_FILTER_LISTEN_MAP=8080:80
export SINKHOLE_FILTER_CONTROL_BIND=0.0.0.0:7001
export SINKHOLE_FILTER_DEBUG=true
export SINKHOLE_FILTER_DB_HYDRATE=true
export SINKHOLE_FILTER_DB_PATH=sinkhole-data/sinkhole-filter.db
export SINKHOLE_FILTER_BODY_LIMIT_BYTES=1048576
export SINKHOLE_FILTER_AUTH_TOKEN_HASH=$hash
EOF
        ;;
    controller)
        cat <<EOF
export SINKHOLE_CONTROLLER_BIND=0.0.0.0:7000
export SINKHOLE_CONTROLLER_DEBUG=true
export SINKHOLE_CONTROLLER_DB_PATH=sinkhole-data/sinkhole-ctrl.db
export SINKHOLE_CONTROLLER_SERVE_FRONTEND=true
export SINKHOLE_CONTROLLER_SERVICES='[{"id":"local","url":"http://127.0.0.1:7001","token":"$TOKEN"}]'
EOF
        ;;
    *)
        echo "usage: $0 <filter|controller>   (e.g. eval \"\$($0 filter)\")" >&2
        exit 1
        ;;
esac
