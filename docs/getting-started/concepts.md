# Core concepts

This page gives you a mental model of sinkhole before the details. Terms in **bold** have a precise meaning that the rest of the docs rely on, and the [Glossary](../reference/glossary.md) collects them in one place.

## The filter and the controller

sinkhole is split into two programs with very different jobs.

| | Filter | Controller |
|---|---|---|
| How many | one in front of each guarded service | one, central |
| Sees | live client and service traffic | no traffic, only the filters it manages |
| Does | proxy traffic, evaluate rules, record decisions | manage filters, collect their data, serve the API |
| Stores | its own records and rules | a copy of every filter's data |

A filter is configured once at startup through environment variables: its ports, the service it forwards to, its token, and a few limits. Everything after that, the rules and any live changes, goes through the controller. You and the web UI only ever talk to the controller, which relays changes to the right filter, and never to a filter directly.

To put a filter under the controller's management, you give the controller the filter's address (its host and port), its token, and a name to refer to it by. From then on you work with that name.

## Messages

A **message** is one complete unit of traffic that the filter evaluates. If the traffic is HTTP and the filter can read a whole request or response, that request or response is the message. Otherwise, for a binary protocol, or a body too large to hold, or one streamed in pieces, the filter works with a raw chunk of bytes instead.

Every message gets a single **decision**, either `Allow` or `Drop`, and produces one **evaluation record** describing what happened. A `Drop` does not just discard that one message: the filter tears down the whole connection. That holds in either direction, an inbound request that matches an attack pattern, or an outbound response where the service is leaking something it should not.

A message also has a **direction**: `inbound` is client to service, `outbound` is service to client. A rule can apply to one direction or both.

## How rules decide

A **ruleset** belongs to a single port and contains two kinds of rules.

- **Markers** look at a message and note whether something is true about it, for example "this request is authenticated" or "this looks like a file upload". A marker never blocks anything on its own. It only records its answer so other rules can use it.
- **Filters** are the rules that actually allow or block. A filter can be set to apply only when certain markers were true, so you can say things like "only check this on uploads". Each filter is one of three modes:
    - `blacklist`: if it matches, the message is dropped.
    - `whitelist_all`: every whitelist-all filter that applies must match, otherwise the message is dropped.
    - `whitelist_any`: at least one whitelist-any filter that applies must match, otherwise the message is dropped.

Each rule reads one part of a message, its **scope** (the path, body, headers, method, status, the raw stream, or timing), and tests it with a **matcher** (a regex, a byte or hex signature, a size check, a timing check, or a plain "always"). If the part a rule needs is not present in the message, the rule is skipped. A missing field never counts as a match.

The full behavior is in [Rule model](../filtering/rules.md) and [Evaluation semantics](../filtering/evaluation.md).

## Records and events

Two separate streams flow from each filter up to the controller.

- **Evaluation records**: one per message. They hold the decision, which markers and filters matched, the reason a message was dropped, the HTTP metadata, and the captured bytes.
- **ECAM events**: sinkhole's own operational log, things like configuration errors, network failures, and security warnings. These are separate from per-message records and from ordinary log lines. See [ECAM events](../reference/ecam.md).

Both are stored on the filter first and pulled into the controller's central database on a schedule. See [Data and persistence](../architecture/persistence.md).

## Authentication

Every request between the web UI, the controller, and the filters carries a **bearer token**. The controller checks the token on requests it receives, and when it calls a filter it sends that filter's token, so it keeps each filter's token in order to do so. Tokens travel over the control network on every request, which is why that network is meant to be private rather than exposed.

!!! note
    What happens when a token is missing or wrong, and how CORS is handled, is covered in [Authentication](../configuration/auth.md).
