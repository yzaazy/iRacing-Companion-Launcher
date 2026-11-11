---
name: port-module
description: Procedure for porting one Python module of the launcher to Rust with its behaviour intact. Use when a brief assigns Python files to port.
---

1. **Read.** Every Python file in your brief, top to bottom, plus the callers that decide how it is used. Then the matching sections of `docs/rewrite/behaviour.md`.

2. **Inventory.** Write down every behaviour of the module: each branch, each return value, each log string, each file or registry touch, each process action, each sleep. This list is the checklist for the rest of the work. Done when every line of the Python file belongs to an inventory item.

3. **Test first.** For each inventory item that can be exercised without the developer's real apps, write a failing Rust test. Use temp directories, fixture files written in Python's exact format, and dummy child processes the test starts and stops.

4. **Port.** Fill in the stubbed signatures from `docs/rewrite/contracts.md` until the tests pass. Keep Python's order of operations: side effects such as saving config happen at the same points.

5. **Gate.** Run the `rust-gate` skill.

6. **Report.** Return the inventory with each item marked `tested`, `ported, untestable here` (say why) or `differs` (with the Python `file:line`). Done when no item is unmarked. An item marked `differs` must be one of the deliberate differences in `behaviour.md`; anything else goes back to step 4.
