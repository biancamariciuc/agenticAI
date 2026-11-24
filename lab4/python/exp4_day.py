"""Lab 4 - Experiment 4: a day that actually works (STARTER).  Tests: pytest test_exp4_day.py -v"""
from pydantic import BaseModel, Field

from llm import ask

DAY_RULES = """Plan ONE day in Rome. Schedule all five items, each exactly once:
- Borghese Gallery: 2 h. Entry ONLY at 09:00, 11:00, 13:00 or 15:00.
- Colosseum: 2 h. Entry on the hour (xx:00), from 09:00 to 15:00.
- Pantheon: 1 h. Open 09:00-19:00 (must be finished by 19:00).
- Lunch: 1 h. Must START between 12:00 and 14:00.
- Trastevere food walk: 1 h 30 min. Starts ONLY at 17:00 or 18:00.
Moving between two consecutive items takes 30 minutes. The day starts at 09:00 and must end by 20:00.
Return the schedule in time order, times as HH:MM."""

WITH_HINT = (
    DAY_RULES
    + "\nBefore writing the schedule, first make a plan: place the items with the fewest allowed start "
    "times first, then fit the others around them, and check every rule (including the 30-minute moves) "
    "before you answer."
)

class Item(BaseModel):
    # A free string on purpose: the verifier catches any other spelling as an unknown item.
    name: str = Field(description="Exactly one of: Borghese Gallery, Colosseum, Pantheon, Lunch, Trastevere food walk")
    start: str
    end: str


class Schedule(BaseModel):
    items: list[Item]


class ScheduleWithPlan(BaseModel):
    """The same output, with a planning field emitted before the schedule."""
    plan: str = Field(description="Briefly plan how the constraints fit together before listing the schedule")
    items: list[Item]


RULES = {  # name -> (duration in minutes, allowed start times)
    "Borghese Gallery": (120, [h * 60 for h in (9, 11, 13, 15)]),
    "Colosseum": (120, [h * 60 for h in range(9, 16)]),
    "Pantheon": (60, list(range(9 * 60, 18 * 60 + 1))),
    "Lunch": (60, list(range(12 * 60, 14 * 60 + 1))),
    "Trastevere food walk": (90, [17 * 60, 18 * 60]),
}


def minutes(hhmm: str) -> int:
    parts = hhmm.strip().split(":")
    if len(parts) != 2:
        raise ValueError("expected HH:MM")
    h, m = (int(part) for part in parts)
    if not 0 <= h <= 23 or not 0 <= m <= 59:
        raise ValueError("time out of range")
    return h * 60 + m


def check(schedule: Schedule) -> list[str]:
    """Deterministic verifier: returns the broken rules ([] = valid)."""
    errors = []
    counts = {name: 0 for name in RULES}
    parsed = []

    for item in schedule.items:
        if item.name not in RULES:
            errors.append(f"unknown item {item.name!r}")
        else:
            counts[item.name] += 1

        try:
            start = minutes(item.start)
            end = minutes(item.end)
        except (ValueError, AttributeError):
            errors.append(f"{item.name}: bad time format")
            continue

        if item.name in RULES:
            duration, allowed_starts = RULES[item.name]
            length = end - start
            if length != duration:
                errors.append(f"{item.name}: lasts {length} min, must be {duration}")
            if start not in allowed_starts:
                errors.append(f"{item.name}: cannot start at {item.start}")
        if start < 9 * 60 or end > 20 * 60:
            errors.append("outside 09:00-20:00")
        parsed.append((start, end, item.name))

    for name, count in counts.items():
        if count != 1:
            errors.append(f"{name} appears {count} times")

    parsed.sort(key=lambda entry: entry[0])
    for first, second in zip(parsed, parsed[1:]):
        if second[0] - first[1] < 30:
            errors.append(f"{first[2]} -> {second[2]}: needs 30 min to move")
    return errors


def sample(prompt: str, schema=Schedule, n: int = 5) -> list[tuple[Schedule | None, list[str], int]]:
    out = []
    for _ in range(n):
        r = ask(prompt, schema=schema, temperature=1.0)
        if r.parsed is None:  # malformed JSON: an invalid sample, not a crash
            out.append((None, ["unparseable output"], r.out_tokens))
        else:
            out.append((r.parsed, check(r.parsed), r.out_tokens))
    return out


def best(samples):
    """TODO (step 2): keep only the VALID samples (no errors) and return the schedule whose first item
    starts latest (Ana hates early mornings), or None if no sample is valid."""
    valid = [schedule for schedule, errors, _ in samples if schedule is not None and not errors]
    if not valid:
        return None
    return max(valid, key=lambda schedule: min(minutes(item.start) for item in schedule.items))


if __name__ == "__main__":
    runs = [("plain", DAY_RULES, Schedule),
            ("hint in the prompt, JSON only", WITH_HINT, Schedule),
            ("hint + a field to plan in", WITH_HINT, ScheduleWithPlan)]
    for label, prompt, schema in runs:
        samples = sample(prompt, schema)
        ok = sum(1 for _, errs, _ in samples if not errs)
        print(f"\n== {label}: {ok}/{len(samples)} valid, "
              f"{sum(t for *_, t in samples) / len(samples):.0f} output tokens on average")
        for _, errs, _ in samples:
            print("   ok" if not errs else f"   {errs[0]}" + (f" (+{len(errs) - 1} more)" if len(errs) > 1 else ""))
        chosen = best(samples)
        if chosen:
            print("   chosen: " + " | ".join(f"{i.start} {i.name}" for i in sorted(chosen.items, key=lambda i: minutes(i.start))))
