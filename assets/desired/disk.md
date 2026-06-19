# Disk

Root and data volumes should keep comfortable free space; filesystems stay writable.

```assertions
recurrence_threshold = 4

[[assert]]
key = "disk.root.use_percent"
op = "lt"
value = 90
description = "root filesystem under 90% used"
```
