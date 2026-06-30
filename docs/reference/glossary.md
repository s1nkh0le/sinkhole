# Glossary

| Term | Definition | Why it exists |
|---|---|---|
| Filter | The proxy that guards one service: it forwards traffic and runs the rules on it. | The thing that actually allows or drops bytes. See [Filter](../architecture/filter.md). |
| Controller | The single orchestrator that manages a fleet of filters. | One place to push rules and collect data. See [Controller](../architecture/controller.md). |
| Service | One managed filter as the controller knows it (id, url, token). | The controller's unit of management. |
| Service id | The stable name you use to refer to a managed filter in the controller; it appears in API paths and database keys. | How you address a filter. See [Identifiers](identifiers.md). |
| Service name | An optional display label for a service. | Shown in the UI; never addresses anything. |
| Message | One unit of evaluated traffic: a full HTTP request or response, or a raw chunk when the traffic is not HTTP. | The thing a decision is made for. |
| Direction | `inbound` (client to service), `outbound` (service to client), or `both`. | Lets a rule target one flow (a status check, for instance, only makes sense outbound). |
| Scope | Which part of a message a rule reads: `stream`, `http.path`, `http.body`, `http.header`, `http.method`, `http.status`, or `timing`. | Keeps matchers simple and reusable. |
| Matcher | The test a rule runs: regex, byte or hex signature, size, session timing, or always. | The actual test logic, under a stable JSON shape. |
| Marker | A rule that only records whether something is true; it never decides. | Check a fact once and gate many filters on it. |
| Filter rule | A rule that can allow or block; it runs after markers and can be limited to apply only when given markers held. | Carries the actual block or allow logic. |
| Gating | Limiting a filter to apply only when certain markers held (`needed_markers_all`, `needed_markers_any`). | Express logic like "A and (B or C)". |
| Mode | How a filter's match affects the verdict: `blacklist`, `whitelist_all`, `whitelist_any`. | Describes what a hit means. |
| Decision | The verdict for a message: allow or drop. | Forward, or tear down and record. |
| Drop reason | The structured reason attached to a drop (blacklist, whitelist-all miss, whitelist-any miss). | Lets the UI show why something was blocked. |
| Evaluation record | The full per-message record: decision, markers, drop reason, per-rule detail, HTTP metadata, and captured bytes. | The analyzable output, stored and centralized. |
| ECAM | sinkhole's operational event log (severity, origin, message). | Operator-facing health and security events, separate from records. See [ECAM](ecam.md). |
| Ruleset | The per-port JSON document of markers and filters. | The unit that is deployed and reloaded. See [Rule model](../filtering/rules.md). |
| Hydration | Restoring a filter's ruleset and config from its local database on boot. | A filter re-enforces its rules after a restart, on its own. |
| Cursor | How far the controller has pulled records or events from a filter. | Resume after a restart instead of re-pulling history. |
| Pruner | The controller loop that trims already-copied records on filters and applies retention to the central database. | Keeps storage bounded on both ends. See [persistence](../architecture/persistence.md). |
| Sink | A consumer of a filter's output (records to the database and stdout; events to the ECAM buffer). | Separates producing records and events from storing them. |
