# Tests

Bulwark is comprised of several sub-crates, each with their own unit tests. These can be run together or independently.

```bash
cargo test -p $CRATE_NAME
```

## Integration Tests

Bulwark's integration tests are run as part of the `bulwark-cli` crate's tests.
