# Memory & load

Memory pressure and CPU load should stay moderate. Sustained pressure is a
deviation to escalate — killing processes is too risky to automate.

```assertions
[[assert]]
key = "mem.used_percent"
op = "lt"
value = 90
[[assert]]
key = "load.per_core"
op = "lt"
value = 2.0
```
