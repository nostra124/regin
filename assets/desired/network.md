# Network & connectivity

A default route and working DNS should be present. Network changes are high-risk,
so deviations are escalated rather than auto-fixed.

```assertions
[[assert]]
key = "network.default_route"
op = "eq"
value = "up"
[[assert]]
key = "network.dns_ok"
op = "eq"
value = "yes"
```
