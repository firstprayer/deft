# Tests

This folder holds the unit tests: functions and command specs, checked in-process.

```bash
cargo test-unit
```

They must pass before every commit, and need no network or external programs. Help text is checked with snapshots: after a change, run `INSTA_UPDATE=always cargo test -p deftcli --test unit_help` and read the `.snap` files.

End-to-end tests and the agent benchmark are kept in a private repository. Results are published at [deftcli.com/tests](https://deftcli.com/tests/) and [deftcli.com/evals](https://deftcli.com/evals/).
