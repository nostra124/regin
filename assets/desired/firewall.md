# Firewall

A firewall should be active. Firewall changes are red-line territory, so a
deviation is escalated, never auto-fixed.

```assertions
[[assert]]
key = "firewall.active"
op = "eq"
value = "yes"
```
