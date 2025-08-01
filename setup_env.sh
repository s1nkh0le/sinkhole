export SINKHOLE_FILTER_UPSTREAM_HOST=127.0.0.1           # challenge host inside the same docker net
export SINKHOLE_FILTER_LISTEN_MAP="8080:80"              # "public1:upstream1,public2:upstream2,..."
export SINKHOLE_FILTER_CTRL_PORT=7000                    # port for apis exposed to the controller
export SINKHOLE_FILTER_DEBUG=true
export SINKHOLE_FILTER_HYDRATE_DB=true
export SINKHOLE_FILTER_DB_PATH="sinkhole.db"
export SINKHOLE_FILTER_BODY_LIMIT_BYTES=1048576          # optional

AUTH_SECRET="ss"
export SINKHOLE_FILTER_AUTH_HASH=$(printf %s "$AUTH_SECRET" | sha256sum | awk '{print $1}')
