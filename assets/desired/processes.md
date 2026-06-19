# Processes

No zombie processes should accumulate. Killing processes is risky, so deviations
are escalated rather than auto-fixed.

```assertions
[[assert]]
key = "processes.zombies"
op = "eq"
value = 0
```
