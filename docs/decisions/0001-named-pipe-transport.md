# 1. A named pipe is the main transport

Date: 2026-10-23 · Status: accepted

## Context

Apps, scripts and agent hooks need to push live activities into Qapsule, and Qapsule needs to call some of them back when a button is pressed. Everything runs on one Windows machine, often as a short-lived process (a hook that runs for a few milliseconds).

## Options

1. **Local HTTP server** on a fixed or discovered port. Easy to call from any language with `curl`, but any process of any user (and any web page, through the browser) can reach `localhost`, ports collide, and firewalls and antivirus tools sometimes interfere.
2. **Named pipe** `\.\pipe\qapsule`. Native to Windows, no port, and access is controlled by a normal Windows security descriptor.
3. **Files in a watched folder.** Simple, but slow, racy and with no way to reply or call back.

## Choice

A named pipe, speaking newline-delimited JSON (protocol v1, defined in `crates/protocol`). The pipe's security descriptor (`D:P(A;;GA;;;<current user SID>)`) grants access to the current user only, and remote clients are rejected. A client keeps its connection open to receive callbacks for its own activities. The first pipe instance is created with `FILE_FLAG_FIRST_PIPE_INSTANCE`, which also stops a second copy of Qapsule from starting.

## Consequences

- Other users on the machine, remote machines and web pages cannot push to Qapsule.
- Clients need a pipe-capable language or the `qapsule` CLI. Shell scripts use the CLI.
- A future breaking change means protocol v2 on a new pipe name, so v1 clients keep working.
- Webhooks from outside the machine (GitHub, CI) arrive through ntfy.sh instead, as planned for v1.0.
