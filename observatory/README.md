# Musical Chairs Observatory

Small Motoko canister for Cycle Manager integration.

It exposes target discovery, latest controller-status snapshots with observed controllers, controlled-status collection, and admin allowlist management. It does not contain gameplay, custody, token, or canister lifecycle wrappers. `collect_controlled_statuses` is admin-only.

See `docs/cycle-manager-observability.md` for deployment and controller setup.
