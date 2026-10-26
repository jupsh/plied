---
id: P01
title: Check controls
wording: Before interpreting a comparison, verify a known positive and delivered dose against this plan.
made: "E001"
state: open
governs: [interpret]
nodes: [T001]
reviews:
  - action: interpret:run-01
    disposition: blocks
    because: E002 has no delivered-dose measurement and only partial observations
    by: example-reviewer (session research)
    at: "2026-10-26 16:18 UTC"
    ref: "E002"
  - action: interpret:run-02
    disposition: satisfied
    because: >-
      E003 positive 6 >= 5 and dose 10 within 9..11 meet E001; run 01 is excluded from the completed
      comparison
    by: example-reviewer (session research)
    at: "2026-10-26 16:18 UTC"
    ref: "E003"
  - action: interpret:run-03
    disposition: satisfied
    because: E005 positive 6.5 >= 5 and dose 9.5 within 9..11 meet E001; difference is 0.2
    by: example-reviewer (session research)
    at: "2026-10-26 16:18 UTC"
    ref: "E005"
---

## History
- 2026-10-26 16:18 UTC example-reviewer (session research): created
- 2026-10-26 16:18 UTC example-reviewer (session research): review for interpret:run-01: blocks — E002 has no delivered-dose measurement and only partial observations
- 2026-10-26 16:18 UTC example-reviewer (session research): review for interpret:run-02: satisfied — E003 positive 6 >= 5 and dose 10 within 9..11 meet E001; run 01 is excluded from the completed comparison
- 2026-10-26 16:18 UTC example-reviewer (session research): review for interpret:run-03: satisfied — E005 positive 6.5 >= 5 and dose 9.5 within 9..11 meet E001; difference is 0.2
