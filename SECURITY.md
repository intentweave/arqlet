# Security Policy

## Supported versions

Arqlet is a prototype without releases. Only the latest `main` branch receives
security fixes; there are no maintained release branches yet.

## Reporting a vulnerability

Please do not open a public issue. Report vulnerabilities privately through
GitHub: open the repository's **Security** tab and choose **Report a
vulnerability** (private vulnerability reporting).

Include what is affected (crate, package, WASM ABI, playground backend), how to
reproduce it, and the revision you tested.

Arqlet is maintained by one person. You can expect a first reply within about
two weeks; there is no guaranteed fix time. If a report is accepted, the fix and
a GitHub security advisory are published together, crediting you unless you
prefer otherwise. If it is declined, you get the reason.

## Scope

The playground backend (`apps/playground/server.py`) is a loopback development
server, not a production service; do not expose it to a network. Reports about
it are still welcome.
