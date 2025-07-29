


## Configuring

Execute `setup_env.sh` before running the filter to parameterize it

```bash
export TFILTER_UPSTREAM_HOST=127.0.0.1           # challenge host inside the same docker net
export TFILTER_LISTEN_MAP=8080:80,8081:81        # "public1:upstream1,public2:upstream2,..."
export TFILTER_CTRL_PORT=7000                    # port for apis exposed to the controller
export TFILTER_DEBUG=true
export TFILTER_HYDRATE_DB=true
export TFILTER_BODY_LIMIT_BYTES=1048576          

AUTH_SECRET="ss"
export TFILTER_AUTH_HASH=$(printf %s "$AUTH_SECRET" | sha256sum | awk '{print $1}')

# run:
cargo run
```


