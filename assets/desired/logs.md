# Logs

Log volume should stay bounded; the journal should not grow without limit.

```assertions
[[assert]]
key = "logs.journal_mb"
op = "lt"
value = 1000
```
