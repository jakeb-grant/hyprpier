# Review findings — 2026-06-12

Legend: `[ ]` todo · `[~]` in progress · `[x]` fixed · `[-]` declined

## Panics

1. `[x]` **Arrange screen panics with zero monitors** — `src/tui/monitor_arrange.rs`
   `move_left/move_right/move_up/move_down/align_up/align_down` index
   `self.rows[self.selected]` unconditionally. New profile → `a` (Arrange)
   before `d` (Detect) → press `h/l/J/K` → index out of bounds.
   *Confirmed:* code inspection; `rows` is empty when `profile.monitors` is.
   *Fix:* guard `monitors.is_empty()` in the movement methods.

2. `[x]` **Byte-slicing EDID descriptions can panic on non-ASCII** —
   `src/tui/monitor_arrange.rs` (`&d[..35]`), `src/tui/profile_editor.rs`
   (`&d[..40]`). EDID vendor strings may be non-ASCII; slicing mid-codepoint
   panics. *Fix:* truncate by chars, not bytes.

3. `[x]` **No panic hook to restore terminal** — `src/tui/app.rs`.
   A panic unwinds past the raw-mode restore in `run()`, leaving the
   terminal raw + alternate screen. *Fix:* panic hook that restores the
   terminal before printing the panic.

## Correctness

4. `[x]` **Deleting a profile leaves dangling metadata** — `src/profile.rs`,
   `src/tui/app.rs`. `active_profile`, `dock_profiles`, `undocked_profile`
   can still reference the deleted name; daemon then fails on every dock
   event. *Fix:* scrub metadata references on delete.

5. `[x]` **Renaming a profile creates a copy** — `src/tui/app.rs`.
   Save under a new name leaves the old file and stale metadata references.
   *Fix:* rename semantics — delete old file, migrate metadata references.

6. `[x]` **Flipped transforms (4–7) mishandled** — `src/profile.rs`
   `logical_size` swaps only for transforms 1/3, but 5/7 (flipped-90/270)
   also swap dimensions. `rotate()` does `(t+1)%4`, destroying the flip bit.
   Rotation display shows `450°` for transform 5. *Fix:* swap on odd
   transforms; rotate low 2 bits only; fix display.

7. `[x]` **One TUI keypress error kills the whole TUI** — `src/tui/app.rs`.
   `a` (apply) in the profile list and `d` (detect) in the editor propagate
   `?` up and exit the TUI. *Fix:* surface in an error message instead.

8. `[x]` **Dock reconnect with same active profile skips port re-resolution**
   — `src/apply.rs`. `apply_auto` early-returns on `current == target`,
   trusting metadata; replug may have reassigned port names. *Fix:* skip
   only when the generated config matches what's on disk.

## Design / robustness

9. `[ ]` **Daemon serializes 3 s sleeps in the accept loop** —
   `src/daemon.rs`. `notify` clients block ~3 s each; N udev events queue
   N×3 s serial sleeps + N redundant applies. *Fix:* debounce on a worker
   thread; reply to clients immediately.

10. `[ ]` **`monitors.lua` written non-atomically** — `src/hyprland.rs`
    `write_config` uses plain `fs::write`; it's the one file Hyprland
    actually reads (incl. mid-`hyprctl reload`). *Fix:* temp + rename.

11. `[ ]` **`generate_workspaces` starves the third monitor** —
    `src/hyprland.rs`. "5 each" → monitors 1–2 take all 10 workspaces,
    monitor 3+ gets none. *Fix:* distribute `10 / count` with remainder.

12. `[ ]` **`y_offsets` not swapped on monitor swap** —
    `src/tui/monitor_arrange.rs` `move_left/move_right` swap `monitors` and
    `rows` but not `y_offsets`; alignment stays with the slot. *Fix:* swap it.

13. `[ ]` **Removing a monitor leaves its workspaces** —
    `src/tui/monitor_arrange.rs` `remove_selected` doesn't drop workspaces
    referencing the removed output. *Fix:* retain-filter + update defaults.

14. `[ ]` **`config_dir` hardcodes `/home/{user}` under sudo** —
    `src/config.rs`. Wrong for non-standard homes (e.g. `/var/home`).
    *Fix:* look up the home dir from `/etc/passwd`.

15. `[ ]` **udev rule auto-authorizes all Thunderbolt devices silently** —
    `src/setup.rs`. Disables TB security (DMA surface) with no warning;
    also bakes `current_exe()` path into the rule. *Fix:* warn at install
    time about both.

## Minor

16. `[ ]` Dead `bottom: bool` param in `best_overlap_edge` (always `true`) —
    `src/hyprland.rs`.
17. `[ ]` `get_hyprland_instance_signature` picks an arbitrary dir when
    multiple Hyprland instances exist; prefer most recent — `src/hyprland.rs`.
18. `[ ]` CLI `apply <name>` skips `validate_profile_name` (TUI validates,
    CLI doesn't) — `src/apply.rs`.
19. `[ ]` Clippy warnings (6): collapsible if, manual clamp, `to_vec`,
    needless `last()` on double-ended iterator, needless borrows.
