# TLS certificates

No certificate should be within two weeks of expiry. Renewal is domain-specific,
so a deviation is escalated rather than auto-fixed.

```assertions
[[assert]]
key = "certificates.min_days_to_expiry"
op = "gt"
value = 14
```
