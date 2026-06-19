# Users & auth

Failed-login volume should stay low; account/sudoers changes are high-risk and
always escalated.

```assertions
[[assert]]
key = "users-auth.failed_logins_1h"
op = "lt"
value = 50
```
