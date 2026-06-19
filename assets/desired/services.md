# systemd services

No systemd units should be in a failed state.

```assertions
[[assert]]
key = "services.failed_units"
op = "eq"
value = 0
```
