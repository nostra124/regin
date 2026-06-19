# Time sync

The clock should track NTP closely (sub-100ms offset).

```assertions
[[assert]]
key = "time-sync.offset_ms"
op = "lt"
value = 100
```
