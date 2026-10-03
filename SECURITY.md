# Security policy

This alpha is intended for trusted local applications. Collection names and
vector/index boundaries are validated, but stored files are not authenticated.
Do not expose database-directory writes to untrusted users. Network filesystems
and external modification of mapped files are unsupported.

Report vulnerabilities privately through the repository's GitHub Security
advisory reporting if enabled; otherwise contact a maintainer privately before
opening a public issue. Include a minimal reproduction, revision and platform.

Only the current development revision receives alpha fixes. Weekly dependency
checks and pull-request checks run cargo-audit and cargo-deny. Process-kill tests
exercise recovery but do not simulate hardware faults or power loss.
