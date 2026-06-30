# sinkhole

sinkhole is a programmable traffic filter that sits in front of a service and
decides, message by message, what is allowed to reach it. It was built to guard
CTF challenge hosts during attack/defense events, but it works for any TCP service
you want to watch and filter.

Full documentation and guides: **https://s1nkh0le.github.io/sinkhole/**

## What it does

A *filter* is a transparent TCP proxy in front of one service. It forwards traffic
upstream and runs a rule engine on every parsed message to reach an allow or drop
decision, then records what it saw. Rules are made of *markers* (predicates that
gate) and *filters* (blacklist or whitelist groups), so you describe the traffic
you trust or want to block without recompiling anything, and reload it live.

A *controller* manages a fleet of filters from one place. It polls each filter for
state, records, and events, mirrors everything into a central database, and serves
the API and web UI that operators use. All edits go through the controller, which
relays them to the filters.

## Layout

- `filter/` is the data plane: one proxy guarding one host.
- `controller/` is the control plane: one orchestrator over many filters.
- `shared/` is the wire model both sides share.
- `frontend/` is the web UI.

## Running it

Both binaries are configured through environment variables. `scripts/print_env.sh`
prints a known-good local-dev config for either binary; eval it to load the vars,
then run:

```bash
eval "$(scripts/print_env.sh filter)"
cargo run -p filter
```

See the [documentation](https://s1nkh0le.github.io/sinkhole/) for the controller,
the rule model, deployment, and the full list of environment variables.

## License

sinkhole is released under the MIT License. See [LICENSE](LICENSE).
