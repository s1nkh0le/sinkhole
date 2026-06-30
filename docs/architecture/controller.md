# Controller

The controller is the single place you manage a fleet of filters from. It does not sit in front of any service and never sees live traffic itself. Instead it keeps a mirror of every filter's data in one database and serves the API that the web UI and operators use. Operators talk to the controller, and the controller talks to the filters.

## Services

A **service** is one managed filter. To manage a filter the controller needs three things: the filter's address, the filter's token, and a name to refer to it by. Services can be defined up front through configuration, or added, reconfigured, and removed while the controller is running, through the API.

The controller is the single source of truth for which filters exist. Adding a service starts the background work for that filter, and removing one stops it. The fleet is saved to the central database, so after a restart the controller comes back managing the same services, including any added at runtime, and resumes from where it left off rather than re-pulling everything. This can be turned off (`SINKHOLE_CONTROLLER_DB_HYDRATE`) but is on by default. See [Data and persistence](persistence.md#controller-fleet-hydration).

## What it polls from each filter

The controller reaches out to each filter on a schedule and pulls four kinds of data, each on its own cadence you can tune.

| What | It pulls | Cadence |
|---|---|---|
| Health | whether the filter is reachable and healthy | `SYNC_HEALTH_INTERVAL_MS` |
| State | each port's config and current ruleset | `SYNC_CONFIG_INTERVAL_MS` |
| Events | the filter's ECAM events | `SYNC_ECAM_INTERVAL_MS` |
| Records | evaluation records, per port | `SYNC_RECORDS_INTERVAL_MS` |

Records and events are pulled with a cursor, so each poll fetches only what is new. Record pulls fan out across a service's ports, with the number of concurrent fetches bounded by `SYNC_CONCURRENCY`. Health is summarized per service as healthy or degraded, with the specific failures noted so you can see what is wrong.

## The mirror

The controller keeps an in-memory copy of the whole fleet: for each service its config, current health, cursors, display metadata, and its ports with their latest snapshots. The `/state` endpoint returns this view. Everything it pulls is also written to the central database, so the whole fleet's history can be queried from one place.

## Edits go through the controller

Operators never edit a filter directly. When you change a port's ruleset or config, the controller checks that the service and port exist, sends the change to that filter using the filter's token, and updates its own copy once the filter accepts it.

This keeps the controller authoritative and makes errors consistent. A filter rejecting a ruleset comes back as a `400` with the reason, an unreachable filter as a `502`, a slow one as a `504`.

!!! note
    This is why the web UI only ever needs to reach the controller. The control path is always UI to controller to filter, never UI to filter.

## Keeping storage bounded

A single background loop prunes both ends. It tells each filter to delete records the controller has already copied, never past the point it has pulled to, and it applies a retention window to the central database. The details, including the safety cutoff that prevents deleting un-pulled data, are in [Data and persistence](persistence.md).

## API

The controller's API is bearer-authenticated and listens by default on `BIND=0.0.0.0:7000`. CORS is configured per deployment and is fail-closed outside debug mode. See [Authentication](../configuration/auth.md). The full endpoint list is the [Controller API reference](../pages/reference/swagger/openapi-ctrl.md).
