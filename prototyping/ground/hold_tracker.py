"""Turns keypad.Keys press/release edges into 'tap' / 'hold' events.

No hardware imports -- HoldTracker only needs an object shaped like
keypad.Keys (an `events` queue whose `.get()` returns objects with
`.key_number` and `.pressed`), so this can be driven with a fake queue and
unit tested off-board.
"""

DEFAULT_HOLD_MS = 2000
DEFAULT_GRACE_MS = 250


class HoldTracker:
    """Turns keypad.Keys press/release edges into 'tap' / 'hold' events.

    keypad.Keys debounces and timestamps presses in the background (a
    supervisor-level scan, not the Python main loop), so an edge is never
    missed just because the loop is stuck in a slow GPS or display call. This
    class only adds the piece keypad.Keys doesn't have: "still held after
    hold_ms" isn't an edge, so it has to be checked every pass rather than
    read off the event queue.

    A tap fires on RELEASE, so a hold does not also register as a tap.
    """

    def __init__(self, names, hold_ms=DEFAULT_HOLD_MS, grace_ms=DEFAULT_GRACE_MS):
        self.names = names  # index (key_number) -> name
        self.hold_ms = hold_ms
        self.grace_ms = grace_ms
        self.down_since = {}     # name -> ms timestamp of the ORIGINAL press
        self.released_at = {}    # name -> ms timestamp of a not-yet-finalized release
        self.hold_fired = set()

    def poll(self, keys, now):
        """Drain queued edges and check for newly-expired holds.

        A release isn't finalized into a tap immediately -- it's held for
        grace_ms in case a same-key re-press arrives right behind it. A cheap
        switch or marginal contact can drop out for a moment mid-hold; without
        this, that one glitch would restart the whole hold_ms countdown and a
        genuine hold could take several retries (many real seconds) to ever
        register, even though tap (which only needs one clean edge) is fine.

        Returns a list of (name, 'tap' | 'hold') pairs, in order.
        """
        out = []
        while True:
            event = keys.events.get()
            if event is None:
                break
            name = self.names[event.key_number]
            if event.pressed:
                if name not in self.down_since:
                    # Genuinely fresh press -- start the timer.
                    self.down_since[name] = now
                    self.hold_fired.discard(name)
                # else: a same-key re-press while already tracked -- bounce
                # during the hold, not a new press. Keep the original
                # down_since so held time keeps accumulating through it.
                self.released_at.pop(name, None)
            else:
                if name in self.down_since:
                    self.released_at[name] = now

        # Finalize releases that survived past the grace window -- a real
        # release, not bounce.
        for name in list(self.released_at):
            if now - self.released_at[name] >= self.grace_ms:
                since = self.down_since.pop(name, None)
                del self.released_at[name]
                if since is not None and name not in self.hold_fired:
                    out.append((name, "tap"))
                self.hold_fired.discard(name)

        for name, since in self.down_since.items():
            if name not in self.hold_fired and now - since >= self.hold_ms:
                self.hold_fired.add(name)
                out.append((name, "hold"))

        return out
