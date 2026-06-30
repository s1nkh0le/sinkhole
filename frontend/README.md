# Sinkhole Frontend Dashboard

The frontend for the Sinkhole project is a modern, high-performance web dashboard built with React and Vite. It provides deep visibility into captured traffic and allows you to dynamically author intercept rules for the Sinkhole backend.

## Architecture

* **Vite + React:** Fast HMR, optimized builds.
* **TypeScript:** End-to-end type safety, heavily leveraging OpenAPI schema generation from the backend.
* **Vanilla CSS / Custom Tokens:** Built from the ground up with a custom design system focusing on performance and modern, sleek UI (no heavy UI frameworks).

## Key Features

### Traffic Analyzer
* **Live Polling:** Real-time polling and grouping of connections.
* **Deep Inspection:** View individual message payloads with side-by-side hexadecimal and raw text representations.
* **Robust Filtering:** 
  * Select Traffic by Service and Port.
  * Filter by specific Connection ID ranges or exact IDs.
  * Filter by Time (Relative ranges like "Last 15m" or Explicit date ranges).
  * Filter by Message Content (HTTP Method, Status Codes, Protocol, Path).
  * Filter by Message Counts limits per connection.
* **Markers & Tags:** Tag specific traffic patterns on the backend and rapidly filter connections that trigger them on the frontend.

### Export Tools
* **Raw Bin Download:** Export the raw chronological bytes of a connection as a `.bin` file. You can optionally annotate the binary stream with `> ` (inbound) and `< ` (outbound) prefixes to clearly delineate the traffic flow.
* **Pwntools Generator:** With a single click, generate a ready-to-run Python Pwntools script that replays a client's attack traffic to easily reproduce exploits locally. Automatically adds comments where server responses are expected so you can synchronize your exploit.

### Dynamic Ruleset Editor
* Visually build complex filtering pipelines.
* Define **Markers** (passive pattern matching) and **Filters** (active drop/allow decisions based on headers, paths, or byte arrays).
* Apply rules selectively per-service or globally.

## Setup & Development

Make sure the Sinkhole backend is running and exposing its Control API (e.g. `127.0.0.1:7001`). 

Install dependencies:
```bash
npm install
```

Run the development server:
```bash
npm run dev
```

Build for production:
```bash
npm run build
```

## API Synchronization

If the backend OpenAPI schema changes, regenerate the TypeScript types using the `openapi-typescript` CLI tool against the running backend:

```bash
npx openapi-typescript http://127.0.0.1:7001/api/v1/openapi.json -o src/api/schema.d.ts
```
*(Ensure the port matches your backend's control bind configuration)*