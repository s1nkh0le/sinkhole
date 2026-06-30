# Philosophy

This page explains the main design decisions behind sinkhole and why they were made. It is background rather than a how-to. If you just want to run it, start with [Core concepts](concepts.md) and the [Quickstart](../usage/quickstart.md).

## Two programs, not one

sinkhole is split into a filter and a controller because they have very different jobs and very different failure modes. A filter sits next to the service it guards, often on a host that can reboot or get compromised, and it has to keep enforcing rules no matter what else is going on. The controller is where a person works: it manages the whole fleet, collects what every filter records, and serves the UI.

Keeping the two apart means the part on the traffic path stays small and focused, while the part with all the moving pieces (a database, a fleet to track, an API) runs separately, off the path of the messages that have to keep flowing. A filter never needs the controller to be reachable in order to keep doing its job.

## A proxy in front, not a library inside

A filter sits in front of a service and treats it as a black box. It works at the network level, on the service's ports, and does not look inside the service or depend on how it is built. You do not modify, rebuild, or even restart the service to guard it: you reroute its port through the filter. That makes sinkhole behave the same way regardless of what the service is written in, and it means a filter can be added or removed without touching the thing it protects.

The one thing a filter may need from the service is its TLS certificate and key, so it can terminate TLS and see the traffic in the clear. That is still a network concern, not knowledge of the service's logic.

## Open by default, tightened live

A fresh filter has no rules and forwards everything unchanged. This is deliberate. Installing the filter should never be the thing that takes a service down, and you should not have to write rules before the service can come up. You bring the filter up first, confirm traffic still flows, then add rules while it runs.

!!! note
    The trade-off is that a filter with no rules protects nothing. Open by default is about deployment order, not security. The protection comes from the rules you add afterwards.

## Rust on the path

Every message, in both directions, is parsed and run through the ruleset before it is allowed onward. That work sits directly between the client and the service, so it has to be fast and its latency has to be steady. The filter is written in Rust for that reason, and the engine keeps the per-message cost low: regular expressions run in linear time, with no catastrophic backtracking on hostile input, and a filter is evaluated only when its markers say it applies, so most messages touch only a few rules.

## Changing rules without downtime

In a live game you rewrite rules constantly. If applying a new ruleset dropped connections or briefly stopped traffic, you would think twice before touching the rules at all, which is the opposite of what a tool for reacting quickly should do. So a port's rules are swapped while traffic keeps flowing, and the change does not tear down existing connections.

## The controller pulls, it does not hold the filters up

The controller polls each filter for its records, events, and health. Filters do not depend on the controller to keep enforcing, and the controller does not depend on a filter reaching out to it. If the controller goes down, the filters carry on with the rules they already have. When a filter restarts, it restores its last ruleset from its local database on boot, so it comes back up already enforcing the same rules rather than empty.

Because of this, a controller outage does not loosen any filter. The filters keep enforcing whether or not the controller is reachable. The controller is there to manage and observe the fleet, not to keep it running.

## Recognize, then decide

Rules come in two kinds on purpose. Markers recognize a situation, such as "this request is authenticated" or "this looks like an upload", without deciding anything. Filters make the allow-or-block call, and can be limited to apply only when certain markers held. Splitting recognition from decision lets you describe a condition once and reuse it, instead of repeating the same detection inside every rule.

## Decide once, record everything

Every message produces a record: the decision, the rule that caused it, and the bytes involved. Knowing that something was blocked is not enough during attack and defense. You need to see what it was and why. Visibility is treated as a feature rather than a debugging aid, which is why records are structured and stream to the controller as traffic happens.

## A small, private control plane

The control API a filter exposes is reached only by the controller, over a network meant to be private. It is not built for browsers, and the web UI never talks to it. Operators authenticate to the controller, the controller authenticates to each filter, and the trust boundary stays in one place.
