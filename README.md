# FOM-MultiPlayer

Standalone Rust + GameNetworkingSockets (GNS) multiplayer mod for Fields of Mistria.

Replaces the legacy client↔relay-hub UDP design with GNS's P2P transport (NAT
traversal via built-in ICE, symmetric-connect mode, AES-GCM-256/Curve25519
encryption) while keeping a host-hub star topology and a small self-hosted
signaling service for rendezvous only — never game traffic.

See `docs/dependency-policy.md` for the project's stance on third-party
dependencies.
