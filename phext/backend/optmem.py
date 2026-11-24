"""The optmem tree math, ported from memory.rs (ravel @ 003fee9).

A soul's biography is a log of notes; summaries merge adjacent pairs up
a binary tree. The wake view shows at most WAKE_LINES lines: a cover of
the log by built blocks and raw notes. Merges happen by (lo, span) —
never by summary text — and a block's id is "lo-(hi-1)".
"""
WAKE_LINES = 96
ENTRY_CHARS = 280
RAW_MAX = 16


def block_id(lo, hi):
    return f"{lo}-{hi - 1}"


def parse_block(block):
    """(lo, hi) for an aligned power-of-two block id, else None.
    A leading '#' is stripped, as OptMem prints ids with one."""
    try:
        a, b = block.lstrip("#").split("-")
        lo, last = int(a), int(b)
    except (ValueError, AttributeError):
        return None
    if lo < 0 or last < lo:
        return None
    hi = last + 1
    span = hi - lo
    if span < 2 or span & (span - 1) or lo % span:
        return None
    return (lo, hi)


def built_blocks(summaries, notes_n):
    """{(lo, hi): summary} for summaries that still fit the log."""
    out = {}
    for s in summaries:
        span = parse_block(s["block"])
        if span and span[1] <= notes_n:
            out[span] = s
    return out


def _cover_at(t, alpha):
    """Tile [0, t) with aligned power-of-two blocks; a block stays whole
    iff its size is at most alpha times its age (t - lo)."""
    root = 1
    while root < t:
        root *= 2
    out = []
    stack = [(0, root)]
    while stack:
        lo, hi = stack.pop()
        if lo >= t:
            continue
        size = hi - lo
        if size > 1 and (hi > t or size > alpha * (t - lo)):
            mid = (lo + hi) // 2
            stack.append((mid, hi))
            stack.append((lo, mid))
        else:
            out.append((lo, hi))
    out.sort()
    return out


def cover(t, budget=WAKE_LINES):
    """The wake cover of a log of t notes: [(lo, hi), ...]. Positional
    only — whether a block is built decides its line's text (the summary
    or the pending marker), never the tiling."""
    if t == 0:
        return []
    if t <= budget:
        return [(i, i + 1) for i in range(t)]
    lo_a, hi_a = 0.0, 1.0
    for _ in range(60):
        alpha = (lo_a + hi_a) / 2
        if len(_cover_at(t, alpha)) > budget:
            lo_a = alpha
        else:
            hi_a = alpha
    out = _cover_at(t, hi_a)
    while len(out) < budget:
        idx = None
        for i in range(len(out) - 1, -1, -1):
            if out[i][1] - out[i][0] > 1:
                idx = i
                break
        if idx is None:
            break
        lo, hi = out[idx]
        mid = (lo + hi) // 2
        out[idx : idx + 1] = [(lo, mid), (mid, hi)]
    return out


def pending(t, built):
    """Blocks still to build, smallest first: [(lo, hi), ...]. `built`
    maps (lo, hi) -> summary; a size counts as had when the dense prefix
    of built blocks of that size is complete."""
    if t < 2:
        return []

    def have(size):
        n = 0
        while (n * size, (n + 1) * size) in built:
            n += 1
        return n

    out = []
    size = 2
    while size <= t:
        start = have(size) * size
        stop = (t // size) * size
        for lo in range(start, stop, size):
            out.append((lo, lo + size))
        size *= 2
    return out


def check(text):
    """memory.rs's check(), word for word: the trimmed text, or the
    error OptMem gives the writer."""
    text = text.strip()
    if not text:
        return None, "Empty. A memory is one line of text."
    if "\n" in text or "\r" in text:
        n = text.count("\n") + 1
        return None, f"{n} lines. A memory is one line: merge them, or note them separately."
    n = len(text.encode("utf-8"))
    if n > ENTRY_CHARS:
        return None, (
            f"Too long: {n} bytes, limit {ENTRY_CHARS}. "
            "Accented characters cost 2 bytes. Compress it further."
        )
    return text, None
